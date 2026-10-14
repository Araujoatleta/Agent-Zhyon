// The phone link end to end: a real TLS server on loopback and a client that pins
// the certificate the way the Android app does. The Kotlin client is tried
// against this same server in android/.../RustDesktopInteropTest.kt.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ring::digest;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use tokio_rustls::rustls::crypto::ring as provider;
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};
use tokio_rustls::TlsConnector;

use super::hub::{hex, ApprovalIn, Host, Hub, SessionIn};
use super::pairing::{self, tests::MemStore};
use super::server::{self, MAX_LINE};

const TOKEN: &str = "test-token-0123456789abcdef";

fn block_on<F: Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(String, bool)>>, Mutex<Vec<(String, Value)>>, Mutex<Vec<(u64, String, u64)>>);
impl Host for Recorder {
    fn get_diff(&self, sub_id: u64, pill_id: &str, file_id: u64) {
        self.2.lock().unwrap().push((sub_id, pill_id.to_string(), file_id));
    }
    fn decide(&self, request_id: &str, allow: bool) {
        self.0.lock().unwrap().push((request_id.to_string(), allow));
    }
    fn answer(&self, request_id: &str, answers: &serde_json::Map<String, Value>) {
        self.1.lock().unwrap().push((request_id.to_string(), Value::Object(answers.clone())));
    }
}

struct Rig {
    port: u16,
    fingerprint: String,
    hub: Arc<Hub>,
    host: Arc<Recorder>,
    shared: Arc<server::Shared>,
    handle: Option<server::Handle>,
}

impl Rig {
    async fn start() -> Rig {
        Rig::start_with(None).await
    }

    async fn start_with(chat: Option<Arc<super::chat::ChatLink>>) -> Rig {
        Rig::start_full(chat, Arc::new(server::NoFeatures)).await
    }

    async fn start_full(chat: Option<Arc<super::chat::ChatLink>>, features: Arc<dyn server::Features>) -> Rig {
        Rig::start_links(chat, features, server::Links::default()).await
    }

    /// With the assistant and Mochi's home wired in, as the app has them.
    async fn start_links(chat: Option<Arc<super::chat::ChatLink>>, features: Arc<dyn server::Features>, links: server::Links) -> Rig {
        let identity = pairing::load_or_create_identity(&MemStore::default()).unwrap();
        let host = Arc::new(Recorder::default());
        let hub = Hub::new(host.clone());
        let shared = server::Shared::with_links(hub.clone(), TOKEN.to_string(), "Test PC".to_string(), chat, features, links);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = server::serve(listener, server::tls_acceptor(&identity).unwrap(), shared.clone());
        Rig { port, fingerprint: identity.fingerprint, hub, host, shared, handle: Some(handle) }
    }

    fn approval(&self, request: &str, command: &str, age_ms: u64) {
        let now = server::now_ms() - age_ms;
        self.hub.publish(
            vec![],
            Some(ApprovalIn {
                request_id: request.into(),
                session_id: "s1".into(),
                pill_id: "integration_claude".into(),
                tool: "Bash".into(),
                command: command.into(),
            }),
            now,
        );
    }

    fn decisions(&self) -> Vec<(String, bool)> {
        self.host.0.lock().unwrap().clone()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            h.stop();
        }
    }
}

/// Accepts exactly one certificate, by its SHA-256 — the Android client's PinnedTrustManager.
#[derive(Debug)]
struct Pin(String);

impl ServerCertVerifier for Pin {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if hex(digest::digest(&digest::SHA256, end_entity).as_ref()) == self.0 {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(Error::General("certificate does not match the pairing".into()))
        }
    }
    fn verify_tls12_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        tokio_rustls::rustls::crypto::verify_tls12_signature(m, c, d, &provider::default_provider().signature_verification_algorithms)
    }
    fn verify_tls13_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        tokio_rustls::rustls::crypto::verify_tls13_signature(m, c, d, &provider::default_provider().signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        provider::default_provider().signature_verification_algorithms.supported_schemes()
    }
}

struct Client {
    r: BufReader<ReadHalf<TlsStream<TcpStream>>>,
    w: WriteHalf<TlsStream<TcpStream>>,
}

impl Client {
    async fn connect(port: u16, pin: &str) -> Result<Client, String> {
        Client::connect_from("127.0.0.1", port, pin).await
    }

    /// From a chosen loopback address (127.0.0.2 and so on), to stand in for another device on the network.
    async fn connect_from(source: &str, port: u16, pin: &str) -> Result<Client, String> {
        let config = ClientConfig::builder_with_provider(Arc::new(provider::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Pin(pin.to_string())))
            .with_no_client_auth();
        let socket = tokio::net::TcpSocket::new_v4().map_err(|e| e.to_string())?;
        socket.bind(format!("{source}:0").parse().unwrap()).map_err(|e| e.to_string())?;
        let tcp = socket.connect(std::net::SocketAddr::from(([127, 0, 0, 1], port))).await.map_err(|e| e.to_string())?;
        let tls = TlsConnector::from(Arc::new(config))
            .connect(ServerName::try_from("coucou").unwrap(), tcp)
            .await
            .map_err(|e| e.to_string())?;
        let (r, w) = tokio::io::split(tls);
        Ok(Client { r: BufReader::new(r), w })
    }

    async fn paired(rig: &Rig) -> Client {
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        c.send(json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "Test phone" })).await;
        assert_eq!(c.recv().await.unwrap()["type"], "welcome");
        c
    }

    async fn send(&mut self, v: Value) {
        self.send_raw(v.to_string().as_bytes()).await;
    }

    async fn send_raw(&mut self, bytes: &[u8]) {
        self.send_unterminated(bytes).await;
        let _ = self.w.write_all(b"\n").await;
        let _ = self.w.flush().await;
    }

    /// Bytes with no end of line, as a misbehaving peer would send them.
    async fn send_unterminated(&mut self, bytes: &[u8]) {
        let _ = self.w.write_all(bytes).await;
        let _ = self.w.flush().await;
    }

    /// The next message, or None if the server closed (or said nothing for 3 s).
    async fn recv(&mut self) -> Option<Value> {
        let mut line = String::new();
        match tokio::time::timeout(Duration::from_secs(3), self.r.read_line(&mut line)).await {
            Ok(Ok(n)) if n > 0 => Some(serde_json::from_str(&line).unwrap()),
            _ => None,
        }
    }

    /// The next message of this type, skipping others.
    async fn expect(&mut self, kind: &str) -> Value {
        for _ in 0..10 {
            let m = self.recv().await.unwrap_or_else(|| panic!("closed while waiting for {kind}"));
            if m["type"] == kind {
                return m;
            }
        }
        panic!("no {kind} message");
    }

    /// True once the server has really closed this connection (an end of stream,
    /// not just a quiet one).
    async fn closed(&mut self) -> bool {
        loop {
            let mut line = String::new();
            match tokio::time::timeout(Duration::from_secs(3), self.r.read_line(&mut line)).await {
                Ok(Ok(0)) | Ok(Err(_)) => return true,
                Ok(Ok(_)) => continue, // a last word before closing
                Err(_) => return false,
            }
        }
    }
}

#[test]
fn a_paired_phone_gets_sessions_and_answers_the_request() {
    block_on(async {
        let rig = Rig::start().await;
        rig.hub.publish(
            vec![SessionIn { pill_id: "integration_claude".into(), agent: "Claude Code".into(), state: "working".into(), status_text: "Editing".into(), step_index: 2, step_count: 6, ..Default::default() }],
            None,
            server::now_ms(),
        );
        let mut c = Client::paired(&rig).await;
        let sessions = c.expect("sessions").await;
        assert_eq!(sessions["sessions"][0]["agent"], "Claude Code");
        assert_eq!(sessions["sessions"][0]["state"], "working");

        rig.approval("req-1", "npm run build", 0);
        let approval = c.expect("approval").await;
        assert_eq!(approval["pillId"], "integration_claude");
        assert_eq!(approval["tool"], "Bash");
        assert_eq!(approval["command"], "npm run build");
        let fp = approval["fingerprint"].as_str().unwrap().to_string();
        assert_eq!(fp, super::hub::fingerprint("integration_claude", "s1", "Bash", "npm run build", "req-1"));

        c.send(json!({ "type": "decision", "fingerprint": fp, "decision": "allow" })).await;
        let resolved = c.expect("approvalResolved").await;
        assert_eq!(resolved["fingerprint"], fp);
        assert_eq!(rig.decisions(), vec![("req-1".to_string(), true)]);

        c.send(json!({ "type": "ping" })).await;
        c.expect("pong").await;
        c.send(json!({ "type": "bye" })).await;
        assert!(c.closed().await);
    });
}

#[test]
fn a_wrong_token_is_refused_and_gets_nothing() {
    block_on(async {
        let rig = Rig::start().await;
        rig.approval("req-1", "rm -rf /", 0);
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        c.send(json!({ "type": "hello", "v": 1, "token": "x".repeat(TOKEN.len()), "device": "Intruder" })).await;
        let first = c.recv().await.unwrap();
        assert_eq!(first["type"], "error");
        assert_eq!(first["code"], "auth");
        assert!(c.closed().await);
        assert_eq!(rig.hub.clients(), 0);
        assert!(rig.decisions().is_empty());
    });
}

#[test]
fn a_token_of_another_length_or_a_missing_one_is_refused_too() {
    block_on(async {
        let rig = Rig::start().await;
        for hello in [
            json!({ "type": "hello", "v": 1, "token": "short", "device": "x" }),
            json!({ "type": "hello", "v": 1, "device": "x" }),
            json!({ "type": "hello", "v": 1, "token": 12345, "device": "x" }),
            json!({ "type": "hello", "v": 1, "token": format!("{TOKEN}extra"), "device": "x" }),
            // Right token, other protocol version: the phone would refuse the desktop anyway.
            json!({ "type": "hello", "v": 2, "token": TOKEN, "device": "x" }),
            json!({ "type": "hello", "token": TOKEN, "device": "x" }),
        ] {
            let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
            c.send(hello.clone()).await;
            assert_eq!(c.recv().await.unwrap()["code"], "auth", "{hello}");
        }
        assert_eq!(rig.hub.clients(), 0);
    });
}

#[test]
fn a_phone_that_does_not_say_hello_first_gets_nowhere() {
    block_on(async {
        let rig = Rig::start().await;
        rig.approval("req-1", "ls", 0);
        let fp = super::hub::fingerprint("integration_claude", "s1", "Bash", "ls", "req-1");
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        c.send(json!({ "type": "decision", "fingerprint": fp, "decision": "allow" })).await;
        assert!(c.closed().await);
        assert!(rig.decisions().is_empty(), "a decision before hello must never be applied");

        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        c.send_raw(b"this is not json").await;
        assert!(c.closed().await);
    });
}

#[test]
fn a_decision_for_another_request_changes_nothing() {
    block_on(async {
        let rig = Rig::start().await;
        rig.approval("req-2", "npm test", 0);
        let mut c = Client::paired(&rig).await;
        let real = c.expect("approval").await["fingerprint"].as_str().unwrap().to_string();

        // The same command, but the fingerprint of an earlier request.
        let stale = super::hub::fingerprint("integration_claude", "s1", "Bash", "npm test", "req-1");
        for odd in [stale.as_str(), "", "not-a-fingerprint", &real.to_uppercase()] {
            c.send(json!({ "type": "decision", "fingerprint": odd, "decision": "allow" })).await;
        }
        c.send(json!({ "type": "decision", "fingerprint": real, "decision": "always" })).await;
        c.send(json!({ "type": "decision", "fingerprint": real })).await;
        c.send(json!({ "type": "decision", "decision": "allow" })).await;
        // A round trip proves the server has read all of the above.
        c.send(json!({ "type": "ping" })).await;
        c.expect("pong").await;
        assert!(rig.decisions().is_empty());

        // And the real request is still waiting for the real answer.
        c.send(json!({ "type": "decision", "fingerprint": real, "decision": "deny" })).await;
        c.expect("approvalResolved").await;
        assert_eq!(rig.decisions(), vec![("req-2".to_string(), false)]);
    });
}

#[test]
fn a_decision_that_comes_too_late_is_ignored() {
    block_on(async {
        let rig = Rig::start().await;
        // The request has been waiting for two minutes: the desktop dismissed it long ago.
        rig.approval("req-1", "npm test", 120_000);
        let mut c = Client::paired(&rig).await;
        // It is not even offered to a phone that connects now.
        c.send(json!({ "type": "ping" })).await;
        let mut offered = false;
        loop {
            let m = c.recv().await.unwrap();
            offered |= m["type"] == "approval";
            if m["type"] == "pong" {
                break;
            }
        }
        assert!(!offered, "an expired request must not be offered");

        // A phone that still holds it from before answers anyway.
        let fp = super::hub::fingerprint("integration_claude", "s1", "Bash", "npm test", "req-1");
        c.send(json!({ "type": "decision", "fingerprint": fp, "decision": "allow" })).await;
        c.send(json!({ "type": "ping" })).await;
        c.expect("pong").await;
        assert!(rig.decisions().is_empty());
    });
}

#[test]
fn a_decision_after_the_desktop_answered_is_ignored() {
    block_on(async {
        let rig = Rig::start().await;
        rig.approval("req-1", "npm test", 0);
        let mut c = Client::paired(&rig).await;
        let fp = c.expect("approval").await["fingerprint"].as_str().unwrap().to_string();

        // Allowed at the desk: the island publishes no pending request any more.
        rig.hub.publish(vec![], None, server::now_ms());
        assert_eq!(c.expect("approvalResolved").await["fingerprint"], fp);

        c.send(json!({ "type": "decision", "fingerprint": fp, "decision": "allow" })).await;
        c.send(json!({ "type": "ping" })).await;
        c.expect("pong").await;
        assert!(rig.decisions().is_empty());
    });
}

#[test]
fn a_line_over_64_kib_closes_the_connection() {
    block_on(async {
        let rig = Rig::start().await;

        // Before hello.
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        c.send_raw(&vec![b'a'; MAX_LINE + 10]).await;
        assert!(c.closed().await);

        // After hello.
        let mut c = Client::paired(&rig).await;
        c.send_raw(&vec![b'a'; MAX_LINE + 10]).await;
        assert!(c.closed().await);

        // A flood with no newline at all is cut at the limit, not buffered for ever.
        let mut c = Client::paired(&rig).await;
        for _ in 0..8 {
            c.send_unterminated(&vec![b'b'; 32 * 1024]).await;
        }
        assert!(c.closed().await);
        assert_eq!(rig.hub.clients(), 0);

        // The server is fine afterwards.
        let mut c = Client::paired(&rig).await;
        c.send(json!({ "type": "ping" })).await;
        c.expect("pong").await;
    });
}

#[test]
fn a_line_of_exactly_64_kib_is_accepted() {
    block_on(async {
        let rig = Rig::start().await;
        let mut c = Client::paired(&rig).await;
        let head = r#"{"type":"ping","pad":""#;
        let tail = r#""}"#;
        let pad = "p".repeat(MAX_LINE - head.len() - tail.len());
        let line = format!("{head}{pad}{tail}");
        assert_eq!(line.len(), MAX_LINE);
        c.send_raw(line.as_bytes()).await;
        c.expect("pong").await;
    });
}

#[test]
fn a_phone_with_another_certificate_in_mind_does_not_connect() {
    block_on(async {
        let rig = Rig::start().await;
        let err = Client::connect(rig.port, &"0".repeat(64)).await.err().expect("the pin must fail");
        assert!(err.contains("does not match"), "{err}");
        assert_eq!(rig.hub.clients(), 0);
    });
}

#[test]
fn plain_text_is_not_spoken_here() {
    block_on(async {
        let rig = Rig::start().await;
        let mut tcp = TcpStream::connect(("127.0.0.1", rig.port)).await.unwrap();
        tcp.write_all(b"{\"type\":\"hello\",\"v\":1,\"token\":\"test-token-0123456789abcdef\"}\n").await.unwrap();
        let mut buf = Vec::new();
        let mut r = BufReader::new(tcp);
        let _ = tokio::time::timeout(Duration::from_secs(3), r.read_until(b'\n', &mut buf)).await;
        assert!(!String::from_utf8_lossy(&buf).contains("welcome"));
        assert_eq!(rig.hub.clients(), 0);
    });
}

