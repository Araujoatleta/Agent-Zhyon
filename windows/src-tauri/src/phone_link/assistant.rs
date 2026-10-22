// The assistant from the phone (docs/ANDROID_LINK.md, "The assistant"): Zheus, Hermes Agent as Coucou sets it up and
// runs it (hermes_setup.rs) — its name and tone, its brain, its skill packs, the ChatGPT sign-in, and whether Coucou
// runs it. The phone is shown what Settings → Hermes Agent shows, and changes it through the same calls
// (`AssistantBackend`, a trait so the tests use a fake).
//
// What it enforces, whatever the phone sends:
//   * the connection asked for the capability while the user's switch was on, and the switch is read again for every
//     request (`off`);
//   * every request carries the phone's own id and is answered once with an `assistantResult`; one without a usable
//     id is ignored, and so is a type this computer does not know;
//   * a name is one line of at most 40 characters, a tone one of the known ones, the user's own tone at most 1500
//     characters, the skill packs known ones and never Mochi's home, which stays Settings' own switch (`invalid`);
//   * a brain and the skill packs only for a Hermes that Coucou installed and runs (`not_installed`): Hermes is never
//     installed from the phone. A brain only once it is ready, its key or its server known here (`not_ready`);
//   * at most 20 requests per 10 minutes per connection (`rate`), carried out one after the other, off the read loop;
//     one that waited while the switch went off is refused then.
// What the phone is told never holds a path, Hermes' address, a key, the installer's address, the account's e-mail or
// the installer's own output; Hermes' words when it stopped or failed are cleaned first (guard.rs).

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::guard::{clean_line, request_id, Rate};
use super::hub::{Hub, Out};
use super::server::Features;
use crate::hermes_setup::{self, GatewayState, LoginState, SetupStatus};

/// Requests that change something, per connection and per RATE_WINDOW_MS.
pub const MAX_REQUESTS: usize = 20;
/// `assistantGet`, per connection and per RATE_WINDOW_MS: it has no id to be refused with, so more go unanswered.
pub const MAX_GETS: usize = 60;
pub const RATE_WINDOW_MS: u64 = 10 * 60 * 1000;
/// The phones hear of a change at most every half second (an installation reports every step).
pub const MIN_GAP_MS: u64 = 500;
/// Hermes' and the setup's own words, at most this long.
const MAX_WORDS: usize = 200;
/// Requests of one connection waiting their turn.
const QUEUE: usize = 8;

/// What the phone may read and change of the assistant. The app's implementation (mod.rs) makes the calls Settings →
/// Hermes Agent makes.
pub trait AssistantBackend: Send + Sync {
    /// How Zheus is now. It reads the disk and the keystore: only ever called on a blocking thread.
    fn status(&self) -> SetupStatus;
    /// The name, the tone and the user's own tone; what is None stays as it is.
    fn set(&self, name: Option<String>, persona: Option<String>, persona_text: Option<String>) -> Result<(), String>;
    /// Sets Hermes up again with this brain, in the background. A ChatGPT sign-in shows its code in the status; the
    /// computer's browser stays closed.
    fn brain(&self, brain: &str) -> Result<(), String>;
    fn skills(&self, skills: Vec<String>) -> Result<(), String>;
    /// Starts the ChatGPT sign-in without opening the computer's browser.
    fn login(&self) -> Result<(), String>;
    fn login_cancel(&self);
    /// Coucou runs Hermes (on), or not (its gateway stops; Hermes stays installed).
    fn gateway(&self, on: bool);
}

/// What a phone asks, checked for its form (how Zheus is now is checked when its turn comes).
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Set { name: Option<String>, persona: Option<String>, persona_text: Option<String> },
    Brain(String),
    Skills(Vec<String>),
    Login,
    LoginCancel,
    Gateway(bool),
}

/// The request in `msg`: None for a type this computer does not know (ignored), Err for one whose fields are wrong.
pub fn parse(msg: &Value) -> Option<Result<Request, ()>> {
    Some(match msg["type"].as_str()? {
        "assistantSet" => parse_set(msg),
        "assistantBrain" => msg["brain"].as_str().filter(|b| hermes_setup::brain(b).is_some()).map(|b| Request::Brain(b.to_string())).ok_or(()),
        "assistantSkills" => msg["skills"]
            .as_array()
            .filter(|list| list.len() <= hermes_setup::SKILL_PACKS.len())
            .and_then(|list| {
                // Mochi's home is not one of them: it is Settings' own switch, read-only on the phone.
                list.iter().map(|s| s.as_str().filter(|s| *s != "home" && hermes_setup::SKILL_PACKS.contains(s)).map(str::to_string)).collect()
            })
            .map(Request::Skills)
            .ok_or(()),
        "assistantLogin" => Ok(Request::Login),
        "assistantLoginCancel" => Ok(Request::LoginCancel),
        "assistantGateway" => msg["on"].as_bool().map(Request::Gateway).ok_or(()),
        _ => return None,
    })
}

