// The tools Mochi's hands are made of, as the model sees them (`definitions`) and as they run (`prepare`, then
// `execute`). Every call is prepared first: its input checked, its path resolved, what it will change worked out
// (an edit's text before and after) and its gate decided by policy.rs. Only then, and after a click if the gate says
// so, does it run. What goes back to the model is plain English text, held to a size; what the island and the phone
// show is a short step.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use serde::Serialize;
use serde_json::{json, Value};

use super::policy::{self, Effect, Gate, Level};
use super::run::BoxFuture;
use super::shell::{self, Shell};
use super::text;
use crate::smart_home::model::{self as home, Action as HomeAction, Device, ACTION_WORDS};

/// Most text one tool call sends back to the model (about 10 000 tokens).
pub const MAX_RESULT: usize = 40 * 1024;
const MAX_READ: usize = 60 * 1024;
const DEFAULT_LINES: usize = 500;
const MAX_LINES: usize = 2000;
/// A file larger than this is not read, edited or searched whole.
const MAX_FILE: u64 = 4 * 1024 * 1024;
const MAX_WRITE: usize = 2 * 1024 * 1024;
const MAX_LIST: usize = 300;
const MAX_FOUND: usize = 200;
const MAX_MATCHES: usize = 150;
/// How many entries a search may look at before it stops, so a search from the root of a disk ends.
const MAX_VISITED: usize = 60_000;
const MAX_SEARCHED_FILES: usize = 20_000;
const MAX_SEARCHED_SIZE: u64 = 1024 * 1024;
const DEFAULT_TIMEOUT: u64 = 120;
const MAX_TIMEOUT: u64 = 600;
const MAX_COMMAND: usize = 8000;
const MAX_FETCH: usize = 3 * 1024 * 1024;
/// Before and after texts an approval card shows as a diff; past this, only the counts.
const MAX_PREVIEW: usize = 100 * 1024;

/// Folders a search does not go into (it still searches one it was pointed at).
const SKIPPED_DIRS: &[&str] = &[
    ".git", ".hg", ".svn", "node_modules", "target", "dist", "build", ".next", ".nuxt", ".svelte-kit", ".turbo",
    ".venv", "venv", "__pycache__", ".mypy_cache", ".pytest_cache", ".cache", ".gradle", ".idea", ".vs", "coverage",
];

/// Where and how the tools work, fixed for one turn.
pub struct Env {
    /// The working folder, as the user picked it.
    pub folder: PathBuf,
    pub home: PathBuf,
    pub level: Level,
    pub shell: Shell,
    /// Set when the turn is stopped: a running command ends.
    pub cancel: Arc<AtomicBool>,
    /// Claude has a web search of its own (the server tool); the others only have fetch_url.
    pub web_search: bool,
    /// Mochi's home (the house, not the home folder), when it is on and has devices for him (smart_home::agent_env).
    pub house: Option<HomeEnv>,
    /// Zheus' superpowers, when the user turned them on (powers::runner).
    pub powers: Option<Arc<dyn PowersRunner>>,
}

/// Mochi's home for one turn: the devices he may use (read at the start of the turn), the routines, and the way to
/// act on them. Behind a trait so the tools are tested without a house.
pub struct HomeEnv {
    pub devices: Vec<Device>,
    pub routines: Vec<RoutineInfo>,
    pub runner: Arc<dyn HomeRunner>,
    /// Zheus One is on (zheus_one/): its World State and missions come with the house's tools.
    pub zheus: bool,
}

/// A routine as Mochi sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineInfo {
    pub id: String,
    pub name: String,
    /// One of its steps is on a device that asks first.
    pub asks: bool,
    /// "TV da sala: turn off", in order.
    pub steps: Vec<String>,
    /// What stops it from running as it is (a device gone, an action the device does not have).
    pub problems: Vec<String>,
}

/// What an action on the house gave: for the model (`text`) and for the step (`detail`, in the user's words).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeResult {
    pub ok: bool,
    pub text: String,
    pub detail: String,
}

pub trait HomeRunner: Send + Sync {
    /// The devices Mochi may use, asked now.
    fn devices<'a>(&'a self) -> BoxFuture<'a, Vec<Device>>;
    /// One action, already checked against the device.
    fn control<'a>(&'a self, id: &'a str, action: &'a HomeAction) -> BoxFuture<'a, HomeResult>;
    fn routine<'a>(&'a self, id: &'a str) -> BoxFuture<'a, HomeResult>;
    /// Zheus One's World State, for the model (zheus_world).
    fn world<'a>(&'a self) -> BoxFuture<'a, HomeResult> {
        Box::pin(async { HomeResult { ok: false, text: NO_ZHEUS.into(), detail: String::new() } })
    }
    /// A Zheus One mission: "arrival" (its plan shown for the user's yes) or "situation" (on the TV).
    fn mission<'a>(&'a self, _mission: &'a str, _eta: Option<u32>) -> BoxFuture<'a, HomeResult> {
        Box::pin(async { HomeResult { ok: false, text: NO_ZHEUS.into(), detail: String::new() } })
    }
}

/// Zheus' superpowers (powers/ in the app, coucou-powers underneath): the user's portfolio, studies, English and
/// training, read against the official data. They only read and keep the user's own records in Coucou: nothing on
/// the computer, no money moved, no message sent. Behind a trait so the tools are tested without the app.
pub trait PowersRunner: Send + Sync {
    /// One of POWER_TOOLS with its arguments: the text for the model, and a short detail for the step.
    fn call<'a>(&'a self, tool: &'a str, input: &'a Value) -> BoxFuture<'a, HomeResult>;
}

/// The superpowers' tools, described (with their arguments) in coucou-hook's house_tools.json, which both Mochi
/// and the agents reached over MCP read.
pub const POWER_TOOLS: &[&str] = &[
    "wealth_portfolio",
    "wealth_update",
    "wealth_trade",
    "wealth_ceiling",
    "wealth_fii",
    "wealth_events",
    "wealth_company",
    "wealth_plan",
    "wealth_audit",
    "wealth_decision",
    "wealth_committee",
    "wealth_macro",
    "life_study",
    "life_cards",
    "life_english",
    "life_gym",
    "life_day",
    "life_content",
];

/// What the model is told when it calls a superpower while they are off.
pub const NO_POWERS: &str = "Zheus' superpowers are off on this computer: the user turns them on in Coucou, Settings → Zheus' superpowers.";

/// What the model is told when it calls a home tool while the home is off.
const NO_HOME: &str = "Mochi's home is not available on this computer: it is off, or no device is ticked for Mochi in Settings → Mochi's home.";
/// What the model is told when it calls a Zheus One tool while Zheus One is off.
const NO_ZHEUS: &str = "Zheus One is off on this computer: the user turns it on in Coucou, Settings → Zheus One.";

/// One tool as the model is told about it.
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: String,
    pub schema: Value,
}

fn def(name: &'static str, description: impl Into<String>, properties: Value, required: &[&str]) -> ToolDef {
    ToolDef {
        name,
        description: description.into(),
        schema: json!({ "type": "object", "properties": properties, "required": required }),
    }
}

/// The tools offered at this level: "read only" gets no tool that could change anything.
pub fn definitions(env: &Env) -> Vec<ToolDef> {
    let path = |what: &str| json!({ "type": "string", "description": what });
    let mut tools = vec![
        def(
            "list_directory",
            "List the files and folders in a folder (one level). Folder names end with a slash. Use it to look around before reading files.",
            json!({ "path": path("The folder. A relative path starts from the working folder. Default: the working folder.") }),
            &[],
        ),
        def(
            "read_file",
            "Read a text file. The lines come numbered (number, tab, text). Long files come in parts: use offset and limit to read on.",
            json!({
                "path": path("The file. A relative path starts from the working folder."),
                "offset": { "type": "integer", "description": "First line to read, from 1. Default 1." },
                "limit": { "type": "integer", "description": "How many lines. Default 500, at most 2000." },
            }),
            &["path"],
        ),
        def(
            "find_files",
            "Find files by name with a glob pattern, in all subfolders: *.py, src/**/*.ts, package.json. Skips .git, node_modules and build folders. Returns paths relative to the folder searched.",
            json!({
                "pattern": { "type": "string", "description": "The glob pattern. Without a slash it is matched against file names." },
                "path": path("The folder to search. Default: the working folder."),
            }),
            &["pattern"],
        ),
        def(
            "search_text",
            "Search the contents of files with a regular expression, like grep. Returns path:line: text for each matching line. Skips binary files, .git, node_modules and build folders.",
            json!({
                "pattern": { "type": "string", "description": "The regular expression (Rust regex syntax)." },
                "path": path("A folder or a file. Default: the working folder."),
                "glob": { "type": "string", "description": "Only files whose name matches this glob, such as *.rs." },
                "case_insensitive": { "type": "boolean", "description": "Ignore case. Default false." },
            }),
            &["pattern"],
        ),
    ];
    if env.level != Level::ReadOnly {
        tools.extend([
            def(
                "write_file",
                "Create a file, or replace everything in it. Missing folders are created. Prefer edit_file to change part of an existing file. The user may have to approve it.",
                json!({
                    "path": path("The file. A relative path starts from the working folder."),
                    "content": { "type": "string", "description": "The whole new content of the file." },
                }),
                &["path", "content"],
            ),
            def(
                "edit_file",
                "Replace an exact piece of text in a file. old_text must appear exactly once (or set replace_all): copy it from read_file without the line numbers, with its indentation. The user may have to approve it.",
                json!({
                    "path": path("The file. A relative path starts from the working folder."),
                    "old_text": { "type": "string", "description": "The exact text to replace." },
                    "new_text": { "type": "string", "description": "The text to put in its place." },
                    "replace_all": { "type": "boolean", "description": "Replace every occurrence. Default false." },
                }),
                &["path", "old_text", "new_text"],
            ),
            def(
                "run_command",
                format!(
                    "Run a command in {} on the user's computer. Returns the exit code and the output (stdout and stderr together). The user approves every command. It is stopped after timeout_seconds (default {DEFAULT_TIMEOUT}, at most {MAX_TIMEOUT}). Nobody can type into it: use flags that avoid questions. Do not start a server or a watcher that never ends unless the user asked for it.",
                    env.shell.describe()
                ),
                json!({
                    "command": { "type": "string", "description": "The command line." },
                    "cwd": path("The folder to run it in. Default: the working folder."),
                    "timeout_seconds": { "type": "integer", "description": "Default 120, at most 600." },
                }),
                &["command"],
            ),
        ]);
    }
    tools.extend([
        def(
            "fetch_url",
            "Download a web page or an API answer (GET, http or https) and read it as text; HTML becomes plain text. The user approves every request. At most 40 KB of text comes back.",
            json!({ "url": { "type": "string", "description": "The address, starting with http:// or https://." } }),
            &["url"],
        ),
        def(
            "open_url",
            "Open a web page in the user's browser, for the user to see. The user approves it.",
            json!({ "url": { "type": "string", "description": "The address, starting with http:// or https://." } }),
            &["url"],
        ),
        def(
            "system_info",
            "The operating system, the shell commands run in, the working folder, the home folder and the local date and time.",
            json!({}),
            &[],
        ),
    ]);
    if let Some(h) = &env.house {
        tools.extend(home_definitions(h));
    }
    if env.powers.is_some() {
        tools.extend(power_definitions());
    }
    tools
}

