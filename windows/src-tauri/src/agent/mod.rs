// Mochi's hands: the chat can act on this computer — look at files, change them, run commands, read and open web
// pages — like Codex or Claude Code, through whichever provider the chat talks to (Claude, OpenAI, Google AI,
// OpenRouter, Ollama, LM Studio or any compatible server).
//
// Off by default: Settings → Mochi's hands, where the user also picks the folder Mochi works in and how much he may
// do without asking. Looking inside that folder is the only thing that needs no click ("Edit files without asking"
// adds file edits there). Every other change, every command and every web request waits for an explicit click, on
// the island, or on a paired phone behind Face ID or the fingerprint. "Read only" offers no tool that changes
// anything. Keys never reach the model or the island; tool results go only to the provider the user picked.
//
// The pieces: policy.rs (what needs a click), tools.rs (the tools), shell.rs (commands), wire.rs (the providers'
// formats), run.rs (the loop) and text.rs (output and page text). This file holds the one turn that may run at a
// time, the action waiting for a click, the island's commands and the phone's way in (phone_link/chat.rs).

mod policy;
mod run;
mod shell;
mod text;
mod tools;
mod wire;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tokio::sync::{mpsc::UnboundedSender, oneshot};

use crate::chat::{self, Chat, ChatContext, ChatReply};
use crate::i18n::t;
use crate::island::WINDOW_LABEL;
use crate::phone_link::{self, ApprovalIn, PhoneLink, SessionIn, StepLine};
use crate::settings::{self, Settings};
use crate::{claude, local_chat, log, openai_compat, Shared};
use policy::Level;
pub use run::BoxFuture;
use run::{Failed, Hands, Job, Status, Step, Transport};
use shell::Shell;
pub use tools::{HomeEnv, HomeResult, HomeRunner, RoutineInfo};
use tools::{Env, Request};
use wire::Flavor;

/// The pill Mochi's own work shows on, on a phone. Not one of the island's pills: on the island, Mochi is the island.
pub const PILL_ID: &str = "agent_mochi";
const COLOR: &str = "#A5B4FC";
/// Ids of Mochi's actions waiting for a click; a phone's decision with such an id comes here, not to a hook.
const PREFIX: &str = "agent-";
/// How long an action waits for a click before it counts as a no: long at the desk, the phone's own limit when the
/// turn came from the phone (it offers a request for 120 s).
const WAIT_DESK: Duration = Duration::from_secs(10 * 60);
const WAIT_PHONE: Duration = Duration::from_secs(120);
/// After a turn, its "done" stays on the phones this long.
const LINGER: Duration = Duration::from_secs(180);
/// One request to the provider; a long answer (a whole file) takes time.
const TIMEOUT: Duration = Duration::from_secs(600);
const CLAUDE_MAX_TOKENS: u32 = 16_000;
const OTHER_MAX_TOKENS: u32 = 8_192;

/// Where a turn was asked from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Island,
    Phone,
}

/// The one turn with hands that may run at a time, and the action waiting for a click.
#[derive(Default)]
pub struct Agent {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    next: u64,
    run: Option<Current>,
    /// The last turn started, so a finished turn's "done" is taken off the phones only if no other turn began.
    last_run: u64,
    pending: Option<Pending>,
    /// What the phones are shown of Mochi's work (sent again whole on every change).
    phone_session: Option<SessionIn>,
    phone_approval: Option<ApprovalIn>,
}

struct Current {
    id: u64,
    origin: Origin,
    cancel: Arc<AtomicBool>,
}

struct Pending {
    id: String,
    run: u64,
    /// What the island's card shows.
    card: Value,
    tx: oneshot::Sender<bool>,
}

impl Agent {
    /// A new turn, unless one is running.
    fn begin(&self, origin: Origin) -> Option<(u64, Arc<AtomicBool>)> {
        let mut inner = self.inner.lock().unwrap();
        if inner.run.is_some() {
            return None;
        }
        inner.next += 1;
        let id = inner.next;
        let cancel = Arc::new(AtomicBool::new(false));
        inner.run = Some(Current { id, origin, cancel: cancel.clone() });
        inner.last_run = id;
        Some((id, cancel))
    }

    /// The turn is over, however it ended. Returns the id of the action that was still waiting, if any (its sender
    /// goes with it, so the waiting side reads a no).
    fn end(&self, run: u64) -> Option<String> {
        let mut inner = self.inner.lock().unwrap();
        if inner.run.as_ref().is_some_and(|r| r.id == run) {
            inner.run = None;
        }
        match inner.pending.take() {
            Some(p) if p.run == run => Some(p.id),
            other => {
                inner.pending = other;
                None
            }
        }
    }

    /// Registers the action waiting for a click and returns its id.
    fn wait_for(&self, run: u64, mut card: Value, tx: oneshot::Sender<bool>) -> (String, Value) {
        let mut inner = self.inner.lock().unwrap();
        inner.next += 1;
        let id = format!("{PREFIX}{}", inner.next);
        card["id"] = json!(id);
        inner.pending = Some(Pending { id: id.clone(), run, card: card.clone(), tx });
        (id, card)
    }