/// `assistantSet`: at least one of the three, each as Settings would take it.
fn parse_set(msg: &Value) -> Result<Request, ()> {
    // Absent (or null) stays as it is; anything but a string is not a name or a tone.
    let field = |key: &str| -> Result<Option<String>, ()> {
        match msg.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(()),
        }
    };
    let name = match field("name")? {
        Some(typed) => Some(hermes_setup::valid_name(&typed).ok_or(())?),
        None => None,
    };
    let persona = field("persona")?;
    if persona.as_deref().is_some_and(|p| !hermes_setup::PERSONAS.contains(&p)) {
        return Err(());
    }
    // The user's own tone keeps its lines; nothing else that is not text.
    let persona_text = field("personaText")?;
    if persona_text.as_deref().is_some_and(|own| {
        own.chars().count() > hermes_setup::MAX_PERSONA_TEXT || own.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    }) {
        return Err(());
    }
    if name.is_none() && persona.is_none() && persona_text.is_none() {
        return Err(());
    }
    Ok(Request::Set { name, persona, persona_text })
}

/// The fixed message of each answer's code: never what Hermes or the setup said.
pub fn reason(code: &str) -> &'static str {
    match code {
        "off" => "Changing the assistant from the phone is turned off on the computer.",
        "invalid" => "The computer cannot take this change.",
        "not_installed" => "AXYON does not install and run AXYON Core on the computer yet: set it up there first.",
        "not_ready" => "The computer has no key or server address for that brain yet.",
        "busy" => "The computer is still busy with the assistant. Try again in a moment.",
        "rate" => "Too many changes. Try again in a few minutes.",
        _ => "It did not go through on the computer.",
    }
}

fn result_line(id: &str, outcome: Result<(), &str>) -> Out {
    let v = match outcome {
        Ok(()) => json!({ "type": "assistantResult", "id": id, "ok": true }),
        Err(code) => json!({ "type": "assistantResult", "id": id, "ok": false, "reason": code, "message": reason(code) }),
    };
    Out::Line(v.to_string().into())
}

/// The `assistant` line: what Settings → Hermes Agent shows, as far as the phone uses it.
pub fn assistant_line(s: &SetupStatus) -> String {
    // Where Hermes is, where it listens and where its installer comes from are blanked wherever they turn up, and so is
    // anything else that looks like an address, a path or a key (guard.rs).
    let hidden = [s.home.clone(), s.address.clone(), s.installer.clone()];
    let words = |text: &str| clean_line(text, MAX_WORDS, &hidden);
    let (gateway, why) = match &s.gateway {
        GatewayState::Off => ("off", String::new()),
        GatewayState::Starting => ("starting", String::new()),
        GatewayState::Running => ("running", String::new()),
        GatewayState::Elsewhere => ("elsewhere", String::new()),
        GatewayState::Stopped { reason } => ("stopped", words(reason)),
    };
    let login = match &s.login {
        LoginState::Unknown => json!({ "state": "unknown" }),
        LoginState::SignedOut => json!({ "state": "signedOut" }),
        LoginState::Waiting { code, url, expires_at } => json!({
            "state": "waiting",
            "code": code.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(32).collect::<String>(),
            // OpenAI's own page only, whatever was printed.
            "url": if url.starts_with("https://auth.openai.com/") { url.as_str() } else { hermes_setup::LOGIN_URL },
            "expiresAt": expires_at,
        }),
        LoginState::SignedIn => json!({ "state": "signedIn" }),
        LoginState::Failed { reason } => json!({ "state": "failed", "reason": words(reason) }),
    };
    let p = &s.progress;
    json!({
        "type": "assistant",
        "name": s.assistant.name,
        "persona": s.assistant.persona,
        "personaText": s.assistant.persona_text.chars().take(hermes_setup::MAX_PERSONA_TEXT).collect::<String>(),
        "personas": s.personas,
        "brain": if hermes_setup::brain(&s.brain).is_some() { s.brain.as_str() } else { "" },
        "brains": s.brains.iter().map(|b| json!({ "id": b.id, "name": b.name, "ready": b.ready, "login": b.login })).collect::<Vec<_>>(),
        "skills": s.skills,
        "skillPacks": s.skill_packs,
        "house": s.house,
        "hermes": {
            "installed": s.installed,
            "managed": s.managed,
            "gateway": gateway,
            "reason": why,
            "setup": {
                "running": p.running,
                "step": words(&p.step),
                "done": p.done,
                "total": p.total,
                "error": p.error.as_deref().map(words).unwrap_or_default(),
            },
        },
        "login": login,
    })
    .to_string()
}