/// The superpowers' tools as coucou-hook offers them to other agents: one description for both.
pub fn power_definitions() -> Vec<ToolDef> {
    let listed: Value = serde_json::from_str(include_str!("../../../hook/src/house_tools.json")).unwrap_or_else(|_| json!([]));
    let Some(listed) = listed.as_array() else { return Vec::new() };
    POWER_TOOLS
        .iter()
        .filter_map(|name| {
            let t = listed.iter().find(|t| t["name"] == *name)?;
            Some(ToolDef { name, description: t["description"].as_str()?.to_string(), schema: t["inputSchema"].clone() })
        })
        .collect()
}

/// The home's tools, offered only while the home has devices for Mochi.
fn home_definitions(h: &HomeEnv) -> Vec<ToolDef> {
    let mut tools = vec![
        def(
            "home_devices",
            "List the devices of the user's home that Mochi may use: each one's id, name, room, kind, whether it answers, its state right now, the actions it reported it can do, and its apps and channels. Call it when you do not know a device's id or what it can do, or to check a device's state.",
            json!({ "device": { "type": "string", "description": "Optional: one device (id or name) to see in full, with all its apps and channels." } }),
            &[],
        ),
        def(
            "home_control",
            "Do one action on one device of the user's home: turn it on or off, set or step the volume, mute, open an app, change the channel, set a light's brightness, start, pause or dock the vacuum or send it to some rooms, set the air conditioning's temperature. Only actions the device reports in home_devices work. One action per call, in order: to turn on the TV and open an app, call turn_on, read its answer, then open_app. The answer says what the device reports afterwards: tell the user exactly that, and never say an action worked if the answer says otherwise. Plugs, switches and some devices wait for the user's approval.",
            json!({
                "device": { "type": "string", "description": "The device's id from home_devices, or its name with its room, like \"TV da sala\"." },
                "action": { "type": "string", "enum": ACTION_WORDS, "description": "What to do." },
                "value": { "type": "string", "description": "set_volume and set_brightness: a percent from 0 to 100. set_temperature: degrees Celsius from 10 to 32. open_app: the app's name. set_channel: the channel's number or name. clean_area: the rooms, comma separated, like \"Quarto, Sala\". Leave it out for the other actions." },
            }),
            &["device", "action"],
        ),
    ];
    if !h.routines.is_empty() {
        let names: Vec<&str> = h.routines.iter().map(|r| r.name.as_str()).collect();
        tools.push(def(
            "home_routine",
            format!("Run one of the user's routines, saved sequences of home actions: {}. Each step goes to its device in order; the answer says how each went.", names.join(", ")),
            json!({ "routine": { "type": "string", "description": "The routine's name or id." } }),
            &["routine"],
        ));
    }
    if h.zheus {
        tools.extend(zheus_definitions());
    }
    tools
}

/// Zheus One's tools, offered while it is on. The same as coucou-hook's house_tools.json.
fn zheus_definitions() -> Vec<ToolDef> {
    vec![
        def(
            "zheus_world",
            "Zheus One's World State: the user's home as one picture — each shared device and its state (the vacuum, the air conditioning, the lights, the TVs, the sensors), who is home, each room's temperature, and the facts that tie them together. Call it before planning anything in the house, or when the user asks how the house is.",
            json!({}),
            &[],
        ),
        def(
            "zheus_mission",
            "Start one of Zheus One's missions. \"arrival\": the user is on their way home; Coucou plans the whole house for their arrival (air conditioning on time, the vacuum done before they arrive, lights after dark, the day's summary), shows the plan with what clashes, and runs it only once the user says yes. \"situation\": the house, the plan and the decisions waiting go full screen on the user's TV. Never act on the devices yourself for a mission: Coucou runs it.",
            json!({
                "mission": { "type": "string", "enum": ["arrival", "situation"], "description": "Which mission." },
                "eta_minutes": { "type": "integer", "description": "arrival: the minutes until the user is home, when they said it. Leave it out otherwise." },
            }),
            &["mission"],
        ),
    ]
}

/// A tool call as the model made it. `broken` is set when its arguments could not be read (invalid JSON).
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub id: String,
    pub name: String,
    pub input: Value,
    pub broken: Option<String>,
}

/// What a call will do, once checked.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    List { path: PathBuf },
    Read { path: PathBuf, offset: usize, limit: usize },
    Find { root: PathBuf, pattern: String },
    Search { root: PathBuf, pattern: String, glob: Option<String>, case_insensitive: bool },
    /// `before`: the file as it was when the call was prepared (None: a new file).
    Write { path: PathBuf, content: String, before: Option<String> },
    Edit { path: PathBuf, before: String, after: String, added: usize, removed: usize },
    Run { command: String, cwd: PathBuf, timeout: Duration },
    Fetch { url: Url },
    Open { url: Url },
    Info,
    Home(HomeCall),
    /// One of Zheus' superpowers, with its arguments as the model sent them.
    Power { tool: String, input: Value },
}

/// A call to Mochi's home, checked.
#[derive(Debug, Clone, PartialEq)]
pub enum HomeCall {
    /// All the devices, or one in full (by id).
    Devices { device: Option<String> },
    Control { device: String, action: HomeAction },
    Routine { id: String },
    /// Zheus One's World State.
    World,
    /// A Zheus One mission, with the minutes until the arrival when the user said them.
    Mission { mission: String, eta: Option<u32> },
}

/// A checked call, its gate, and the one line that says what it does (the command, the path, the address).
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    pub tool: String,
    pub action: Action,
    pub gate: Gate,
    pub summary: String,
    /// The path or the command's folder is outside the working folder.
    pub outside: bool,
    /// The path looks like a secret.
    pub secret: bool,
}

/// What an approval card shows. The island shows all of it; a phone gets `tool` and `summary`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub tool: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// For a file change: the text before (None for a new file) and after, when small enough to show as a diff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// A write that creates the file (whether or not the text is small enough to show).
    pub new_file: bool,
    pub outside: bool,
    pub secret: bool,
    /// Why it asks, when another agent said so (Hermes: "recursive delete").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Prepared {
    pub fn request(&self) -> Request {
        let small = |s: &str| (s.len() <= MAX_PREVIEW).then(|| s.to_string());
        let (cwd, before, after) = match &self.action {
            Action::Run { cwd, .. } => (Some(policy::display(cwd)), None, None),
            Action::Write { content, before, .. } => {
                let fits = content.len() <= MAX_PREVIEW && before.as_ref().is_none_or(|b| b.len() <= MAX_PREVIEW);
                if fits { (None, before.clone(), Some(content.clone())) } else { (None, None, None) }
            }
            Action::Edit { before, after, .. } => match (small(before), small(after)) {
                (Some(b), Some(a)) => (None, Some(b), Some(a)),
                _ => (None, None, None),
            },
            _ => (None, None, None),
        };
        let new_file = matches!(&self.action, Action::Write { before: None, .. });
        Request { tool: self.tool.clone(), summary: self.summary.clone(), cwd, before, after, new_file, outside: self.outside, secret: self.secret, reason: None }
    }
}

/// What a call returned: the text for the model, whether it failed, and a short `detail` for the step ("exit 0",
/// "120 lines"). `output` is what the island may show under the step (a command's last lines, an error).
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub text: String,
    pub is_error: bool,
    pub detail: Option<String>,
    pub output: Option<String>,
}

impl Output {
    pub fn error(text: impl Into<String>) -> Output {
        let text = text.into();
        Output { output: Some(text::one_line(&text, 300)), text, is_error: true, detail: None }
    }

