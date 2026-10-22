// The phone told through a push service when nothing else can reach it.
//
// iOS pauses Coucou for iPhone about half a minute after it leaves the screen, and only Apple's push wakes an app,
// which the sideload build (a free Apple account) cannot have. So when the user turns it on (Settings → Phone →
// Notifications with Coucou closed), a request that is still waiting a few seconds after it came, while no phone is
// connected, is announced through ntfy (https://ntfy.sh, or the user's own ntfy server): the ntfy app on the phone
// shows it, and a tap opens Coucou, which connects and shows the request. Off by default; nothing is sent while it
// is off, and nothing while a phone is connected (the phone notifies by itself then).
//
// What crosses the push service: "Coucou" and one short sentence ("Mochi is waiting for your OK"). Never the
// command, a path, a question's text or this computer's name. The topic is random and kept in the OS keystore like
// the pairing token: whoever knows it can read those sentences, nothing more.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::Url;
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{json, Value};
use tauri::{State, WebviewWindow};

use super::pairing::SecretStore;
use super::PhoneLink;
use crate::i18n::{t, tf};
use crate::{log, net, Shared};

/// The public ntfy server, used when the user leaves the address empty.
pub const DEFAULT_SERVER: &str = "https://ntfy.sh";
/// Keystore entries (secrets.rs, internal: never reachable from a webview).
pub const TOPIC_KEY: &str = "phone-push-topic";
pub const TOKEN_KEY: &str = "phone-push-token";
/// How long a request waits before the push: answered at the desk by then, nothing is sent.
pub const DELAY: Duration = Duration::from_secs(8);
/// At most one push in this window, so a burst of requests rings once.
pub const MIN_GAP: Duration = Duration::from_secs(20);
/// Requests remembered so each is pushed at most once.
const REMEMBERED: usize = 64;
/// What a tap on the notification opens: Coucou, which connects and shows what waits.
pub const CLICK: &str = "coucou://open";
const TIMEOUT: Duration = Duration::from_secs(15);

/// Which requests were already pushed, and when the last push went.
#[derive(Default)]
pub struct Pusher {
    inner: Mutex<PushState>,
}

#[derive(Default)]
struct PushState {
    seen: VecDeque<String>,
    last_sent: Option<Instant>,
}

impl Pusher {
    /// True the first time `request_id` comes: a request is pushed once, however often the island publishes it.
    pub fn first_time(&self, request_id: &str) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner.seen.iter().any(|s| s == request_id) {
            return false;
        }
        if inner.seen.len() >= REMEMBERED {
            inner.seen.pop_front();
        }
        inner.seen.push_back(request_id.to_string());
        true
    }

    /// Whether a push may go at `now` (one per MIN_GAP); if so, it counts as sent.
    pub fn may_send(&self, now: Instant) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner.last_sent.is_some_and(|last| now.duration_since(last) < MIN_GAP) {
            return false;
        }
        inner.last_sent = Some(now);
        true
    }
}

/// A new random topic: `coucou-` and 24 hex digits (96 bits), so nobody finds it by guessing.
pub fn new_topic() -> Result<String, String> {
    let mut bytes = [0u8; 12];
    SystemRandom::new().fill(&mut bytes).map_err(|_| "no secure random source".to_string())?;
    Ok(format!("coucou-{}", super::hub::hex(&bytes)))
}

/// The stored topic, if a valid one is there.
pub fn topic(store: &dyn SecretStore) -> Option<String> {
    store.get(TOPIC_KEY).filter(|t| is_topic(t))
}

/// The stored topic, or a new one stored now (the switch was just turned on).
pub fn ensure_topic(store: &dyn SecretStore) -> Result<String, String> {
    match topic(store) {
        Some(t) => Ok(t),
        None => renew_topic(store),
    }
}

/// A new topic: the phone subscribed to the old one stops getting anything.
pub fn renew_topic(store: &dyn SecretStore) -> Result<String, String> {
    let topic = new_topic()?;
    store.set(TOPIC_KEY, &topic)?;
    Ok(topic)
}

/// The access token for a protected topic (a self-hosted server), if any. Never sent back to a window.
pub fn token(store: &dyn SecretStore) -> Option<String> {
    store.get(TOKEN_KEY).filter(|t| !t.trim().is_empty())
}