    fn forget(&self, id: &str) {
        let mut inner = self.inner.lock().unwrap();
        if inner.pending.as_ref().is_some_and(|p| p.id == id) {
            inner.pending = None;
        }
    }

    /// The user's answer. True when it reached the action still waiting; anything else changes nothing.
    pub fn decide(&self, id: &str, allow: bool) -> bool {
        let mut inner = self.inner.lock().unwrap();
        match inner.pending.take() {
            Some(p) if p.id == id => {
                let _ = p.tx.send(allow);
                true
            }
            other => {
                inner.pending = other;
                false
            }
        }
    }

    /// Stop: the command running ends, the action waiting is denied, and the turn ends at its next step.
    pub fn cancel(&self) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let Some(run) = &inner.run else { return false };
        run.cancel.store(true, Ordering::SeqCst);
        if let Some(p) = inner.pending.take() {
            let _ = p.tx.send(false);
        }
        true
    }

    fn running(&self) -> Option<Origin> {
        self.inner.lock().unwrap().run.as_ref().map(|r| r.origin)
    }

    fn card(&self) -> Option<Value> {
        self.inner.lock().unwrap().pending.as_ref().map(|p| p.card.clone())
    }
}

/// True for the id of one of Mochi's actions (a phone's decision about it comes here).
pub fn is_request(id: &str) -> bool {
    id.starts_with(PREFIX)
}

/// A phone allowed or denied one of Mochi's actions (phone_link: only for the action still waiting, by fingerprint).
pub fn decide_from_phone(app: &AppHandle, id: &str, allow: bool) {
    if let Some(agent) = app.try_state::<Agent>() {
        agent.decide(id, allow);
    }
}

// ── What the phones see ───────────────────────────────────────────────────────

fn show_on_phones(app: &AppHandle, session: Option<Option<SessionIn>>, approval: Option<Option<ApprovalIn>>) {
    let Some(agent) = app.try_state::<Agent>() else { return };
    let (session, approval) = {
        let mut inner = agent.inner.lock().unwrap();
        if let Some(s) = session {
            inner.phone_session = s;
        }
        if let Some(a) = approval {
            inner.phone_approval = a;
        }
        (inner.phone_session.clone(), inner.phone_approval.clone())
    };
    if let Some(link) = app.try_state::<PhoneLink>() {
        link.publish_agent(session, approval);
    }
}

/// The name of a tool as the phones (and the island's labels) know it: Claude Code's.
fn alias(tool: &str) -> &'static str {
    match tool {
        "run_command" => "Bash",
        "read_file" => "Read",
        "write_file" => "Write",
        "edit_file" => "Edit",
        "list_directory" => "LS",
        "find_files" => "Glob",
        "search_text" => "Grep",
        "fetch_url" => "WebFetch",
        "open_url" => "Open",
        "system_info" => "Info",
        "home_devices" | "home_control" | "home_routine" => "Home",
        _ => "Tool",
    }
}

/// One step on one line, as a phone shows it: "Bash · npm test", "Edit · src\main.rs (+1 −1)".
fn step_line(step: &Step, folder: &Path, home: &Path) -> String {
    let tool = alias(&step.tool);
    let summary = phone_summary(&step.tool, &step.summary, folder, home);
    if summary.is_empty() {
        tool.to_string()
    } else {
        format!("{tool} · {}", text::one_line(&summary, 180))
    }
}

/// The tools whose summary names a path.
const PATH_TOOLS: &[&str] = &["read_file", "write_file", "edit_file", "list_directory", "find_files", "search_text"];

/// A summary as a phone shows it, with no whole path crossing the link (it names the user, and the island has it):
/// a path in the working folder is written from there ("src\main.rs", the folder itself by its name), one elsewhere
/// in the home folder from "~" ("~\Downloads\notes.txt"). A command is shown as it will run, since that is what
/// Allow is for, and an address as it is.
fn phone_summary(tool: &str, summary: &str, folder: &Path, home: &Path) -> String {
    if !PATH_TOOLS.contains(&tool) {
        return summary.to_string();
    }
    let folder_text = policy::display(folder);
    let name = folder.file_name().map_or_else(|| folder_text.clone(), |n| n.to_string_lossy().into_owned());
    let in_home = format!("~{}", std::path::MAIN_SEPARATOR);
    swap_root(summary, &folder_text, &name, "")
        .or_else(|| swap_root(summary, &policy::display(home), "~", &in_home))
        .unwrap_or_else(|| summary.to_string())
}

/// `text` with the first path that starts with `root` (at the start of `text` or after a space) written from it:
/// `root` itself becomes `itself`, and `root` and its separator before the rest of a path become `inside`. None when
/// no path in `text` starts there. Case-insensitive on Windows, where C:\Users and c:\users are the same folder.
fn swap_root(text: &str, root: &str, itself: &str, inside: &str) -> Option<String> {
    let root = root.trim_end_matches(['\\', '/']);
    if root.is_empty() {
        return None;
    }
    // ASCII case folding keeps every byte where it was, so positions found in one are positions in the other.
    let fold = |s: &str| if cfg!(windows) { s.to_ascii_lowercase() } else { s.to_string() };
    let (hay, needle) = (fold(text), fold(root));
    for (at, _) in hay.match_indices(&needle) {
        if at > 0 && !text[..at].ends_with(' ') {
            continue;
        }
        let end = at + needle.len();
        let (cut, put) = match text[end..].chars().next() {
            None | Some(' ') => (end, itself),
            Some('\\' | '/') => (end + 1, inside),
            Some(_) => continue,
        };
        return Some(format!("{}{put}{}", &text[..at], &text[cut..]));
    }
    None
}

