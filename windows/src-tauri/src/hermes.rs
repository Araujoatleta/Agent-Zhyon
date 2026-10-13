// Hermes Agent as the chat's brain (docs/HERMES.md). Hermes Agent (Nous Research, MIT,
// github.com/NousResearch/hermes-agent) runs on the user's computer with its own tools and memory — their calendar,
// Spotify, Home Assistant, scheduled jobs, skills — and offers an API server (`hermes gateway` with
// API_SERVER_ENABLED=true, on http://127.0.0.1:8642 by default, behind the bearer key API_SERVER_KEY).
//
// This module is that API as Coucou uses it: one run per message (POST /v1/runs), in a session of Coucou's own so
// Hermes remembers the conversation; the run's events (GET /v1/runs/{id}/events, server-sent): the answer as it is
// written, each tool Hermes uses, an action Hermes wants a person to approve; the person's answer
// (POST /v1/runs/{id}/approval) and Stop (POST /v1/runs/{id}/stop). agent/hermes.rs puts it on the island and the
// phones.
//
// The key is kept in the credential store with the one address it was entered for and is only ever sent there, and
// only to this computer or over https. Hermes gets the user's message and a short note on where it comes from;
// nothing else of Coucou's (no other key, no setting) goes to it.

use std::time::Duration;

use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::i18n::{t, tf};
use crate::settings::Settings;
use crate::{net, secrets};

/// The chat provider id.
pub const PROVIDER: &str = "hermes";
/// Credential store entry of the API server's key, bound to its address.
pub const KEY: &str = "hermes-api-key";
/// Where `hermes gateway` serves its API by default.
pub const USUAL_ADDRESS: &str = "http://127.0.0.1:8642";
/// One plain request (capabilities, start, approval, stop, status).
const TIMEOUT: Duration = Duration::from_secs(20);
/// The events of one run: a long-lived stream, reopened when it breaks.
const STREAM_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// Hermes writes a keepalive every 10 s: this much silence means the stream is gone.
pub const SILENCE: Duration = Duration::from_secs(45);
/// One event line, at most.
const MAX_LINE: usize = 1024 * 1024;

// ── The key ───────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct BoundKey {
    url: String,
    key: String,
}

fn may_carry_key(url: &Url) -> bool {
    url.scheme() == "https" || net::is_loopback_url(url)
}

/// The address as it is stored and shown: no trailing slash.
fn address(url: &Url) -> String {
    url.as_str().trim_end_matches('/').to_string()
}

/// The address typed, or the usual one when nothing was.
fn typed_or_usual(typed: &str) -> Result<Url, String> {
    net::normalise_server_url(if typed.trim().is_empty() { USUAL_ADDRESS } else { typed })
}

/// Stores `key` for the address typed next to it: it will only ever be sent there.
pub fn set_key(typed_url: &str, key: &str) -> Result<(), String> {
    let url = typed_or_usual(typed_url)?;
    if !may_carry_key(&url) {
        return Err(t("A key is only sent over https, or to a server on this computer."));
    }
    let bound = BoundKey { url: address(&url), key: key.trim().to_string() };
    secrets::set_internal(KEY, &serde_json::to_string(&bound).map_err(|e| e.to_string())?)
}

/// The key kept for this address, if any (hermes_setup.rs: Coucou's own gateway).
pub fn key_for_address(typed_url: &str) -> Option<String> {
    key_for(&typed_or_usual(typed_url).ok()?)
}

/// The key, if it was entered for exactly this address.
fn key_for(url: &Url) -> Option<String> {
    bound_key(&secrets::get_internal(KEY)?, url)
}

fn bound_key(stored: &str, url: &Url) -> Option<String> {
    let bound: BoundKey = serde_json::from_str(stored).ok()?;
    (may_carry_key(url) && bound.url == address(url) && !bound.key.is_empty()).then_some(bound.key)
}

// ── Connecting ────────────────────────────────────────────────────────────────

/// What Settings shows once Hermes answered.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connected {
    /// The address as it is stored.
    pub url: String,
    /// The name Hermes answers as: its profile, "hermes-agent" by default.
    pub model: String,
    pub loopback: bool,
}