    fn ok(text: String, detail: Option<String>) -> Output {
        Output { text: text::clip_middle(&text, MAX_RESULT), is_error: false, detail, output: None }
    }
}

// ── Preparing a call ──────────────────────────────────────────────────────────

fn str_arg<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}

fn required<'a>(input: &'a Value, key: &str) -> Result<&'a str, String> {
    str_arg(input, key).ok_or_else(|| format!("The \"{key}\" argument is missing or is not text."))
}

/// A whole number argument; models sometimes send "20" for 20.
fn number(input: &Value, key: &str) -> Option<u64> {
    let v = input.get(key)?;
    v.as_u64().or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64)).or_else(|| v.as_str()?.trim().parse().ok())
}

fn flag(input: &Value, key: &str) -> bool {
    match input.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

fn web_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|_| format!("\"{raw}\" is not a web address."))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Only http:// and https:// addresses can be used.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("The address must not carry a user name or password.".into());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(format!("\"{raw}\" has no host."));
    }
    Ok(url)
}

/// Checks a call and works out what it will do. An Err is sent back to the model as the call's (failed) result.
pub fn prepare(call: &Call, env: &Env) -> Result<Prepared, String> {
    if let Some(why) = &call.broken {
        return Err(format!("The arguments of this call could not be read ({why}). Send them again as a JSON object."));
    }
    let input = &call.input;
    let resolve = |raw: &str| policy::resolve(&env.folder, &env.home, raw);
    let place = |path: &Path| (policy::inside(&env.folder, path), policy::looks_secret(path));
    let (action, effect, path_for_gate, summary): (Action, Effect, Option<PathBuf>, String) = match call.name.as_str() {
        "list_directory" => {
            let path = resolve(str_arg(input, "path").unwrap_or(""))?;
            let summary = policy::display(&path);
            (Action::List { path: path.clone() }, Effect::Read, Some(path), summary)
        }
        "read_file" => {
            let path = resolve(required(input, "path")?)?;
            let offset = number(input, "offset").unwrap_or(1).max(1) as usize;
            let limit = (number(input, "limit").unwrap_or(DEFAULT_LINES as u64) as usize).clamp(1, MAX_LINES);
            let summary = policy::display(&path);
            (Action::Read { path: path.clone(), offset, limit }, Effect::Read, Some(path), summary)
        }
        "find_files" => {
            let pattern = required(input, "pattern")?.trim().to_string();
            if pattern.is_empty() || pattern.len() > 300 {
                return Err("The pattern must be between 1 and 300 characters.".into());
            }
            glob_regex(&pattern)?;
            let root = resolve(str_arg(input, "path").unwrap_or(""))?;
            let summary = format!("{pattern} · {}", policy::display(&root));
            (Action::Find { root: root.clone(), pattern }, Effect::Read, Some(root), summary)
        }
        "search_text" => {
            let pattern = required(input, "pattern")?.to_string();
            if pattern.is_empty() || pattern.len() > 1000 {
                return Err("The pattern must be between 1 and 1000 characters.".into());
            }
            let case_insensitive = flag(input, "case_insensitive");
            line_regex(&pattern, case_insensitive)?;
            let glob = str_arg(input, "glob").map(str::trim).filter(|g| !g.is_empty()).map(str::to_string);
            if let Some(g) = &glob {
                glob_regex(g)?;
            }
            let root = resolve(str_arg(input, "path").unwrap_or(""))?;
            let summary = format!("{pattern} · {}", policy::display(&root));
            (Action::Search { root: root.clone(), pattern, glob, case_insensitive }, Effect::Read, Some(root), summary)
        }
        "write_file" => {
            let path = resolve(required(input, "path")?)?;
            let content = required(input, "content")?.to_string();
            if content.len() > MAX_WRITE {
                return Err(format!("The content is too large ({} bytes, at most {MAX_WRITE}). Write it in parts.", content.len()));
            }
            if path.is_dir() {
                return Err(format!("{} is a folder.", policy::display(&path)));
            }
            let before = if path.is_file() { Some(read_text(&path)?) } else { None };
            let summary = policy::display(&path);
            (Action::Write { path: path.clone(), content, before }, Effect::Write, Some(path), summary)
        }
        "edit_file" => {
            let path = resolve(required(input, "path")?)?;
            let old_text = required(input, "old_text")?;
            let new_text = required(input, "new_text")?;
            if old_text.is_empty() {
                return Err("old_text is empty. To write a whole file, use write_file.".into());
            }
            if !path.is_file() {
                return Err(format!("{} does not exist. To create it, use write_file.", policy::display(&path)));
            }
            let before = read_text(&path)?;
            let (after, count) = replace(&before, old_text, new_text, flag(input, "replace_all"))?;
            let removed = old_text.lines().count().max(1) * count;
            let added = if new_text.is_empty() { 0 } else { new_text.lines().count().max(1) * count };
            let summary = format!("{} (+{added} −{removed})", policy::display(&path));
            (Action::Edit { path: path.clone(), before, after, added, removed }, Effect::Write, Some(path), summary)
        }
        "run_command" => {
            let command = required(input, "command")?.trim().to_string();
            if command.is_empty() {
                return Err("The command is empty.".into());
            }
            if command.len() > MAX_COMMAND {
                return Err(format!("The command is too long (at most {MAX_COMMAND} characters). Put long scripts in a file first."));
            }
            let cwd = resolve(str_arg(input, "cwd").unwrap_or(""))?;
            if !cwd.is_dir() {
                return Err(format!("The folder {} does not exist.", policy::display(&cwd)));
            }
            let timeout = Duration::from_secs(number(input, "timeout_seconds").unwrap_or(DEFAULT_TIMEOUT).clamp(1, MAX_TIMEOUT));
            let summary = command.clone();
            (Action::Run { command, cwd: cwd.clone(), timeout }, Effect::Command, Some(cwd), summary)
        }
        "fetch_url" => {
            let url = web_url(required(input, "url")?)?;
            let summary = url.to_string();
            (Action::Fetch { url }, Effect::Fetch, None, summary)
        }
        "open_url" => {
            let url = web_url(required(input, "url")?)?;
            let summary = url.to_string();
            (Action::Open { url }, Effect::Open, None, summary)
        }
        "system_info" => (Action::Info, Effect::Info, None, String::new()),
        "home_devices" => {
            let h = env.house.as_ref().ok_or(NO_HOME)?;
            let device = match str_arg(input, "device").map(str::trim).filter(|d| !d.is_empty()) {
                Some(d) => Some(home::find(&h.devices, d)?.id.clone()),
                None => None,
            };
            (Action::Home(HomeCall::Devices { device }), Effect::Info, None, String::new())
        }
        "home_control" => {
            let h = env.house.as_ref().ok_or(NO_HOME)?;
            let device = home::find(&h.devices, required(input, "device")?)?;
            let action = HomeAction::parse(required(input, "action")?, input.get("value"))?;
            home::check(device, &action)?;
            // The app and the channel as the device lists them, so the card says "Open YouTube", not "youtube".
            let action = match action {
                HomeAction::OpenApp(app) => HomeAction::OpenApp(home::resolve_app(device, &app)?),
                HomeAction::SetChannel(c) => HomeAction::SetChannel(home::resolve_channel(device, &c)?.number),
                other => other,
            };
            let summary = format!("{} · {}", device.name, action.label());
            (Action::Home(HomeCall::Control { device: device.id.clone(), action }), Effect::Home { ask: device.ask }, None, summary)
        }
        "home_routine" => {
            let h = env.house.as_ref().ok_or(NO_HOME)?;
            let r = find_routine(&h.routines, required(input, "routine")?)?;
            if !r.problems.is_empty() {
                return Err(format!(
                    "The routine \"{}\" cannot run as it is ({}). Tell the user to fix it in Settings → Mochi's home.",
                    r.name,
                    r.problems.join("; ")
                ));
            }
            (Action::Home(HomeCall::Routine { id: r.id.clone() }), Effect::Home { ask: r.asks }, None, r.name.clone())
        }
        "zheus_world" => {
            let h = env.house.as_ref().ok_or(NO_HOME)?;
            if !h.zheus {
                return Err(NO_ZHEUS.into());
            }
            (Action::Home(HomeCall::World), Effect::Info, None, String::new())
        }
        "zheus_mission" => {
            let h = env.house.as_ref().ok_or(NO_HOME)?;
            if !h.zheus {
                return Err(NO_ZHEUS.into());
            }
            let mission = required(input, "mission")?.trim().to_ascii_lowercase();
            if !matches!(mission.as_str(), "arrival" | "situation") {
                return Err(format!("\"{mission}\" is not a mission. Use arrival or situation."));
            }
            let eta = match input.get("eta_minutes") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
                        .filter(|m| (1..=240).contains(m))
                        .ok_or("eta_minutes must be a number of minutes from 1 to 240.")? as u32,
                ),
            };
            // Showing a plan acts on nothing: the user's yes runs it, in Coucou.
            (Action::Home(HomeCall::Mission { mission: mission.clone(), eta }), Effect::Info, None, mission)
        }
        name if POWER_TOOLS.contains(&name) => {
            if env.powers.is_none() {
                return Err(NO_POWERS.into());
            }
            let input = if input.is_object() { input.clone() } else { json!({}) };
            let summary = power_summary(&input);
            // They read the official data and keep the user's own records in Coucou: nothing to approve.
            (Action::Power { tool: name.to_string(), input }, Effect::Info, None, summary)
        }
        other => return Err(format!("There is no tool named \"{other}\".")),
    };
    let (inside, secret) = path_for_gate.as_deref().map_or((true, false), place);
    let gate = policy::gate(env.level, effect, inside, secret);
    Ok(Prepared { tool: call.name.clone(), action, gate, summary, outside: !inside, secret })
}