#[test]
fn pairing_again_locks_the_old_phone_out() {
    block_on(async {
        let rig = Rig::start().await;
        let mut old = Client::paired(&rig).await;

        // What phone_link_new_pairing does.
        *rig.shared.token.lock().unwrap() = "a-brand-new-token-0123456789".to_string();
        rig.hub.kick_all("auth", "unpaired");
        let word = old.expect("error").await;
        assert_eq!(word["code"], "auth");
        assert!(old.closed().await);

        let mut again = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        again.send(json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "old" })).await;
        assert_eq!(again.recv().await.unwrap()["code"], "auth");
        let mut fresh = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        fresh.send(json!({ "type": "hello", "v": 1, "token": "a-brand-new-token-0123456789", "device": "new" })).await;
        assert_eq!(fresh.recv().await.unwrap()["type"], "welcome");
    });
}

#[test]
fn turning_it_off_closes_the_phones_and_stops_listening() {
    block_on(async {
        let mut rig = Rig::start().await;
        let mut c = Client::paired(&rig).await;
        let port = rig.port;
        rig.handle.take().unwrap().stop();
        assert!(c.closed().await);
        // Nobody answers any more (the listener was dropped with the task).
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(Client::connect(port, &rig.fingerprint).await.is_err());
    });
}

/// True when this machine lets us use 127.0.0.2 and 127.0.0.3 as other devices (Linux and Windows do; some systems do not).
fn loopback_aliases() -> bool {
    ["127.0.0.2", "127.0.0.3"].iter().all(|a| std::net::TcpListener::bind((*a, 0)).is_ok())
}

/// Whether a connection that was admitted answers a hello (a dropped one is closed unserved or never completes TLS).
async fn served(c: Result<Client, String>) -> bool {
    match c {
        Err(_) => false,
        Ok(mut c) => {
            c.send(json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "probe" })).await;
            c.recv().await.is_some()
        }
    }
}

#[test]
fn the_total_number_of_connections_is_limited() {
    block_on(async {
        let rig = Rig::start().await;
        // Eight phones that said a valid hello fill the pool; the ninth is dropped.
        let mut phones = Vec::new();
        for _ in 0..8 {
            phones.push(Client::paired(&rig).await);
        }
        assert!(!served(Client::connect(rig.port, &rig.fingerprint).await).await, "the ninth connection");
        drop(phones);
    });
}

#[test]
fn one_address_cannot_hold_more_than_two_silent_connections() {
    block_on(async {
        let rig = Rig::start().await;
        let a = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        let b = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        // A third that never says hello is refused before it can wait.
        assert!(!served(Client::connect(rig.port, &rig.fingerprint).await).await, "the third from one address");
        // Closing one frees its place.
        drop(a);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(served(Client::connect(rig.port, &rig.fingerprint).await).await);
        drop(b);
    });
}

#[test]
fn a_flood_from_one_address_cannot_lock_out_the_paired_phone() {
    if !loopback_aliases() {
        eprintln!("skipped: this machine has no 127.0.0.2/127.0.0.3");
        return;
    }
    block_on(async {
        let rig = Rig::start().await;
        // The phone (127.0.0.1) pairs once, so the computer knows its address.
        drop(Client::paired(&rig).await);
        tokio::time::sleep(Duration::from_millis(200)).await;
        // Another device floods with silent connections: only two are kept.
        let mut flood = Vec::new();
        for _ in 0..10 {
            if let Ok(c) = Client::connect_from("127.0.0.2", rig.port, &rig.fingerprint).await {
                flood.push(c);
            }
        }
        assert!(flood.len() <= 2, "the flood is capped at two, got {}", flood.len());
        // The paired phone connects and is served, again and again, while the flood goes on.
        for _ in 0..3 {
            let mut phone = Client::paired(&rig).await;
            phone.send(json!({ "type": "ping" })).await;
            phone.expect("pong").await;
        }
        drop(flood);
    });
}

#[test]
fn a_flood_from_several_addresses_still_leaves_room_for_the_known_phone() {
    if !loopback_aliases() {
        eprintln!("skipped: this machine has no 127.0.0.2/127.0.0.3");
        return;
    }
    block_on(async {
        let rig = Rig::start().await;
        drop(Client::paired(&rig).await);
        tokio::time::sleep(Duration::from_millis(200)).await;
        // Two other devices fill the whole stranger pool (2 + 2).
        let mut flood = Vec::new();
        for source in ["127.0.0.2", "127.0.0.3"] {
            for _ in 0..4 {
                if let Ok(c) = Client::connect_from(source, rig.port, &rig.fingerprint).await {
                    flood.push(c);
                }
            }
        }
        assert_eq!(flood.len(), 4, "strangers share four places");
        let mut phone = Client::paired(&rig).await;
        phone.send(json!({ "type": "ping" })).await;
        phone.expect("pong").await;
        drop(flood);
    });
}


// ── Chat from the phone: the rules, with a fake provider ──────────────────

mod chat_tests {
    use super::*;
    use crate::phone_link::chat::{
        reason, BoxFuture, ChatBackend, ChatConfig, ChatFail, ChatLink, ModelOption, DONE_TEXT_MAX, MAX_SENDS, MAX_TEXT_CHARS,
    };
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::mpsc::UnboundedSender;

    #[derive(Clone)]
    enum Script {
        /// Streams these parts one after the other (the answer so far each time).
        Words(Vec<String>),
        /// Streams `streamed`, then returns `full` (which is not an extension of it).
        Rewrite { streamed: String, full: String },
        Fail(ChatFail),
        /// Never answers (a slow provider).
        Hang,
    }

    struct Fake {
        config: Mutex<ChatConfig>,
        script: Mutex<Script>,
        calls: Mutex<Vec<(String, String)>>,
        resets: AtomicUsize,
        dropped: Arc<AtomicBool>,
    }

    impl Fake {
        fn new(enabled: bool) -> Arc<Fake> {
            let models = vec![
                ModelOption { id: "anthropic/claude-x".into(), provider: "anthropic".into(), label: "Claude X".into() },
                ModelOption { id: "openai/gpt-y".into(), provider: "openai".into(), label: "GPT Y".into() },
            ];
            Arc::new(Fake {
                config: Mutex::new(ChatConfig { enabled, allowed: models }),
                script: Mutex::new(Script::Words(vec!["Hel".into(), "lo".into(), " world".into()])),
                calls: Mutex::new(Vec::new()),
                resets: AtomicUsize::new(0),
                dropped: Arc::new(AtomicBool::new(false)),
            })
        }
        fn script(&self, s: Script) {
            *self.script.lock().unwrap() = s;
        }
        fn enable(&self, on: bool) {
            self.config.lock().unwrap().enabled = on;
        }
    }

    impl ChatBackend for Fake {
        fn config(&self) -> ChatConfig {
            self.config.lock().unwrap().clone()
        }
        fn send(&self, model_id: &str, text: String, deltas: UnboundedSender<String>) -> BoxFuture<Result<String, ChatFail>> {
            self.calls.lock().unwrap().push((model_id.to_string(), text));
            let script = self.script.lock().unwrap().clone();
            let dropped = self.dropped.clone();
            self.dropped.store(false, Ordering::SeqCst);
            Box::pin(async move {
                struct Mark(Arc<AtomicBool>);
                impl Drop for Mark {
                    fn drop(&mut self) {
                        self.0.store(true, Ordering::SeqCst);
                    }
                }
                let _mark = Mark(dropped);
                match script {
                    Script::Words(parts) => {
                        let mut so_far = String::new();
                        for p in parts {
                            so_far.push_str(&p);
                            let _ = deltas.send(so_far.clone());
                            tokio::task::yield_now().await;
                        }
                        Ok(so_far)
                    }
                    Script::Rewrite { streamed, full } => {
                        let _ = deltas.send(streamed);
                        tokio::task::yield_now().await;
                        Ok(full)
                    }
                    Script::Fail(f) => Err(f),
                    Script::Hang => std::future::pending().await,
                }
            })
        }
        fn reset(&self) {
            self.resets.fetch_add(1, Ordering::SeqCst);
        }
    }

    async fn rig(fake: &Arc<Fake>) -> Rig {
        Rig::start_with(Some(ChatLink::new(fake.clone()))).await
    }