/// Settings → Hermes Agent → Connect: keeps the key typed with the address (when one was typed), then asks the server
/// what it is. Only a Hermes API server that takes the key and can run, stream, stop and wait for an approval is kept.
pub async fn connect(typed_url: &str, typed_key: &str) -> Result<Connected, String> {
    let url = typed_or_usual(typed_url)?;
    if !typed_key.trim().is_empty() {
        set_key(url.as_str(), typed_key)?;
    }
    let key = key_for(&url).ok_or_else(|| t("Enter the key of Hermes' API server (API_SERVER_KEY in its .env file)."))?;
    let caps = Client::new(url.clone(), key).capabilities().await?;
    Ok(Connected { url: address(&url), model: caps.model, loopback: net::is_loopback_url(&url) })
}

/// What a Hermes API server says it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Capabilities {
    pub model: String,
}

/// Hermes' `/v1/capabilities`, if it is one that can do what Coucou needs.
pub fn parse_capabilities(v: &Value) -> Result<Capabilities, String> {
    let features = &v["features"];
    let able = ["run_submission", "run_events_sse", "run_stop", "run_approval_response"].iter().all(|f| features[*f] == true);
    if v["object"] != "hermes.api_server.capabilities" || !able {
        return Err(not_hermes());
    }
    let model = v["model"].as_str().map(str::trim).filter(|m| !m.is_empty() && m.len() <= 200).unwrap_or("hermes-agent");
    Ok(Capabilities { model: model.to_string() })
}

fn not_hermes() -> String {
    t("This server is not Hermes Agent, or its Hermes is too old: update it with `hermes update`.")
}

/// A run id as Hermes makes them ("run_" and hex); anything else is never put in a path.
fn valid_run_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 100 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

// ── The client ────────────────────────────────────────────────────────────────

/// A request that did not succeed: the HTTP status when Hermes answered, and what to tell the user.
struct Refused {
    status: Option<u16>,
    message: String,
}

impl Refused {
    fn other(message: String) -> Self {
        Self { status: None, message }
    }
}

#[derive(Clone)]
pub struct Client {
    base: Url,
    key: String,
}

impl Client {
    pub fn new(base: Url, key: String) -> Self {
        Self { base, key }
    }

    /// The Hermes the user connected in Settings.
    pub fn from_settings(settings: &Settings) -> Result<Self, String> {
        let missing = || t("Connect Hermes Agent first, in Settings → Hermes Agent.");
        if settings.hermes_url.trim().is_empty() {
            return Err(missing());
        }
        let base = net::normalise_server_url(&settings.hermes_url)?;
        let key = key_for(&base).ok_or_else(missing)?;
        Ok(Self { base, key })
    }

    fn not_running(&self) -> String {
        tf(
            "Hermes Agent does not answer at {url}. Is `hermes gateway` running, with its API server on?",
            &[("url", &address(&self.base))],
        )
    }

    /// One request with the key. A refusal keeps its HTTP status, so callers decide on the status and never on words.
    async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response, Refused> {
        let response = request.bearer_auth(&self.key).send().await.map_err(|_| Refused { status: None, message: self.not_running() })?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let code = status.as_u16();
        if code == 401 {
            return Err(Refused { status: Some(code), message: t("Hermes refused the key: use the API_SERVER_KEY of its .env file.") });
        }
        let body = net::read_capped(response, net::MAX_ERROR_BODY).await.unwrap_or_default();
        let detail = net::error_detail(&body);
        Err(Refused { status: Some(code), message: tf("Hermes Agent answered {status}: {detail}", &[("status", &code.to_string()), ("detail", &detail)]) })
    }

    async fn json(&self, response: reqwest::Response) -> Result<Value, Refused> {
        let bytes = net::read_capped(response, net::MAX_BODY).await.map_err(Refused::other)?;
        serde_json::from_slice(&bytes).map_err(|_| Refused::other(not_hermes()))
    }

    async fn get(&self, path: &str) -> Result<Value, Refused> {
        let client = net::client(&self.base, TIMEOUT).map_err(Refused::other)?;
        let response = self.send(client.get(net::join(&self.base, path))).await?;
        self.json(response).await
    }