/// Stores the access token; empty forgets it. Only printable ASCII without spaces, as a bearer token is.
pub fn set_token(store: &dyn SecretStore, token: &str) -> Result<(), String> {
    let token = token.trim();
    if token.len() > 512 || !token.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(t("This access token is not valid."));
    }
    store.set(TOKEN_KEY, token)
}

/// ntfy's rule for a topic name.
pub fn is_topic(topic: &str) -> bool {
    (1..=64).contains(&topic.len()) && topic.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The push server as the user typed it: empty is ntfy.sh. https, or plain http to a server on the local network.
pub fn server_url(raw: &str) -> Result<Url, String> {
    let raw = raw.trim();
    let raw = if raw.is_empty() { DEFAULT_SERVER } else { raw };
    let with_scheme = if raw.contains("://") { raw.to_string() } else { format!("https://{raw}") };
    let mut url = Url::parse(&with_scheme).map_err(|_| tf("Not a valid address: {address}", &[("address", raw)]))?;
    let host = url.host_str().unwrap_or("").to_string();
    if host.is_empty() || !url.username().is_empty() || url.password().is_some() {
        return Err(tf("Not a valid address: {address}", &[("address", raw)]));
    }
    match url.scheme() {
        "https" => {}
        "http" if net::is_local_host(&host) => {}
        _ => return Err(t("The push server must use https:// (plain http only on your local network).")),
    }
    let path = url.path().trim_end_matches('/').to_string();
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

/// What the notification says. `who` is the agent's name when known; `question` for a question rather than a
/// permission request. Never the command or the question's text.
pub fn words(who: Option<&str>, question: bool) -> String {
    match (who.map(str::trim).filter(|w| !w.is_empty()), question) {
        (Some(agent), false) => tf("{agent} is waiting for your OK", &[("agent", agent)]),
        (None, false) => t("An agent is waiting for your OK"),
        (Some(agent), true) => tf("{agent} has a question for you", &[("agent", agent)]),
        (None, true) => t("An agent has a question for you"),
    }
}

/// The JSON ntfy takes on its root address (UTF-8 safe, unlike headers).
pub fn body(topic: &str, message: &str) -> Value {
    json!({
        "topic": topic,
        "title": "AXYON",
        "message": message,
        "priority": 4,
        "click": CLICK,
    })
}

/// Sends one notification. The error is for the user (Settings' test button); the log only says it failed.
pub async fn send(server: &Url, topic: &str, token: Option<&str>, message: &str) -> Result<(), String> {
    if !is_topic(topic) {
        return Err(t("Make a topic first."));
    }
    let client = net::client(server, TIMEOUT)?;
    let mut request = client.post(server.clone()).json(&body(topic, message));
    if let Some(token) = token.filter(|t| !t.is_empty()) {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|e| tf("Could not reach the push service: {error}", &[("error", &e.to_string())]))?;
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(tf("The push service answered {status}.", &[("status", &status.as_u16().to_string())]))
    }
}

/// What a push needs, read from the settings and the keystore. None when the switch is off or no topic exists.
pub fn config(shared: &Shared, store: &dyn SecretStore) -> Option<(Url, String, Option<String>)> {
    let (on, server) = {
        let s = shared.settings.lock().unwrap();
        (s.phone_push, s.phone_push_server.clone())
    };
    if !on {
        return None;
    }
    Some((server_url(&server).ok()?, topic(store)?, token(store)))
}

/// Settings → Phone: "Send a test notification". Sent at once, whether a phone is connected or not.
#[tauri::command]
pub async fn phone_push_test(window: WebviewWindow, shared: State<'_, Shared>, link: State<'_, PhoneLink>) -> Result<(), String> {
    super::only_settings(&window)?;
    let server = {
        let s = shared.settings.lock().unwrap();
        s.phone_push_server.clone()
    };
    let server = server_url(&server)?;
    let topic = topic(&*link.store).ok_or_else(|| t("Make a topic first."))?;
    let token = token(&*link.store);
    let result = send(&server, &topic, token.as_deref(), &t("Notifications with Coucou closed work. Tap to open Coucou.")).await;
    log::line(format!("phone push: test {}", if result.is_ok() { "sent" } else { "failed" }));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_request_is_pushed_once_and_a_burst_rings_once() {
        let pusher = Pusher::default();
        assert!(pusher.first_time("req-1"));
        assert!(!pusher.first_time("req-1"));
        assert!(pusher.first_time("req-2"));
        for i in 0..REMEMBERED + 5 {
            pusher.first_time(&format!("other-{i}"));
        }
        assert!(pusher.first_time("req-1"), "only the latest requests are remembered");

        let start = Instant::now();
        assert!(pusher.may_send(start));
        assert!(!pusher.may_send(start + Duration::from_secs(5)));
        assert!(pusher.may_send(start + MIN_GAP));
    }

    #[test]
    fn topics_are_random_and_valid() {
        let a = new_topic().unwrap();
        let b = new_topic().unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with("coucou-") && a.len() == 7 + 24, "{a}");
        assert!(is_topic(&a));
        assert!(!is_topic(""));
        assert!(!is_topic("with space"));
        assert!(!is_topic("slash/no"));
        assert!(!is_topic(&"x".repeat(65)));
    }

    #[test]
    fn the_topic_and_the_token_live_in_the_keystore() {
        use super::super::pairing::tests::StrictStore;
        let store = StrictStore::default();
        assert_eq!(topic(&store), None);
        let first = ensure_topic(&store).unwrap();
        assert_eq!(ensure_topic(&store).unwrap(), first, "kept once made");
        let second = renew_topic(&store).unwrap();
        assert_ne!(second, first);
        assert_eq!(topic(&store), Some(second));
        assert_eq!(token(&store), None);
        set_token(&store, "  tk_abc123  ").unwrap();
        assert_eq!(token(&store).as_deref(), Some("tk_abc123"));
        assert!(set_token(&store, "two words").is_err());
        set_token(&store, "").unwrap();
        assert_eq!(token(&store), None);
    }

    #[test]
    fn the_server_is_https_or_on_the_local_network() {
        let u = |s: &str| server_url(s).map(|u| u.to_string());
        assert_eq!(u("").unwrap(), "https://ntfy.sh/");
        assert_eq!(u("ntfy.example.com/").unwrap(), "https://ntfy.example.com/");
        assert_eq!(u("https://ntfy.example.com/base/?x=1#y").unwrap(), "https://ntfy.example.com/base");
        assert_eq!(u("http://192.168.1.5:8080").unwrap(), "http://192.168.1.5:8080/");
        assert_eq!(u("http://ntfy.lan").unwrap(), "http://ntfy.lan/");
        assert!(u("http://ntfy.example.com").is_err(), "plain http over the internet");
        assert!(u("https://user:pw@ntfy.sh").is_err());
        assert!(u("ftp://ntfy.sh").is_err());
    }

    #[test]
    fn the_notification_never_carries_the_command_or_the_question() {
        assert_eq!(words(Some("Mochi"), false), "Mochi is waiting for your OK");
        assert_eq!(words(Some("  "), false), "An agent is waiting for your OK");
        assert_eq!(words(None, true), "An agent has a question for you");
        assert_eq!(words(Some("Claude Code"), true), "Claude Code has a question for you");
        let b = body("coucou-abc", "Mochi is waiting for your OK");
        assert_eq!(b["topic"], "coucou-abc");
        assert_eq!(b["title"], "AXYON");
        assert_eq!(b["click"], CLICK);
        assert_eq!(b.as_object().unwrap().len(), 5, "nothing else goes out: {b}");
    }

    /// A push server on loopback that answers `status` and hands back the request it got.
    fn fake_ntfy(status: &'static str) -> (Url, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            // Headers, then the body by its length.
            loop {
                let n = conn.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&got).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                        .unwrap_or(0);
                    if got.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&got).to_string());
            let _ = write!(conn, "HTTP/1.1 {status}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}");
        });
        (url, rx)
    }

    fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    #[test]
    fn a_push_posts_the_json_with_the_token_when_there_is_one() {
        let (url, got) = fake_ntfy("200 OK");
        block_on(send(&url, "coucou-0123", Some("tk_secret"), "Mochi is waiting for your OK")).unwrap();
        let request = got.recv().unwrap();
        assert!(request.starts_with("POST / HTTP/1.1"), "{request}");
        assert!(request.to_ascii_lowercase().contains("authorization: bearer tk_secret"), "{request}");
        let json: Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(json["topic"], "coucou-0123");
        assert_eq!(json["message"], "Mochi is waiting for your OK");

        let (url, got) = fake_ntfy("403 Forbidden");
        let err = block_on(send(&url, "coucou-0123", None, "x")).unwrap_err();
        assert!(err.contains("403"), "{err}");
        assert!(!got.recv().unwrap().to_ascii_lowercase().contains("authorization"));
        assert!(block_on(send(&url, "bad topic", None, "x")).is_err());
    }
}