    async fn phone(rig: &Rig, caps: Option<Value>) -> (Client, Value) {
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        let mut hello = json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "Test phone" });
        if let Some(caps) = caps {
            hello["caps"] = caps;
        }
        c.send(hello).await;
        let welcome = c.expect("welcome").await;
        (c, welcome)
    }

    fn send(id: &str, model: &str, text: &str) -> Value {
        json!({ "type": "chatSend", "id": id, "model": model, "text": text })
    }

    /// Everything of one answer until chatDone or chatError.
    async fn answer(c: &mut Client, id: &str) -> (String, Value) {
        let mut text = String::new();
        for _ in 0..200 {
            let m = c.recv().await.expect("the connection closed or went quiet");
            match m["type"].as_str() {
                Some("chatDelta") if m["id"] == id => text.push_str(m["text"].as_str().unwrap()),
                Some("chatDone") | Some("chatError") if m["id"] == id => return (text, m),
                _ => {}
            }
        }
        panic!("no end of answer");
    }

    #[test]
    fn a_phone_that_asks_is_offered_chat_and_gets_only_the_allowed_models() {
        block_on(async {
            let fake = Fake::new(true);
            let rig = rig(&fake).await;
            let (mut c, welcome) = phone(&rig, Some(json!(["chat"]))).await;
            assert_eq!(welcome["caps"], json!(["chat"]));
            c.send(json!({ "type": "chatModels" })).await;
            let m = c.expect("chatModels").await;
            assert_eq!(m["models"][0], json!({ "id": "anthropic/claude-x", "provider": "anthropic", "label": "Claude X" }));
            assert_eq!(m["models"].as_array().unwrap().len(), 2);
            // only ids, providers and labels: nothing else is in the message
            assert_eq!(m["models"][0].as_object().unwrap().len(), 3);
        });
    }

    #[test]
    fn an_older_phone_is_never_offered_nor_answered_anything_about_chat() {
        block_on(async {
            let fake = Fake::new(true);
            let rig = rig(&fake).await;
            let (mut c, welcome) = phone(&rig, None).await;
            assert!(welcome.get("caps").is_none(), "{welcome}");
            c.send(send("a1", "anthropic/claude-x", "hi")).await;
            c.send(json!({ "type": "chatModels" })).await;
            c.send(json!({ "type": "ping" })).await;
            // the first thing back is the pong: nothing was said about chat in between
            loop {
                let m = c.recv().await.unwrap();
                assert!(!m["type"].as_str().unwrap().starts_with("chat"), "{m}");
                if m["type"] == "pong" {
                    break;
                }
            }
            assert!(fake.calls.lock().unwrap().is_empty());
        });
    }

    #[test]
    fn an_older_computer_is_the_same_as_chat_being_off_no_caps_in_welcome() {
        block_on(async {
            // No chat wired in at all (as before this feature): the welcome has no caps even if the phone asks.
            let rig = Rig::start().await;
            let (_c, welcome) = phone(&rig, Some(json!(["chat", "details"]))).await;
            assert!(welcome.get("caps").is_none());
            assert_eq!(welcome["v"], 1);
        });
    }

    #[test]
    fn with_the_switch_off_nothing_is_offered_and_a_send_is_ignored() {
        block_on(async {
            let fake = Fake::new(false);
            let rig = rig(&fake).await;
            let (mut c, welcome) = phone(&rig, Some(json!(["chat"]))).await;
            assert!(welcome.get("caps").is_none());
            c.send(send("a1", "anthropic/claude-x", "hi")).await;
            c.send(json!({ "type": "ping" })).await;
            loop {
                let m = c.recv().await.unwrap();
                assert!(!m["type"].as_str().unwrap().starts_with("chat"), "{m}");
                if m["type"] == "pong" {
                    break;
                }
            }
            assert!(fake.calls.lock().unwrap().is_empty());
        });
    }

    #[test]
    fn an_answer_arrives_in_pieces_that_add_up_and_the_model_gets_the_text() {
        block_on(async {
            let fake = Fake::new(true);
            let rig = rig(&fake).await;
            let (mut c, _) = phone(&rig, Some(json!(["chat"]))).await;
            c.send(send("a1", "openai/gpt-y", "Say hello")).await;
            let (text, end) = answer(&mut c, "a1").await;
            assert_eq!(text, "Hello world");
            assert_eq!(end["type"], "chatDone");
            assert!(end.get("text").is_none(), "no correction needed");
            assert_eq!(*fake.calls.lock().unwrap(), vec![("openai/gpt-y".to_string(), "Say hello".to_string())]);
        });
    }

    #[test]
    fn a_rewritten_answer_is_corrected_in_done() {
        block_on(async {
            let fake = Fake::new(true);
            fake.script(Script::Rewrite { streamed: "<think>hmm".into(), full: "Answer".into() });
            let rig = rig(&fake).await;
            let (mut c, _) = phone(&rig, Some(json!(["chat"]))).await;
            c.send(send("a1", "openai/gpt-y", "q")).await;
            let (_, end) = answer(&mut c, "a1").await;
            assert_eq!(end["type"], "chatDone");
            assert_eq!(end["text"], "Answer");
        });
    }

    #[test]
    fn a_huge_answer_is_cut_into_lines_under_64_kib() {
        block_on(async {
            let fake = Fake::new(true);
            let big = "é".repeat(60_000); // 120 KB in one go
            fake.script(Script::Words(vec![big.clone()]));
            let rig = rig(&fake).await;
            let (mut c, _) = phone(&rig, Some(json!(["chat"]))).await;
            c.send(send("a1", "openai/gpt-y", "q")).await;
            let (text, end) = answer(&mut c, "a1").await;
            assert_eq!(end["type"], "chatDone");
            assert_eq!(text, big);
        });
    }

    #[test]
    fn failures_carry_a_fixed_code_and_message_only() {
        block_on(async {
            for (fail, code) in [
                (ChatFail::NoKey, "no_key"),
                (ChatFail::Unreachable, "unreachable"),
                (ChatFail::Auth, "auth"),
                (ChatFail::Provider, "provider"),
                (ChatFail::Internal, "internal"),
            ] {
                let fake = Fake::new(true);
                fake.script(Script::Fail(fail));
                let rig = rig(&fake).await;
                let (mut c, _) = phone(&rig, Some(json!(["chat"]))).await;
                c.send(send("a1", "openai/gpt-y", "q")).await;
                let (_, end) = answer(&mut c, "a1").await;
                assert_eq!(end["type"], "chatError");
                assert_eq!(end["reason"], code);
                assert_eq!(end["message"], reason(code));
                assert_eq!(end.as_object().unwrap().len(), 4, "type, id, reason, message and nothing else: {end}");
            }
        });
    }

    #[test]
    fn the_text_the_model_and_the_id_are_checked() {
        block_on(async {
            let fake = Fake::new(true);
            let rig = rig(&fake).await;
            let (mut c, _) = phone(&rig, Some(json!(["chat"]))).await;
            c.send(send("e1", "openai/gpt-y", &"x".repeat(MAX_TEXT_CHARS + 1))).await;
            assert_eq!(answer(&mut c, "e1").await.1["reason"], "too_long");
            c.send(send("e2", "openai/gpt-y", "   ")).await;
            assert_eq!(answer(&mut c, "e2").await.1["reason"], "empty");
            c.send(send("e3", "openai/not-allowed", "hi")).await;
            assert_eq!(answer(&mut c, "e3").await.1["reason"], "not_allowed");
            c.send(send("e4", "", "hi")).await;
            assert_eq!(answer(&mut c, "e4").await.1["reason"], "not_allowed");
            // exactly the limit is fine
            c.send(send("ok", "openai/gpt-y", &"é".repeat(MAX_TEXT_CHARS))).await;
            assert_eq!(answer(&mut c, "ok").await.1["type"], "chatDone");
            // an id that is empty, too long or odd is not answered at all
            for bad in ["", &"a".repeat(65), "a b", "a\u{0}b"] {
                c.send(send(bad, "openai/gpt-y", "hi")).await;
            }
            c.send(json!({ "type": "ping" })).await;
            c.expect("pong").await;
            assert_eq!(fake.calls.lock().unwrap().len(), 1, "only the valid send reached the provider");
        });
    }

    #[test]
    fn only_one_answer_runs_at_a_time_and_cancel_frees_the_slot() {
        block_on(async {
            let fake = Fake::new(true);
            fake.script(Script::Hang);
            let rig = rig(&fake).await;
            let (mut c, _) = phone(&rig, Some(json!(["chat"]))).await;
            c.send(send("a1", "openai/gpt-y", "slow")).await;
            // wait until the provider call has started
            for _ in 0..100 {
                if !fake.calls.lock().unwrap().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            c.send(send("a2", "openai/gpt-y", "second")).await;
            assert_eq!(answer(&mut c, "a2").await.1["reason"], "busy");
            c.send(json!({ "type": "chatCancel", "id": "a1" })).await;
            assert_eq!(answer(&mut c, "a1").await.1["reason"], "canceled");
            assert!(wait_for(|| fake.dropped.load(Ordering::SeqCst)).await, "the provider call was dropped");
            fake.script(Script::Words(vec!["ok".into()]));
            c.send(send("a3", "openai/gpt-y", "third")).await;
            assert_eq!(answer(&mut c, "a3").await.0, "ok");
        });
    }

    #[test]
    fn a_cancel_for_another_id_changes_nothing() {
        block_on(async {
            let fake = Fake::new(true);
            fake.script(Script::Hang);
            let rig = rig(&fake).await;
            let (mut c, _) = phone(&rig, Some(json!(["chat"]))).await;
            c.send(send("a1", "openai/gpt-y", "slow")).await;
            c.send(json!({ "type": "chatCancel", "id": "other" })).await;
            c.send(send("a2", "openai/gpt-y", "x")).await;
            assert_eq!(answer(&mut c, "a2").await.1["reason"], "busy", "a1 is still running");
        });
    }

    #[test]
    fn the_rate_limit_stops_the_thirteenth_send() {
        block_on(async {
            let fake = Fake::new(true);
            let rig = rig(&fake).await;
            let (mut c, _) = phone(&rig, Some(json!(["chat"]))).await;
            for i in 0..MAX_SENDS {
                let id = format!("m{i}");
                c.send(send(&id, "openai/gpt-y", "hi")).await;
                assert_eq!(answer(&mut c, &id).await.1["type"], "chatDone", "send {i}");
            }
            c.send(send("over", "openai/gpt-y", "hi")).await;
            assert_eq!(answer(&mut c, "over").await.1["reason"], "rate");
            assert_eq!(fake.calls.lock().unwrap().len(), MAX_SENDS);
        });
    }

    #[test]
    fn leaving_stops_the_answer_and_frees_the_slot_for_the_next_phone() {
        block_on(async {
            let fake = Fake::new(true);
            fake.script(Script::Hang);
            let rig = rig(&fake).await;
            let (mut c, _) = phone(&rig, Some(json!(["chat"]))).await;
            c.send(send("a1", "openai/gpt-y", "slow")).await;
            assert!(wait_for(|| !fake.calls.lock().unwrap().is_empty()).await);
            c.send(json!({ "type": "bye" })).await;
            assert!(c.closed().await);
            assert!(wait_for(|| fake.dropped.load(Ordering::SeqCst)).await, "no answer keeps running for a phone that left");
            fake.script(Script::Words(vec!["fine".into()]));
            let (mut d, _) = phone(&rig, Some(json!(["chat"]))).await;
            d.send(send("b1", "openai/gpt-y", "hi")).await;
            assert_eq!(answer(&mut d, "b1").await.0, "fine");
        });
    }

    #[test]
    fn switching_chat_off_while_connected_refuses_further_sends() {
        block_on(async {
            let fake = Fake::new(true);
            let rig = rig(&fake).await;
            let (mut c, _) = phone(&rig, Some(json!(["chat"]))).await;
            fake.enable(false);
            c.send(send("a1", "openai/gpt-y", "hi")).await;
            let (_, end) = answer(&mut c, "a1").await;
            assert_eq!(end["reason"], "off");
            assert!(fake.calls.lock().unwrap().is_empty());
            c.send(json!({ "type": "chatModels" })).await;
            assert_eq!(c.expect("chatModels").await["models"], json!([]));
        });
    }

    #[test]
    fn turning_the_link_off_cancels_the_answer_and_forgets_the_conversation() {
        block_on(async {
            let fake = Fake::new(true);
            fake.script(Script::Hang);
            let mut rig = rig(&fake).await;
            let (mut c, _) = phone(&rig, Some(json!(["chat"]))).await;
            c.send(send("a1", "openai/gpt-y", "slow")).await;
            assert!(wait_for(|| !fake.calls.lock().unwrap().is_empty()).await);
            rig.handle.take().unwrap().stop();
            assert!(wait_for(|| fake.dropped.load(Ordering::SeqCst)).await);
            assert_eq!(fake.resets.load(Ordering::SeqCst), 1);
        });
    }

    #[test]
    fn new_chat_forgets_the_computers_side_and_stops_a_running_answer() {
        block_on(async {
            let fake = Fake::new(true);
            fake.script(Script::Hang);
            let rig = rig(&fake).await;
            let (mut c, _) = phone(&rig, Some(json!(["chat"]))).await;
            c.send(send("a1", "openai/gpt-y", "slow")).await;
            assert!(wait_for(|| !fake.calls.lock().unwrap().is_empty()).await);
            c.send(json!({ "type": "chatReset" })).await;
            assert_eq!(answer(&mut c, "a1").await.1["reason"], "canceled");
            assert_eq!(fake.resets.load(Ordering::SeqCst), 1);
        });
    }

    #[test]
    fn nothing_that_leaves_the_computer_can_contain_a_key_or_a_provider_message() {
        block_on(async {
            // The backend has no way to hand text to the link besides the answer itself, so
            // a failure cannot carry any; this pins the messages that exist.
            for code in ["off", "not_allowed", "busy", "rate", "too_long", "empty", "no_key", "unreachable", "auth", "provider", "canceled", "internal"] {
                let m = reason(code);
                assert!(!m.contains("sk-") && !m.to_lowercase().contains("bearer") && !m.contains("key:"), "{m}");
            }
            assert!(DONE_TEXT_MAX < 64 * 1024);
        });
    }

    async fn wait_for(cond: impl Fn() -> bool) -> bool {
        for _ in 0..200 {
            if cond() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }
}


// ── Session details (cap `details`) end to end ───────────────────────────────────────────────

mod details_tests {
    use super::*;
    use crate::phone_link::server::Features;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Switch(AtomicBool);
    impl Features for Switch {
        fn details(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn rich() -> SessionIn {
        SessionIn {
            pill_id: "integration_claude".into(),
            agent: "Claude Code".into(),
            state: "finished".into(),
            status_text: "Done".into(),
            step_index: 2,
            step_count: 3,
            steps: vec!["Read · a.rs".into(), "Edit · a.rs".into()],
            final_line: Some("Fixed the bug".into()),
            project: Some("/home/me/secret-folder/coucou".into()),
            color: Some("#2DD4BF".into()),
            ..Default::default()
        }
    }

    async fn rig(on: bool) -> Rig {
        let rig = Rig::start_full(None, Arc::new(Switch(AtomicBool::new(on)))).await;
        rig.hub.publish(vec![rich()], None, server::now_ms());
        rig
    }

    async fn phone(rig: &Rig, caps: Option<Value>) -> (Client, Value, Value) {
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        let mut hello = json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "Test phone" });
        if let Some(caps) = caps {
            hello["caps"] = caps;
        }
        c.send(hello).await;
        let welcome = c.expect("welcome").await;
        let sessions = c.expect("sessions").await;
        (c, welcome, sessions)
    }

    #[test]
    fn a_phone_that_asks_gets_the_details_when_the_switch_is_on() {
        block_on(async {
            let rig = rig(true).await;
            let (_c, welcome, sessions) = phone(&rig, Some(json!(["details"]))).await;
            assert_eq!(welcome["caps"], json!(["details"]));
            let s = &sessions["sessions"][0];
            assert_eq!(s["steps"], json!(["Read · a.rs", "Edit · a.rs"]));
            assert_eq!(s["finalLine"], "Fixed the bug");
            assert_eq!(s["project"], "coucou", "the folder name, never the path");
            assert_eq!(s["color"], "#2DD4BF");
            assert!(!sessions.to_string().contains("secret-folder"));
        });
    }

    #[test]
    fn an_older_phone_gets_the_v1_sessions_and_no_caps() {
        block_on(async {
            let rig = rig(true).await;
            let (_c, welcome, sessions) = phone(&rig, None).await;
            assert!(welcome.get("caps").is_none(), "{welcome}");
            let s = sessions["sessions"][0].as_object().unwrap();
            let mut keys: Vec<&str> = s.keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(keys, ["agent", "pillId", "state", "statusText", "stepCount", "stepIndex", "updatedAt"]);
            assert!(!sessions.to_string().contains("coucou") && !sessions.to_string().contains("Fixed the bug"));
        });
    }

    #[test]
    fn with_the_switch_off_nothing_extra_is_offered_even_to_a_phone_that_asks() {
        block_on(async {
            let rig = rig(false).await;
            let (_c, welcome, sessions) = phone(&rig, Some(json!(["details", "chat"]))).await;
            assert!(welcome.get("caps").is_none());
            assert!(sessions["sessions"][0].get("steps").is_none());
            assert!(sessions["sessions"][0].get("project").is_none());
        });
    }

    #[test]
    fn a_phone_gets_a_live_update_with_details_and_another_phone_without() {
        block_on(async {
            let rig = rig(true).await;
            let (mut with, _, _) = phone(&rig, Some(json!(["details"]))).await;
            let (mut without, _, _) = phone(&rig, None).await;
            let mut next = rich();
            next.final_line = Some("A second line".into());
            rig.hub.publish(vec![next], None, server::now_ms());
            let a = with.expect("sessions").await;
            let b = without.expect("sessions").await;
            assert_eq!(a["sessions"][0]["finalLine"], "A second line");
            assert!(b["sessions"][0].get("finalLine").is_none());
        });
    }

    #[test]
    fn a_full_load_of_details_still_arrives_under_the_line_limit() {
        block_on(async {
            let rig = Rig::start_full(None, Arc::new(Switch(AtomicBool::new(true)))).await;
            let sessions: Vec<SessionIn> = (0..16)
                .map(|i| SessionIn { pill_id: format!("agent_{i}"), steps: (0..20).map(|n| format!("{n} {}", "x".repeat(250))).collect(), ..rich() })
                .collect();
            rig.hub.publish(sessions, None, server::now_ms());
            let (_c, _, line) = phone(&rig, Some(json!(["details"]))).await;
            assert_eq!(line["sessions"].as_array().unwrap().len(), 16);
        });
    }
}


// ── Answering questions (cap `answers`) end to end ───────────────────────────────────────────

mod answers_tests {
    use super::*;
    use crate::phone_link::hub::{OptionIn, QuestionIn, QuestionItemIn};
    use crate::phone_link::server::Features;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Switch(AtomicBool);
    impl Features for Switch {
        fn details(&self) -> bool {
            false
        }
        fn answers(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn opt(l: &str) -> OptionIn {
        OptionIn { label: l.into(), description: format!("about {l}") }
    }

    fn question(request: &str) -> QuestionIn {
        QuestionIn {
            request_id: request.into(),
            session_id: "s1".into(),
            pill_id: "integration_claude".into(),
            questions: vec![
                QuestionItemIn { question: "Which engine?".into(), options: vec![opt("Postgres"), opt("Meilisearch")], multi_select: false },
                QuestionItemIn { question: "Which extras?".into(), options: vec![opt("Typos"), opt("Facets"), opt("Synonyms")], multi_select: true },
            ],
        }
    }

    async fn rig(on: bool) -> Rig {
        Rig::start_full(None, Arc::new(Switch(AtomicBool::new(on)))).await
    }

    async fn phone(rig: &Rig, caps: Option<Value>) -> (Client, Value) {
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        let mut hello = json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "Test phone" });
        if let Some(caps) = caps {
            hello["caps"] = caps;
        }
        c.send(hello).await;
        let welcome = c.expect("welcome").await;
        (c, welcome)
    }

    /// Everything the phone is sent up to the answer to a ping: proves what it was and was not told.
    async fn types_until_pong(c: &mut Client) -> Vec<String> {
        c.send(json!({ "type": "ping" })).await;
        let mut seen = Vec::new();
        for _ in 0..10 {
            let m = c.recv().await.expect("closed");
            let t = m["type"].as_str().unwrap_or("").to_string();
            if t == "pong" {
                return seen;
            }
            seen.push(t);
        }
        panic!("no pong");
    }

    fn answers_of(rig: &Rig) -> Vec<(String, Value)> {
        rig.host.1.lock().unwrap().clone()
    }

    #[test]
    fn a_phone_that_asks_is_offered_answers_only_while_the_switch_is_on() {
        block_on(async {
            let on = rig(true).await;
            let (_c, welcome) = phone(&on, Some(json!(["answers"]))).await;
            assert_eq!(welcome["caps"], json!(["answers"]));
            let off = rig(false).await;
            let (_c, welcome) = phone(&off, Some(json!(["answers"]))).await;
            assert!(welcome.get("caps").is_none(), "{welcome}");
        });
    }

    #[test]
    fn the_question_reaches_a_phone_with_the_capability_and_no_other() {
        block_on(async {
            let rig = rig(true).await;
            let (mut with, _) = phone(&rig, Some(json!(["answers"]))).await;
            let (mut old, _) = phone(&rig, None).await;
            rig.hub.publish_question(Some(question("r1")), server::now_ms());
            let q = with.expect("question").await;
            assert_eq!(q["questions"][0]["question"], "Which engine?");
            assert_eq!(q["questions"][1]["multiSelect"], true);
            assert_eq!(q["questions"][0]["options"][1]["label"], "Meilisearch");
            assert!(q["fingerprint"].as_str().unwrap().len() == 64);
            assert!(q.get("requestId").is_none() && !q.to_string().contains("r1"), "the island's request id stays on the computer");
            // the older phone is told nothing, not even that something was resolved
            rig.hub.publish_question(None, server::now_ms());
            assert_eq!(with.expect("approvalResolved").await["fingerprint"], q["fingerprint"]);
            let told = types_until_pong(&mut old).await;
            assert!(!told.iter().any(|t| t == "question" || t == "approvalResolved"), "{told:?}");
        });
    }

    #[test]
    fn a_phone_that_connects_late_still_gets_the_question_that_waits() {
        block_on(async {
            let rig = rig(true).await;
            rig.hub.publish_question(Some(question("r1")), server::now_ms());
            let (mut c, _) = phone(&rig, Some(json!(["answers"]))).await;
            assert_eq!(c.expect("question").await["questions"].as_array().unwrap().len(), 2);
        });
    }

    #[test]
    fn a_valid_answer_goes_back_to_the_agent_in_the_shape_of_the_island() {
        block_on(async {
            let rig = rig(true).await;
            let (mut c, _) = phone(&rig, Some(json!(["answers"]))).await;
            rig.hub.publish_question(Some(question("r1")), server::now_ms());
            let q = c.expect("question").await;
            c.send(json!({ "type": "answer", "fingerprint": q["fingerprint"], "picks": [["Meilisearch"], ["Typos", "Synonyms"]] })).await;
            c.expect("approvalResolved").await;
            let got = answers_of(&rig);
            assert_eq!(got.len(), 1);
            assert_eq!(got[0].0, "r1");
            assert_eq!(got[0].1, json!({ "Which engine?": "Meilisearch", "Which extras?": ["Typos", "Synonyms"] }));
            // once only
            c.send(json!({ "type": "answer", "fingerprint": q["fingerprint"], "picks": [["Postgres"], ["Facets"]] })).await;
            c.send(json!({ "type": "ping" })).await;
            c.expect("pong").await;
            assert_eq!(answers_of(&rig).len(), 1);
        });
    }

    #[test]
    fn a_wrong_answer_changes_nothing_and_the_question_stays_open() {
        block_on(async {
            let rig = rig(true).await;
            let (mut c, _) = phone(&rig, Some(json!(["answers"]))).await;
            rig.hub.publish_question(Some(question("r1")), server::now_ms());
            let fp = c.expect("question").await["fingerprint"].clone();
            let bad = [
                json!([["Postgres"]]),                              // one pick too few
                json!([["Mongo"], ["Typos"]]),                      // not an option
                json!([["postgres"], ["Typos"]]),                   // not the exact label
                json!([["Postgres", "Meilisearch"], ["Typos"]]),    // two for a single choice
                json!([["Postgres"], []]),                          // nothing for a multiple choice
                json!([["Postgres"], ["Typos", "Typos"]]),          // the same twice
                json!([["Postgres"], "Typos"]),                     // wrong shape
                json!("Postgres"),
                json!([[1], [2]]),
            ];
            for picks in bad {
                c.send(json!({ "type": "answer", "fingerprint": fp, "picks": picks })).await;
            }
            c.send(json!({ "type": "answer", "fingerprint": "0".repeat(64), "picks": [["Postgres"], ["Typos"]] })).await;
            c.send(json!({ "type": "ping" })).await;
            c.expect("pong").await;
            assert!(answers_of(&rig).is_empty());
            // and the right one still works afterwards
            c.send(json!({ "type": "answer", "fingerprint": fp, "picks": [["Postgres"], ["Facets"]] })).await;
            c.expect("approvalResolved").await;
            assert_eq!(answers_of(&rig).len(), 1);
        });
    }

    #[test]
    fn a_phone_without_the_capability_cannot_answer_even_with_the_right_fingerprint() {
        block_on(async {
            let rig = rig(true).await;
            let (mut with, _) = phone(&rig, Some(json!(["answers"]))).await;
            let (mut old, _) = phone(&rig, None).await;
            rig.hub.publish_question(Some(question("r1")), server::now_ms());
            let fp = with.expect("question").await["fingerprint"].clone();
            old.send(json!({ "type": "answer", "fingerprint": fp, "picks": [["Postgres"], ["Typos"]] })).await;
            old.send(json!({ "type": "ping" })).await;
            old.expect("pong").await;
            assert!(answers_of(&rig).is_empty());
        });
    }

    #[test]
    fn a_question_that_does_not_fit_the_limits_stays_on_the_island() {
        block_on(async {
            let rig = rig(true).await;
            let (mut c, _) = phone(&rig, Some(json!(["answers"]))).await;
            let mut long = question("r1");
            long.questions[0].options[0].label = "x".repeat(200);
            rig.hub.publish_question(Some(long), server::now_ms());
            let mut twins = question("r2");
            twins.questions[0].options = vec![opt("Same"), opt("Same")];
            rig.hub.publish_question(Some(twins), server::now_ms());
            let told = types_until_pong(&mut c).await;
            assert!(!told.iter().any(|t| t == "question"), "nothing was offered: {told:?}");
        });
    }

    #[test]
    fn a_permission_request_still_reaches_every_phone_as_before() {
        block_on(async {
            let rig = rig(true).await;
            let (mut with, _) = phone(&rig, Some(json!(["answers"]))).await;
            let (mut old, _) = phone(&rig, None).await;
            rig.approval("r9", "ls", 0);
            assert_eq!(with.expect("approval").await["tool"], "Bash");
            assert_eq!(old.expect("approval").await["tool"], "Bash");
        });
    }
}

mod prefs_tests {
    use super::*;

    async fn phone(rig: &Rig, caps: Option<Value>) -> (Client, Value) {
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        let mut hello = json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "Test phone" });
        if let Some(caps) = caps {
            hello["caps"] = caps;
        }
        c.send(hello).await;
        let welcome = c.expect("welcome").await;
        (c, welcome)
    }

    #[test]
    fn prefs_is_offered_to_a_phone_that_asks_and_the_outfit_follows() {
        block_on(async {
            let rig = Rig::start().await;
            rig.hub.publish_outfit("witchHat");
            let (mut c, welcome) = phone(&rig, Some(json!(["prefs"]))).await;
            assert_eq!(welcome["caps"], json!(["prefs"]));
            assert_eq!(c.expect("sessions").await["type"], "sessions");
            let prefs = c.expect("prefs").await;
            assert_eq!(prefs["outfit"], "witchHat");
            rig.hub.publish_outfit("auto");
            assert_eq!(c.expect("prefs").await["outfit"], "auto");
        });
    }

    #[test]
    fn an_older_phone_is_offered_nothing_and_hears_nothing() {
        block_on(async {
            let rig = Rig::start().await;
            rig.hub.publish_outfit("bow");
            let (mut c, welcome) = phone(&rig, None).await;
            assert!(welcome.get("caps").is_none(), "{welcome}");
            rig.hub.publish_outfit("scarf");
            c.send(json!({ "type": "ping" })).await;
            let mut seen = Vec::new();
            loop {
                let m = c.recv().await.expect("closed");
                let t = m["type"].as_str().unwrap_or("").to_string();
                if t == "pong" {
                    break;
                }
                seen.push(t);
            }
            assert_eq!(seen, vec!["sessions".to_string()]);
        });
    }
}