    async fn post(&self, path: &str, body: &Value) -> Result<Value, Refused> {
        let client = net::client(&self.base, TIMEOUT).map_err(Refused::other)?;
        let response = self.send(client.post(net::join(&self.base, path)).json(body)).await?;
        self.json(response).await
    }

    pub async fn capabilities(&self) -> Result<Capabilities, String> {
        match self.get("v1/capabilities").await {
            Ok(v) => parse_capabilities(&v),
            Err(Refused { status: Some(404), .. }) => Err(not_hermes()),
            Err(e) => Err(e.message),
        }
    }

    /// The name Hermes answers as, for the chat's model picker.
    pub async fn models(&self) -> Result<Vec<String>, String> {
        Ok(vec![self.capabilities().await?.model])
    }

    /// Starts a run with the user's message, in `session` (Hermes keeps its history there). Returns the run's id.
    pub async fn start(&self, input: &str, session: &str, instructions: &str) -> Result<String, String> {
        let body = json!({ "input": input, "session_id": session, "instructions": instructions });
        let answer = self.post("v1/runs", &body).await.map_err(|e| e.message)?;
        answer["run_id"].as_str().filter(|id| valid_run_id(id)).map(str::to_string).ok_or_else(not_hermes)
    }

    /// The run's events from the one after `after` (all of them when None).
    pub async fn events(&self, run: &str, after: Option<u64>) -> Result<reqwest::Response, String> {
        let client = net::client(&self.base, STREAM_TIMEOUT)?;
        let mut request = client.get(net::join(&self.base, &format!("v1/runs/{run}/events"))).header("Accept", "text/event-stream");
        if let Some(seq) = after {
            request = request.header("Last-Event-ID", seq.to_string());
        }
        self.send(request).await.map_err(|e| e.message)
    }

    /// The person's answer to the action the run waits on. False when nothing waits any more (Hermes stopped waiting).
    pub async fn answer(&self, run: &str, request_id: Option<&str>, allow: bool) -> Result<bool, String> {
        let mut body = json!({ "choice": if allow { "once" } else { "deny" } });
        if let Some(id) = request_id {
            body["request_id"] = json!(id);
        }
        match self.post(&format!("v1/runs/{run}/approval"), &body).await {
            Ok(_) => Ok(true),
            Err(Refused { status: Some(409), .. }) => Ok(false),
            Err(e) => Err(e.message),
        }
    }

    pub async fn stop(&self, run: &str) -> Result<(), String> {
        self.post(&format!("v1/runs/{run}/stop"), &json!({})).await.map(|_| ()).map_err(|e| e.message)
    }

    /// Where the run is, for when its stream broke.
    pub async fn status(&self, run: &str) -> Result<Value, String> {
        self.get(&format!("v1/runs/{run}")).await.map_err(|e| e.message)
    }
}

// ── The run's events ──────────────────────────────────────────────────────────

/// What Hermes waits on a person for.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    /// Hermes' own id for this question, sent back with the answer.
    pub request_id: Option<String>,
    /// What would run, as Hermes shows it (its secrets already hidden by Hermes).
    pub command: String,
    /// Why it asks ("recursive delete", "outside the workspace"…).
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// More of the answer.
    Text(String),
    ToolStarted { tool: String, preview: String },
    ToolDone { tool: String, error: bool },
    /// Hermes waits for a person.
    Approval(Ask),
    /// An approval was answered.
    Answered { request_id: Option<String> },
    Completed(String),
    Failed(String),
    /// Stopped, or Hermes shut down during the run.
    Stopped,
    /// The stream is gone and the run's end is unknown.
    Lost(String),
    Other,
}

impl Event {
    pub fn ends_run(&self) -> bool {
        matches!(self, Event::Completed(_) | Event::Failed(_) | Event::Stopped | Event::Lost(_))
    }
}

fn text_of(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or("").to_string()
}