/// A superpower call in a few words, for its step: the asset or what it does ("GARE11", "record").
fn power_summary(input: &Value) -> String {
    let word = |key: &str| str_arg(input, key).map(str::trim).filter(|w| !w.is_empty() && w.chars().count() <= 40);
    match (word("ticker"), word("action")) {
        (Some(t), Some(a)) => format!("{a} · {t}"),
        (Some(t), None) => t.to_uppercase(),
        (None, Some(a)) => a.to_string(),
        // wealth_trade: the trades' tickers ("GGRC11 +1").
        (None, None) => {
            let tickers: Vec<String> = input["trades"].as_array().into_iter().flatten().filter_map(|t| t["ticker"].as_str()).map(|t| t.trim().to_uppercase()).filter(|t| !t.is_empty() && t.len() <= 12).collect();
            match tickers.as_slice() {
                [] => String::new(),
                [one] => one.clone(),
                [first, rest @ ..] => format!("{first} +{}", rest.len()),
            }
        }
    }
}

/// The routine `query` names: its id, or its name (case and accents aside).
fn find_routine<'a>(routines: &'a [RoutineInfo], query: &str) -> Result<&'a RoutineInfo, String> {
    let q = home::normalize(query);
    routines
        .iter()
        .find(|r| r.id == query.trim() || home::normalize(&r.name) == q)
        .ok_or_else(|| format!("No routine is called \"{query}\". The user's routines: {}.", routines.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", ")))
}

/// The devices as the model reads them. `one`: that device in full (all its apps and channels).
fn home_list(devices: &[Device], one: Option<&str>) -> String {
    let shown: Vec<&Device> = devices.iter().filter(|d| one.is_none_or(|id| d.id == id)).collect();
    if shown.is_empty() {
        return "No device: the user has not ticked any for Mochi in Settings → Mochi's home.".into();
    }
    let (max_apps, max_channels) = if one.is_some() { (300, 600) } else { (30, 25) };
    let mut out = format!("Devices Mochi may use ({}):\n", shown.len());
    for d in shown {
        let room = if d.room.is_empty() { String::new() } else { format!(" in {}", d.room) };
        let model = d.model.as_deref().map(|m| format!(", model {m}")).unwrap_or_default();
        let can: Vec<&str> = d.caps.iter().map(|c| c.as_str()).collect();
        out.push_str(&format!(
            "- {} — \"{}\"{room} ({}, {}{model}): {}. Can: {}.",
            d.id,
            d.name,
            d.kind.as_str(),
            d.source,
            crate::smart_home::state_text(&d.state),
            if can.is_empty() { "nothing".to_string() } else { can.join(", ") }
        ));
        if !d.apps.is_empty() {
            let more = d.apps.len().saturating_sub(max_apps);
            out.push_str(&format!(" Apps ({}): {}{}.", d.apps.len(), d.apps.iter().take(max_apps).cloned().collect::<Vec<_>>().join(", "), if more > 0 { format!(", and {more} more") } else { String::new() }));
        }
        if !d.channels.is_empty() {
            let more = d.channels.len().saturating_sub(max_channels);
            let list: Vec<String> = d.channels.iter().take(max_channels).map(|c| format!("{} {}", c.number, c.name)).collect();
            out.push_str(&format!(" Channels ({}): {}{}.", d.channels.len(), list.join(", "), if more > 0 { format!(", and {more} more (ask for this device alone to see all)") } else { String::new() }));
        }
        if d.ask {
            out.push_str(" Each action waits for the user's approval.");
        }
        out.push('\n');
    }
    out
}

/// A text file, whole. Refused when it is large or not text.
fn read_text(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Cannot read {}: {e}", policy::display(path)))?;
    if meta.len() > MAX_FILE {
        return Err(format!("{} is too large ({} bytes) to work on whole.", policy::display(path), meta.len()));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", policy::display(path)))?;
    if text::looks_binary(&bytes) {
        return Err(format!("{} is not a text file.", policy::display(path)));
    }
    String::from_utf8(bytes).map_err(|_| format!("{} is not UTF-8 text.", policy::display(path)))
}

/// `text` with `old` replaced by `new`: once (it must appear exactly once) or everywhere. A file with Windows line
/// ends is matched with the model's plain `\n` turned into `\r\n`.
fn replace(text: &str, old: &str, new: &str, all: bool) -> Result<(String, usize), String> {
    let mut count = text.matches(old).count();
    let (old, new) = if count == 0 && text.contains("\r\n") && old.contains('\n') && !old.contains("\r\n") {
        let (o, n) = (old.replace('\n', "\r\n"), new.replace('\n', "\r\n"));
        count = text.matches(o.as_str()).count();
        (o, n)
    } else {
        (old.to_string(), new.to_string())
    };
    match count {
        0 => Err("old_text was not found in the file. Read the file again and copy the text exactly, spaces and line ends included.".into()),
        n if n > 1 && !all => Err(format!("old_text appears {n} times. Give more of the surrounding text so it appears once, or set replace_all.")),
        _ => {
            let after = if all { text.replace(&old, &new) } else { text.replacen(&old, &new, 1) };
            if after == text {
                return Err("This would change nothing: old_text and new_text are the same.".into());
            }
            Ok((after, count))
        }
    }
}

// ── Running a call ────────────────────────────────────────────────────────────

/// Runs a prepared call. Never asks anything: the gate was settled before.
pub async fn execute(prepared: &Prepared, env: &Env) -> Output {
    match &prepared.action {
        Action::List { path } => list(path),
        Action::Read { path, offset, limit } => read(path, *offset, *limit),
        Action::Find { root, pattern } => find(root, pattern),
        Action::Search { root, pattern, glob, case_insensitive } => search(root, pattern, glob.as_deref(), *case_insensitive),
        Action::Write { path, content, before } => write(path, content, before.as_deref()),
        Action::Edit { path, before, after, added, removed } => edit(path, before, after, *added, *removed),
        Action::Run { command, cwd, timeout } => run(command, cwd, *timeout, env).await,
        Action::Fetch { url } => fetch(url).await,
        Action::Open { url } => {
            crate::platform::open_url(url.as_str());
            Output::ok("Opened in the user's browser.".into(), None)
        }
        Action::Info => Output::ok(system_info(env), None),
        Action::Home(call) => home_execute(call, env).await,
        Action::Power { tool, input } => match &env.powers {
            Some(p) => from_result(p.call(tool, input).await),
            None => Output::error(NO_POWERS),
        },
    }
}

/// What a house action or a superpower gave, as a step.
fn from_result(result: HomeResult) -> Output {
    Output {
        text: text::clip_middle(&result.text, MAX_RESULT),
        is_error: !result.ok,
        detail: (!result.detail.is_empty()).then(|| text::one_line(&result.detail, 120)),
        output: (!result.ok).then(|| text::one_line(&result.detail, 300)),
    }
}

async fn home_execute(call: &HomeCall, env: &Env) -> Output {
    let Some(h) = &env.house else { return Output::error(NO_HOME) };
    let result = match call {
        HomeCall::Devices { device } => {
            let devices = h.runner.devices().await;
            return Output::ok(home_list(&devices, device.as_deref()), Some(devices.len().to_string()));
        }
        HomeCall::Control { device, action } => h.runner.control(device, action).await,
        HomeCall::Routine { id } => h.runner.routine(id).await,
        HomeCall::World => h.runner.world().await,
        HomeCall::Mission { mission, eta } => h.runner.mission(mission, *eta).await,
    };
    from_result(result)
}

fn human_size(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        b if b < 1024 * 1024 * 1024 => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
        b => format!("{:.1} GB", b as f64 / (1024.0 * 1024.0 * 1024.0)),
    }
}

fn list(path: &Path) -> Output {
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(e) => return Output::error(format!("Cannot list {}: {e}", policy::display(path))),
    };
    let mut rows: Vec<(bool, String, u64)> = entries
        .filter_map(Result::ok)
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let meta = e.metadata().ok();
            let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
            (is_dir, name, meta.map_or(0, |m| m.len()))
        })
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    let total = rows.len();
    let mut out = format!("{} ({total} entries)\n", policy::display(path));
    for (is_dir, name, size) in rows.iter().take(MAX_LIST) {
        if *is_dir {
            out.push_str(&format!("{name}/\n"));
        } else {
            out.push_str(&format!("{name}  ({})\n", human_size(*size)));
        }
    }
    if total > MAX_LIST {
        out.push_str(&format!("[… {} more not shown]\n", total - MAX_LIST));
    }
    Output::ok(out, Some(format!("{total}")))
}

fn read(path: &Path, offset: usize, limit: usize) -> Output {
    if path.is_dir() {
        return Output::error(format!("{} is a folder: use list_directory.", policy::display(path)));
    }
    let text = match read_text(path) {
        Ok(t) => t,
        Err(e) => return Output::error(e),
    };
    let (out, total) = text::numbered(&text, offset, limit, MAX_READ);
    Output { text: out, is_error: false, detail: Some(format!("{total}")), output: None }
}