mod diffs_tests {
    use super::*;
    use crate::phone_link::hub::{DiffIn, FileIn, SessionIn};
    use crate::phone_link::server::Features;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Switch(AtomicBool);
    impl Features for Switch {
        fn details(&self) -> bool {
            false
        }
        fn diffs(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
    }

    async fn phone(rig: &Rig, caps: Option<Value>) -> (Client, Value) {
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        let mut hello = json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "Test phone" });
        if let Some(caps) = caps {
            hello["caps"] = caps;
        }
        c.send(hello).await;
        let welcome = c.expect("welcome").await;
        (c, welcome)
    }

    fn working() -> SessionIn {
        SessionIn {
            pill_id: "integration_claude".into(),
            state: "working".into(),
            files: vec![FileIn { id: 7, name: "/home/me/secret/app.ts".into(), added: 3, removed: 1, ..FileIn::default() }],
            ..SessionIn::default()
        }
    }

    #[test]
    fn diffs_are_offered_only_while_the_switch_is_on_and_the_phone_asked() {
        block_on(async {
            let on = Rig::start_full(None, Arc::new(Switch(AtomicBool::new(true)))).await;
            let (_c, welcome) = phone(&on, Some(json!(["diffs"]))).await;
            assert_eq!(welcome["caps"], json!(["diffs"]));
            let (_c, welcome) = phone(&on, None).await;
            assert!(welcome.get("caps").is_none());
            let off = Rig::start_full(None, Arc::new(Switch(AtomicBool::new(false)))).await;
            let (_c, welcome) = phone(&off, Some(json!(["diffs"]))).await;
            assert!(welcome.get("caps").is_none(), "{welcome}");
        });
    }

    #[test]
    fn a_phone_asks_for_a_file_and_gets_it_and_nobody_else_does() {
        block_on(async {
            let rig = Rig::start_full(None, Arc::new(Switch(AtomicBool::new(true)))).await;
            let (mut c, _) = phone(&rig, Some(json!(["diffs"]))).await;
            let (mut other, _) = phone(&rig, Some(json!(["diffs"]))).await;
            c.expect("sessions").await;
            other.expect("sessions").await;
            rig.hub.publish(vec![working()], None, server::now_ms());
            let list = c.expect("sessions").await;
            assert_eq!(list["sessions"][0]["files"][0]["name"], "app.ts");
            c.send(json!({ "type": "getDiff", "pillId": "integration_claude", "fileId": 7 })).await;
            let conn = loop {
                if let Some(first) = rig.host.2.lock().unwrap().first().cloned() {
                    break first;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            };
            assert_eq!((conn.1.as_str(), conn.2), ("integration_claude", 7));
            rig.hub.send_diff(conn.0, DiffIn { pill_id: conn.1.clone(), file_id: 7, name: "app.ts".into(), added: 3, removed: 1, lines: vec![("+".into(), "let x = 1;".into())], ..DiffIn::default() });
            let diff = c.expect("diff").await;
            assert_eq!(diff["lines"][0], json!(["+", "let x = 1;"]));
            // the other phone hears nothing of it
            other.send(json!({ "type": "ping" })).await;
            assert_eq!(other.recv().await.unwrap()["type"], "sessions");
            assert_eq!(other.recv().await.unwrap()["type"], "pong");
        });
    }

    #[test]
    fn a_malformed_request_or_one_without_the_capability_never_reaches_the_island() {
        block_on(async {
            let rig = Rig::start_full(None, Arc::new(Switch(AtomicBool::new(true)))).await;
            let (mut c, _) = phone(&rig, Some(json!(["diffs"]))).await;
            let (mut plain, _) = phone(&rig, None).await;
            for bad in [
                json!({ "type": "getDiff" }),
                json!({ "type": "getDiff", "pillId": "", "fileId": 1 }),
                json!({ "type": "getDiff", "pillId": "x", "fileId": "1" }),
                json!({ "type": "getDiff", "pillId": "x", "fileId": -1 }),
                json!({ "type": "getDiff", "pillId": "x".repeat(65), "fileId": 1 }),
            ] {
                c.send(bad).await;
            }
            plain.send(json!({ "type": "getDiff", "pillId": "integration_claude", "fileId": 7 })).await;
            c.send(json!({ "type": "ping" })).await;
            loop {
                if c.recv().await.unwrap()["type"] == "pong" {
                    break;
                }
            }
            plain.send(json!({ "type": "ping" })).await;
            loop {
                if plain.recv().await.unwrap()["type"] == "pong" {
                    break;
                }
            }
            assert!(rig.host.2.lock().unwrap().is_empty());
        });
    }
}

mod usage_tests {
    use super::*;
    use crate::phone_link::hub::{PlanIn, UsageIn, WindowIn};
    use crate::phone_link::server::Features;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Switch(AtomicBool);
    impl Features for Switch {
        fn details(&self) -> bool {
            false
        }
        fn usage(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
    }

    async fn phone(rig: &Rig, caps: Option<Value>) -> (Client, Value) {
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        let mut hello = json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "Test phone" });
        if let Some(caps) = caps {
            hello["caps"] = caps;
        }
        c.send(hello).await;
        let welcome = c.expect("welcome").await;
        (c, welcome)
    }

    fn usage() -> UsageIn {
        UsageIn { claude: Some(PlanIn { five_hour: Some(WindowIn { used_pct: 61.0, resets_at: 1_900_000_000_000 }), ..PlanIn::default() }), codex: None }
    }

    #[test]
    fn usage_is_offered_only_while_the_switch_is_on_and_the_phone_asked() {
        block_on(async {
            let on = Rig::start_full(None, Arc::new(Switch(AtomicBool::new(true)))).await;
            let (_c, welcome) = phone(&on, Some(json!(["usage"]))).await;
            assert_eq!(welcome["caps"], json!(["usage"]));
            let (_c, welcome) = phone(&on, None).await;
            assert!(welcome.get("caps").is_none());
            let off = Rig::start_full(None, Arc::new(Switch(AtomicBool::new(false)))).await;
            let (_c, welcome) = phone(&off, Some(json!(["usage"]))).await;
            assert!(welcome.get("caps").is_none(), "{welcome}");
        });
    }

    #[test]
    fn the_phone_hears_the_usage_and_its_changes() {
        block_on(async {
            let rig = Rig::start_full(None, Arc::new(Switch(AtomicBool::new(true)))).await;
            rig.hub.publish_usage(usage());
            let (mut c, _) = phone(&rig, Some(json!(["usage"]))).await;
            c.expect("sessions").await;
            assert_eq!(c.expect("usage").await["claude"]["fiveHour"]["pct"], 61);
            rig.hub.publish_usage(UsageIn { claude: Some(PlanIn { five_hour: Some(WindowIn { used_pct: 80.0, resets_at: 1_900_000_000_000 }), ..PlanIn::default() }), codex: None });
            assert_eq!(c.expect("usage").await["claude"]["fiveHour"]["pct"], 80);
        });
    }
}

mod services_tests {
    use super::*;
    use crate::phone_link::hub::{ServiceIn, ServiceItemIn};
    use crate::phone_link::server::Features;
    use std::sync::Mutex as StdMutex;

    struct Ticks(StdMutex<Vec<String>>);
    impl Features for Ticks {
        fn details(&self) -> bool {
            false
        }
        fn services(&self) -> Vec<String> {
            self.0.lock().unwrap().clone()
        }
    }

    async fn phone(rig: &Rig, caps: Option<Value>) -> (Client, Value) {
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        let mut hello = json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "Test phone" });
        if let Some(caps) = caps {
            hello["caps"] = caps;
        }
        c.send(hello).await;
        let welcome = c.expect("welcome").await;
        (c, welcome)
    }

    fn cards() -> Vec<ServiceIn> {
        vec![
            ServiceIn { id: "integration_stripe".into(), title: "Stripe".into(), headline: "12.50 EUR".into(), reason: None, items: vec![ServiceItemIn { label: "Payment".into(), detail: "+9.00".into() }] },
            ServiceIn { id: "integration_notion".into(), title: "Notion".into(), headline: "2".into(), reason: None, items: vec![ServiceItemIn { label: "Private page".into(), detail: "1m".into() }] },
        ]
    }

    #[test]
    fn services_are_offered_only_when_the_phone_asked_and_something_is_ticked() {
        block_on(async {
            let ticked = Rig::start_full(None, Arc::new(Ticks(StdMutex::new(vec!["integration_stripe".into()])))).await;
            let (_c, welcome) = phone(&ticked, Some(json!(["services"]))).await;
            assert_eq!(welcome["caps"], json!(["services"]));
            let (_c, welcome) = phone(&ticked, None).await;
            assert!(welcome.get("caps").is_none());
            let none = Rig::start_full(None, Arc::new(Ticks(StdMutex::new(vec![])))).await;
            let (_c, welcome) = phone(&none, Some(json!(["services"]))).await;
            assert!(welcome.get("caps").is_none(), "{welcome}");
        });
    }

    #[test]
    fn the_phone_gets_the_ticked_services_only() {
        block_on(async {
            let rig = Rig::start_full(None, Arc::new(Ticks(StdMutex::new(vec!["integration_stripe".into()])))).await;
            rig.hub.publish_services(cards());
            let (mut c, _) = phone(&rig, Some(json!(["services"]))).await;
            c.expect("sessions").await;
            let m = c.expect("services").await;
            assert_eq!(m["services"].as_array().unwrap().len(), 1);
            assert_eq!(m["services"][0]["id"], "integration_stripe");
            assert!(!m.to_string().contains("Private page"));
        });
    }
}

mod relay_cap_tests {
    use super::*;
    use crate::phone_link::server::Features;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Switch(AtomicBool);
    impl Features for Switch {
        fn details(&self) -> bool {
            false
        }
        fn relay(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
    }