/// One event of the run's stream, and its place in the stream.
pub fn parse_event(v: &Value) -> (Option<u64>, Event) {
    let seq = v["seq"].as_u64();
    let event = match v["event"].as_str().unwrap_or("") {
        "message.delta" => Event::Text(text_of(v, "delta")),
        "tool.started" => Event::ToolStarted { tool: text_of(v, "tool"), preview: text_of(v, "preview") },
        "tool.completed" => Event::ToolDone { tool: text_of(v, "tool"), error: v["error"] == true },
        "approval.request" => Event::Approval(Ask {
            request_id: v["request_id"].as_str().map(str::to_string),
            command: text_of(v, "command"),
            description: text_of(v, "description"),
        }),
        "approval.responded" => Event::Answered { request_id: v["request_id"].as_str().map(str::to_string) },
        "run.completed" => Event::Completed(text_of(v, "output")),
        "run.failed" => Event::Failed(text_of(v, "error")),
        "run.cancelled" | "run.interrupted" => Event::Stopped,
        _ => Event::Other,
    };
    (seq, event)
}

/// The end of a run from its status (GET /v1/runs/{id}), when it has ended.
pub fn ended(status: &Value) -> Option<Event> {
    match status["status"].as_str()? {
        "completed" => Some(Event::Completed(text_of(status, "output"))),
        "failed" => Some(Event::Failed(text_of(status, "error"))),
        "cancelled" | "interrupted" => Some(Event::Stopped),
        _ => None,
    }
}

/// Cuts a server-sent stream into its `data:` payloads, whole lines only (a chunk may end in the middle of one).
#[derive(Default)]
pub struct Frames {
    pending: Vec<u8>,
}

impl Frames {
    pub fn feed(&mut self, chunk: &[u8]) -> Result<Vec<Value>, String> {
        self.pending.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            if let Some(payload) = line.trim_end().strip_prefix("data:") {
                if let Ok(v) = serde_json::from_str::<Value>(payload.trim_start()) {
                    out.push(v);
                }
            }
        }
        if self.pending.len() > MAX_LINE {
            return Err(t("The server's answer is too large."));
        }
        Ok(out)
    }
}