/// Checks the request against Zheus as it is now, then makes the change: Ok, or the code of the refusal or failure.
fn carry_out(backend: &dyn AssistantBackend, request: Request) -> Result<(), &'static str> {
    let status = backend.status();
    let ours = status.installed && status.managed;
    let failed = |_: String| "failed";
    match request {
        Request::Set { name, persona, persona_text } => backend.set(name, persona, persona_text).map_err(failed),
        Request::Brain(brain) => {
            // Never an installation from the phone: only a Hermes Coucou already installed and runs gets a new brain.
            if !ours {
                return Err("not_installed");
            }
            if !status.brains.iter().any(|b| b.id == brain && b.ready) {
                return Err("not_ready");
            }
            if status.progress.running {
                return Err("busy");
            }
            backend.brain(&brain).map_err(failed)
        }
        Request::Skills(skills) => {
            if !ours {
                return Err("not_installed");
            }
            backend.skills(skills).map_err(failed)
        }
        Request::Login => {
            if !status.installed {
                return Err("not_installed");
            }
            // One sign-in at a time. One that already shows its code is the one the phone wants: the code is in the
            // `assistant` line.
            if matches!(status.login, LoginState::Waiting { .. }) {
                return Ok(());
            }
            if status.progress.running {
                return Err("busy");
            }
            backend.login().map_err(failed)
        }
        Request::LoginCancel => {
            backend.login_cancel();
            Ok(())
        }
        Request::Gateway(on) => {
            if !status.installed {
                return Err("not_installed");
            }
            if status.progress.running {
                return Err("busy");
            }
            backend.gateway(on);
            Ok(())
        }
    }
}

// ── The phones that negotiated the capability ─────────────────────────────────

pub struct AssistantLink {
    backend: Arc<dyn AssistantBackend>,
    /// How often the phones hear of a change.
    throttle: Mutex<Throttle>,
}

/// One connection's side: its own line of requests (started with the first, ended with the connection), and how many
/// it sent lately.
#[derive(Default)]
pub struct AssistantConn {
    jobs: Option<mpsc::Sender<Job>>,
    requests: Rate,
    gets: Rate,
}

enum Job {
    /// The assistant as it is now, to this phone: its first picture, or the one it asked for.
    Show,
    Do { id: String, request: Request },
}

impl AssistantLink {
    pub fn new(backend: Arc<dyn AssistantBackend>) -> Arc<AssistantLink> {
        Arc::new(AssistantLink { backend, throttle: Mutex::new(Throttle::default()) })
    }

    /// The phone was just offered the capability: its first picture, after its first `sessions`.
    pub fn greet(&self, conn: &mut AssistantConn, sub: u64, tx: &mpsc::Sender<Out>, hub: &Arc<Hub>, features: &Arc<dyn Features>) {
        self.queue(conn, sub, tx, hub, features, Job::Show);
    }

    /// One `assistant…` message of a phone that negotiated the capability (connection `sub`). Never blocks: the work is
    /// done on the connection's own task, one request after the other. The user's switch is read now, and again when
    /// the request's turn comes.
    #[allow(clippy::too_many_arguments)]
    pub fn handle(&self, conn: &mut AssistantConn, sub: u64, tx: &mpsc::Sender<Out>, hub: &Arc<Hub>, features: &Arc<dyn Features>, msg: &Value, now: u64) {
        let on = features.assistant();
        if msg["type"] == "assistantGet" {
            if on && conn.gets.allow(now, MAX_GETS, RATE_WINDOW_MS) {
                self.queue(conn, sub, tx, hub, features, Job::Show);
            }
            return;
        }
        let Some(parsed) = parse(msg) else { return };
        // Without a usable id there is nothing to answer to.
        let Some(id) = request_id(msg) else { return };
        let refuse = |code: &str| {
            let _ = tx.try_send(result_line(id, Err(code)));
        };
        if !on {
            return refuse("off");
        }
        let Ok(request) = parsed else { return refuse("invalid") };
        if !conn.requests.allow(now, MAX_REQUESTS, RATE_WINDOW_MS) {
            return refuse("rate");
        }
        if !self.queue(conn, sub, tx, hub, features, Job::Do { id: id.to_string(), request }) {
            refuse("busy");
        }
    }