    async fn welcome_for(rig: &Rig, caps: Option<Value>) -> (Client, Value) {
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        let mut hello = json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "Test phone" });
        if let Some(caps) = caps {
            hello["caps"] = caps;
        }
        c.send(hello).await;
        let welcome = c.expect("welcome").await;
        (c, welcome)
    }

    #[test]
    fn relay_is_offered_only_while_the_switch_is_on_and_the_phone_asked() {
        block_on(async {
            let on = Rig::start_full(None, Arc::new(Switch(AtomicBool::new(true)))).await;
            let (_c, welcome) = welcome_for(&on, Some(json!(["relay"]))).await;
            assert_eq!(welcome["caps"], json!(["relay"]));
            let (_c, welcome) = welcome_for(&on, None).await;
            assert!(welcome.get("caps").is_none(), "an older phone is never offered it");
            let off = Rig::start_full(None, Arc::new(Switch(AtomicBool::new(false)))).await;
            let (_c, welcome) = welcome_for(&off, Some(json!(["relay"]))).await;
            assert!(welcome.get("caps").is_none(), "{welcome}");
        });
    }

    #[test]
    fn a_rotated_access_key_reaches_a_phone_that_negotiated_relay() {
        block_on(async {
            let rig = Rig::start_full(None, Arc::new(Switch(AtomicBool::new(true)))).await;
            let (mut c, _) = welcome_for(&rig, Some(json!(["relay"]))).await;
            let (mut other, _) = welcome_for(&rig, None).await;
            let access = "D".repeat(43);
            assert_eq!(rig.hub.send_relay_access(&access), 1);
            let m = c.expect("relayAccess").await;
            assert_eq!(m["access"], access);
            other.send(json!({ "type": "ping" })).await;
            other.expect("pong").await; // it only ever sees ordinary traffic
        });
    }
}

// ── The keystore's key list (secrets::INTERNAL_KEYS) ──

/// Every key the phone link stores must be in the real keystore's list: the keystore refuses the others with "unknown key".
/// MemStore accepts anything, which is how a missing key once went unnoticed.
#[test]
fn every_secret_the_phone_link_stores_is_in_the_keystores_key_list() {
    for key in [
        pairing::CERT_KEY,
        pairing::PRIVATE_KEY,
        pairing::TOKEN_KEY,
        super::relay_client::ROOM_KEY,
        super::relay_client::PAIRING_KEY,
        super::relay_client::ACCESS_KEY,
        super::push::TOPIC_KEY,
        super::push::TOKEN_KEY,
    ] {
        assert!(crate::secrets::INTERNAL_KEYS.contains(&key), "{key} is not in secrets::INTERNAL_KEYS");
    }
    // And nothing in the list is a stray: each one is used by the phone link, by Mochi's home, or for Hermes.
    let others = [crate::smart_home::config::HA_TOKEN_KEY, crate::smart_home::config::PHILIPS_KEY, crate::hermes::KEY];
    assert!(others.iter().all(|k| crate::secrets::INTERNAL_KEYS.contains(k)));
    assert_eq!(crate::secrets::INTERNAL_KEYS.len(), 8 + others.len());
}

#[test]
fn the_whole_pairing_life_works_through_the_real_key_list() {
    use pairing::tests::StrictStore;
    use super::relay_client as relay;
    let store = StrictStore::default();
    pairing::load_or_create_identity(&store).unwrap();
    let t1 = pairing::token(&store).unwrap();
    let (room1, _) = relay::pairing(&store).unwrap();
    relay::set_access_key(&store, &super::relay_crypto::base64url_encode(&[7u8; 32])).unwrap();
    assert!(relay::access_key(&store).is_some());
    // "Pair again"
    let hub = Hub::new(Arc::new(Recorder::default()));
    let token = Mutex::new(t1.clone());
    super::renew_pairing(&hub, &token, &store).unwrap();
    assert_ne!(*token.lock().unwrap(), t1);
    assert_ne!(relay::pairing(&store).unwrap().0, room1);
    // Clearing the access key goes through too.
    relay::set_access_key(&store, "").unwrap();
}

#[test]
fn the_hub_says_which_request_still_waits() {
    let hub = Hub::new(Arc::new(Recorder::default()));
    let now = server::now_ms();
    let approval = |id: &str| ApprovalIn {
        request_id: id.into(),
        session_id: "s1".into(),
        pill_id: "integration_claude".into(),
        tool: "Bash".into(),
        command: "ls".into(),
    };
    assert!(!hub.waiting("r1"));
    hub.publish(vec![], Some(approval("r1")), now);
    assert!(hub.waiting("r1"));
    hub.publish_agent(None, Some(approval("agent-7")), now);
    assert!(hub.waiting("agent-7") && hub.waiting("r1"));
    // Answered at the desk: the island publishes no request any more.
    hub.publish(vec![], None, now);
    assert!(!hub.waiting("r1"));
    hub.publish_agent(None, None, now);
    assert!(!hub.waiting("agent-7"));
    assert!(!hub.waiting(""), "an empty id never waits");
}

#[test]
fn a_key_outside_the_list_is_refused_by_the_strict_store() {
    use pairing::tests::StrictStore;
    use pairing::SecretStore;
    assert!(StrictStore::default().set("phone-something-new", "x").is_err());
}

#[test]
fn pairing_again_disconnects_the_phones_even_when_the_keystore_refuses_to_save() {
    use pairing::tests::StrictStore;
    use std::sync::atomic::Ordering;
    block_on(async {
        let rig = Rig::start().await;
        let mut phone = Client::paired(&rig).await;
        let store = StrictStore::default();
        store.refuse_writes.store(true, Ordering::SeqCst);
        let token = Mutex::new(TOKEN.to_string());
        assert!(super::renew_pairing(&rig.hub, &token, &store).is_err());
        // The error is reported, and the phone was cut off all the same.
        assert_eq!(phone.expect("error").await["code"], "auth");
        assert!(phone.closed().await);
    });
}

#[test]
fn over_the_wire_a_10000_character_command_is_flagged_and_allow_does_nothing() {
    block_on(async {
        let rig = Rig::start().await;
        let mut c = Client::paired(&rig).await;
        c.expect("sessions").await;
        let command = format!("echo {}", "x".repeat(10_000));
        rig.approval("req-long", &command, 0);
        let approval = c.expect("approval").await;
        assert_eq!(approval["truncated"], true);
        assert_eq!(approval["command"].as_str().unwrap().chars().count(), 500);
        let fp = approval["fingerprint"].as_str().unwrap().to_string();

        c.send(json!({ "type": "decision", "fingerprint": fp, "decision": "allow" })).await;
        c.send(json!({ "type": "ping" })).await;
        c.expect("pong").await; // the decision before it was read, and ignored
        assert!(rig.decisions().is_empty(), "Allow from the phone must not reach the agent");

        // Deny still works.
        c.send(json!({ "type": "decision", "fingerprint": fp, "decision": "deny" })).await;
        c.expect("approvalResolved").await;
        assert_eq!(rig.decisions(), vec![("req-long".to_string(), false)]);
    });
}

// ── Which window may call which command ──

#[test]
fn only_the_right_window_may_call_a_command_that_changes_something() {
    use super::{caller_allowed, Caller};
    assert!(caller_allowed("settings", Caller::Settings));
    assert!(caller_allowed("island", Caller::Island));
    for other in ["island", "mochi", "", "Settings", "settings ", "main", "settings2"] {
        assert!(!caller_allowed(other, Caller::Settings), "{other:?} must not act as the settings window");
    }
    for other in ["settings", "mochi", "", "Island", "island ", "main"] {
        assert!(!caller_allowed(other, Caller::Island), "{other:?} must not act as the island");
    }
}

/// Reads mod.rs: every `#[tauri::command]` that is not a plain read of a status must check the calling window. A command added later
/// without the check fails here, which is how phone_link_set_enabled (it starts a network server) was found.
#[test]
fn every_command_that_changes_something_checks_the_calling_window() {
    // The Windows runner checks files out with CRLF line endings.
    let source = include_str!("mod.rs").replace("\r\n", "\n");
    let mut seen = 0;
    for chunk in source.split("#[tauri::command]\n").skip(1) {
        let name = chunk.split("pub fn ").nth(1).and_then(|r| r.split('(').next()).expect("a command has a name");
        let end = chunk.find("\n}\n").expect("a command ends");
        let body = &chunk[..end];
        seen += 1;
        if name.ends_with("_status") {
            // Reads only. None of them returns a secret: the relay's access key is reported as a yes/no.
            assert!(!body.contains("settings::save") && !body.contains(".start()") && !body.contains(".stop()"), "{name} changes something");
            continue;
        }
        assert!(
            body.contains("window: WebviewWindow") && (body.contains("only_settings(&window)") || body.contains("only_island(&window)")),
            "{name} changes something but does not check the calling window"
        );
    }
    assert!(seen >= 25, "the test must see the commands (saw {seen})");
}

// ── Mochi's hands (agent/): his own session and action on the phone, and the chat with hands ──────────────

mod agent_tests {
    use super::*;
    use crate::phone_link::chat::{BoxFuture, ChatBackend, ChatConfig, ChatFail, ChatLink, ModelOption, StepLine};
    use crate::phone_link::hub::{Out, APPROVAL_TTL_MS};
    use tokio::sync::mpsc;
    use tokio::sync::mpsc::UnboundedSender;

    fn mochi(state: &str) -> SessionIn {
        SessionIn {
            pill_id: "agent_mochi".into(),
            agent: "Mochi".into(),
            state: state.into(),
            status_text: "Bash · npm test".into(),
            step_index: 0,
            step_count: 1,
            steps: vec!["Bash · npm test".into()],
            color: Some("#A5B4FC".into()),
            ..Default::default()
        }
    }

    fn mochi_action(id: &str) -> ApprovalIn {
        ApprovalIn { request_id: id.into(), session_id: "mochi-1".into(), pill_id: "agent_mochi".into(), tool: "Bash".into(), command: "npm test".into() }
    }

    fn claude_action(id: &str) -> ApprovalIn {
        ApprovalIn { request_id: id.into(), session_id: "s1".into(), pill_id: "integration_claude".into(), tool: "Bash".into(), command: "rm -rf build".into() }
    }

    fn lines(rx: &mut mpsc::Receiver<Out>) -> Vec<Value> {
        let mut v = Vec::new();
        while let Ok(o) = rx.try_recv() {
            if let Out::Line(l) = o {
                v.push(serde_json::from_str(&l).unwrap());
            }
        }
        v
    }

    fn recorder_hub() -> (Arc<Hub>, Arc<Recorder>) {
        let host = Arc::new(Recorder::default());
        (Hub::new(host.clone()), host)
    }