/// What a phone shows for an action waiting for a click: the command, the path or the address, and why it asks
/// when the reason is not the action itself.
fn phone_command(request: &Request, folder: &Path, home: &Path) -> String {
    let mut command = phone_summary(&request.tool, &request.summary, folder, home);
    if request.secret {
        command.push_str(" — looks like a secret file");
    } else if request.outside {
        command.push_str(" — outside the working folder");
    }
    command
}

fn status_word(status: Status) -> &'static str {
    match status {
        Status::Running => "running",
        Status::Waiting => "waiting",
        Status::Ok => "ok",
        Status::Error => "error",
        Status::Denied => "denied",
    }
}

/// The island and the phones, as run.rs sees them.
struct LiveHands {
    app: AppHandle,
    run: u64,
    origin: Origin,
    project: Option<String>,
    /// The working folder and the home folder, which paths on a phone are written from.
    folder: PathBuf,
    home: PathBuf,
    /// The phone that asked, when it did: one `chatStep` per change.
    phone_steps: Option<UnboundedSender<StepLine>>,
    lines: Mutex<Vec<String>>,
}

impl LiveHands {
    fn session(&self, state: &str, final_line: Option<String>) -> SessionIn {
        let lines = self.lines.lock().unwrap().clone();
        SessionIn {
            pill_id: PILL_ID.into(),
            agent: "Mochi".into(),
            state: state.into(),
            status_text: lines.last().cloned().unwrap_or_default(),
            step_index: lines.len().saturating_sub(1) as u32,
            step_count: lines.len() as u32,
            steps: lines,
            final_line,
            project: self.project.clone(),
            color: Some(COLOR.into()),
            files: Vec::new(),
        }
    }
}

impl Hands for LiveHands {
    fn step(&self, step: &Step) {
        let _ = self.app.emit_to(WINDOW_LABEL, "agent-step", json!({ "run": self.run, "origin": self.origin, "step": step }));
        let line = step_line(step, &self.folder, &self.home);
        {
            let mut lines = self.lines.lock().unwrap();
            match lines.get_mut(step.index) {
                Some(existing) => *existing = line.clone(),
                None => lines.push(line.clone()),
            }
        }
        let state = if step.status == Status::Waiting { "approval" } else { "working" };
        show_on_phones(&self.app, Some(Some(self.session(state, None))), None);
        if let Some(tx) = &self.phone_steps {
            let _ = tx.send(StepLine { index: step.index, status: status_word(step.status).into(), text: line });
        }
    }

    fn approve<'a>(&'a self, request: Request) -> BoxFuture<'a, bool> {
        Box::pin(async move {
            let Some(agent) = self.app.try_state::<Agent>() else { return false };
            let (tx, rx) = oneshot::channel();
            let mut card = serde_json::to_value(&request).unwrap_or_else(|_| json!({}));
            card["run"] = json!(self.run);
            card["origin"] = json!(self.origin);
            let (id, card) = agent.wait_for(self.run, card, tx);
            let _ = self.app.emit_to(WINDOW_LABEL, "agent-approval", card);
            show_on_phones(
                &self.app,
                None,
                Some(Some(ApprovalIn {
                    request_id: id.clone(),
                    session_id: format!("mochi-{}", self.run),
                    pill_id: PILL_ID.into(),
                    tool: alias(&request.tool).into(),
                    command: phone_command(&request, &self.folder, &self.home),
                })),
            );
            let wait = if self.origin == Origin::Phone { WAIT_PHONE } else { WAIT_DESK };
            let allowed = matches!(tokio::time::timeout(wait, rx).await, Ok(Ok(true)));
            agent.forget(&id);
            let _ = self.app.emit_to(WINDOW_LABEL, "agent-approval-resolved", json!({ "id": id, "allowed": allowed }));
            show_on_phones(&self.app, None, Some(None));
            log::line(format!("agent: action {id} {}", if allowed { "allowed" } else { "not allowed" }));
            allowed
        })
    }
}

/// Ends the turn however it ends — finished, failed, stopped, or dropped because the phone that asked went away:
/// the command still running is ended, the action still waiting is withdrawn everywhere.
struct RunGuard {
    app: AppHandle,
    run: u64,
    cancel: Arc<AtomicBool>,
    /// The turn's last state was put on the phones; leave it there.
    kept: bool,
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(agent) = self.app.try_state::<Agent>() {
            if let Some(id) = agent.end(self.run) {
                let _ = self.app.emit_to(WINDOW_LABEL, "agent-approval-resolved", json!({ "id": id, "allowed": false }));
            }
        }
        show_on_phones(&self.app, if self.kept { None } else { Some(None) }, Some(None));
        let _ = self.app.emit_to(WINDOW_LABEL, "agent-run", json!({ "run": self.run, "running": false }));
    }
}