    /// Puts a job on the connection's line, which starts with its first job. False when the line is full.
    fn queue(&self, conn: &mut AssistantConn, sub: u64, tx: &mpsc::Sender<Out>, hub: &Arc<Hub>, features: &Arc<dyn Features>, job: Job) -> bool {
        let jobs = conn.jobs.get_or_insert_with(|| {
            let (jobs, line) = mpsc::channel(QUEUE);
            tokio::spawn(work(self.backend.clone(), line, sub, tx.clone(), hub.clone(), features.clone()));
            jobs
        });
        jobs.try_send(job).is_ok()
    }

    /// Zheus changed (hermes_setup.rs): `line` goes to every phone that has the capability now, or once MIN_GAP_MS
    /// has gone by since the last one. Some(wait): a send is planned, and `due` must be called that many ms later.
    pub fn changed(&self, hub: &Hub, line: String, now: u64) -> Option<u64> {
        let next = self.throttle.lock().unwrap().offer(line, now);
        match next {
            Next::Now(line) => {
                hub.publish_assistant(line.into());
                None
            }
            Next::Later(wait) => Some(wait),
            Next::Planned => None,
        }
    }

    /// The send `changed` planned: the latest picture.
    pub fn due(&self, hub: &Hub, now: u64) {
        let line = self.throttle.lock().unwrap().due(now);
        if let Some(line) = line {
            hub.publish_assistant(line.into());
        }
    }
}

/// One connection's requests, one after the other, each on a blocking thread (they read the disk and the keystore, and
/// some start processes). What arrived is carried out even if the phone left meanwhile, unless the switch went off;
/// the line ends with the connection.
async fn work(backend: Arc<dyn AssistantBackend>, mut line: mpsc::Receiver<Job>, sub: u64, tx: mpsc::Sender<Out>, hub: Arc<Hub>, features: Arc<dyn Features>) {
    while let Some(job) = line.recv().await {
        let backend = backend.clone();
        match job {
            Job::Show => {
                if !features.assistant() {
                    continue;
                }
                if let Ok(status) = tokio::task::spawn_blocking(move || backend.status()).await {
                    hub.send_assistant(sub, assistant_line(&status).into());
                }
            }
            Job::Do { id, request } => {
                let outcome = if features.assistant() {
                    tokio::task::spawn_blocking(move || carry_out(&*backend, request)).await.unwrap_or(Err("failed"))
                } else {
                    Err("off")
                };
                let _ = tx.send(result_line(&id, outcome)).await;
            }
        }
    }
}

// ── How often the phones hear of a change ─────────────────────────────────────

/// Keeps the phones to at most one `assistant` line every MIN_GAP_MS while Zheus changes quickly, the latest picture
/// always sent in the end.
#[derive(Debug, Default)]
pub struct Throttle {
    last: Option<u64>,
    waiting: Option<String>,
    planned: bool,
}

/// What to do with a new picture.
#[derive(Debug, PartialEq, Eq)]
pub enum Next {
    /// Send it now.
    Now(String),
    /// Send what waits this many ms from now (`due` says what).
    Later(u64),
    /// A send is planned already: it will carry this picture.
    Planned,
}

impl Throttle {
    pub fn offer(&mut self, line: String, now: u64) -> Next {
        if self.planned {
            self.waiting = Some(line);
            return Next::Planned;
        }
        match self.last {
            Some(last) if now.saturating_sub(last) < MIN_GAP_MS => {
                self.waiting = Some(line);
                self.planned = true;
                Next::Later(MIN_GAP_MS - now.saturating_sub(last))
            }
            _ => {
                self.last = Some(now);
                Next::Now(line)
            }
        }
    }