    #[test]
    fn mochis_session_and_action_sit_next_to_the_islands_and_never_replace_them() {
        let (hub, host) = recorder_hub();
        hub.publish(vec![SessionIn { pill_id: "integration_claude".into(), agent: "Claude Code".into(), state: "working".into(), ..Default::default() }], Some(claude_action("r1")), 1_000);
        hub.publish_agent(Some(mochi("approval")), Some(mochi_action("agent-7")), 1_000);

        // A phone that connects now gets both sessions and both requests.
        let (tx, mut rx) = mpsc::channel(32);
        hub.subscribe(tx, 1_001, false);
        let got = lines(&mut rx);
        let pills: Vec<&str> = got[0]["sessions"].as_array().unwrap().iter().map(|s| s["pillId"].as_str().unwrap()).collect();
        assert_eq!(pills, ["integration_claude", "agent_mochi"]);
        let approvals: Vec<&Value> = got.iter().filter(|l| l["type"] == "approval").collect();
        assert_eq!(approvals.len(), 2);
        assert_eq!(approvals[1]["pillId"], "agent_mochi");
        assert_eq!(approvals[1]["command"], "npm test");
        let mochi_fp = approvals[1]["fingerprint"].as_str().unwrap().to_string();
        let claude_fp = approvals[0]["fingerprint"].as_str().unwrap().to_string();

        // Allowing Mochi's action answers Mochi, and leaves Claude Code's request waiting.
        assert_eq!(hub.decide(&mochi_fp, "allow", 2_000), super::super::hub::Outcome::Applied);
        assert_eq!(*host.0.lock().unwrap(), vec![("agent-7".to_string(), true)]);
        let got = lines(&mut rx);
        assert!(got.iter().any(|l| l["type"] == "approvalResolved" && l["fingerprint"] == mochi_fp.as_str()));
        assert_eq!(hub.decide(&mochi_fp, "allow", 2_001), super::super::hub::Outcome::NoMatch, "answered once");
        assert_eq!(hub.decide(&claude_fp, "deny", 2_002), super::super::hub::Outcome::Applied);
        assert_eq!(host.0.lock().unwrap()[1], ("r1".to_string(), false));

        // The island publishing again (no request any more) does not take Mochi's session away.
        hub.publish(vec![], None, 3_000);
        let got = lines(&mut rx);
        let last_sessions = got.iter().rev().find(|l| l["type"] == "sessions").unwrap();
        assert_eq!(last_sessions["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(last_sessions["sessions"][0]["pillId"], "agent_mochi");
        // And Mochi going away leaves the island's alone.
        hub.publish_agent(None, None, 4_000);
        let got = lines(&mut rx);
        assert_eq!(got.last().unwrap()["sessions"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn mochis_action_expires_for_the_phone_like_any_request() {
        let (hub, host) = recorder_hub();
        hub.publish_agent(Some(mochi("approval")), Some(mochi_action("agent-3")), 1_000);
        let (tx, mut rx) = mpsc::channel(32);
        hub.subscribe(tx, 1_001, false);
        let fp = lines(&mut rx).iter().find(|l| l["type"] == "approval").unwrap()["fingerprint"].as_str().unwrap().to_string();
        assert_eq!(hub.decide(&fp, "allow", 1_000 + APPROVAL_TTL_MS + 1), super::super::hub::Outcome::Late);
        assert!(host.0.lock().unwrap().is_empty(), "a late answer changes nothing");
        // A new action replaces the old one: the old one is resolved for the phones.
        hub.publish_agent(Some(mochi("approval")), Some(mochi_action("agent-4")), 5_000);
        hub.publish_agent(Some(mochi("approval")), Some(mochi_action("agent-5")), 6_000);
        let got = lines(&mut rx);
        assert!(got.iter().any(|l| l["type"] == "approvalResolved"));
        assert_eq!(got.iter().filter(|l| l["type"] == "approval").count(), 2);
    }

    /// A provider that "acts": two steps, then an answer.
    struct Hands {
        agent_calls: Mutex<Vec<(String, String)>>,
    }

    impl ChatBackend for Hands {
        fn config(&self) -> ChatConfig {
            ChatConfig { enabled: true, allowed: vec![ModelOption { id: "openrouter/kimi".into(), provider: "openrouter".into(), label: "Kimi".into() }] }
        }
        fn send(&self, _model_id: &str, _text: String, _deltas: UnboundedSender<String>) -> BoxFuture<Result<String, ChatFail>> {
            Box::pin(async { Ok("plain chat".to_string()) })
        }
        fn reset(&self) {}
        fn send_agent(&self, model_id: &str, text: String, steps: UnboundedSender<StepLine>) -> BoxFuture<Result<String, ChatFail>> {
            self.agent_calls.lock().unwrap().push((model_id.to_string(), text));
            Box::pin(async move {
                let _ = steps.send(StepLine { index: 0, status: "running".into(), text: "LS · C:\\work".into() });
                let _ = steps.send(StepLine { index: 0, status: "ok".into(), text: "LS · C:\\work".into() });
                let _ = steps.send(StepLine { index: 1, status: "waiting".into(), text: "Bash · npm test".into() });
                Ok("Tests pass.".to_string())
            })
        }
    }

    struct AgentFeatures(bool);
    impl server::Features for AgentFeatures {
        fn details(&self) -> bool {
            false
        }
        fn agent(&self) -> bool {
            self.0
        }
    }

    async fn phone(rig: &Rig, caps: Value) -> (Client, Value) {
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        c.send(json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "Test phone", "caps": caps })).await;
        let welcome = c.expect("welcome").await;
        (c, welcome)
    }

    #[test]
    fn a_phone_with_hands_sees_each_step_and_then_the_answer() {
        block_on(async {
            let backend = Arc::new(Hands { agent_calls: Mutex::new(Vec::new()) });
            let rig = Rig::start_full(Some(ChatLink::new(backend.clone())), Arc::new(AgentFeatures(true))).await;
            let (mut c, welcome) = phone(&rig, json!(["chat", "agent"])).await;
            assert_eq!(welcome["caps"], json!(["chat", "agent"]));
            c.send(json!({ "type": "chatSend", "id": "h1", "model": "openrouter/kimi", "text": "run the tests", "agent": true })).await;
            let mut steps = Vec::new();
            let mut answer = String::new();
            loop {
                let m = c.recv().await.expect("an answer");
                match m["type"].as_str().unwrap() {
                    "chatStep" => {
                        assert_eq!(m["id"], "h1");
                        steps.push((m["index"].as_u64().unwrap(), m["status"].as_str().unwrap().to_string(), m["text"].as_str().unwrap().to_string()));
                    }
                    "chatDelta" => answer.push_str(m["text"].as_str().unwrap()),
                    "chatDone" => break,
                    "chatError" => panic!("{m}"),
                    _ => {}
                }
            }
            assert_eq!(answer, "Tests pass.");
            assert_eq!(steps.len(), 3);
            assert_eq!(steps[2], (1, "waiting".to_string(), "Bash · npm test".to_string()));
            assert_eq!(*backend.agent_calls.lock().unwrap(), vec![("openrouter/kimi".to_string(), "run the tests".to_string())]);
        });
    }

    #[test]
    fn hands_are_neither_offered_nor_used_without_both_switches() {
        block_on(async {
            // The computer does not offer them: a phone asking anyway is refused, and the plain chat still works.
            let backend = Arc::new(Hands { agent_calls: Mutex::new(Vec::new()) });
            let rig = Rig::start_full(Some(ChatLink::new(backend.clone())), Arc::new(AgentFeatures(false))).await;
            let (mut c, welcome) = phone(&rig, json!(["chat", "agent"])).await;
            assert_eq!(welcome["caps"], json!(["chat"]));
            c.send(json!({ "type": "chatSend", "id": "h2", "model": "openrouter/kimi", "text": "delete it all", "agent": true })).await;
            let m = c.expect("chatError").await;
            assert_eq!(m["reason"], "hands_off");
            assert!(backend.agent_calls.lock().unwrap().is_empty());
            c.send(json!({ "type": "chatSend", "id": "h3", "model": "openrouter/kimi", "text": "hi" })).await;
            loop {
                let m = c.recv().await.unwrap();
                if m["type"] == "chatDone" {
                    break;
                }
                assert_ne!(m["type"], "chatStep");
            }

            // Hands without the chat are never offered (they ride on it).
            let rig = Rig::start_full(None, Arc::new(AgentFeatures(true))).await;
            let (_c, welcome) = phone(&rig, json!(["chat", "agent"])).await;
            assert!(welcome.get("caps").is_none(), "{welcome}");
        });
    }
}

mod remote_tests {
    use super::*;

    /// The user's switch for "away from home, directly".
    struct Away(bool);
    impl server::Features for Away {
        fn details(&self) -> bool {
            false
        }
        fn remote(&self) -> bool {
            self.0
        }
    }

    async fn phone(rig: &Rig, caps: Option<Value>) -> (Client, Value) {
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        let mut hello = json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "Test phone" });
        if let Some(caps) = caps {
            hello["caps"] = caps;
        }
        c.send(hello).await;
        let welcome = c.expect("welcome").await;
        (c, welcome)
    }

    #[test]
    fn the_internet_gets_in_only_while_the_switch_is_on_and_never_in_the_homes_places() {
        let shared = |on: bool| server::Shared::with_features(Hub::new(Arc::new(Recorder::default())), TOKEN.into(), "PC".into(), None, Arc::new(Away(on)));
        let off = shared(false);
        assert!(server::ticket_for(&off, "203.0.113.5".parse().unwrap()).is_none(), "switched off: the internet is dropped");
        assert!(server::ticket_for(&off, "192.168.1.5".parse().unwrap()).is_some_and(|t| !t.outside()));
        let on = shared(true);
        let outside: Vec<_> = (0..5).filter_map(|n| server::ticket_for(&on, format!("203.0.113.{n}").parse().unwrap())).collect();
        assert_eq!(outside.len(), 2, "the internet's own small pool");
        assert!(outside.iter().all(|t| t.outside()));
        assert!(server::ticket_for(&on, "2804:14c::9".parse().unwrap()).is_none(), "IPv6 from the internet shares that pool");
        assert!(server::ticket_for(&on, "fe80::1".parse().unwrap()).is_some(), "the local network keeps its places");
    }

    #[test]
    fn a_phone_that_understands_it_is_told_where_to_try_from_outside() {
        block_on(async {
            let rig = Rig::start().await;
            rig.hub.set_remote(Some(vec!["[2804:14c:1:2::10]:47821".into(), "198.51.100.7:47821".into()]));
            let (mut c, welcome) = phone(&rig, Some(json!(["remote"]))).await;
            assert_eq!(welcome["caps"], json!(["remote"]));
            assert_eq!(c.expect("remote").await["endpoints"], json!(["[2804:14c:1:2::10]:47821", "198.51.100.7:47821"]));
            // Switched off: told there is nowhere, so it forgets.
            rig.hub.set_remote(Some(Vec::new()));
            assert_eq!(c.expect("remote").await["endpoints"], json!([]));
            // An older phone is not offered it and never hears of it.
            let (mut old, welcome) = phone(&rig, None).await;
            assert!(welcome.get("caps").is_none(), "{welcome}");
            rig.hub.set_remote(Some(vec!["198.51.100.8:47821".into()]));
            old.send(json!({ "type": "ping" })).await;
            loop {
                let m = old.recv().await.expect("closed");
                assert_ne!(m["type"], "remote");
                if m["type"] == "pong" {
                    break;
                }
            }
        });
    }
}

// ── The assistant from the phone (assistant.rs) ──────────────────────────────

mod assistant_tests {
    use super::*;
    use crate::hermes_setup::{self, Assistant, BrainOption, GatewayState, LoginState, Progress, SetupStatus};
    use crate::phone_link::assistant::{self, AssistantBackend, AssistantLink};
    use crate::phone_link::server::{Features, Links};
    use std::sync::atomic::{AtomicBool, Ordering};

    /// The user's switches: the assistant from the phone, and Mochi's home from the phone (with Mochi's home itself).
    pub struct Switches {
        pub assistant: AtomicBool,
        pub home: AtomicBool,
    }

    impl Switches {
        pub fn new(assistant: bool, home: bool) -> Arc<Switches> {
            Arc::new(Switches { assistant: AtomicBool::new(assistant), home: AtomicBool::new(home) })
        }
    }

    impl Features for Switches {
        fn details(&self) -> bool {
            false
        }
        fn assistant(&self) -> bool {
            self.assistant.load(Ordering::SeqCst)
        }
        fn home(&self) -> bool {
            self.home.load(Ordering::SeqCst)
        }
    }

    /// Zheus as Settings would show it, and what the phone had done to it.
    pub struct Fake {
        pub status: Mutex<SetupStatus>,
        calls: Mutex<Vec<String>>,
    }

    impl Fake {
        pub fn new(status: SetupStatus) -> Arc<Fake> {
            Arc::new(Fake { status: Mutex::new(status), calls: Mutex::new(Vec::new()) })
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
        fn did(&self, what: String) {
            self.calls.lock().unwrap().push(what);
        }
    }

    impl AssistantBackend for Fake {
        fn status(&self) -> SetupStatus {
            self.status.lock().unwrap().clone()
        }
        fn set(&self, name: Option<String>, persona: Option<String>, persona_text: Option<String>) -> Result<(), String> {
            self.did(format!("set {name:?} {persona:?} {persona_text:?}"));
            Ok(())
        }
        fn brain(&self, brain: &str) -> Result<(), String> {
            self.did(format!("brain {brain}"));
            Ok(())
        }
        fn skills(&self, skills: Vec<String>) -> Result<(), String> {
            self.did(format!("skills {skills:?}"));
            Ok(())
        }
        fn login(&self) -> Result<(), String> {
            self.did("login".into());
            Ok(())
        }
        fn login_cancel(&self) {
            self.did("login cancel".into());
        }
        fn gateway(&self, on: bool) {
            self.did(format!("gateway {on}"));
        }
    }

    /// Zheus installed and run by Coucou on ChatGPT, with what a line must never carry in its paths and addresses.
    pub fn zheus() -> SetupStatus {
        SetupStatus {
            installed: true,
            managed: true,
            home: r"C:\Users\Lea\AppData\Local\hermes".into(),
            brains: vec![
                BrainOption { id: "chatgpt".into(), name: "ChatGPT".into(), ready: true, model: "gpt-5.5".into(), login: true },
                BrainOption { id: "openai".into(), name: "OpenAI".into(), ready: false, model: String::new(), login: false },
            ],
            brain: "chatgpt".into(),
            progress: Progress::default(),
            gateway: GatewayState::Running,
            address: "http://127.0.0.1:8642".into(),
            commit: "6e7b83a2f1".into(),
            installer: "https://raw.githubusercontent.com/NousResearch/hermes-agent/6e7b83a2f1/scripts/install.ps1".into(),
            assistant: Assistant { name: "Zheus".into(), persona: "friendly".into(), persona_text: String::new() },
            personas: hermes_setup::PERSONAS.iter().map(|p| p.to_string()).collect(),
            skill_packs: hermes_setup::SKILL_PACKS.iter().map(|p| p.to_string()).collect(),
            skills: vec!["search".into()],
            house: true,
            login: LoginState::SignedIn,
        }
    }

    pub fn links(fake: &Arc<Fake>) -> Links {
        Links { assistant: Some(AssistantLink::new(fake.clone())), home: None }
    }

    pub async fn phone(rig: &Rig, caps: Option<Value>) -> (Client, Value) {
        let mut c = Client::connect(rig.port, &rig.fingerprint).await.unwrap();
        let mut hello = json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "Test phone" });
        if let Some(caps) = caps {
            hello["caps"] = caps;
        }
        c.send(hello).await;
        let welcome = c.expect("welcome").await;
        (c, welcome)
    }

    /// The answer to the request `id` (the pictures that come meanwhile are skipped). Requests are carried out in order,
    /// so an answer to anything sent before would come first.
    async fn result(c: &mut Client, id: &str) -> Value {
        let m = c.expect("assistantResult").await;
        assert_eq!(m["id"], id, "{m}");
        m
    }

    fn refused(m: &Value) -> (Option<bool>, Option<&str>) {
        (m["ok"].as_bool(), m["reason"].as_str())
    }

    #[test]
    fn the_assistant_is_offered_only_to_a_phone_that_asks_while_the_switch_is_on() {
        block_on(async {
            let fake = Fake::new(zheus());
            let rig = Rig::start_links(None, Switches::new(true, true), links(&fake)).await;
            let (_c, welcome) = phone(&rig, Some(json!(["assistant", "home"]))).await;
            assert_eq!(welcome["caps"], json!(["assistant"]), "Mochi's home is not wired in here");
            let (_c, welcome) = phone(&rig, Some(json!(["chat", "details"]))).await;
            assert!(welcome.get("caps").is_none(), "{welcome}");
            let (_c, welcome) = phone(&rig, None).await;
            assert!(welcome.get("caps").is_none(), "an older phone is never offered it");
            let off = Rig::start_links(None, Switches::new(false, false), links(&fake)).await;
            let (_c, welcome) = phone(&off, Some(json!(["assistant"]))).await;
            assert!(welcome.get("caps").is_none(), "{welcome}");
            let unwired = Rig::start_full(None, Switches::new(true, true)).await;
            let (_c, welcome) = phone(&unwired, Some(json!(["assistant", "home"]))).await;
            assert!(welcome.get("caps").is_none(), "where the app wired nothing in: {welcome}");
        });
    }

    #[test]
    fn the_first_picture_follows_the_sessions_in_the_contracts_shape_and_tells_nothing_it_must_not() {
        block_on(async {
            let mut status = zheus();
            status.gateway = GatewayState::Stopped { reason: r"config.yaml unreadable in C:\Users\Lea\AppData\Local\hermes (http://127.0.0.1:8642)".into() };
            status.progress = Progress {
                running: true,
                step: "Install Python dependencies".into(),
                done: 2,
                total: 6,
                last: "pip install --index-url https://pypi.org/simple".into(),
                error: Some("token sk-ant-api03-AbCdEfGhIjKlMnOp1234567890 refused by https://api.anthropic.com".into()),
                finished: false,
            };
            status.login = LoginState::Failed { reason: "Login failed: lea.martin@example.com has no Plus plan".into() };
            let fake = Fake::new(status);
            let rig = Rig::start_links(None, Switches::new(true, false), links(&fake)).await;
            let (mut c, _) = phone(&rig, Some(json!(["assistant"]))).await;
            assert_eq!(c.recv().await.unwrap()["type"], "sessions", "the sessions come first");
            let m = c.expect("assistant").await;
            let text = m.to_string();
            for secret in ["http", "127.0.0.1", "8642", "Lea", "AppData", "example.com", "@", "sk-ant", "pypi", "githubusercontent", "gpt-5.5", "6e7b83a2"] {
                assert!(!text.contains(secret), "{secret} in {text}");
            }
            let mut keys: Vec<&str> = m.as_object().unwrap().keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(keys, ["brain", "brains", "hermes", "house", "login", "name", "persona", "personaText", "personas", "skillPacks", "skills", "type"]);
            assert_eq!((m["name"].as_str(), m["persona"].as_str(), m["personaText"].as_str(), m["brain"].as_str()), (Some("Zheus"), Some("friendly"), Some(""), Some("chatgpt")));
            assert_eq!(m["personas"], json!(["friendly", "direct", "professional", "playful", "butler", "custom"]));
            assert_eq!(m["brains"], json!([{ "id": "chatgpt", "name": "ChatGPT", "ready": true, "login": true }, { "id": "openai", "name": "OpenAI", "ready": false, "login": false }]));
            assert_eq!((m["skills"].clone(), m["skillPacks"].clone(), m["house"].clone()), (json!(["search"]), json!(["home", "search", "news"]), json!(true)));
            assert_eq!(m["hermes"]["installed"], true);
            assert_eq!(m["hermes"]["managed"], true);
            assert_eq!(m["hermes"]["gateway"], "stopped");
            assert!(m["hermes"]["reason"].as_str().unwrap().starts_with("config.yaml unreadable in •••"), "{m}");
            assert_eq!(m["hermes"]["setup"], json!({ "running": true, "step": "Install Python dependencies", "done": 2, "total": 6, "error": "token ••• refused by •••" }));
            assert_eq!(m["login"], json!({ "state": "failed", "reason": "Login failed: ••• has no Plus plan" }));

            // A code to type: shown, with OpenAI's page only, whatever was printed.
            fake.status.lock().unwrap().login = LoginState::Waiting { code: "ABCD-1234".into(), url: "https://auth.openai.com.example.net/device".into(), expires_at: 1_900_000_000_000 };
            fake.status.lock().unwrap().gateway = GatewayState::Running;
            c.send(json!({ "type": "assistantGet" })).await;
            let m = c.expect("assistant").await;
            assert_eq!(m["login"], json!({ "state": "waiting", "code": "ABCD-1234", "url": "https://auth.openai.com/codex/device", "expiresAt": 1_900_000_000_000u64 }));
            assert_eq!((m["hermes"]["gateway"].as_str(), m["hermes"]["reason"].as_str()), (Some("running"), Some("")));
        });
    }