/// A glob as a regular expression: `**/` any folders, `*` within a name, `?` one character, `{a,b}` either.
pub fn glob_regex(glob: &str) -> Result<regex::Regex, String> {
    let mut re = String::from("^");
    let chars: Vec<char> = glob.replace('\\', "/").chars().collect();
    let mut i = 0;
    let mut braces = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '*' if chars.get(i + 1) == Some(&'*') => {
                if chars.get(i + 2) == Some(&'/') {
                    re.push_str("(?:.*/)?");
                    i += 3;
                } else {
                    re.push_str(".*");
                    i += 2;
                }
                continue;
            }
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            '{' => {
                braces += 1;
                re.push_str("(?:");
            }
            '}' if braces > 0 => {
                braces -= 1;
                re.push(')');
            }
            ',' if braces > 0 => re.push('|'),
            other => re.push_str(&regex::escape(&other.to_string())),
        }
        i += 1;
    }
    if braces > 0 {
        return Err(format!("The pattern \"{glob}\" has an unclosed brace."));
    }
    re.push('$');
    regex::RegexBuilder::new(&re)
        .case_insensitive(cfg!(windows))
        .build()
        .map_err(|_| format!("\"{glob}\" is not a usable pattern."))
}

fn line_regex(pattern: &str, case_insensitive: bool) -> Result<regex::Regex, String> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(case_insensitive)
        .size_limit(1 << 20)
        .build()
        .map_err(|e| format!("The regular expression is not valid: {}", text::one_line(&e.to_string(), 300)))
}

/// Every file under `root` (or `root` itself if it is a file), skipping the usual heavy folders below it and never
/// following a link to a folder. `visit` gets the file and its path relative to `root` with `/`; it returns false
/// to stop. Returns false when the walk was cut short by MAX_VISITED.
fn walk(root: &Path, mut visit: impl FnMut(&Path, &str) -> bool) -> bool {
    if root.is_file() {
        let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        visit(root, &name);
        return true;
    }
    let mut stack = vec![(root.to_path_buf(), String::new())];
    let mut visited = 0;
    while let Some((dir, rel)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        // Pushed in reverse so the walk goes in name order.
        let mut subdirs = Vec::new();
        for entry in entries {
            visited += 1;
            if visited > MAX_VISITED {
                return false;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let child_rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if !SKIPPED_DIRS.contains(&name.as_str()) {
                    subdirs.push((entry.path(), child_rel));
                }
            } else if (kind.is_file() || kind.is_symlink()) && !visit(&entry.path(), &child_rel) {
                return true;
            }
        }
        stack.extend(subdirs.into_iter().rev());
    }
    true
}

fn find(root: &Path, pattern: &str) -> Output {
    if !root.exists() {
        return Output::error(format!("{} does not exist.", policy::display(root)));
    }
    let Ok(re) = glob_regex(pattern) else { return Output::error(format!("\"{pattern}\" is not a usable pattern.")) };
    let by_name = !pattern.contains('/') && !pattern.contains('\\');
    let mut found = Vec::new();
    let complete = walk(root, |_, rel| {
        let subject = if by_name { rel.rsplit('/').next().unwrap_or(rel) } else { rel };
        if re.is_match(subject) {
            found.push(rel.to_string());
        }
        found.len() < MAX_FOUND
    });
    found.sort();
    let mut out = if found.is_empty() { format!("No file matches {pattern}.\n") } else { found.join("\n") + "\n" };
    if found.len() >= MAX_FOUND {
        out.push_str(&format!("[stopped at {MAX_FOUND} files: use a narrower pattern or folder]\n"));
    } else if !complete {
        out.push_str("[the folder is very large: the search stopped before the end]\n");
    }
    Output::ok(out, Some(format!("{}", found.len())))
}

fn search(root: &Path, pattern: &str, glob: Option<&str>, case_insensitive: bool) -> Output {
    if !root.exists() {
        return Output::error(format!("{} does not exist.", policy::display(root)));
    }
    let re = match line_regex(pattern, case_insensitive) {
        Ok(r) => r,
        Err(e) => return Output::error(e),
    };
    let names = match glob.map(glob_regex).transpose() {
        Ok(n) => n,
        Err(e) => return Output::error(e),
    };
    let mut matches = Vec::new();
    let mut files = 0;
    let mut skipped = 0;
    let root_real = root.canonicalize().ok();
    let complete = walk(root, |path, rel| {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        if names.as_ref().is_some_and(|n| !n.is_match(name)) {
            return true;
        }
        // A search never opens what read_file would ask about first: a file that looks like it holds secrets, or a
        // link that leads out of the folder searched. Only the file the search was pointed at (and approved) is.
        let leads_out = || !root_real.as_ref().is_some_and(|r| policy::real(path).is_some_and(|p| p.starts_with(r)));
        if path != root && (policy::looks_secret(path) || leads_out()) {
            skipped += 1;
            return true;
        }
        files += 1;
        if files > MAX_SEARCHED_FILES {
            return false;
        }
        if std::fs::metadata(path).map_or(true, |m| m.len() > MAX_SEARCHED_SIZE) {
            return true;
        }
        let Ok(bytes) = std::fs::read(path) else { return true };
        if text::looks_binary(&bytes) {
            return true;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (n, line) in text.lines().enumerate() {
            if re.is_match(line) {
                matches.push(format!("{rel}:{}: {}", n + 1, text::one_line(line, 300)));
                if matches.len() >= MAX_MATCHES {
                    return false;
                }
            }
        }
        true
    });
    let mut out = if matches.is_empty() { format!("No line matches {pattern}.\n") } else { matches.join("\n") + "\n" };
    if matches.len() >= MAX_MATCHES {
        out.push_str(&format!("[stopped at {MAX_MATCHES} matching lines: narrow the pattern, the folder or the glob]\n"));
    } else if !complete || files > MAX_SEARCHED_FILES {
        out.push_str("[the folder is very large: the search stopped before the end]\n");
    }
    if skipped > 0 {
        out.push_str(&format!(
            "[{skipped} file(s) not searched: they may hold secrets or lead outside this folder. read_file can open one; it asks the user first]\n"
        ));
    }
    Output::ok(out, Some(format!("{}", matches.len())))
}

/// The file still holds what it held when the change was approved; otherwise the change is not made.
fn unchanged_since(path: &Path, before: Option<&str>) -> Result<(), String> {
    let now = if path.is_file() { Some(read_text(path)?) } else { None };
    if now.as_deref() != before {
        return Err(format!("{} changed since this edit was prepared: nothing was written. Read it again.", policy::display(path)));
    }
    Ok(())
}

fn write(path: &Path, content: &str, before: Option<&str>) -> Output {
    if let Err(e) = unchanged_since(path, before) {
        return Output::error(e);
    }
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Output::error(format!("Cannot create {}: {e}", policy::display(parent)));
        }
    }
    match std::fs::write(path, content) {
        Ok(()) => {
            let lines = content.lines().count();
            let what = if before.is_some() { "Replaced" } else { "Created" };
            Output::ok(format!("{what} {} ({lines} lines).", policy::display(path)), Some(format!("{lines}")))
        }
        Err(e) => Output::error(format!("Cannot write {}: {e}", policy::display(path))),
    }
}

fn edit(path: &Path, before: &str, after: &str, added: usize, removed: usize) -> Output {
    if let Err(e) = unchanged_since(path, Some(before)) {
        return Output::error(e);
    }
    match std::fs::write(path, after) {
        Ok(()) => Output::ok(format!("Edited {}.", policy::display(path)), Some(format!("+{added} −{removed}"))),
        Err(e) => Output::error(format!("Cannot write {}: {e}", policy::display(path))),
    }
}

async fn run(command: &str, cwd: &Path, timeout: Duration, env: &Env) -> Output {
    let (shell, command, cwd, cancel) = (env.shell.clone(), command.to_string(), cwd.to_path_buf(), env.cancel.clone());
    let ran = tokio::task::spawn_blocking(move || shell::run(&shell, &command, &cwd, timeout, &cancel)).await;
    let ran = match ran {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Output::error(e),
        Err(_) => return Output::error("The command could not be run."),
    };
    let mut head = match (ran.code, ran.timed_out, ran.canceled) {
        (_, true, _) => format!("Stopped after {} s (the time limit). What it printed before:", timeout.as_secs()),
        (_, _, true) => "Stopped by the user.".to_string(),
        (Some(code), ..) => format!("Exit code: {code}"),
        (None, ..) => "Ended by a signal.".to_string(),
    };
    if ran.left_running {
        head.push_str("\n(Something the command started is still running in the background; what it prints from now on is not shown.)");
    }
    let body = if ran.output.trim().is_empty() { "(no output)".to_string() } else { ran.output.clone() };
    let detail = match ran.code {
        _ if ran.timed_out => "timeout".to_string(),
        _ if ran.canceled => "stopped".to_string(),
        Some(code) => format!("exit {code}"),
        None => "signal".to_string(),
    };
    let tail = text::clip_middle(ran.output.trim_end(), 4 * 1024);
    Output {
        text: text::clip_middle(&format!("{head}\n{body}"), MAX_RESULT),
        is_error: false,
        detail: Some(detail),
        output: (!tail.is_empty()).then_some(tail),
    }
}