    /// The planned send is due: the latest picture.
    pub fn due(&mut self, now: u64) -> Option<String> {
        self.planned = false;
        let line = self.waiting.take()?;
        self.last = Some(now);
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_of_changes_is_two_lines_a_second_at_most_and_the_last_one_is_always_sent() {
        let mut t = Throttle::default();
        assert_eq!(t.offer("a".into(), 1_000), Next::Now("a".into()));
        assert_eq!(t.offer("b".into(), 1_100), Next::Later(400));
        assert_eq!(t.offer("c".into(), 1_200), Next::Planned);
        assert_eq!(t.offer("d".into(), 1_300), Next::Planned);
        assert_eq!(t.due(1_500), Some("d".into()), "the latest picture, not the first one that waited");
        assert_eq!(t.offer("e".into(), 1_600), Next::Later(400));
        assert_eq!(t.due(2_000), Some("e".into()));
        assert_eq!(t.due(2_100), None, "nothing waits");
        assert_eq!(t.offer("f".into(), 3_000), Next::Now("f".into()), "after a quiet half second, at once");
    }

    #[test]
    fn requests_are_read_as_settings_would_take_them() {
        let set = |mut fields: Value| {
            fields["type"] = json!("assistantSet");
            parse(&fields)
        };
        assert_eq!(set(json!({ "name": "  Athena " })), Some(Ok(Request::Set { name: Some("Athena".into()), persona: None, persona_text: None })));
        assert_eq!(set(json!({ "name": "" })), Some(Ok(Request::Set { name: Some(String::new()), persona: None, persona_text: None })), "empty is the default name");
        assert_eq!(set(json!({ "persona": "butler", "personaText": null })), Some(Ok(Request::Set { name: None, persona: Some("butler".into()), persona_text: None })));
        let own = "Short answers.\nCall me Ana.".to_string();
        assert_eq!(set(json!({ "persona": "custom", "personaText": own })), Some(Ok(Request::Set { name: None, persona: Some("custom".into()), persona_text: Some(own) })));
        for bad in [
            json!({}),
            json!({ "name": "x".repeat(41) }),
            json!({ "name": "two\nlines" }),
            json!({ "name": 7 }),
            json!({ "persona": "pirate" }),
            json!({ "persona": "Friendly" }),
            json!({ "personaText": "é".repeat(1501) }),
            json!({ "personaText": "a\u{0}b" }),
        ] {
            assert_eq!(set(bad.clone()), Some(Err(())), "{bad}");
        }
        assert!(matches!(set(json!({ "personaText": "é".repeat(1500) })), Some(Ok(_))), "exactly the limit");

        assert_eq!(parse(&json!({ "type": "assistantBrain", "brain": "chatgpt" })), Some(Ok(Request::Brain("chatgpt".into()))));
        for brain in [json!("skynet"), json!(""), json!(1), Value::Null] {
            assert_eq!(parse(&json!({ "type": "assistantBrain", "brain": brain })), Some(Err(())));
        }
        assert_eq!(parse(&json!({ "type": "assistantSkills", "skills": ["search", "news"] })), Some(Ok(Request::Skills(vec!["search".into(), "news".into()]))));
        assert_eq!(parse(&json!({ "type": "assistantSkills", "skills": [] })), Some(Ok(Request::Skills(Vec::new()))), "none: every pack unticked");
        for skills in [json!(["home"]), json!(["search", "weather"]), json!("search"), json!([1]), json!(["search", "news", "search", "news"])] {
            assert_eq!(parse(&json!({ "type": "assistantSkills", "skills": skills })), Some(Err(())), "{skills}");
        }
        assert_eq!(parse(&json!({ "type": "assistantGateway", "on": false })), Some(Ok(Request::Gateway(false))));
        assert_eq!(parse(&json!({ "type": "assistantGateway", "on": "yes" })), Some(Err(())));
        assert_eq!(parse(&json!({ "type": "assistantLogin" })), Some(Ok(Request::Login)));
        assert_eq!(parse(&json!({ "type": "assistantLoginCancel" })), Some(Ok(Request::LoginCancel)));
        assert_eq!(parse(&json!({ "type": "assistantSelfDestruct" })), None, "a type this computer does not know");
    }

    #[test]
    fn every_code_has_a_fixed_message() {
        for code in ["off", "invalid", "not_installed", "not_ready", "busy", "rate", "failed", "whatever"] {
            let m = reason(code);
            assert!(!m.is_empty() && m.len() < 100 && !m.contains("http"), "{code}");
        }
    }
}