    #[test]
    fn each_request_is_checked_carried_out_and_answered_once_by_its_id() {
        block_on(async {
            let fake = Fake::new(zheus());
            let rig = Rig::start_links(None, Switches::new(true, false), links(&fake)).await;
            let (mut c, _) = phone(&rig, Some(json!(["assistant"]))).await;
            c.expect("assistant").await;
            c.send(json!({ "type": "assistantSet", "id": "a1", "name": " Athena ", "persona": "butler" })).await;
            assert_eq!(result(&mut c, "a1").await, json!({ "type": "assistantResult", "id": "a1", "ok": true }));
            for (id, mut bad) in [
                ("a2", json!({ "type": "assistantSet", "name": "two\nlines" })),
                ("a3", json!({ "type": "assistantSet", "name": "x".repeat(41) })),
                ("a4", json!({ "type": "assistantSet", "persona": "pirate" })),
                ("a5", json!({ "type": "assistantSet", "persona": "custom", "personaText": "é".repeat(1501) })),
                ("a6", json!({ "type": "assistantSet" })),
                ("a7", json!({ "type": "assistantSkills", "skills": ["home"] })),
                ("a8", json!({ "type": "assistantSkills", "skills": ["search", "weather"] })),
                ("a9", json!({ "type": "assistantBrain", "brain": "skynet" })),
                ("a10", json!({ "type": "assistantGateway", "on": "yes" })),
            ] {
                bad["id"] = json!(id);
                c.send(bad).await;
                let m = result(&mut c, id).await;
                assert_eq!(refused(&m), (Some(false), Some("invalid")), "{m}");
                assert_eq!(m["message"], assistant::reason("invalid"));
            }
            c.send(json!({ "type": "assistantBrain", "id": "b1", "brain": "openai" })).await;
            assert_eq!(refused(&result(&mut c, "b1").await), (Some(false), Some("not_ready")), "no key for it on the computer");
            for (id, mut good) in [
                ("c1", json!({ "type": "assistantSkills", "skills": ["search", "news"] })),
                ("c2", json!({ "type": "assistantBrain", "brain": "chatgpt" })),
                ("c3", json!({ "type": "assistantLogin" })),
                ("c4", json!({ "type": "assistantLoginCancel" })),
                ("c5", json!({ "type": "assistantGateway", "on": false })),
            ] {
                good["id"] = json!(id);
                c.send(good).await;
                assert_eq!(result(&mut c, id).await["ok"], true);
            }
            assert_eq!(
                fake.calls(),
                [r#"set Some("Athena") Some("butler") None"#, r#"skills ["search", "news"]"#, "brain chatgpt", "login", "login cancel", "gateway false"]
            );

            // Without a usable id, or of a type this computer does not know: ignored, and nothing is carried out.
            c.send(json!({ "type": "assistantSet", "name": "Nobody" })).await;
            c.send(json!({ "type": "assistantSet", "id": "", "name": "Nobody" })).await;
            c.send(json!({ "type": "assistantSet", "id": "x".repeat(65), "name": "Nobody" })).await;
            c.send(json!({ "type": "assistantSelfDestruct", "id": "d1" })).await;
            c.send(json!({ "type": "assistantResult", "id": "d2", "ok": true })).await;
            c.send(json!({ "type": "assistantLoginCancel", "id": "d3" })).await;
            assert_eq!(result(&mut c, "d3").await["ok"], true, "the next answer is the last request's");
            assert_eq!(fake.calls().len(), 7);
        });
    }

    #[test]
    fn a_brain_and_skills_only_for_a_hermes_coucou_runs_and_nothing_is_ever_installed_from_the_phone() {
        block_on(async {
            let mut status = zheus();
            status.installed = false;
            status.managed = false;
            let fake = Fake::new(status);
            let rig = Rig::start_links(None, Switches::new(true, false), links(&fake)).await;
            let (mut c, _) = phone(&rig, Some(json!(["assistant"]))).await;
            c.expect("assistant").await;
            let ask = |id: &'static str, mut msg: Value| {
                msg["id"] = json!(id);
                msg
            };
            for (id, msg) in [
                ("n1", json!({ "type": "assistantBrain", "brain": "chatgpt" })),
                ("n2", json!({ "type": "assistantSkills", "skills": ["search"] })),
                ("n3", json!({ "type": "assistantLogin" })),
                ("n4", json!({ "type": "assistantGateway", "on": true })),
            ] {
                c.send(ask(id, msg)).await;
                let m = result(&mut c, id).await;
                assert_eq!(refused(&m), (Some(false), Some("not_installed")), "{m}");
                assert_eq!(m["message"], assistant::reason("not_installed"));
            }
            // Installed, but not run by Coucou: its brain and packs are not the phone's to change.
            fake.status.lock().unwrap().installed = true;
            c.send(ask("m1", json!({ "type": "assistantBrain", "brain": "chatgpt" }))).await;
            assert_eq!(refused(&result(&mut c, "m1").await), (Some(false), Some("not_installed")));
            c.send(ask("m2", json!({ "type": "assistantSkills", "skills": [] }))).await;
            assert_eq!(refused(&result(&mut c, "m2").await), (Some(false), Some("not_installed")));
            // The name and the tone are the assistant's own, whoever runs it.
            c.send(ask("m3", json!({ "type": "assistantSet", "name": "Athena" }))).await;
            assert_eq!(result(&mut c, "m3").await["ok"], true);
            // While a setup runs, a new brain waits.
            {
                let mut s = fake.status.lock().unwrap();
                s.managed = true;
                s.progress.running = true;
            }
            c.send(ask("m4", json!({ "type": "assistantBrain", "brain": "chatgpt" }))).await;
            assert_eq!(refused(&result(&mut c, "m4").await), (Some(false), Some("busy")));
            // A sign-in that already shows its code is the one the phone wants: nothing new is started.
            {
                let mut s = fake.status.lock().unwrap();
                s.progress.running = false;
                s.login = LoginState::Waiting { code: "ABCD-1234".into(), url: hermes_setup::LOGIN_URL.into(), expires_at: 1_900_000_000_000 };
            }
            c.send(ask("m5", json!({ "type": "assistantLogin" }))).await;
            assert_eq!(result(&mut c, "m5").await["ok"], true);
            assert_eq!(fake.calls(), [r#"set Some("Athena") None None"#]);
        });
    }

    #[test]
    fn the_switch_is_read_again_for_every_request() {
        block_on(async {
            let fake = Fake::new(zheus());
            let switches = Switches::new(true, false);
            let rig = Rig::start_links(None, switches.clone(), links(&fake)).await;
            let (mut c, _) = phone(&rig, Some(json!(["assistant"]))).await;
            c.expect("assistant").await;
            switches.assistant.store(false, Ordering::SeqCst);
            c.send(json!({ "type": "assistantGet" })).await;
            c.send(json!({ "type": "assistantSet", "id": "o1", "name": "Athena" })).await;
            let m = c.recv().await.unwrap();
            assert_eq!(m["type"], "assistantResult", "no picture any more: {m}");
            assert_eq!(refused(&m), (Some(false), Some("off")));
            assert_eq!(m["message"], assistant::reason("off"));
            assert!(fake.calls().is_empty());
        });
    }

    #[test]
    fn at_most_twenty_requests_in_ten_minutes_per_connection() {
        block_on(async {
            let fake = Fake::new(zheus());
            let rig = Rig::start_links(None, Switches::new(true, false), links(&fake)).await;
            let (mut c, _) = phone(&rig, Some(json!(["assistant"]))).await;
            c.expect("assistant").await;
            for n in 0..assistant::MAX_REQUESTS {
                let id = format!("r{n}");
                c.send(json!({ "type": "assistantLoginCancel", "id": id })).await;
                assert_eq!(result(&mut c, &id).await["ok"], true);
            }
            c.send(json!({ "type": "assistantLoginCancel", "id": "over" })).await;
            let m = result(&mut c, "over").await;
            assert_eq!(refused(&m), (Some(false), Some("rate")));
            assert_eq!(m["message"], assistant::reason("rate"));
            assert_eq!(fake.calls().len(), assistant::MAX_REQUESTS);
            // The picture is still there to read, and another connection has its own count.
            c.send(json!({ "type": "assistantGet" })).await;
            c.expect("assistant").await;
            let (mut other, _) = phone(&rig, Some(json!(["assistant"]))).await;
            other.send(json!({ "type": "assistantLoginCancel", "id": "o1" })).await;
            assert_eq!(result(&mut other, "o1").await["ok"], true);
        });
    }

    #[test]
    fn a_burst_of_changes_reaches_the_phones_twice_a_second_at_most_and_the_latest_last() {
        block_on(async {
            let fake = Fake::new(zheus());
            let link = AssistantLink::new(fake.clone());
            let rig = Rig::start_links(None, Switches::new(true, false), Links { assistant: Some(link.clone()), home: None }).await;
            let (mut old, _) = phone(&rig, None).await;
            assert!(!rig.hub.wants_assistant(), "no phone has it: no picture is made");
            let (mut c, _) = phone(&rig, Some(json!(["assistant"]))).await;
            c.expect("assistant").await;
            assert!(rig.hub.wants_assistant());
            let picture = |name: &str| json!({ "type": "assistant", "name": name }).to_string();
            assert_eq!(link.changed(&rig.hub, picture("A"), 10_000), None, "the first one goes at once");
            assert_eq!(c.expect("assistant").await["name"], "A");
            assert_eq!(link.changed(&rig.hub, picture("B"), 10_100), Some(400));
            assert_eq!(link.changed(&rig.hub, picture("C"), 10_200), None, "a send is planned already");
            link.due(&rig.hub, 10_500);
            assert_eq!(c.expect("assistant").await["name"], "C", "the latest; B never went");
            // The same picture again is sent once.
            assert_eq!(link.changed(&rig.hub, picture("C"), 20_000), None);
            c.send(json!({ "type": "ping" })).await;
            assert_eq!(c.recv().await.unwrap()["type"], "pong");
            // A phone that did not ask never hears of the assistant.
            old.send(json!({ "type": "ping" })).await;
            loop {
                let m = old.recv().await.expect("closed");
                assert_ne!(m["type"], "assistant");
                if m["type"] == "pong" {
                    break;
                }
            }
        });
    }
}

// ── Mochi's home from the phone (home.rs) ────────────────────────────────────

mod home_link_tests {
    use super::assistant_tests::{phone, zheus, Fake, Switches};
    use super::*;
    use crate::phone_link::assistant::AssistantLink;
    use crate::phone_link::chat::BoxFuture;
    use crate::phone_link::home::{self, tests::house, HomeBackend, HomeLink, HomePicture};
    use crate::phone_link::server::Links;
    use crate::smart_home::config::TvConf;
    use crate::smart_home::journal::Entry;
    use crate::smart_home::model::{Action, Channel};
    use crate::smart_home::routines::Routine;
    use crate::smart_home::Outcome;
    use std::sync::atomic::Ordering;
    use tokio::io::AsyncWriteExt;
    use tokio::sync::Notify;

    /// The house, what reached its devices, and how an action ends.
    struct FakeHouse {
        picture: Mutex<HomePicture>,
        done: Mutex<Vec<String>>,
        /// What a device answers: it worked, and its words.
        answer: Mutex<(bool, String)>,
        /// Set: an action waits for it before it ends.
        gate: Mutex<Option<Arc<Notify>>>,
    }

    impl FakeHouse {
        fn new(picture: HomePicture) -> Arc<FakeHouse> {
            Arc::new(FakeHouse { picture: Mutex::new(picture), done: Mutex::new(Vec::new()), answer: Mutex::new((true, "On · 40%".into())), gate: Mutex::new(None) })
        }
        fn done(&self) -> Vec<String> {
            self.done.lock().unwrap().clone()
        }
    }

    impl HomeBackend for FakeHouse {
        fn picture(&self) -> BoxFuture<HomePicture> {
            let p = self.picture.lock().unwrap().clone();
            Box::pin(async move { p })
        }
        fn control(&self, device: String, action: Action) -> BoxFuture<Outcome> {
            self.done.lock().unwrap().push(format!("{device} {}", action.word().0));
            let (gate, (ok, detail)) = (self.gate.lock().unwrap().clone(), self.answer.lock().unwrap().clone());
            Box::pin(async move {
                if let Some(gate) = gate {
                    gate.notified().await;
                }
                Outcome { ok, text: "Done.".into(), detail }
            })
        }
        fn run_routine(&self, routine: Routine) -> BoxFuture<Outcome> {
            self.done.lock().unwrap().push(format!("routine {}", routine.id));
            Box::pin(async { Outcome { ok: true, text: "Done.".into(), detail: "1 of 1 steps done".into() } })
        }
    }

    fn links(house: &Arc<FakeHouse>) -> Links {
        Links { assistant: None, home: Some(HomeLink::new(house.clone())) }
    }

    /// A phone that has `home`, past its first picture of the house.
    async fn home_phone(rig: &Rig) -> Client {
        let (mut c, welcome) = phone(rig, Some(json!(["home"]))).await;
        assert_eq!(welcome["caps"], json!(["home"]));
        c.expect("home").await;
        c
    }

    fn control(id: &str, device: &str, action: &str, value: Value, confirmed: Value) -> Value {
        json!({ "type": "homeControl", "id": id, "device": device, "action": action, "value": value, "confirmed": confirmed })
    }

    async fn result(c: &mut Client, id: &str) -> Value {
        let m = c.expect("homeResult").await;
        assert_eq!(m["id"], id, "{m}");
        m
    }

    async fn refusal(c: &mut Client, id: &str) -> String {
        let m = result(c, id).await;
        assert_eq!(m["ok"], false, "{m}");
        let code = m["reason"].as_str().unwrap().to_string();
        if code != "failed" {
            assert_eq!(m["message"], home::reason(&code), "a fixed message");
        }
        code
    }

    /// Reads up to the pong, saying what came before it.
    async fn before_pong(c: &mut Client) -> Vec<String> {
        c.send(json!({ "type": "ping" })).await;
        let mut seen = Vec::new();
        loop {
            let m = c.recv().await.expect("closed");
            if m["type"] == "pong" {
                return seen;
            }
            seen.push(m["type"].as_str().unwrap_or("").to_string());
        }
    }

    #[test]
    fn home_is_offered_only_to_a_phone_that_asks_while_its_switches_are_on() {
        block_on(async {
            let house = FakeHouse::new(house());
            let rig = Rig::start_links(None, Switches::new(true, true), links(&house)).await;
            let (_c, welcome) = phone(&rig, Some(json!(["assistant", "home"]))).await;
            assert_eq!(welcome["caps"], json!(["home"]), "the assistant is not wired in here");
            let (_c, welcome) = phone(&rig, None).await;
            assert!(welcome.get("caps").is_none(), "an older phone is never offered it");
            let off = Rig::start_links(None, Switches::new(true, false), links(&house)).await;
            let (_c, welcome) = phone(&off, Some(json!(["home"]))).await;
            assert!(welcome.get("caps").is_none(), "{welcome}");
            // Both, when both are wired in and on.
            let both = Links { assistant: Some(AssistantLink::new(Fake::new(zheus()))), home: Some(HomeLink::new(house.clone())) };
            let rig = Rig::start_links(None, Switches::new(true, true), both).await;
            let (_c, welcome) = phone(&rig, Some(json!(["home", "assistant", "details"]))).await;
            assert_eq!(welcome["caps"], json!(["assistant", "home"]));
        });
    }

    #[test]
    fn the_house_follows_the_sessions_with_only_the_ticked_devices_and_nothing_that_says_where_they_are() {
        block_on(async {
            let mut p = house();
            p.devices[1].name = "TV 192.168.1.20".into();
            p.devices[1].apps = vec!["http://192.168.1.20:8000/stream".into(), "YouTube".into()];
            p.devices[1].channels.push(Channel { number: "9".into(), name: "A4:B1:C2:D3:E4:F5".into() });
            p.devices[1].state.app = Some("http://homeassistant.local:8123/api/camera_proxy/camera.porta?token=eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9abc123".into());
            p.log.push(Entry {
                at: "2026-10-13 21:06:00".into(),
                origin: "phone".into(),
                device: "TV da sala".into(),
                action: "Turn on".into(),
                ok: false,
                detail: "Not a valid address: tv-sala.lan (MAC a4-b1-c2-d3-e4-f5), key sk-test-0123456789abcdefghijkl".into(),
            });
            let tv = TvConf { host: "tv-sala.lan".into(), mac: "A4:B1:C2:D3:E4:F5".into(), ..TvConf::default() };
            p.hidden = home::hidden_of("http://homeassistant.local:8123", &[tv]);
            let house = FakeHouse::new(p);
            let rig = Rig::start_links(None, Switches::new(false, true), links(&house)).await;
            let (mut c, _) = phone(&rig, Some(json!(["home"]))).await;
            assert_eq!(c.recv().await.unwrap()["type"], "sessions", "the sessions come first");
            let m = c.expect("home").await;
            let text = m.to_string();
            for secret in ["http", "192.168.", "homeassistant", "tv-sala", "A4:B1", "a4-b1", "eyJ0", "sk-test", "8123", "token="] {
                assert!(!text.contains(secret), "{secret} in {text}");
            }
            let ids: Vec<&str> = m["devices"].as_array().unwrap().iter().map(|d| d["id"].as_str().unwrap()).collect();
            assert_eq!(ids, ["ha:light.sala", "samsung:1a2b3c4d", "ha:switch.aquecedor"], "never a device the user did not tick for Mochi");
            assert_eq!(m["devices"][1]["apps"][1], "YouTube");
            assert_eq!(m["routines"].as_array().unwrap().len(), 2);
            assert_eq!(m["log"].as_array().unwrap().len(), 2);
            assert!(house.done().is_empty(), "showing the house acts on nothing");
        });
    }

    #[test]
    fn an_action_is_answered_by_its_id_then_every_phone_with_home_gets_the_house_again() {
        block_on(async {
            let house = FakeHouse::new(house());
            let rig = Rig::start_links(None, Switches::new(false, true), links(&house)).await;
            let mut c = home_phone(&rig).await;
            let mut other = home_phone(&rig).await;
            let (mut old, _) = phone(&rig, None).await;
            c.send(control("h1", "ha:light.sala", "set_brightness", json!(40), json!(false))).await;
            assert_eq!(result(&mut c, "h1").await, json!({ "type": "homeResult", "id": "h1", "ok": true, "detail": "On · 40%" }));
            assert_eq!(c.expect("home").await["devices"][0]["id"], "ha:light.sala", "the house as it is afterwards");
            assert_eq!(other.expect("home").await["type"], "home");
            assert_eq!(house.done(), ["ha:light.sala set_brightness"]);
            assert!(before_pong(&mut old).await.iter().all(|t| !t.starts_with("home")), "a phone that did not ask hears nothing of the house");
            // A device that says why it did not go through: its words, cleaned.
            *house.answer.lock().unwrap() = (false, "Not a valid address: 192.168.1.20".into());
            c.send(control("h2", "ha:light.sala", "turn_off", Value::Null, json!(false))).await;
            assert_eq!(result(&mut c, "h2").await, json!({ "type": "homeResult", "id": "h2", "ok": false, "reason": "failed", "message": "Not a valid address: •••" }));
            // A routine.
            c.send(json!({ "type": "homeRoutine", "id": "h3", "routine": "r-0a1b2c3d4e5f" })).await;
            assert_eq!(result(&mut c, "h3").await["detail"], "1 of 1 steps done");
            assert_eq!(house.done(), ["ha:light.sala set_brightness", "ha:light.sala turn_off", "routine r-0a1b2c3d4e5f"]);
        });
    }

    #[test]
    fn a_device_or_a_routine_that_asks_first_waits_for_the_phones_confirmation() {
        block_on(async {
            let house = FakeHouse::new(house());
            let rig = Rig::start_links(None, Switches::new(false, true), links(&house)).await;
            let mut c = home_phone(&rig).await;
            c.send(control("p1", "ha:switch.aquecedor", "turn_on", Value::Null, json!(false))).await;
            assert_eq!(refusal(&mut c, "p1").await, "confirm", "a plug asks first");
            c.send(control("p2", "ha:switch.aquecedor", "turn_on", Value::Null, json!("yes"))).await;
            assert_eq!(refusal(&mut c, "p2").await, "confirm", "only true confirms");
            c.send(json!({ "type": "homeRoutine", "id": "p3", "routine": "r-aaaaaaaaaaaa" })).await;
            assert_eq!(refusal(&mut c, "p3").await, "confirm", "its step is on the plug");
            assert!(house.done().is_empty(), "nothing reached a device");
            c.send(control("p4", "ha:switch.aquecedor", "turn_on", Value::Null, json!(true))).await;
            assert_eq!(result(&mut c, "p4").await["ok"], true);
            c.send(json!({ "type": "homeRoutine", "id": "p5", "routine": "r-aaaaaaaaaaaa", "confirmed": true })).await;
            assert_eq!(result(&mut c, "p5").await["ok"], true);
            // "Ask before every action" makes everything ask.
            house.picture.lock().unwrap().ask_all = true;
            c.send(control("p6", "ha:light.sala", "turn_on", Value::Null, json!(false))).await;
            assert_eq!(refusal(&mut c, "p6").await, "confirm");
            c.send(json!({ "type": "homeRoutine", "id": "p7", "routine": "r-0a1b2c3d4e5f" })).await;
            assert_eq!(refusal(&mut c, "p7").await, "confirm");
            assert_eq!(house.done(), ["ha:switch.aquecedor turn_on", "routine r-aaaaaaaaaaaa"]);
        });
    }

    #[test]
    fn a_wrong_action_or_a_device_the_phone_may_not_use_never_reaches_a_device() {
        block_on(async {
            let house = FakeHouse::new(house());
            let rig = Rig::start_links(None, Switches::new(false, true), links(&house)).await;
            let mut c = home_phone(&rig).await;
            for (id, mut msg, code) in [
                ("w1", json!({ "type": "homeControl", "device": "ha:light.sala", "action": "self_destruct" }), "invalid"),
                ("w2", json!({ "type": "homeControl", "device": "ha:light.sala", "action": "set_brightness", "value": 140 }), "invalid"),
                ("w3", json!({ "type": "homeControl", "device": "ha:light.sala", "action": "set_brightness", "value": [40] }), "invalid"),
                ("w4", json!({ "type": "homeControl", "device": "", "action": "turn_on" }), "invalid"),
                ("w5", json!({ "type": "homeRoutine", "routine": 7 }), "invalid"),
                ("w6", json!({ "type": "homeControl", "device": "ha:light.gone", "action": "turn_on" }), "unknown"),
                ("w7", json!({ "type": "homeControl", "device": "samsung:0badc0de", "action": "turn_on", "confirmed": true }), "unknown"),
                ("w8", json!({ "type": "homeControl", "device": "ha:light.sala", "action": "open_app", "value": "YouTube" }), "invalid"),
                ("w9", json!({ "type": "homeRoutine", "routine": "r-missing", "confirmed": true }), "unknown"),
            ] {
                msg["id"] = json!(id);
                c.send(msg).await;
                assert_eq!(refusal(&mut c, id).await, code, "{id}");
            }
            // Without a usable id, or of a type this computer does not know: ignored.
            c.send(json!({ "type": "homeControl", "device": "ha:light.sala", "action": "turn_on" })).await;
            c.send(control(&"x".repeat(65), "ha:light.sala", "turn_on", Value::Null, json!(true))).await;
            c.send(json!({ "type": "homeUnlockTheDoor", "id": "x1" })).await;
            c.send(json!({ "type": "homeResult", "id": "x2", "ok": true })).await;
            assert!(before_pong(&mut c).await.iter().all(|t| t == "home"), "no answer to any of them");
            assert!(house.done().is_empty());
        });
    }

    #[test]
    fn one_action_at_a_time_on_the_computer() {
        block_on(async {
            let house = FakeHouse::new(house());
            let gate = Arc::new(Notify::new());
            *house.gate.lock().unwrap() = Some(gate.clone());
            let rig = Rig::start_links(None, Switches::new(false, true), links(&house)).await;
            let mut a = home_phone(&rig).await;
            let mut b = home_phone(&rig).await;
            a.send(control("a1", "ha:light.sala", "turn_on", Value::Null, json!(false))).await;
            // Its line is read before the ping's: once the pong is there, the action holds the computer's place.
            a.send(json!({ "type": "ping" })).await;
            a.expect("pong").await;
            b.send(control("b1", "ha:light.sala", "turn_off", Value::Null, json!(false))).await;
            assert_eq!(refusal(&mut b, "b1").await, "busy");
            b.send(json!({ "type": "homeRoutine", "id": "b2", "routine": "r-0a1b2c3d4e5f" })).await;
            assert_eq!(refusal(&mut b, "b2").await, "busy");
            gate.notify_one();
            assert_eq!(result(&mut a, "a1").await["ok"], true);
            *house.gate.lock().unwrap() = None;
            b.send(control("b3", "ha:light.sala", "turn_off", Value::Null, json!(false))).await;
            assert_eq!(result(&mut b, "b3").await["ok"], true, "the place is free again");
            assert_eq!(house.done(), ["ha:light.sala turn_on", "ha:light.sala turn_off"]);
        });
    }

    #[test]
    fn at_most_thirty_actions_in_ten_minutes_per_connection_and_what_sent_nothing_does_not_count() {
        block_on(async {
            let house = FakeHouse::new(house());
            let rig = Rig::start_links(None, Switches::new(false, true), links(&house)).await;
            let mut c = home_phone(&rig).await;
            for n in 0..5 {
                let id = format!("q{n}");
                c.send(control(&id, "ha:switch.aquecedor", "turn_on", Value::Null, json!(false))).await;
                assert_eq!(refusal(&mut c, &id).await, "confirm");
            }
            for n in 0..home::MAX_ACTIONS {
                let id = format!("n{n}");
                c.send(control(&id, "ha:light.sala", "turn_on", Value::Null, json!(false))).await;
                assert_eq!(result(&mut c, &id).await["ok"], true, "{id}");
            }
            c.send(control("over", "ha:light.sala", "turn_on", Value::Null, json!(false))).await;
            assert_eq!(refusal(&mut c, "over").await, "rate");
            assert_eq!(house.done().len(), home::MAX_ACTIONS);
            // Another connection has its own count.
            let mut other = home_phone(&rig).await;
            other.send(control("o1", "ha:light.sala", "turn_off", Value::Null, json!(false))).await;
            assert_eq!(result(&mut other, "o1").await["ok"], true);
        });
    }

    #[test]
    fn the_switches_are_read_again_for_every_action() {
        block_on(async {
            let house = FakeHouse::new(house());
            let switches = Switches::new(false, true);
            let rig = Rig::start_links(None, switches.clone(), links(&house)).await;
            let mut c = home_phone(&rig).await;
            switches.home.store(false, Ordering::SeqCst);
            c.send(json!({ "type": "homeGet" })).await;
            c.send(control("f1", "ha:light.sala", "turn_on", Value::Null, json!(true))).await;
            let m = c.recv().await.unwrap();
            assert_eq!(m["type"], "homeResult", "no picture any more: {m}");
            assert_eq!((m["ok"].as_bool(), m["reason"].as_str()), (Some(false), Some("off")));
            // Mochi's home itself switched off between the phone's request and its turn.
            switches.home.store(true, Ordering::SeqCst);
            house.picture.lock().unwrap().enabled = false;
            c.send(control("f2", "ha:light.sala", "turn_on", Value::Null, json!(true))).await;
            assert_eq!(refusal(&mut c, "f2").await, "off");
            assert!(house.done().is_empty());
        });
    }

    #[test]
    fn a_big_house_still_comes_in_one_line_under_60_kib() {
        block_on(async {
            let mut p = house();
            let tv = p.devices[1].clone();
            p.devices = (0..130)
                .map(|i| crate::smart_home::model::Device {
                    id: format!("samsung:{i:08x}"),
                    apps: (0..80).map(|n| format!("App {n} {}", "x".repeat(40))).collect(),
                    channels: (0..90).map(|n| Channel { number: n.to_string(), name: format!("Canal {n} {}", "y".repeat(40)) }).collect(),
                    ..tv.clone()
                })
                .collect();
            p.log = (0..45).map(|n| Entry { detail: "z".repeat(250), device: format!("TV {n}"), ..p.log[0].clone() }).collect();
            let house = FakeHouse::new(p);
            let rig = Rig::start_links(None, Switches::new(false, true), links(&house)).await;
            let (mut c, _) = phone(&rig, Some(json!(["home"]))).await;
            let mut line = String::new();
            loop {
                line.clear();
                tokio::time::timeout(Duration::from_secs(3), c.r.read_line(&mut line)).await.unwrap().unwrap();
                if line.contains("\"type\":\"home\"") {
                    break;
                }
            }
            assert!(line.len() < home::LINE_LIMIT, "{} bytes", line.len());
            let m: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(m["devices"].as_array().unwrap().len(), home::MAX_DEVICES);
        });
    }

    #[test]
    fn turning_the_link_off_stops_the_phones_action_and_frees_the_place() {
        block_on(async {
            let house = FakeHouse::new(house());
            *house.gate.lock().unwrap() = Some(Arc::new(Notify::new()));
            let link = HomeLink::new(house.clone());
            let mut rig = Rig::start_links(None, Switches::new(false, true), Links { assistant: None, home: Some(link.clone()) }).await;
            let mut c = home_phone(&rig).await;
            c.send(control("s1", "ha:light.sala", "turn_on", Value::Null, json!(false))).await;
            c.send(json!({ "type": "ping" })).await;
            c.expect("pong").await;
            rig.handle.take().unwrap().stop();
            // The link on again, with the same home: the place the stopped action held is free.
            *house.gate.lock().unwrap() = None;
            let again = Rig::start_links(None, Switches::new(false, true), Links { assistant: None, home: Some(link) }).await;
            let mut c = home_phone(&again).await;
            c.send(control("s2", "ha:light.sala", "turn_off", Value::Null, json!(false))).await;
            assert_eq!(result(&mut c, "s2").await["ok"], true);
        });
    }

    /// The relay (relay_client.rs) runs the very conversation of the local network over its own stream:
    /// `server::run_with`. Over memory here, the capabilities ride along the same.
    #[test]
    fn the_relays_way_in_offers_and_carries_them_the_same() {
        block_on(async {
            let fake = Fake::new(zheus());
            let house = FakeHouse::new(house());
            let links = Links { assistant: Some(AssistantLink::new(fake.clone())), home: Some(HomeLink::new(house.clone())) };
            let shared = server::Shared::with_links(Hub::new(Arc::new(Recorder::default())), TOKEN.into(), "PC".into(), None, Switches::new(true, true), links);
            let (near, far) = tokio::io::duplex(256 * 1024);
            tokio::spawn(async move { server::run_with(far, &shared, None).await });
            let (r, mut w) = tokio::io::split(near);
            let mut r = BufReader::new(r);
            let line = |v: Value| format!("{v}\n");
            let hello = line(json!({ "type": "hello", "v": 1, "token": TOKEN, "device": "Relay phone", "caps": ["assistant", "home"] }));
            w.write_all(hello.as_bytes()).await.unwrap();
            let mut types = Vec::new();
            let mut welcome = Value::Null;
            while types.len() < 4 {
                let mut line = String::new();
                tokio::time::timeout(Duration::from_secs(3), r.read_line(&mut line)).await.unwrap().unwrap();
                let m: Value = serde_json::from_str(&line).unwrap();
                if m["type"] == "welcome" {
                    welcome = m.clone();
                }
                types.push(m["type"].as_str().unwrap().to_string());
            }
            assert_eq!(welcome["caps"], json!(["assistant", "home"]));
            assert_eq!(types[..2], ["welcome", "sessions"]);
            assert!(types.contains(&"assistant".to_string()) && types.contains(&"home".to_string()), "{types:?}");
            w.write_all(line(json!({ "type": "assistantSet", "id": "r1", "name": "Athena" })).as_bytes()).await.unwrap();
            let mut answer = String::new();
            tokio::time::timeout(Duration::from_secs(3), r.read_line(&mut answer)).await.unwrap().unwrap();
            assert_eq!(serde_json::from_str::<Value>(&answer).unwrap(), json!({ "type": "assistantResult", "id": "r1", "ok": true }));
        });
    }
}