/// The turn's last state on the phones for a few minutes, then gone (unless another turn started meanwhile).
fn linger(app: &AppHandle, hands: &LiveHands, state: &str, answer: &str) {
    let line = text::one_line(answer, 200);
    show_on_phones(app, Some(Some(hands.session(state, (!line.is_empty()).then_some(line)))), None);
    let (app, run) = (app.clone(), hands.run);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(LINGER).await;
        let Some(agent) = app.try_state::<Agent>() else { return };
        let idle = {
            let inner = agent.inner.lock().unwrap();
            inner.run.is_none() && inner.last_run == run
        };
        if idle {
            show_on_phones(&app, Some(None), None);
        }
    });
}

// ── The providers ─────────────────────────────────────────────────────────────

/// Who answers: Claude, a cloud provider that speaks the OpenAI API, or a model server the user runs.
enum Brain {
    Claude,
    Cloud(&'static openai_compat::Provider),
    Local(local_chat::Server),
}

impl Brain {
    fn of(settings: &Settings, provider: &str) -> Result<Brain, String> {
        if provider == chat::ANTHROPIC || provider.is_empty() {
            return Ok(Brain::Claude);
        }
        if let Some(p) = openai_compat::provider(provider) {
            return Ok(Brain::Cloud(p));
        }
        if let Some(server) = local_chat::server(settings, provider) {
            return Ok(Brain::Local(server));
        }
        Err(format!("Unknown chat provider: {provider}"))
    }