/// Follows a run until it ends, sending each event on `tx`; reopens the stream where it broke, and asks the run's
/// status when it cannot. The last event sent always ends the run.
pub async fn follow(client: Client, run: String, tx: tokio::sync::mpsc::UnboundedSender<Event>) {
    const ATTEMPTS: usize = 4;
    let mut after: Option<u64> = None;
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        if let Ok(mut response) = client.events(&run, after).await {
            let mut frames = Frames::default();
            while let Ok(Ok(Some(chunk))) = tokio::time::timeout(SILENCE, response.chunk()).await {
                let Ok(values) = frames.feed(&chunk) else { break };
                for v in values {
                    let (seq, event) = parse_event(&v);
                    if seq.is_some() {
                        after = seq;
                    }
                    let last = event.ends_run();
                    if tx.send(event).is_err() || last {
                        return;
                    }
                }
            }
        }
        // The stream ended before the run did, as far as it said: where is the run?
        if let Ok(status) = client.status(&run).await {
            if let Some(event) = ended(&status) {
                let _ = tx.send(event);
                return;
            }
        }
    }
    let _ = tx.send(Event::Lost(t("Hermes stopped answering before the end of its work.")));
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    /// One request the simulated Hermes received.
    #[derive(Debug, Clone)]
    pub struct Seen {
        pub method: String,
        pub path: String,
        pub authorization: Option<String>,
        pub last_event_id: Option<String>,
        pub body: String,
    }

    /// What the simulated Hermes does, request by request.
    pub enum Reply {
        Json(u16, Value),
        /// A stream of events: each frame is written, then the connection closes.
        Events(Vec<Value>),
        /// Frames written at once, then more frames once `approved` is set (by a POST …/approval), then the end.
        EventsUntilAnswer(Vec<Value>, Vec<Value>),
    }

    /// A Hermes API server on loopback. `respond` answers each request; a thread per connection, so a stream can stay
    /// open while an approval comes in on another connection.
    pub fn fake_hermes(respond: impl Fn(&Seen) -> Reply + Send + Sync + 'static) -> (Url, Arc<Mutex<Vec<Seen>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let answered = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let respond = Arc::new(respond);
        let log = seen.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(conn) = conn else { break };
                let (respond, log, answered) = (respond.clone(), log.clone(), answered.clone());
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(conn.try_clone().unwrap());
                    let mut first = String::new();
                    if reader.read_line(&mut first).is_err() || first.is_empty() {
                        return;
                    }
                    let mut parts = first.split_whitespace();
                    let (method, path) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
                    let (mut length, mut authorization, mut last_event_id) = (0usize, None, None);
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty() {
                            break;
                        }
                        let lower = line.to_ascii_lowercase();
                        if let Some(v) = lower.strip_prefix("content-length:") {
                            length = v.trim().parse().unwrap_or(0);
                        }
                        if lower.starts_with("authorization:") {
                            authorization = Some(line["authorization:".len()..].trim().to_string());
                        }
                        if lower.starts_with("last-event-id:") {
                            last_event_id = Some(line["last-event-id:".len()..].trim().to_string());
                        }
                    }
                    let mut body = vec![0; length];
                    let _ = reader.read_exact(&mut body);
                    let request = Seen { method, path, authorization, last_event_id, body: String::from_utf8_lossy(&body).to_string() };
                    log.lock().unwrap().push(request.clone());
                    if request.path.ends_with("/approval") {
                        *answered.0.lock().unwrap() = true;
                        answered.1.notify_all();
                    }
                    let mut conn = conn;
                    let frame = |v: &Value| format!("id: {}\ndata: {}\n\n", v["seq"], v);
                    match respond(&request) {
                        Reply::Json(status, value) => {
                            let text = value.to_string();
                            let _ = write!(conn, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
                        }
                        Reply::Events(frames) => {
                            let _ = write!(conn, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n: open\n\n");
                            for v in &frames {
                                let _ = conn.write_all(frame(v).as_bytes());
                                let _ = conn.flush();
                            }
                        }
                        Reply::EventsUntilAnswer(before, after) => {
                            let _ = write!(conn, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n: open\n\n");
                            for v in &before {
                                let _ = conn.write_all(frame(v).as_bytes());
                            }
                            let _ = conn.flush();
                            let (lock, cv) = &*answered;
                            let mut done = lock.lock().unwrap();
                            while !*done {
                                let (next, timeout) = cv.wait_timeout(done, Duration::from_secs(10)).unwrap();
                                done = next;
                                if timeout.timed_out() {
                                    break;
                                }
                            }
                            drop(done);
                            let _ = conn.write_all(b": keepalive\n\n");
                            for v in &after {
                                let _ = conn.write_all(frame(v).as_bytes());
                            }
                            let _ = conn.flush();
                        }
                    }
                });
            }
        });
        (Url::parse(&format!("http://127.0.0.1:{port}")).unwrap(), seen)
    }

    /// The capabilities a current Hermes answers with.
    pub fn capabilities_json() -> Value {
        json!({
            "object": "hermes.api_server.capabilities", "platform": "hermes-agent", "model": "zheus",
            "auth": {"type": "bearer", "required": true},
            "features": {"run_submission": true, "run_events_sse": true, "run_stop": true, "run_approval_response": true, "approval_events": true}
        })
    }

    pub fn event(seq: u64, name: &str, fields: Value) -> Value {
        let mut v = json!({ "event": name, "run_id": "run_abc", "timestamp": 1.0, "seq": seq });
        for (k, x) in fields.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    }

    #[test]
    fn the_key_is_only_ever_sent_to_the_address_it_was_entered_for() {
        let at = |s: &str| Url::parse(s).unwrap();
        let stored = serde_json::to_string(&BoundKey { url: "http://127.0.0.1:8642".into(), key: "k".into() }).unwrap();
        assert_eq!(bound_key(&stored, &at("http://127.0.0.1:8642/")).as_deref(), Some("k"));
        assert_eq!(bound_key(&stored, &at("http://127.0.0.1:8643")), None, "another port");
        let remote = serde_json::to_string(&BoundKey { url: "http://192.168.0.5:8642".into(), key: "k".into() }).unwrap();
        assert_eq!(bound_key(&remote, &at("http://192.168.0.5:8642")), None, "never in clear text to another machine");
        let https = serde_json::to_string(&BoundKey { url: "https://hermes.example.com".into(), key: "k".into() }).unwrap();
        assert_eq!(bound_key(&https, &at("https://hermes.example.com")).as_deref(), Some("k"));
        assert_eq!(bound_key("not json", &at("http://127.0.0.1:8642")), None);
        assert!(set_key("http://192.168.0.5:8642", "k").unwrap_err().contains("https"));
    }

    #[test]
    fn only_a_hermes_that_can_run_stream_stop_and_wait_for_approval_is_taken() {
        assert_eq!(parse_capabilities(&capabilities_json()), Ok(Capabilities { model: "zheus".into() }));
        let mut old = capabilities_json();
        old["features"]["run_approval_response"] = json!(false);
        assert!(parse_capabilities(&old).unwrap_err().contains("hermes update"));
        assert!(parse_capabilities(&json!({"object": "list", "data": []})).is_err(), "an OpenAI-compatible server is not Hermes");
        let mut unnamed = capabilities_json();
        unnamed["model"] = json!("");
        assert_eq!(parse_capabilities(&unnamed).unwrap().model, "hermes-agent");
    }

    #[test]
    fn events_are_read_from_whole_lines_whatever_the_chunks() {
        let mut frames = Frames::default();
        let stream = ": open\n\nid: 0\ndata: {\"event\":\"message.delta\",\"delta\":\"Ol\\u00e1\",\"seq\":0}\n\n: keepalive\n\nid: 1\ndata: {\"event\":\"run.completed\",\"output\":\"Ol\\u00e1!\",\"seq\":1}\n\n";
        let mut values = Vec::new();
        for chunk in stream.as_bytes().chunks(7) {
            values.extend(frames.feed(chunk).unwrap());
        }
        let events: Vec<_> = values.iter().map(parse_event).collect();
        assert_eq!(events, vec![(Some(0), Event::Text("Olá".into())), (Some(1), Event::Completed("Olá!".into()))]);
        assert!(Frames::default().feed(&vec![b'x'; MAX_LINE + 1]).is_err(), "an endless line is cut off");
    }

    #[test]
    fn each_event_hermes_sends_means_one_thing() {
        let e = |name: &str, fields: Value| parse_event(&event(3, name, fields)).1;
        assert_eq!(e("tool.started", json!({"tool": "spotify_playback", "preview": "play"})), Event::ToolStarted { tool: "spotify_playback".into(), preview: "play".into() });
        assert_eq!(e("tool.completed", json!({"tool": "terminal", "duration": 1.5, "error": true})), Event::ToolDone { tool: "terminal".into(), error: true });
        assert_eq!(
            e("approval.request", json!({"command": "rm -rf build", "description": "recursive delete", "request_id": "r1", "choices": ["once", "deny"]})),
            Event::Approval(Ask { request_id: Some("r1".into()), command: "rm -rf build".into(), description: "recursive delete".into() })
        );
        assert_eq!(e("approval.responded", json!({"choice": "once", "request_id": "r1"})), Event::Answered { request_id: Some("r1".into()) });
        assert_eq!(e("run.failed", json!({"error": "no model"})), Event::Failed("no model".into()));
        assert_eq!(e("run.interrupted", json!({})), Event::Stopped);
        assert_eq!(e("reasoning.available", json!({"text": "hmm"})), Event::Other, "Hermes' thinking stays out of the chat");
        assert_eq!(ended(&json!({"status": "completed", "output": "ok"})), Some(Event::Completed("ok".into())));
        assert_eq!(ended(&json!({"status": "waiting_for_approval"})), None);
        assert!(valid_run_id("run_0a1b") && !valid_run_id("../x") && !valid_run_id(""));
    }

    #[test]
    fn a_run_is_started_followed_answered_and_stopped_with_the_key() {
        let (base, seen) = fake_hermes(|r| match (r.method.as_str(), r.path.as_str()) {
            ("GET", "/v1/capabilities") => Reply::Json(200, capabilities_json()),
            ("POST", "/v1/runs") => Reply::Json(202, json!({"run_id": "run_abc", "status": "started"})),
            ("GET", "/v1/runs/run_abc/events") => Reply::Events(vec![
                event(0, "tool.started", json!({"tool": "google_calendar", "preview": "today"})),
                event(1, "message.delta", json!({"delta": "Hoje: "})),
                event(2, "run.completed", json!({"output": "Hoje: dentista às 15h."})),
            ]),
            ("POST", "/v1/runs/run_abc/approval") => Reply::Json(409, json!({"error": {"message": "Run has no pending approval"}})),
            ("POST", "/v1/runs/run_abc/stop") => Reply::Json(200, json!({"status": "stopping"})),
            _ => Reply::Json(404, json!({})),
        });
        let client = Client::new(base, "secret-key".into());
        assert_eq!(block_on(client.capabilities()).unwrap().model, "zheus");
        let run = block_on(client.start("Como está minha agenda hoje?", "coucou-1", "be brief")).unwrap();
        assert_eq!(run, "run_abc");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        block_on(follow(client.clone(), run.clone(), tx));
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        assert_eq!(events.last(), Some(&Event::Completed("Hoje: dentista às 15h.".into())));
        assert_eq!(events.len(), 3);
        assert_eq!(block_on(client.answer(&run, Some("r1"), true)), Ok(false), "nothing waits any more");
        assert_eq!(block_on(client.stop(&run)), Ok(()));
        let seen = seen.lock().unwrap();
        assert!(seen.iter().all(|r| r.authorization.as_deref() == Some("Bearer secret-key")));
        let start: Value = serde_json::from_str(&seen[1].body).unwrap();
        assert_eq!(start, json!({"input": "Como está minha agenda hoje?", "session_id": "coucou-1", "instructions": "be brief"}));
        let answer: Value = serde_json::from_str(&seen[3].body).unwrap();
        assert_eq!(answer, json!({"choice": "once", "request_id": "r1"}));
    }

    #[test]
    fn a_broken_stream_is_reopened_after_the_last_event_and_the_status_settles_it() {
        let status_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls = status_calls.clone();
        let (base, seen) = fake_hermes(move |r| match (r.path.as_str(), r.last_event_id.as_deref()) {
            ("/v1/runs/run_abc/events", None) => Reply::Events(vec![event(0, "message.delta", json!({"delta": "Já "}))]),
            ("/v1/runs/run_abc/events", Some(_)) => Reply::Events(vec![event(1, "message.delta", json!({"delta": "vai"}))]),
            ("/v1/runs/run_abc", _) if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 => {
                Reply::Json(200, json!({"object": "hermes.run", "status": "running"}))
            }
            ("/v1/runs/run_abc", _) => Reply::Json(200, json!({"object": "hermes.run", "status": "completed", "output": "Já vai."})),
            _ => Reply::Json(404, json!({})),
        });
        let client = Client::new(base, "k".into());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        block_on(follow(client, "run_abc".into(), tx));
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        assert_eq!(events, vec![Event::Text("Já ".into()), Event::Text("vai".into()), Event::Completed("Já vai.".into())]);
        let seen = seen.lock().unwrap();
        let reopened: Vec<_> = seen.iter().filter(|r| r.path.ends_with("/events")).map(|r| r.last_event_id.clone()).collect();
        assert_eq!(reopened, vec![None, Some("0".to_string())], "reopened after the last event it had");
        assert_eq!(status_calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn a_server_that_is_off_or_refuses_the_key_says_so() {
        let client = Client::new(Url::parse("http://127.0.0.1:9").unwrap(), "k".into());
        assert!(block_on(client.capabilities()).unwrap_err().contains("hermes gateway"));
        let (base, _) = fake_hermes(|_| Reply::Json(401, json!({"error": {"message": "Invalid gateway API key (API_SERVER_KEY)"}})));
        assert!(block_on(Client::new(base, "wrong".into()).capabilities()).unwrap_err().contains("API_SERVER_KEY"));
        let (base, _) = fake_hermes(|_| Reply::Json(404, json!({"error": "not found"})));
        assert!(block_on(Client::new(base, "k".into()).capabilities()).unwrap_err().contains("hermes update"));
    }
}