async fn fetch(url: &Url) -> Output {
    let client = match crate::net::client(url, Duration::from_secs(20)) {
        Ok(c) => c,
        Err(e) => return Output::error(e),
    };
    let response = match client
        .get(url.clone())
        .header("User-Agent", "Coucou (Mochi's hands)")
        .header("Accept", "text/html, text/plain, application/json, */*;q=0.5")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return Output::error(format!("The request failed: {}", text::one_line(&e.to_string(), 300))),
    };
    let status = response.status();
    let final_url = response.url().to_string();
    let kind = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = match crate::net::read_capped(response, MAX_FETCH).await {
        Ok(b) => b,
        Err(e) => return Output::error(e),
    };
    let body = if kind.contains("html") || (kind.is_empty() && bytes.starts_with(b"<")) {
        text::html_to_text(&String::from_utf8_lossy(&bytes))
    } else if kind.starts_with("text/") || kind.contains("json") || kind.contains("xml") || kind.contains("javascript") || kind.is_empty() {
        if text::looks_binary(&bytes) {
            format!("[binary content, {} bytes]", bytes.len())
        } else {
            String::from_utf8_lossy(&bytes).to_string()
        }
    } else {
        format!("[{kind} content, {} bytes: not text]", bytes.len())
    };
    let (body, cut) = text::clip_end(&body, MAX_RESULT - 512);
    let mut out = format!("URL: {final_url}\nStatus: {}\n\n{body}", status.as_u16());
    if cut {
        out.push_str("\n[the rest of the page was cut]");
    }
    Output { text: out, is_error: !status.is_success(), detail: Some(format!("{}", status.as_u16())), output: None }
}