    /// The turn as this provider wants it: its format, its output limit, the user's message with the dropped file
    /// or window as context on the first turn.
    fn job(&self, model: &str, system: String, history: Vec<Value>, first: bool, context: Option<&ChatContext>, query: &str) -> Job {
        let (flavor, max_tokens, max_tokens_field, user) = match self {
            Brain::Claude => (
                Flavor::Anthropic,
                CLAUDE_MAX_TOKENS,
                "max_tokens",
                json!({ "role": "user", "content": claude::user_content(first, context, query) }),
            ),
            Brain::Cloud(p) => (Flavor::OpenAi, OTHER_MAX_TOKENS, p.max_tokens_field, openai_compat::user_message(first, context, query)),
            Brain::Local(_) => (
                Flavor::OpenAi,
                OTHER_MAX_TOKENS,
                "max_tokens",
                json!({ "role": "user", "content": local_chat::user_text(first, context, query) }),
            ),
        };
        Job { flavor, model: model.to_string(), system, max_tokens, max_tokens_field, history, user }
    }
}

impl Transport for Brain {
    fn post<'a>(&'a self, body: &'a Value) -> BoxFuture<'a, Result<Value, String>> {
        Box::pin(async move {
            match self {
                Brain::Claude => claude::post(body, TIMEOUT).await,
                Brain::Cloud(p) => openai_compat::post(p, body, TIMEOUT).await,
                Brain::Local(server) => local_chat::post(server, body, TIMEOUT).await,
            }
        })
    }
}

/// Mochi's instructions when he has hands.
fn system_prompt(env: &Env, origin: Origin) -> String {
    let opening = match crate::identity::first_name() {
        Some(name) => format!("You are Mochi, {name}'s personal AI assistant living at the top of their screen."),
        None => "You are Mochi, a personal AI assistant living at the top of the user's screen.".to_string(),
    };
    let web = if env.web_search { "web search, " } else { "" };
    let level = match env.level {
        Level::ReadOnly => "You may only look: nothing can be changed and no command can run on this computer.",
        Level::Ask => "Every file change, command and web request is shown to the user, who approves or denies it.",
        Level::Edits => "File changes inside the working folder are applied at once; commands, web requests and anything outside the folder are shown to the user, who approves or denies them.",
    };
    let house = if env.house.is_some() {
        "\n- You can also act on the user's home: home_devices lists the devices you may use and what each can do, home_control does one action on one device, home_routine runs one of the user's routines. When the user asks for something in the house (\"liga a TV da sala e abre o YouTube\"), do it with these tools, one action at a time and in order, then say in one short sentence what the devices report now. Never claim an action worked if its answer says otherwise."
    } else {
        ""
    };
    let phone = if origin == Origin::Phone {
        "\nThe user is talking to you from their phone, away from the computer: keep your answers short."
    } else {
        ""
    };
    format!(
        "{opening} You have hands on this computer: tools to look at files, change them, run commands, {web}and read or open web pages. \
Respond in the user's language. Be thorough and complete, and use as much detail as the task requires.

How you work:
- The working folder is {folder}. Relative paths start there. The computer runs {os}; commands run in {shell}.
- {level} Before such an action, say in one short sentence what you are about to do and why.
- Look before you change: list_directory, find_files, search_text and read_file are better than shell commands for exploring. Change files with edit_file (exact replacements) or write_file (new files).
- Work in small steps and check them: run the tests or the program, read back what you changed.
- If the user denies an action, do not try another way to do the same thing: ask what they want instead.
- Never print, copy or send secrets you come across (keys, tokens, passwords).
- When you are done, say briefly what you did and what changed.{house}{phone}

Use light Markdown when it helps: short paragraphs, bullet lists, **bold**, `inline code` and fenced code blocks. Avoid tables and big headings: the chat window is small.",
        folder = policy::display(&env.folder),
        os = if cfg!(windows) { "Windows" } else { "Linux" },
        shell = env.shell.describe(),
    )
}

// ── A turn ────────────────────────────────────────────────────────────────────

enum TurnError {
    /// Mochi's hands are off, or have no folder to work in: the message says what to do.
    Off(String),
    Busy,
    Canceled,
    Failed(String),
}

/// The folder and the level, when Mochi's hands are on and the folder is still there.
fn configured(settings: &Settings) -> Result<(PathBuf, Level), String> {
    if !settings.agent_enabled {
        return Err(t("Mochi's hands are off. Turn them on in Settings → Mochi's hands."));
    }
    let folder = PathBuf::from(settings.agent_folder.trim());
    if settings.agent_folder.trim().is_empty() || !folder.is_dir() {
        return Err(t("Choose the folder Mochi works in, in Settings → Mochi's hands."));
    }
    Ok((folder, Level::parse(&settings.agent_level)))
}

#[allow(clippy::too_many_arguments)]
async fn turn(
    app: &AppHandle,
    chat: &Chat,
    settings: &Settings,
    provider: &str,
    model: &str,
    query: String,
    context: Option<ChatContext>,
    origin: Origin,
    phone_steps: Option<UnboundedSender<StepLine>>,
) -> Result<String, TurnError> {
    let (folder, level) = configured(settings).map_err(TurnError::Off)?;
    let brain = Brain::of(settings, provider).map_err(TurnError::Failed)?;
    if model.trim().is_empty() && !matches!(brain, Brain::Claude) {
        return Err(TurnError::Failed(t("Pick a model above the chat box first.")));
    }
    let agent = app.state::<Agent>();
    let (run_id, cancel) = agent.begin(origin).ok_or(TurnError::Busy)?;
    let mut guard = RunGuard { app: app.clone(), run: run_id, cancel: cancel.clone(), kept: false };
    // The house is read before the turn (from the last minute's reading when there is one), never for long.
    let house = tokio::time::timeout(Duration::from_secs(8), crate::smart_home::agent_env(app, origin == Origin::Phone)).await.ok().flatten();
    let env = Env {
        folder,
        home: crate::platform::home_dir(),
        level,
        shell: Shell::detect(),
        cancel,
        web_search: matches!(brain, Brain::Claude),
        house,
    };
    let started = chat.begin(&format!("{provider}+hands"));
    let job = brain.job(model, system_prompt(&env, origin), started.history.clone(), started.first, context.as_ref(), &query);
    let hands = LiveHands {
        app: app.clone(),
        run: run_id,
        origin,
        project: env.folder.file_name().map(|n| n.to_string_lossy().to_string()),
        folder: env.folder.clone(),
        home: env.home.clone(),
        phone_steps,
        lines: Mutex::new(Vec::new()),
    };
    let _ = app.emit_to(WINDOW_LABEL, "agent-run", json!({ "run": run_id, "origin": origin, "running": true }));
    show_on_phones(app, Some(Some(hands.session("thinking", None))), None);
    log::line(format!("agent: turn {run_id} from the {} ({provider}, {})", if origin == Origin::Phone { "phone" } else { "island" }, level.as_str()));
    match run::run(job, &env, &brain, &hands).await {
        Ok(done) => {
            let plain = chat::plain_question(started.first, context.as_ref(), &query);
            chat.commit_turn(&started, done.messages, &plain, &done.text);
            log::line(format!("agent: turn {run_id} done, {} action(s)", done.actions));
            linger(app, &hands, "finished", &done.text);
            guard.kept = true;
            Ok(done.text)
        }
        Err(Failed::Canceled) => {
            log::line(format!("agent: turn {run_id} stopped"));
            Err(TurnError::Canceled)
        }
        Err(Failed::Provider(message)) => {
            // Not the message: a provider's error text may echo part of a key.
            log::line(format!("agent: turn {run_id} failed"));
            linger(app, &hands, "error", "");
            guard.kept = true;
            Err(TurnError::Failed(message))
        }
    }
}

/// One turn of the island's chat with hands.
pub async fn run_island(
    app: &AppHandle,
    chat: &Chat,
    settings: &Settings,
    query: String,
    context: Option<ChatContext>,
) -> Result<ChatReply, String> {
    let provider = if settings.chat_provider.is_empty() { chat::ANTHROPIC } else { settings.chat_provider.as_str() };
    let model = chat::model_for(settings, provider);
    match turn(app, chat, settings, provider, &model, query, context, Origin::Island, None).await {
        Ok(text) => Ok(ChatReply { text }),
        Err(TurnError::Off(message) | TurnError::Failed(message)) => Err(message),
        Err(TurnError::Busy) => Err(t("Mochi is already busy with another task. Stop it first.")),
        Err(TurnError::Canceled) => Err(t("Stopped.")),
    }
}

/// Why a phone's turn with hands did not answer, as far as the phone may know (phone_link/chat_backend.rs).
pub enum PhoneError {
    HandsOff,
    Busy,
    Canceled,
    Failed(String),
}

/// One turn of the phone's chat with hands (cap `agent`): the phone's own conversation, `steps` for its chat.
pub async fn run_for_phone(
    app: &AppHandle,
    chat: &Chat,
    settings: &Settings,
    provider: &str,
    model: &str,
    text: String,
    steps: UnboundedSender<StepLine>,
) -> Result<String, PhoneError> {
    if !settings.phone_agent {
        return Err(PhoneError::HandsOff);
    }
    match turn(app, chat, settings, provider, model, text, None, Origin::Phone, Some(steps)).await {
        Ok(text) => Ok(text),
        Err(TurnError::Off(_)) => Err(PhoneError::HandsOff),
        Err(TurnError::Busy) => Err(PhoneError::Busy),
        Err(TurnError::Canceled) => Err(PhoneError::Canceled),
        Err(TurnError::Failed(message)) => Err(PhoneError::Failed(message)),
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Settings → Mochi's hands, and what the island needs to know.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub enabled: bool,
    pub folder: String,
    pub level: String,
    /// The shell commands run in, in words.
    pub shell: String,
    /// A turn is running, asked from there.
    pub running: Option<Origin>,
    /// The action waiting for a click (the island's card), if any.
    pub waiting: Option<Value>,
    /// A folder to offer when none is chosen yet: Documents, else the home folder.
    pub suggested: String,
}

fn status_of(settings: &Settings, agent: &Agent) -> AgentStatus {
    let home = crate::platform::home_dir();
    let documents = home.join("Documents");
    AgentStatus {
        enabled: settings.agent_enabled,
        folder: settings.agent_folder.clone(),
        level: Level::parse(&settings.agent_level).as_str().to_string(),
        shell: Shell::detect().describe().to_string(),
        running: agent.running(),
        waiting: agent.card(),
        suggested: policy::display(if documents.is_dir() { &documents } else { &home }),
    }
}

#[tauri::command]
pub fn agent_status(shared: State<Shared>, agent: State<Agent>) -> AgentStatus {
    let settings = shared.settings.lock().unwrap().clone();
    status_of(&settings, &agent)
}

/// Allow or Deny on the island's card. Only the island may answer.
#[tauri::command]
pub fn agent_decide(window: WebviewWindow, agent: State<Agent>, id: String, allow: bool) -> Result<bool, String> {
    phone_link::only_island(&window)?;
    Ok(agent.decide(&id, allow))
}

/// The chat's stop button: the command running ends and the turn stops.
#[tauri::command]
pub fn agent_cancel(window: WebviewWindow, agent: State<Agent>) -> Result<bool, String> {
    phone_link::only_island(&window)?;
    let stopped = agent.cancel();
    if stopped {
        log::line("agent: stopped from the island");
    }
    Ok(stopped)
}

/// Settings → Mochi's hands: on or off, the folder, the level. Only the settings window may change them.
#[tauri::command]
pub fn agent_settings_set(
    app: AppHandle,
    window: WebviewWindow,
    shared: State<Shared>,
    agent: State<Agent>,
    enabled: bool,
    folder: String,
    level: String,
) -> Result<AgentStatus, String> {
    phone_link::only_settings(&window)?;
    let folder = folder.trim();
    let clean = if folder.is_empty() {
        String::new()
    } else {
        let path = PathBuf::from(folder);
        if !path.is_absolute() {
            return Err(t("Enter the full path of a folder."));
        }
        match path.canonicalize() {
            Ok(real) if real.is_dir() => policy::display(&real),
            _ => return Err(t("This folder does not exist.")),
        }
    };
    if enabled && clean.is_empty() {
        return Err(t("Choose the folder Mochi works in first."));
    }
    let level = Level::parse(&level).as_str().to_string();
    let (switched, updated) = {
        let mut current = shared.settings.lock().unwrap();
        let switched = current.agent_enabled != enabled;
        current.agent_enabled = enabled;
        current.agent_folder = clean;
        current.agent_level = level.clone();
        if let Err(err) = settings::save(&current) {
            log::line(format!("could not save settings: {err}"));
        }
        (switched, current.clone())
    };
    if !enabled {
        agent.cancel();
    }
    if switched {
        // Phones learn whether they may use the hands.
        phone_link::agent_setting_changed(&app);
    }
    log::line(format!("agent: hands {} ({level})", if enabled { "on" } else { "off" }));
    let _ = app.emit("settings-changed", updated.clone());
    Ok(status_of(&updated, &agent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_turn_at_a_time_and_a_decision_reaches_only_the_action_waiting() {
        let agent = Agent::default();
        let (run, cancel) = agent.begin(Origin::Island).unwrap();
        assert!(agent.begin(Origin::Phone).is_none(), "busy");
        assert_eq!(agent.running(), Some(Origin::Island));

        let (tx, mut rx) = oneshot::channel();
        let (id, card) = agent.wait_for(run, json!({"tool":"run_command"}), tx);
        assert!(is_request(&id));
        assert_eq!(card["id"], id);
        assert_eq!(agent.card().unwrap()["tool"], "run_command");
        assert!(!agent.decide("agent-999", true), "another id changes nothing");
        assert!(rx.try_recv().is_err());
        assert!(agent.decide(&id, true));
        assert_eq!(rx.try_recv(), Ok(true));
        assert!(!agent.decide(&id, false), "answered once");

        // Stop: the waiting action reads a no and the running command is told.
        let (tx, mut rx) = oneshot::channel();
        agent.wait_for(run, json!({}), tx);
        assert!(agent.cancel());
        assert_eq!(rx.try_recv(), Ok(false));
        assert!(cancel.load(Ordering::SeqCst));

        // The end of the turn withdraws an action still waiting, and frees the place.
        let (tx, mut rx) = oneshot::channel();
        let (id, _) = agent.wait_for(run, json!({}), tx);
        assert_eq!(agent.end(run), Some(id));
        assert!(rx.try_recv().is_err(), "the sender is gone: the waiting side reads a no");
        assert!(agent.running().is_none());
        assert!(agent.begin(Origin::Phone).is_some());
        assert!(!Agent::default().cancel(), "nothing to stop");
    }

    /// A path from the file system's root, on this OS: `at(&["home", "me"])` is /home/me or C:\home\me.
    fn at(parts: &[&str]) -> PathBuf {
        let mut path = PathBuf::from(if cfg!(windows) { "C:\\" } else { "/" });
        path.extend(parts);
        path
    }

    #[test]
    fn phones_see_claude_codes_tool_names_and_why_an_action_asks() {
        let (folder, home) = (at(&["home", "me", "proj"]), at(&["home", "me"]));
        let step = Step { index: 0, tool: "run_command".into(), summary: "npm test".into(), status: Status::Running, detail: None, output: None };
        assert_eq!(step_line(&step, &folder, &home), "Bash · npm test");
        assert_eq!(step_line(&Step { tool: "system_info".into(), summary: String::new(), ..step.clone() }, &folder, &home), "Info");
        assert_eq!(alias("edit_file"), "Edit");
        assert_eq!(alias("something_new"), "Tool");
        let hosts = policy::display(&at(&["etc", "hosts"]));
        let mut request = Request { tool: "read_file".into(), summary: hosts.clone(), cwd: None, before: None, after: None, new_file: false, outside: true, secret: false };
        assert_eq!(phone_command(&request, &folder, &home), format!("{hosts} — outside the working folder"));
        request.secret = true;
        assert_eq!(phone_command(&request, &folder, &home), format!("{hosts} — looks like a secret file"));
    }

    #[test]
    fn no_whole_path_reaches_a_phone() {
        let (folder, home) = (at(&["home", "me", "proj"]), at(&["home", "me"]));
        let sep = std::path::MAIN_SEPARATOR;
        let shown = |tool: &str, summary: String| phone_summary(tool, &summary, &folder, &home);
        let d = |parts: &[&str]| policy::display(&at(parts));
        // In the working folder: from there, the folder itself by its name.
        assert_eq!(shown("read_file", d(&["home", "me", "proj", "notes", "todo.txt"])), format!("notes{sep}todo.txt"));
        assert_eq!(shown("edit_file", format!("{} (+1 −1)", d(&["home", "me", "proj", "src", "main.rs"]))), format!("src{sep}main.rs (+1 −1)"));
        assert_eq!(shown("list_directory", d(&["home", "me", "proj"])), "proj");
        assert_eq!(shown("search_text", format!("TODO · {}", d(&["home", "me", "proj"]))), "TODO · proj");
        // Elsewhere at home: from "~". A folder whose name only starts like the working folder's is not in it.
        assert_eq!(shown("write_file", d(&["home", "me", "Downloads", "a.txt"])), format!("~{sep}Downloads{sep}a.txt"));
        assert_eq!(shown("find_files", format!("*.md · {}", d(&["home", "me", "project2"]))), format!("*.md · ~{sep}project2"));
        assert_eq!(shown("list_directory", d(&["home", "me"])), "~");
        // Outside the home folder nothing names the user; a command and an address are shown as they are.
        assert_eq!(shown("read_file", d(&["etc", "hosts"])), d(&["etc", "hosts"]));
        let command = format!("cat {}", d(&["home", "me", "proj", "a.txt"]));
        assert_eq!(shown("run_command", command.clone()), command);
        assert_eq!(shown("fetch_url", "https://example.com/home/me".into()), "https://example.com/home/me");
        // The line a phone gets.
        let step = Step { index: 0, tool: "write_file".into(), summary: d(&["home", "me", "proj", "README.md"]), status: Status::Ok, detail: None, output: None };
        assert_eq!(step_line(&step, &folder, &home), "Write · README.md");
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_match_whatever_their_case() {
        let (folder, home) = (PathBuf::from(r"C:\Users\Eduardo\Documents"), PathBuf::from(r"C:\Users\Eduardo"));
        assert_eq!(phone_summary("read_file", r"c:\users\eduardo\documents\nota.txt", &folder, &home), "nota.txt");
        assert_eq!(phone_summary("read_file", r"C:\USERS\EDUARDO\Desktop\a.txt", &folder, &home), r"~\Desktop\a.txt");
    }

    #[test]
    fn the_instructions_name_the_folder_the_shell_and_the_level() {
        let env = Env {
            folder: PathBuf::from("/work/proj"),
            home: PathBuf::from("/home/me"),
            level: Level::Ask,
            shell: Shell::Bash(PathBuf::from("/bin/bash")),
            cancel: Arc::new(AtomicBool::new(false)),
            web_search: false,
            house: None,
        };
        let p = system_prompt(&env, Origin::Island);
        assert!(p.contains("The working folder is /work/proj."));
        assert!(p.contains("commands run in bash"));
        assert!(p.contains("approves or denies it"));
        assert!(!p.contains("web search"));
        assert!(!p.contains("phone"));
        let p = system_prompt(&Env { level: Level::ReadOnly, web_search: true, ..env }, Origin::Phone);
        assert!(p.contains("You may only look"));
        assert!(p.contains("web search, "));
        assert!(p.contains("from their phone"));
    }

    #[test]
    fn hands_need_the_switch_and_an_existing_folder() {
        let mut s = Settings::default();
        assert!(configured(&s).is_err());
        s.agent_enabled = true;
        assert!(configured(&s).is_err(), "no folder yet");
        s.agent_folder = std::env::temp_dir().to_string_lossy().to_string();
        s.agent_level = "edits".into();
        let (folder, level) = configured(&s).unwrap();
        assert!(folder.is_dir());
        assert_eq!(level, Level::Edits);
        s.agent_folder = "/no/such/folder/anywhere".into();
        assert!(configured(&s).is_err());
    }

    /// A model server on loopback answering each request with the next of `replies`; returns its address and the
    /// bodies it was sent.
    fn fake_server(replies: Vec<Value>) -> (String, Arc<Mutex<Vec<Value>>>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let got = seen.clone();
        std::thread::spawn(move || {
            for reply in replies {
                let (conn, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(conn.try_clone().unwrap());
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                got.lock().unwrap().push(serde_json::from_slice::<Value>(&body).unwrap());
                let text = reply.to_string();
                let mut conn = conn;
                write!(conn, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len()).unwrap();
            }
        });
        (url, seen)
    }

    /// Quiet hands for a turn without a window: no click is ever given.
    struct Nobody;
    impl Hands for Nobody {
        fn step(&self, _step: &Step) {}
        fn approve<'a>(&'a self, _request: Request) -> BoxFuture<'a, bool> {
            Box::pin(async { false })
        }
    }

    #[test]
    fn a_turn_with_a_model_server_over_http() {
        let dir = std::env::temp_dir().join(format!("coucou-agent-http-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("todo.txt"), "buy milk\n").unwrap();
        let (url, seen) = fake_server(vec![
            json!({"choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":null,"tool_calls":[
                {"id":"call_1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"todo.txt\"}"}}]}}]}),
            json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Your list says: buy milk."}}]}),
        ]);
        let brain = Brain::Local(local_chat::Server { id: "custom", name: "Custom", url, key: None });
        let env = Env {
            folder: dir.clone(),
            home: dir.clone(),
            level: Level::Ask,
            shell: Shell::detect(),
            cancel: Arc::new(AtomicBool::new(false)),
            web_search: false,
            house: None,
        };
        let job = brain.job("qwen", system_prompt(&env, Origin::Island), vec![], true, None, "what is on my list?");
        let done = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run::run(job, &env, &brain, &Nobody))
            .unwrap();
        assert_eq!(done.text, "Your list says: buy milk.");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0]["model"], "qwen");
        assert_eq!(seen[0]["max_tokens"], OTHER_MAX_TOKENS);
        assert!(seen[0]["tools"].as_array().unwrap().iter().any(|t| t["function"]["name"] == "run_command"));
        let second = seen[1]["messages"].as_array().unwrap();
        assert_eq!(second[0]["role"], "system");
        assert_eq!(second[2]["tool_calls"][0]["id"], "call_1");
        assert_eq!(second[3]["role"], "tool");
        assert!(second[3]["content"].as_str().unwrap().contains("buy milk"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reads this file: every command that changes something checks the calling window, as in phone_link.
    #[test]
    fn every_command_that_changes_something_checks_the_calling_window() {
        let source = include_str!("mod.rs").replace("\r\n", "\n");
        let mut seen = 0;
        for chunk in source.split("#[tauri::command]\n").skip(1) {
            let name = chunk.split("pub fn ").nth(1).and_then(|r| r.split('(').next()).expect("a command has a name");
            let end = chunk.find("\n}\n").expect("a command ends");
            let body = &chunk[..end];
            seen += 1;
            if name.ends_with("_status") {
                assert!(!body.contains("settings::save") && !body.contains(".decide(") && !body.contains(".cancel("), "{name} changes something");
                continue;
            }
            assert!(
                body.contains("window: WebviewWindow") && (body.contains("only_settings(&window)") || body.contains("only_island(&window)")),
                "{name} changes something but does not check the calling window"
            );
        }
        assert_eq!(seen, 4);
    }
}