fn system_info(env: &Env) -> String {
    let t = crate::platform::local_time();
    format!(
        "Operating system: {} ({})\nShell for run_command: {}\nWorking folder: {}\nHome folder: {}\nLocal date and time: {:04}-{:02}-{:02} {:02}:{:02}\nPermission level: {}",
        if cfg!(windows) { "Windows" } else { std::env::consts::OS },
        std::env::consts::ARCH,
        env.shell.describe(),
        policy::display(&env.folder),
        policy::display(&env.home),
        t.year, t.month, t.day, t.hour, t.minute,
        match env.level {
            Level::ReadOnly => "read only: nothing can be changed and no command can run",
            Level::Ask => "every change, command and web request waits for the user's approval",
            Level::Edits => "file edits inside the working folder are applied directly; commands and web requests wait for approval",
        },
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("coucou-agent-tools-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("src/deep")).unwrap();
            std::fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
            std::fs::write(dir.join("src/main.rs"), "fn main() {\n    println!(\"hello\");\n}\n").unwrap();
            std::fs::write(dir.join("src/deep/lib.rs"), "pub fn add(a: i32) -> i32 {\n    a + 1\n}\n").unwrap();
            std::fs::write(dir.join("README.md"), "# Title\nHello world\n").unwrap();
            std::fs::write(dir.join("node_modules/pkg/index.js"), "hello from a dependency\n").unwrap();
            std::fs::write(dir.join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
            Scratch(dir)
        }

        fn env(&self, level: Level) -> Env {
            Env {
                folder: self.0.clone(),
                home: self.0.join("home"),
                level,
                shell: Shell::detect(),
                cancel: Arc::new(AtomicBool::new(false)),
                web_search: false,
                house: None,
                powers: None,
            }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn call(name: &str, input: Value) -> Call {
        Call { id: "c1".into(), name: name.into(), input, broken: None }
    }

    fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    fn run_call(env: &Env, name: &str, input: Value) -> Output {
        let prepared = prepare(&call(name, input), env).unwrap_or_else(|e| panic!("{name}: {e}"));
        block_on(execute(&prepared, env))
    }

    #[test]
    fn read_only_offers_nothing_that_changes_anything() {
        let s = Scratch::new("defs");
        let names = |level| definitions(&s.env(level)).iter().map(|d| d.name).collect::<Vec<_>>();
        let read_only = names(Level::ReadOnly);
        for tool in ["write_file", "edit_file", "run_command"] {
            assert!(!read_only.contains(&tool), "{tool}");
            assert!(names(Level::Ask).contains(&tool), "{tool}");
        }
        for tool in ["list_directory", "read_file", "find_files", "search_text", "fetch_url", "open_url", "system_info"] {
            assert!(read_only.contains(&tool), "{tool}");
        }
        // A model that calls one anyway is refused.
        let p = prepare(&call("write_file", json!({"path":"x.txt","content":"x"})), &s.env(Level::ReadOnly)).unwrap();
        assert!(matches!(p.gate, Gate::Refuse(_)));
        // Every schema is an object with properties, which every provider accepts.
        for d in definitions(&s.env(Level::Ask)) {
            assert_eq!(d.schema["type"], "object", "{}", d.name);
            assert!(d.schema["properties"].is_object(), "{}", d.name);
            assert!(d.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        }
    }

    #[test]
    fn looking_in_the_folder_is_free_and_elsewhere_asks() {
        let s = Scratch::new("gates");
        let env = s.env(Level::Ask);
        let gate = |name: &str, input: Value| prepare(&call(name, input), &env).unwrap().gate;
        assert_eq!(gate("read_file", json!({"path":"src/main.rs"})), Gate::Free);
        assert_eq!(gate("list_directory", json!({})), Gate::Free);
        assert_eq!(gate("read_file", json!({"path":"../elsewhere.txt"})), Gate::Ask);
        assert_eq!(gate("write_file", json!({"path":"new.txt","content":"x"})), Gate::Ask);
        assert_eq!(gate("run_command", json!({"command":"echo hi"})), Gate::Ask);
        assert_eq!(gate("fetch_url", json!({"url":"https://example.com"})), Gate::Ask);
        assert_eq!(gate("system_info", json!({})), Gate::Free);
        let edits = s.env(Level::Edits);
        assert_eq!(prepare(&call("write_file", json!({"path":"new.txt","content":"x"})), &edits).unwrap().gate, Gate::Free);
        assert_eq!(prepare(&call("write_file", json!({"path":".env","content":"x"})), &edits).unwrap().gate, Gate::Ask);
        assert_eq!(prepare(&call("run_command", json!({"command":"ls"})), &edits).unwrap().gate, Gate::Ask);
    }

    #[test]
    fn bad_calls_say_what_is_wrong() {
        let s = Scratch::new("bad");
        let env = s.env(Level::Ask);
        let err = |name: &str, input: Value| prepare(&call(name, input), &env).unwrap_err();
        assert!(err("read_file", json!({})).contains("\"path\""));
        assert!(err("nope", json!({})).contains("no tool"));
        assert!(err("edit_file", json!({"path":"missing.rs","old_text":"a","new_text":"b"})).contains("write_file"));
        assert!(err("edit_file", json!({"path":"src/main.rs","old_text":"absent","new_text":"b"})).contains("not found"));
        assert!(err("fetch_url", json!({"url":"file:///etc/passwd"})).contains("http"));
        assert!(err("fetch_url", json!({"url":"https://user:pw@example.com"})).contains("password"));
        assert!(err("run_command", json!({"command":"  "})).contains("empty"));
        assert!(err("run_command", json!({"command":"ls","cwd":"no/such/dir"})).contains("does not exist"));
        assert!(err("search_text", json!({"pattern":"(unclosed"})).contains("not valid"));
        assert!(err("find_files", json!({"pattern":"{a,b"})).contains("brace"));
        assert!(err("write_file", json!({"path":"src","content":"x"})).contains("folder"));
        let broken = Call { broken: Some("expected value".into()), ..call("read_file", json!({})) };
        assert!(prepare(&broken, &env).unwrap_err().contains("could not be read"));
        // Numbers given as text are accepted.
        let p = prepare(&call("read_file", json!({"path":"README.md","offset":"2","limit":"1"})), &env).unwrap();
        assert!(matches!(p.action, Action::Read { offset: 2, limit: 1, .. }));
    }

    #[test]
    fn listing_reading_finding_and_searching() {
        let s = Scratch::new("look");
        let env = s.env(Level::Ask);
        let listed = run_call(&env, "list_directory", json!({}));
        assert!(listed.text.contains("src/\n") && listed.text.contains("README.md  ("), "{}", listed.text);
        assert!(listed.text.find("src/").unwrap() < listed.text.find("README.md").unwrap(), "folders first");

        let read = run_call(&env, "read_file", json!({"path":"src/main.rs"}));
        assert_eq!(read.text, "     1\tfn main() {\n     2\t    println!(\"hello\");\n     3\t}\n");
        assert!(run_call(&env, "read_file", json!({"path":"blob.bin"})).is_error);
        assert!(run_call(&env, "read_file", json!({"path":"src"})).text.contains("list_directory"));

        let found = run_call(&env, "find_files", json!({"pattern":"*.rs"}));
        assert_eq!(found.text, "src/deep/lib.rs\nsrc/main.rs\n");
        let found = run_call(&env, "find_files", json!({"pattern":"src/**/*.rs"}));
        assert_eq!(found.text, "src/deep/lib.rs\nsrc/main.rs\n");
        let found = run_call(&env, "find_files", json!({"pattern":"*.{md,js}"}));
        assert_eq!(found.text, "README.md\n", "node_modules is skipped");
        // Pointed into a skipped folder, it is searched.
        let found = run_call(&env, "find_files", json!({"pattern":"*.js","path":"node_modules"}));
        assert_eq!(found.text, "pkg/index.js\n");

        let hits = run_call(&env, "search_text", json!({"pattern":"hello"}));
        assert_eq!(hits.text, "src/main.rs:2: println!(\"hello\");\n", "case-sensitive, and node_modules is skipped");
        let hits = run_call(&env, "search_text", json!({"pattern":"hello","case_insensitive":true}));
        assert!(hits.text.contains("README.md:2: Hello world") && hits.text.contains("src/main.rs:2:"), "{}", hits.text);
        assert!(!hits.text.contains("node_modules"));
        let hits = run_call(&env, "search_text", json!({"pattern":"fn","glob":"lib.rs"}));
        assert_eq!(hits.text, "src/deep/lib.rs:1: pub fn add(a: i32) -> i32 {\n");
        let none = run_call(&env, "search_text", json!({"pattern":"zzz"}));
        assert!(none.text.starts_with("No line matches"));
    }

    #[test]
    fn a_search_never_opens_a_secret_or_a_link_out_of_the_folder() {
        let s = Scratch::new("search-secrets");
        let env = s.env(Level::Ask);
        std::fs::write(s.0.join(".env"), "API_KEY=sk-123\n").unwrap();
        std::fs::write(s.0.join("src/config.rs"), "// reads API_KEY from the environment\n").unwrap();
        let hits = run_call(&env, "search_text", json!({"pattern":"API_KEY"}));
        assert!(hits.text.contains("src/config.rs:1:"), "{}", hits.text);
        assert!(!hits.text.contains("sk-123"), "{}", hits.text);
        assert!(hits.text.contains("[1 file(s) not searched"), "{}", hits.text);
        // Pointed at the file itself, the search asks first (policy), then reads it.
        let p = prepare(&call("search_text", json!({"pattern":"API_KEY","path":".env"})), &env).unwrap();
        assert_eq!(p.gate, Gate::Ask);
        assert!(run_call(&env, "search_text", json!({"pattern":"API_KEY","path":".env"})).text.contains("sk-123"));
        #[cfg(unix)]
        {
            let outside = Scratch::new("search-outside");
            std::fs::write(outside.0.join("private.txt"), "API_KEY=elsewhere\n").unwrap();
            std::os::unix::fs::symlink(outside.0.join("private.txt"), s.0.join("src/linked.txt")).unwrap();
            let hits = run_call(&env, "search_text", json!({"pattern":"API_KEY"}));
            assert!(!hits.text.contains("elsewhere"), "{}", hits.text);
            assert!(hits.text.contains("[2 file(s) not searched"), "{}", hits.text);
        }
    }

    #[test]
    fn writing_and_editing_check_the_file_did_not_change_meanwhile() {
        let s = Scratch::new("change");
        let env = s.env(Level::Ask);
        let out = run_call(&env, "write_file", json!({"path":"docs/new/notes.md","content":"one\ntwo\n"}));
        assert!(!out.is_error, "{}", out.text);
        assert_eq!(std::fs::read_to_string(s.0.join("docs/new/notes.md")).unwrap(), "one\ntwo\n");

        let p = prepare(&call("edit_file", json!({"path":"src/main.rs","old_text":"hello","new_text":"bonjour"})), &env).unwrap();
        assert!(p.summary.ends_with("main.rs (+1 −1)"), "{}", p.summary);
        let request = p.request();
        assert!(request.before.as_deref().unwrap().contains("hello") && request.after.as_deref().unwrap().contains("bonjour"));
        // Someone edits the file between the approval card and the click: nothing is written.
        std::fs::write(s.0.join("src/main.rs"), "fn main() {}\n").unwrap();
        let out = block_on(execute(&p, &env));
        assert!(out.is_error && out.text.contains("changed since"));
        assert_eq!(std::fs::read_to_string(s.0.join("src/main.rs")).unwrap(), "fn main() {}\n");

        std::fs::write(s.0.join("twice.txt"), "x\nx\n").unwrap();
        let e = prepare(&call("edit_file", json!({"path":"twice.txt","old_text":"x","new_text":"y"})), &env).unwrap_err();
        assert!(e.contains("2 times"));
        let out = run_call(&env, "edit_file", json!({"path":"twice.txt","old_text":"x","new_text":"y","replace_all":true}));
        assert!(!out.is_error);
        assert_eq!(std::fs::read_to_string(s.0.join("twice.txt")).unwrap(), "y\ny\n");

        // A file with Windows line ends is edited with plain \n from the model.
        std::fs::write(s.0.join("crlf.txt"), "a\r\nb\r\nc\r\n").unwrap();
        let out = run_call(&env, "edit_file", json!({"path":"crlf.txt","old_text":"a\nb","new_text":"a\nB"}));
        assert!(!out.is_error, "{}", out.text);
        assert_eq!(std::fs::read_to_string(s.0.join("crlf.txt")).unwrap(), "a\r\nB\r\nc\r\n");
    }

    #[test]
    fn a_write_over_an_existing_file_shows_before_and_after() {
        let s = Scratch::new("overwrite");
        let env = s.env(Level::Ask);
        let p = prepare(&call("write_file", json!({"path":"README.md","content":"# New\n"})), &env).unwrap();
        let r = p.request();
        assert_eq!(r.before.as_deref(), Some("# Title\nHello world\n"));
        assert_eq!(r.after.as_deref(), Some("# New\n"));
        assert!(!r.outside && !r.secret && !r.new_file);
        let new = prepare(&call("write_file", json!({"path":"brand-new.txt","content":"hi"})), &env).unwrap().request();
        assert_eq!(new.before, None);
        assert_eq!(new.after.as_deref(), Some("hi"));
        assert!(new.new_file);
        // Too large to show, still a new file.
        let big = "x".repeat(MAX_PREVIEW + 1);
        let large = prepare(&call("write_file", json!({"path":"big.txt","content":big})), &env).unwrap().request();
        assert_eq!((large.before, large.after, large.new_file), (None, None, true));
    }

    #[cfg(unix)]
    #[test]
    fn a_command_runs_in_the_folder_and_reports_its_exit_code() {
        let s = Scratch::new("cmd");
        let env = s.env(Level::Ask);
        let out = run_call(&env, "run_command", json!({"command":"ls src; exit 2"}));
        assert!(out.text.starts_with("Exit code: 2"), "{}", out.text);
        assert!(out.text.contains("main.rs"));
        assert_eq!(out.detail.as_deref(), Some("exit 2"));
        let p = prepare(&call("run_command", json!({"command":"pwd","cwd":"src","timeout_seconds":5})), &env).unwrap();
        assert!(matches!(&p.action, Action::Run { timeout, .. } if *timeout == Duration::from_secs(5)));
        assert_eq!(p.request().cwd.as_deref().map(|c| c.ends_with("src")), Some(true));
        let out = run_call(&env, "run_command", json!({"command":"sleep 5","timeout_seconds":1}));
        assert!(out.text.starts_with("Stopped after 1 s"), "{}", out.text);
    }

    #[test]
    fn globs_become_anchored_regexes() {
        let m = |g: &str, s: &str| glob_regex(g).unwrap().is_match(s);
        assert!(m("*.rs", "main.rs"));
        assert!(!m("*.rs", "src/main.rs"));
        assert!(m("src/**/*.ts", "src/a.ts"));
        assert!(m("src/**/*.ts", "src/x/y/a.ts"));
        assert!(!m("src/**/*.ts", "lib/a.ts"));
        assert!(m("**/test_*.py", "test_a.py"));
        assert!(m("file?.txt", "file1.txt"));
        assert!(m("*.{js,ts}", "a.ts"));
        assert!(!m("*.{js,ts}", "a.tsx"));
        assert!(m("a+b(c).txt", "a+b(c).txt"), "regex characters are literal");
    }

    /// A house that records what it was asked and answers as a device would.
    pub(crate) struct FakeHouse {
        pub calls: std::sync::Mutex<Vec<String>>,
    }

    impl HomeRunner for FakeHouse {
        fn devices<'a>(&'a self) -> BoxFuture<'a, Vec<Device>> {
            Box::pin(async { house_devices() })
        }
        fn control<'a>(&'a self, id: &'a str, action: &'a HomeAction) -> BoxFuture<'a, HomeResult> {
            self.calls.lock().unwrap().push(format!("{id} {}", action.word().0));
            Box::pin(async move { HomeResult { ok: true, text: format!("Done: {} on {id}. The device now reports: on.", action.describe()), detail: "On".into() } })
        }
        fn routine<'a>(&'a self, id: &'a str) -> BoxFuture<'a, HomeResult> {
            self.calls.lock().unwrap().push(format!("routine {id}"));
            Box::pin(async { HomeResult { ok: true, text: "Routine done.".into(), detail: "2 of 2 steps done".into() } })
        }
    }

    pub(crate) fn house_devices() -> Vec<Device> {
        use crate::smart_home::model::tests::{light, tv};
        let mut plug = light("ha:switch.aquecedor", "Aquecedor", "Quarto");
        plug.kind = crate::smart_home::model::Kind::Plug;
        plug.caps = vec![crate::smart_home::model::Capability::TurnOn, crate::smart_home::model::Capability::TurnOff];
        plug.ask = true;
        vec![tv("philips:aa", "TV", "Sala"), tv("philips:bb", "TV", "Quarto"), plug]
    }

    pub(crate) fn house(runner: Arc<FakeHouse>) -> HomeEnv {
        HomeEnv {
            devices: house_devices(),
            routines: vec![RoutineInfo { id: "r-1".into(), name: "Boa noite".into(), asks: false, steps: vec!["TV: turn off".into()], problems: vec![] }],
            runner,
            zheus: false,
        }
    }

    #[test]
    fn zheus_ones_tools_come_only_while_it_is_on_and_only_propose() {
        let s = Scratch::new("zheus");
        let runner = Arc::new(FakeHouse { calls: Default::default() });
        let mut env = s.env(Level::Ask);
        env.house = Some(house(runner.clone()));
        assert!(!definitions(&env).iter().any(|d| d.name.starts_with("zheus_")));
        assert!(prepare(&call("zheus_world", json!({})), &env).unwrap_err().contains("Zheus One is off"));
        env.house.as_mut().unwrap().zheus = true;
        let names: Vec<&str> = definitions(&env).iter().map(|d| d.name).collect();
        assert!(names.contains(&"zheus_world") && names.contains(&"zheus_mission"));
        let p = prepare(&call("zheus_mission", json!({"mission":"Arrival","eta_minutes":"25"})), &env).unwrap();
        assert_eq!(p.action, Action::Home(HomeCall::Mission { mission: "arrival".into(), eta: Some(25) }));
        assert_eq!(p.gate, Gate::Free, "a plan shown acts on nothing");
        assert!(prepare(&call("zheus_mission", json!({"mission":"party"})), &env).unwrap_err().contains("not a mission"));
        assert!(prepare(&call("zheus_mission", json!({"mission":"arrival","eta_minutes":900})), &env).unwrap_err().contains("1 to 240"));
        assert_eq!(prepare(&call("zheus_world", json!({})), &env).unwrap().gate, Gate::Free);
        // The fake house has no Zheus: its answer says so, it is not invented.
        let out = block_on(execute(&prepare(&call("zheus_world", json!({})), &env).unwrap(), &env));
        assert!(out.is_error && out.text.contains("Zheus One is off"));
    }

    #[test]
    fn the_home_tools_come_only_with_a_house_and_check_before_acting() {
        let s = Scratch::new("home");
        let runner = Arc::new(FakeHouse { calls: Default::default() });
        let mut env = s.env(Level::Ask);
        assert!(definitions(&env).iter().all(|d| !d.name.starts_with("home_")));
        assert!(prepare(&call("home_control", json!({"device":"TV","action":"turn_on"})), &env).unwrap_err().contains("not available"));
        env.house = Some(house(runner.clone()));
        let names: Vec<&str> = definitions(&env).iter().map(|d| d.name).collect();
        assert!(names.contains(&"home_devices") && names.contains(&"home_control") && names.contains(&"home_routine"));
        let control = definitions(&env).into_iter().find(|d| d.name == "home_control").unwrap();
        assert_eq!(control.schema["properties"]["action"]["enum"].as_array().unwrap().len(), ACTION_WORDS.len());

        // A TV acts at once; the plug asks; the name with its room finds the device.
        let p = prepare(&call("home_control", json!({"device":"TV da sala","action":"turn_on"})), &env).unwrap();
        assert_eq!(p.gate, Gate::Free);
        assert_eq!(p.summary, "TV · Turn on");
        assert!(matches!(&p.action, Action::Home(HomeCall::Control { device, .. }) if device == "philips:aa"));
        let plug = prepare(&call("home_control", json!({"device":"aquecedor","action":"turn_on"})), &env).unwrap();
        assert_eq!(plug.gate, Gate::Ask);
        // Read only refuses the house too.
        let mut read_only = s.env(Level::ReadOnly);
        read_only.house = Some(house(runner.clone()));
        assert!(matches!(prepare(&call("home_control", json!({"device":"TV da sala","action":"turn_on"})), &read_only).unwrap().gate, Gate::Refuse(_)));

        // Refused before anything is sent: an action the device did not report, an app it does not have, a device
        // that is ambiguous, an action that is not one.
        assert!(prepare(&call("home_control", json!({"device":"TV da sala","action":"set_brightness","value":50})), &env).unwrap_err().contains("did not report"));
        assert!(prepare(&call("home_control", json!({"device":"TV da sala","action":"open_app","value":"Spotify"})), &env).unwrap_err().contains("no app"));
        assert!(prepare(&call("home_control", json!({"device":"TV","action":"turn_on"})), &env).unwrap_err().contains("could be"));
        assert!(prepare(&call("home_control", json!({"device":"TV da sala","action":"reboot"})), &env).is_err());
        assert!(runner.calls.lock().unwrap().is_empty());

        // Carried out through the house, with the device's answer for the model and the state for the step.
        let out = block_on(execute(&prepare(&call("home_control", json!({"device":"philips:aa","action":"open_app","value":"youtube"})), &env).unwrap(), &env));
        assert!(!out.is_error && out.text.contains("open YouTube"), "{}", out.text);
        assert_eq!(out.detail.as_deref(), Some("On"));
        assert_eq!(runner.calls.lock().unwrap().as_slice(), ["philips:aa open_app"]);

        // The list, and one device in full.
        let list = block_on(execute(&prepare(&call("home_devices", json!({})), &env).unwrap(), &env));
        assert!(list.text.contains("philips:aa") && list.text.contains("in Sala") && list.text.contains("Apps (4): YouTube"), "{}", list.text);
        assert!(list.text.contains("waits for the user's approval"), "the plug says it asks");
        let one = block_on(execute(&prepare(&call("home_devices", json!({"device":"tv do quarto"})), &env).unwrap(), &env));
        assert!(one.text.contains("philips:bb") && !one.text.contains("philips:aa"));

        // Routines by name.
        let r = prepare(&call("home_routine", json!({"routine":"boa noite"})), &env).unwrap();
        assert_eq!((r.gate.clone(), r.summary.as_str()), (Gate::Free, "Boa noite"));
        assert!(prepare(&call("home_routine", json!({"routine":"festa"})), &env).unwrap_err().contains("Boa noite"));
    }

    /// Superpowers that record what they were asked and answer as Coucou would.
    struct FakePowers {
        calls: std::sync::Mutex<Vec<(String, Value)>>,
    }

    impl PowersRunner for FakePowers {
        fn call<'a>(&'a self, tool: &'a str, input: &'a Value) -> BoxFuture<'a, HomeResult> {
            self.calls.lock().unwrap().push((tool.to_string(), input.clone()));
            let ok = tool != "wealth_company";
            Box::pin(async move { HomeResult { ok, text: format!("{tool} answered"), detail: if ok { String::new() } else { "not read yet".into() } } })
        }
    }

    #[test]
    fn the_superpowers_come_only_while_they_are_on_and_need_no_click() {
        let s = Scratch::new("powers");
        let mut env = s.env(Level::ReadOnly);
        assert!(!definitions(&env).iter().any(|d| POWER_TOOLS.contains(&d.name)));
        assert_eq!(prepare(&call("wealth_portfolio", json!({})), &env).unwrap_err(), NO_POWERS);
        let runner = Arc::new(FakePowers { calls: Default::default() });
        env.powers = Some(runner.clone());
        let names: Vec<&str> = definitions(&env).iter().map(|d| d.name).filter(|n| POWER_TOOLS.contains(n)).collect();
        assert_eq!(names, POWER_TOOLS, "all of them, from house_tools.json");
        assert!(power_definitions().iter().all(|d| d.schema["type"] == "object" && !d.description.is_empty()));
        // Even read only: they change nothing on the computer.
        let p = prepare(&call("wealth_decision", json!({"action": "record", "ticker": "GARE11", "reasons": "P/VP abaixo de 1"})), &env).unwrap();
        assert_eq!((p.gate.clone(), p.summary.as_str()), (Gate::Free, "record · GARE11"));
        let trade = prepare(&call("wealth_trade", json!({"trades": [{"ticker": "ggrc11", "side": "buy", "quantity": 10}, {"ticker": "TIMS3", "side": "buy", "quantity": 1}]})), &env).unwrap();
        assert_eq!((trade.gate.clone(), trade.summary.as_str()), (Gate::Free, "GGRC11 +1"), "recording a trade the user made changes only Coucou's record");
        let out = block_on(execute(&p, &env));
        assert!(!out.is_error && out.text == "wealth_decision answered");
        assert_eq!(runner.calls.lock().unwrap()[0].1["reasons"], "P/VP abaixo de 1", "the arguments go as the model sent them");
        // Arguments that are not an object arrive as an empty one; a failure is the step's error.
        let p = prepare(&call("wealth_company", json!("TIMS3")), &env).unwrap();
        let out = block_on(execute(&p, &env));
        assert!(out.is_error && out.output.as_deref() == Some("not read yet"));
        assert_eq!(runner.calls.lock().unwrap()[1].1, json!({}));
    }

    #[test]
    fn a_thesis_condition_names_the_metrics_coucou_measures() {
        let defs = power_definitions();
        let company = defs.iter().find(|d| d.name == "wealth_company").unwrap();
        let metrics: Vec<&str> = company.schema["properties"]["conditions"]["items"]["properties"]["metric"]["enum"].as_array().unwrap().iter().map(|m| m.as_str().unwrap()).collect();
        let known: Vec<&str> = coucou_powers::company::METRICS.iter().map(|(k, _)| *k).collect();
        assert_eq!(metrics, known);
        let classes = defs.iter().find(|d| d.name == "wealth_update").unwrap().schema["properties"]["positions"]["items"]["properties"]["class"]["enum"].clone();
        for c in classes.as_array().unwrap() {
            assert!(coucou_powers::model::AssetClass::parse(c.as_str().unwrap()).is_some(), "{c}");
        }
    }

    #[test]
    fn system_info_names_the_shell_and_the_folder() {
        let s = Scratch::new("info");
        let out = run_call(&s.env(Level::Edits), "system_info", json!({}));
        assert!(out.text.contains("Shell for run_command:"));
        assert!(out.text.contains(&policy::display(&s.0)));
        assert!(out.text.contains("applied directly"));
    }
}
