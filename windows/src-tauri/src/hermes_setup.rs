// Zheus: Hermes Agent installed, set up and run by Coucou (docs/HERMES.md, "Let Coucou set it up").
//
// One click in Settings → Hermes Agent, after the window has shown what will happen:
//   1. Hermes' own installer, at a pinned commit (COMMIT): Coucou downloads it from GitHub and checks its SHA-256
//      before anything runs, then runs its stages one by one with no question asked — the stage protocol Hermes' own
//      setup app drives (`-Manifest`, `-Stage NAME -Json -NonInteractive`) — reporting each one to the window. The
//      browser and computer-use tools are left out; `hermes pm install` adds them later.
//   2. `hermes config set`: the brain the user picked among the providers Coucou already has a key or an address for,
//      and Mochi's home as an MCP server when the user ticked it. No secret ever goes on a command line or in a file.
//   3. `hermes gateway run --external-supervisor`, as Coucou's own child: its API server on 127.0.0.1 with a key
//      Coucou made (kept in the keystore, bound to that address), the brain's key handed in its environment only.
//      Started again if it stops (a few times), and ended with everything it started when Coucou quits.
// A Hermes that was already on this computer is never reconfigured unless the user asks: Coucou only runs it, or
// connects to the one already running. Hermes keeps one gateway per computer: when another one answers, Coucou
// does not start a second.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::Url;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::i18n::{t, tf};
use crate::settings::{self, Settings};
use crate::{hermes, log, secrets, Shared};

/// The Hermes Agent commit Coucou installs (`hermes update` moves on from there), and the SHA-256 of its installers
/// at that commit, as GitHub serves them (`git cat-file blob COMMIT:scripts/install.ps1 | sha256sum`).
pub const COMMIT: &str = "6e7b83a2f1c093e992a8d838ed77ea320fbd8e51";
const PS1_SHA256: &str = "d7045a813ac5bbaec09ea4b65e5f77e47603ef56ae493414f72a4b3ce01de048";
const SH_SHA256: &str = "f4cdb0e20f1aca2c3871a76b962fd4db61510f549cf973759c753ac13349beb5";
const MAX_INSTALLER: usize = 512 * 1024;
/// Where Hermes' API server listens by default; Coucou's gateway listens there, on this computer only.
pub const PORT: u16 = 8642;
/// A stage that takes longer than this (Python's dependencies on a slow line) is stopped.
const STAGE_TIMEOUT: Duration = Duration::from_secs(45 * 60);
/// The gateway's first start reads every skill and tool: give it time.
const READY_TIMEOUT: Duration = Duration::from_secs(150);
/// Lines of the gateway's output kept in memory, to say why it stopped.
const TAIL: usize = 30;

pub fn installer_url() -> String {
    let file = if cfg!(windows) { "install.ps1" } else { "install.sh" };
    format!("https://raw.githubusercontent.com/NousResearch/hermes-agent/{COMMIT}/scripts/{file}")
}

fn installer_sha256() -> &'static str {
    if cfg!(windows) {
        PS1_SHA256
    } else {
        SH_SHA256
    }
}

/// The address of Coucou's gateway: on this computer, at the port Hermes' own .env names (it wins over the
/// environment), else the usual one.
pub fn address() -> String {
    format!("http://127.0.0.1:{}", port_in(&hermes_home()))
}

fn port_in(home: &Path) -> u16 {
    std::fs::read_to_string(home.join(".env"))
        .ok()
        .and_then(|text| env_value(&text, "API_SERVER_PORT"))
        .and_then(|p| p.parse::<u16>().ok())
        .filter(|p| *p > 0)
        .unwrap_or(PORT)
}

// ── Where Hermes is ───────────────────────────────────────────────────────────

/// Hermes' folder: $HERMES_HOME, else %LOCALAPPDATA%\hermes on Windows and ~/.hermes elsewhere (its installers' own
/// defaults).
pub fn hermes_home() -> PathBuf {
    if let Some(home) = std::env::var_os("HERMES_HOME").filter(|h| !h.is_empty()) {
        return PathBuf::from(home);
    }
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(crate::platform::home_dir).join("hermes")
    }
    #[cfg(not(windows))]
    {
        crate::platform::home_dir().join(".hermes")
    }
}

fn user_bin() -> Option<PathBuf> {
    (!cfg!(windows)).then(|| crate::platform::home_dir().join(".local").join("bin"))
}

/// The `hermes` command: the one the installer published for the user, else the checkout's own.
pub fn launcher_in(home: &Path, user_bin: Option<&Path>) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["hermes.exe", "hermes.cmd"] } else { &["hermes"] };
    let mut dirs = vec![home.join("bin")];
    dirs.extend(user_bin.map(Path::to_path_buf));
    dirs.push(home.join("hermes-agent").join(".hermes").join("bin"));
    dirs.iter().flat_map(|dir| names.iter().map(move |name| dir.join(name))).find(|p| p.is_file())
}

pub fn launcher() -> Option<PathBuf> {
    launcher_in(&hermes_home(), user_bin().as_deref())
}

/// The installer's own "done" mark (its last stage writes it).
fn completed_in(home: &Path) -> bool {
    home.join("hermes-agent").join(".hermes-bootstrap-complete").is_file()
}

/// Coucou's own note that an installation it started has not finished yet (in Coucou's folder, not Hermes').
fn pending_mark() -> PathBuf {
    settings::local_dir().join("hermes-install-pending")
}

/// Hermes is there and whole: its command is found, and no installation Coucou started stopped halfway (that one
/// runs again, rather than being taken for the user's own Hermes).
pub fn installed_in(home: &Path, launcher: Option<&Path>, pending: bool) -> bool {
    launcher.is_some() && !(pending && !completed_in(home))
}

fn installed() -> bool {
    installed_in(&hermes_home(), launcher().as_deref(), pending_mark().exists())
}

/// The value of `name` in a .env file's text (the last assignment wins, as python-dotenv reads it), quotes and an
/// end-of-line comment removed.
pub fn env_value(text: &str, name: &str) -> Option<String> {
    let mut found = None;
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
        let Some((key, value)) = line.split_once('=') else { continue };
        if key.trim() != name || line.starts_with('#') {
            continue;
        }
        let value = value.trim();
        let value = match value.chars().next() {
            Some(q @ ('"' | '\'')) => value[1..].split(q).next().unwrap_or("").to_string(),
            _ => value.split(" #").next().unwrap_or("").trim().to_string(),
        };
        found = Some(value).filter(|v| !v.is_empty());
    }
    found
}

/// What Hermes' own .env says its API server's key is: it wins over what Coucou would hand it (Hermes loads its
/// .env over the environment).
fn env_key_of(home: &Path) -> Option<String> {
    let text = std::fs::read_to_string(home.join(".env")).ok()?;
    env_value(&text, "API_SERVER_KEY").filter(|k| k.len() >= 16)
}

// ── The brain ─────────────────────────────────────────────────────────────────

/// A provider Coucou knows, as Hermes names it (hermes_cli/providers.py), and where its key comes from.
pub struct Brain {
    /// Coucou's provider id.
    pub id: &'static str,
    pub name: &'static str,
    /// Hermes' `model.provider`.
    pub hermes: &'static str,
    /// Coucou's keystore entry; None for a server on this computer.
    pub key: Option<&'static str>,
    /// The variables Hermes reads that key from.
    pub env: &'static [&'static str],
}

pub const BRAINS: &[Brain] = &[
    Brain { id: "anthropic", name: "Anthropic", hermes: "anthropic", key: Some(crate::claude::KEY), env: &["ANTHROPIC_API_KEY"] },
    Brain { id: "openai", name: "OpenAI", hermes: "openai-api", key: Some("openai-api-key"), env: &["OPENAI_API_KEY"] },
    Brain { id: "google", name: "Google Gemini", hermes: "gemini", key: Some("google-api-key"), env: &["GOOGLE_API_KEY", "GEMINI_API_KEY"] },
    Brain { id: "openrouter", name: "OpenRouter", hermes: "openrouter", key: Some("openrouter-api-key"), env: &["OPENROUTER_API_KEY"] },
    Brain { id: "ollama", name: "Ollama", hermes: "custom", key: None, env: &[] },
    Brain { id: "lmstudio", name: "LM Studio", hermes: "lmstudio", key: None, env: &[] },
];

pub fn brain(id: &str) -> Option<&'static Brain> {
    BRAINS.iter().find(|b| b.id == id)
}

/// A brain as Settings offers it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainOption {
    pub id: String,
    pub name: String,
    /// Coucou has its key, or knows the server's address.
    pub ready: bool,
    /// The model Coucou's chat uses with it ("": Hermes picks its own).
    pub model: String,
}

/// The local server's address for Hermes (its OpenAI-compatible API), or None for a provider with a key.
pub fn base_url(settings: &Settings, brain: &Brain) -> Option<String> {
    let typed = match brain.id {
        "ollama" => &settings.ollama_url,
        "lmstudio" => &settings.lmstudio_url,
        _ => return None,
    };
    let usual = if brain.id == "ollama" { "http://127.0.0.1:11434" } else { "http://127.0.0.1:1234" };
    let root = if typed.trim().is_empty() { usual } else { typed.trim() };
    Some(format!("{}/v1", root.trim_end_matches('/').trim_end_matches("/v1")))
}

/// Every brain, with whether Coucou can give it to Hermes. `has_key` says whether a keystore entry holds a key.
pub fn brain_options(settings: &Settings, has_key: impl Fn(&str) -> bool) -> Vec<BrainOption> {
    BRAINS
        .iter()
        .map(|b| BrainOption {
            id: b.id.into(),
            name: b.name.into(),
            ready: match b.key {
                Some(key) => has_key(key),
                None => !(if b.id == "ollama" { &settings.ollama_url } else { &settings.lmstudio_url }).trim().is_empty(),
            },
            model: settings.chat_models.get(b.id).cloned().unwrap_or_default(),
        })
        .collect()
}

// ── Setting Hermes up ─────────────────────────────────────────────────────────

/// `{command: 'C:\…\coucou-hook.exe', args: ['--mcp']}`: a YAML flow mapping (what `hermes config set` parses for
/// a structured value), in single quotes so a Windows path needs no escape.
pub fn mcp_entry(hook: &Path) -> String {
    let quoted = hook.display().to_string().replace('\'', "''");
    format!("{{command: '{quoted}', args: ['--mcp']}}")
}

/// What `hermes` is run with to set it up, in order: the brain, its model (none: Hermes' own default for that
/// provider, rather than the model of another provider left in its example config), a local server's address, and
/// Mochi's home. Never a secret: keys reach Hermes through its environment only.
pub fn config_commands(brain: &Brain, model: &str, base_url: Option<&str>, hook: Option<&Path>) -> Vec<Vec<String>> {
    let set = |key: &str, value: &str| vec!["config".to_string(), "set".into(), key.into(), value.into()];
    let mut out = vec![set("model.provider", brain.hermes)];
    if model.trim().is_empty() {
        out.push(vec!["config".into(), "unset".into(), "model.default".into()]);
    } else {
        out.push(set("model.default", model.trim()));
    }
    if let Some(url) = base_url {
        out.push(set("model.base_url", url));
    }
    if let Some(hook) = hook {
        out.push(set("mcp_servers.coucou", &mcp_entry(hook)));
    }
    out
}

/// The gateway's environment, on top of what Coucou's own holds: its API server (this computer only, Coucou's key)
/// and the brain's key. Nothing else of Coucou's keystore.
pub fn gateway_env(api_key: &str, port: u16, brain: Option<(&Brain, &str)>) -> Vec<(String, String)> {
    let mut env = vec![
        ("API_SERVER_ENABLED".to_string(), "true".to_string()),
        ("API_SERVER_KEY".into(), api_key.into()),
        ("API_SERVER_HOST".into(), "127.0.0.1".into()),
        ("API_SERVER_PORT".into(), port.to_string()),
        ("PYTHONIOENCODING".into(), "utf-8".into()),
        ("NO_COLOR".into(), "1".into()),
    ];
    if let Some((brain, key)) = brain {
        env.extend(brain.env.iter().map(|name| (name.to_string(), key.to_string())));
    }
    env
}

/// A line of output with every secret in `secrets` blanked out.
pub fn redact(line: &str, secrets: &[String]) -> String {
    let mut out: String = line.chars().take(400).collect();
    for secret in secrets.iter().filter(|s| s.len() >= 8) {
        out = out.replace(secret.as_str(), "•••");
    }
    out
}

/// A new key for the gateway's API server: 32 random bytes in hex.
fn new_api_key() -> Result<String, String> {
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; 32];
    ring::rand::SystemRandom::new().fill(&mut bytes).map_err(|_| "no secure random source".to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// The installer's text, only if it is exactly the pinned one.
pub fn verified(bytes: Vec<u8>, expected: &str) -> Result<Vec<u8>, String> {
    let actual = sha256_hex(&bytes);
    if actual != expected {
        return Err(tf("The installer downloaded is not the expected one (SHA-256 {actual}): nothing was run.", &[("actual", &actual[..16])]));
    }
    Ok(bytes)
}

// ── The installer's stages ────────────────────────────────────────────────────

/// One stage of Hermes' installer (its `-Manifest`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    pub name: String,
    pub title: String,
    pub needs_input: bool,
}

pub fn parse_manifest(text: &str) -> Result<Vec<Stage>, String> {
    let line = text.lines().rev().find(|l| l.trim_start().starts_with('{')).unwrap_or("");
    let v: Value = serde_json::from_str(line.trim()).map_err(|_| t("Hermes' installer did not list its steps."))?;
    let stages: Vec<Stage> = v["stages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let name = s["name"].as_str()?.to_string();
            let ok = !name.is_empty() && name.len() <= 40 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            ok.then(|| Stage {
                title: s["title"].as_str().unwrap_or(&name).chars().take(80).collect(),
                needs_input: s["needs_user_input"].as_bool().unwrap_or(false),
                name,
            })
        })
        .collect();
    if stages.is_empty() || stages.len() > 30 {
        return Err(t("Hermes' installer did not list its steps."));
    }
    Ok(stages)
}

/// What a stage said when it ended: `{"ok":true,"stage":"venv","skipped":false}`, its last JSON line.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub ok: bool,
    pub skipped: bool,
    pub reason: String,
}

pub fn parse_frame(lines: &[String], stage: &str) -> Option<Frame> {
    lines.iter().rev().find_map(|line| {
        let v: Value = serde_json::from_str(line.trim()).ok()?;
        (v["stage"] == stage).then(|| Frame {
            ok: v["ok"].as_bool().unwrap_or(false),
            skipped: v["skipped"].as_bool().unwrap_or(false),
            reason: v["reason"].as_str().unwrap_or("").chars().take(300).collect(),
        })
    })
}

/// The installer run for one stage (or its manifest, `stage` None), with nothing that asks.
fn installer_command(script: &Path, stage: Option<&str>) -> Command {
    #[cfg(windows)]
    let mut cmd = {
        let mut cmd = Command::new("powershell.exe");
        cmd.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(script);
        match stage {
            None => {
                cmd.arg("-Manifest");
            }
            Some(name) => {
                cmd.args(["-Stage", name, "-Json", "-NonInteractive", "-SkipBrowser", "-SkipComputerUse", "-Commit", COMMIT]);
            }
        }
        cmd
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut cmd = Command::new("bash");
        cmd.arg(script);
        match stage {
            None => {
                cmd.arg("--manifest");
            }
            Some(name) => {
                cmd.args(["--stage", name, "--json", "--non-interactive", "--skip-browser", "--skip-computer-use", "--commit", COMMIT]);
            }
        }
        cmd
    };
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.env("NO_COLOR", "1");
    crate::platform::no_console(&mut cmd);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd
}

/// Runs `cmd` to its end (or `timeout`, or `cancel`), each line of its output to `line`; its exit status and lines.
fn run_lines(mut cmd: Command, timeout: Duration, cancel: &AtomicBool, mut line: impl FnMut(&str)) -> Result<(bool, Vec<String>), String> {
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    for stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>)]
        .into_iter()
        .flatten()
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for_each_line(stream, |l| tx.send(l).is_ok());
        });
    }
    drop(tx);
    let started = Instant::now();
    let mut lines = Vec::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(l) => {
                line(&l);
                lines.push(l);
                if lines.len() > 4000 {
                    lines.drain(..2000);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        if cancel.load(Ordering::SeqCst) || started.elapsed() > timeout {
            kill_tree(&mut child);
            let _ = child.wait();
            return Err(if cancel.load(Ordering::SeqCst) { t("Stopped.") } else { t("This step took too long and was stopped.") });
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    Ok((status.success(), lines))
}

/// Each line of `stream` to `line` (until it answers false), whatever its encoding: Windows PowerShell writes in the
/// console's code page, not UTF-8, and a line it cannot read must not end the reading (the stage's result comes last).
fn for_each_line(stream: impl Read, mut line: impl FnMut(String) -> bool) {
    let mut reader = BufReader::new(stream);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let text = String::from_utf8_lossy(&buf);
                if !line(text.trim_end_matches(['\r', '\n']).to_string()) {
                    break;
                }
            }
        }
    }
}

/// Ends a process and everything it started.
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut taskkill = Command::new("taskkill");
        taskkill.args(["/T", "/F", "/PID", &child.id().to_string()]).stdout(Stdio::null()).stderr(Stdio::null());
        crate::platform::no_console(&mut taskkill);
        let _ = taskkill.status();
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(pid) = i32::try_from(child.id()) {
            unsafe {
                libc::kill(-pid, libc::SIGTERM);
            }
            std::thread::sleep(Duration::from_millis(600));
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
}

// ── What Settings shows ───────────────────────────────────────────────────────

/// The installation, while and after it runs.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub running: bool,
    /// The step being done, in Hermes' installer's words ("Install Python dependencies").
    pub step: String,
    pub done: usize,
    pub total: usize,
    /// The last line it printed (secrets blanked).
    pub last: String,
    pub error: Option<String>,
    /// It went through, and Hermes answered.
    pub finished: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum GatewayState {
    /// Not run by Coucou.
    Off,
    Starting,
    Running,
    /// Another Hermes gateway already answers on this computer (Hermes runs one per computer).
    Elsewhere,
    /// It stopped and Coucou gave up starting it again: why, in its own last words.
    Stopped { reason: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    /// The `hermes` command is on this computer.
    pub installed: bool,
    /// Coucou installed or runs it (Settings shows its gateway rather than the address and key).
    pub managed: bool,
    pub home: String,
    pub brains: Vec<BrainOption>,
    /// The brain Coucou handed to Hermes.
    pub brain: String,
    pub progress: Progress,
    pub gateway: GatewayState,
    pub address: String,
    /// The Hermes commit Coucou installs, short.
    pub commit: String,
    pub installer: String,
}

// ── The setup and the gateway, while Coucou runs ──────────────────────────────

/// Coucou's part: one installation at a time, and the gateway it runs.
#[derive(Default)]
pub struct Zheus {
    progress: Mutex<Progress>,
    cancel: Arc<AtomicBool>,
    gateway: Mutex<Option<Running>>,
    state: Arc<Mutex<GatewayState>>,
    tail: Arc<Mutex<VecDeque<String>>>,
    /// Coucou wants it up: a stop that was not asked for is followed by a new start.
    wanted: Arc<AtomicBool>,
}

struct Running {
    child: Arc<Mutex<Option<Child>>>,
    #[cfg(windows)]
    job: Option<job::Job>,
}

impl Default for GatewayState {
    fn default() -> Self {
        GatewayState::Off
    }
}

fn status_of(app: &AppHandle) -> SetupStatus {
    let zheus = app.state::<Zheus>();
    let shared = app.state::<Shared>();
    let settings = shared.settings.lock().unwrap().clone();
    let progress = zheus.progress.lock().unwrap().clone();
    let gateway = zheus.state.lock().unwrap().clone();
    SetupStatus {
        installed: installed(),
        managed: settings.hermes_managed,
        home: hermes_home().display().to_string(),
        brains: brain_options(&settings, secrets::present),
        brain: settings.hermes_brain.clone(),
        progress,
        gateway,
        address: address(),
        commit: COMMIT[..10].to_string(),
        installer: installer_url(),
    }
}

fn changed(app: &AppHandle) {
    let _ = app.emit("hermes-setup", status_of(app));
}

fn set_progress(app: &AppHandle, change: impl FnOnce(&mut Progress)) {
    change(&mut app.state::<Zheus>().progress.lock().unwrap());
    changed(app);
}

/// Runs `hermes` with `args` (setting it up), to its end; its last lines when it fails.
fn run_hermes(launcher: &Path, args: &[String], cancel: &AtomicBool) -> Result<(), String> {
    let mut cmd = Command::new(launcher);
    cmd.args(args).current_dir(crate::platform::home_dir()).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.env("NO_COLOR", "1").env("PYTHONIOENCODING", "utf-8");
    crate::platform::no_console(&mut cmd);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let (ok, lines) = run_lines(cmd, Duration::from_secs(120), cancel, |_| {})?;
    if ok {
        return Ok(());
    }
    let said: Vec<&str> = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    Err(said.iter().rev().take(2).rev().cloned().collect::<Vec<_>>().join(" · ").chars().take(300).collect())
}

/// Downloads the pinned installer, checks it, and puts it in a folder of its own.
async fn fetch_installer() -> Result<(PathBuf, PathBuf), String> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(60)).build().map_err(|e| e.to_string())?;
    let response = client
        .get(installer_url())
        .send()
        .await
        .map_err(|_| t("Could not download Hermes' installer. Is this computer online?"))?;
    if !response.status().is_success() {
        return Err(tf("GitHub answered {status} for Hermes' installer.", &[("status", &response.status().as_u16().to_string())]));
    }
    let bytes = verified(crate::net::read_capped(response, MAX_INSTALLER).await?, installer_sha256())?;
    let dir = std::env::temp_dir().join(format!("coucou-hermes-{}", &new_api_key()?[..12]));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let script = dir.join(if cfg!(windows) { "install.ps1" } else { "install.sh" });
    std::fs::write(&script, bytes).map_err(|e| e.to_string())?;
    Ok((dir, script))
}

/// The whole setup, from a click in Settings: install (unless Hermes is there), set the brain and Mochi's home up
/// (on a fresh install, or when the user asked), keep the choice, run the gateway and connect the chat to it.
async fn setup(app: AppHandle, brain_id: String, house: bool, configure: bool) -> Result<(), String> {
    let brain = brain(&brain_id).ok_or_else(|| t("Pick the AI Hermes should use."))?;
    let zheus_cancel = app.state::<Zheus>().cancel.clone();
    let fresh = !installed();
    if fresh {
        if let Some(dir) = pending_mark().parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(pending_mark(), COMMIT);
        set_progress(&app, |p| p.step = t("Downloading Hermes' installer…"));
        let (dir, script) = fetch_installer().await?;
        log::line(format!("hermes setup: installer checked (commit {})", &COMMIT[..10]));
        let cancel = zheus_cancel.clone();
        let progress_app = app.clone();
        let outcome = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            let (ok, lines) = run_lines(installer_command(&script, None), Duration::from_secs(60), &cancel, |_| {})?;
            let stages = parse_manifest(&lines.join("\n")).map_err(|e| if ok { e } else { t("Hermes' installer could not start (PowerShell or bash refused it).") })?;
            let total = stages.len();
            for (index, stage) in stages.iter().enumerate() {
                set_progress(&progress_app, |p| {
                    p.step = stage.title.clone();
                    p.done = index;
                    p.total = total;
                    p.last.clear();
                });
                log::line(format!("hermes setup: step {}/{total} {}", index + 1, stage.name));
                let app_line = progress_app.clone();
                let (_, lines) = run_lines(installer_command(&script, Some(&stage.name)), STAGE_TIMEOUT, &cancel, move |l| {
                    let shown = redact(l.trim(), &[]);
                    if !shown.is_empty() && !shown.starts_with('{') {
                        app_line.state::<Zheus>().progress.lock().unwrap().last = shown;
                    }
                })?;
                match parse_frame(&lines, &stage.name) {
                    Some(Frame { ok: true, .. }) => {}
                    Some(frame) => {
                        return Err(tf("Hermes' installer stopped at \"{step}\": {reason}", &[("step", &stage.title), ("reason", &frame.reason)]));
                    }
                    None => {
                        let last = lines.iter().rev().map(|l| l.trim()).find(|l| !l.is_empty()).unwrap_or("").to_string();
                        return Err(tf("Hermes' installer stopped at \"{step}\": {reason}", &[("step", &stage.title), ("reason", &redact(&last, &[]))]));
                    }
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&dir);
        outcome?;
        let _ = std::fs::remove_file(pending_mark());
        set_progress(&app, |p| p.done = p.total);
    }
    let launcher = launcher().ok_or_else(|| t("Hermes was installed, but its hermes command was not found."))?;
    if fresh || configure {
        set_progress(&app, |p| p.step = t("Setting Hermes up…"));
        let settings = app.state::<Shared>().settings.lock().unwrap().clone();
        let model = settings.chat_models.get(brain.id).cloned().unwrap_or_default();
        let hook = settings::hook_exe_path();
        let commands = config_commands(brain, &model, base_url(&settings, brain).as_deref(), house.then_some(hook.as_path()));
        let cancel = zheus_cancel.clone();
        let launcher_for = launcher.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            for args in &commands {
                // An unset of a key that is not there is no failure.
                let optional = args.get(1).is_some_and(|a| a == "unset");
                if let Err(e) = run_hermes(&launcher_for, args, &cancel) {
                    if !optional {
                        return Err(tf("Hermes refused its settings ({error}).", &[("error", &e)]));
                    }
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())??;
        log::line(format!("hermes setup: brain {} set{}", brain.id, if house { ", with Mochi's home" } else { "" }));
    }
    // The choice is kept: Coucou runs Hermes from now on, at every start.
    let updated = {
        let shared = app.state::<Shared>();
        let mut current = shared.settings.lock().unwrap();
        current.hermes_managed = true;
        current.hermes_brain = brain.id.to_string();
        if house {
            current.home_mcp = true;
        }
        if let Err(err) = settings::save(&current) {
            log::line(format!("could not save settings: {err}"));
        }
        current.clone()
    };
    let _ = app.emit("settings-changed", updated);
    set_progress(&app, |p| p.step = t("Starting Hermes…"));
    let model = bring_up(&app).await?;
    connect_everything(&app, &model);
    Ok(())
}

/// Coucou's gateway up: the one already answering with Coucou's key (left by an earlier run), else a new one. Another
/// gateway that answers without that key is the user's own: Coucou starts no second one (Hermes runs one per
/// computer) and says so.
async fn bring_up(app: &AppHandle) -> Result<String, String> {
    // The key in Hermes' own .env is the one its gateway runs with, Coucou's or not.
    if let Some(key) = env_key_of(&hermes_home()) {
        let _ = hermes::set_key(&address(), &key);
    }
    if someone_answers().await {
        let ours = match hermes::key_for_address(&address()) {
            Some(key) => hermes::Client::new(Url::parse(&address()).map_err(|e| e.to_string())?, key).capabilities().await.ok(),
            None => None,
        };
        let Some(caps) = ours else {
            *app.state::<Zheus>().state.lock().unwrap() = GatewayState::Elsewhere;
            changed(app);
            return Err(t("Another Hermes already runs on this computer: connect to it below, with the key of its .env."));
        };
        *app.state::<Zheus>().state.lock().unwrap() = GatewayState::Running;
        changed(app);
        remember_address(app, &caps.model);
        return Ok(caps.model);
    }
    start_gateway(app)?;
    wait_ready(app).await
}

/// The chat knows where Coucou's gateway is (the provider the user picks stays theirs).
fn remember_address(app: &AppHandle, model: &str) {
    let updated = {
        let shared = app.state::<Shared>();
        let mut current = shared.settings.lock().unwrap();
        if current.hermes_url == address() && current.chat_models.get(hermes::PROVIDER).map(String::as_str) == Some(model) {
            return;
        }
        current.hermes_url = address();
        current.chat_models.insert(hermes::PROVIDER.to_string(), model.to_string());
        if let Err(err) = settings::save(&current) {
            log::line(format!("could not save settings: {err}"));
        }
        current.clone()
    };
    let _ = app.emit("settings-changed", updated);
}

/// Once the gateway answers: the chat (here and on the phone) talks to it.
fn connect_everything(app: &AppHandle, model: &str) {
    let updated = {
        let shared = app.state::<Shared>();
        let mut current = shared.settings.lock().unwrap();
        current.hermes_url = address();
        current.chat_models.insert(hermes::PROVIDER.to_string(), model.to_string());
        current.chat_provider = hermes::PROVIDER.to_string();
        let phone = format!("{}/{model}", hermes::PROVIDER);
        if current.phone_chat && !current.phone_chat_models.contains(&phone) {
            current.phone_chat_models.insert(0, phone);
            current.phone_chat_models = crate::phone_link::clean_chat_models(&current.phone_chat_models);
        }
        if let Err(err) = settings::save(&current) {
            log::line(format!("could not save settings: {err}"));
        }
        current.clone()
    };
    log::line("hermes setup: the chat talks to Hermes");
    let _ = app.emit("settings-changed", updated);
}

/// The key Coucou's gateway runs with: Hermes' own (its .env wins over the environment), else the one Coucou made
/// before, else a new one, kept in the keystore for that address.
fn api_key() -> Result<String, String> {
    let key = match env_key_of(&hermes_home()).or_else(|| hermes::key_for_address(&address())) {
        Some(key) => key,
        None => new_api_key()?,
    };
    hermes::set_key(&address(), &key)?;
    Ok(key)
}

/// Starts the gateway (unless another one already answers, or Coucou's is up), and keeps it up.
pub fn start_gateway(app: &AppHandle) -> Result<(), String> {
    let zheus = app.state::<Zheus>();
    if zheus.gateway.lock().unwrap().is_some() {
        return Ok(());
    }
    let launcher = launcher().ok_or_else(|| t("Hermes is not installed on this computer."))?;
    let api_key = api_key()?;
    let chosen = app.state::<Shared>().settings.lock().unwrap().hermes_brain.clone();
    let brain_key = brain(&chosen).and_then(|b| b.key.and_then(secrets::get).map(|k| (b, k)));
    let secrets_shown: Vec<String> = std::iter::once(api_key.clone()).chain(brain_key.as_ref().map(|(_, k)| k.clone())).collect();
    let env = gateway_env(&api_key, port_in(&hermes_home()), brain_key.as_ref().map(|(b, k)| (*b, k.as_str())));
    zheus.wanted.store(true, Ordering::SeqCst);
    *zheus.state.lock().unwrap() = GatewayState::Starting;

    let mut cmd = Command::new(&launcher);
    cmd.args(["gateway", "run", "--external-supervisor"])
        .current_dir(crate::platform::home_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in &env {
        cmd.env(name, value);
    }
    crate::platform::no_console(&mut cmd);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| tf("Hermes did not start ({error}).", &[("error", &e.to_string())]))?;
    #[cfg(windows)]
    let job = job::Job::holding(&child);
    let tail = zheus.tail.clone();
    tail.lock().unwrap().clear();
    for stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>)]
        .into_iter()
        .flatten()
    {
        let (tail, shown) = (tail.clone(), secrets_shown.clone());
        std::thread::spawn(move || {
            for_each_line(stream, |line| {
                let line = redact(line.trim(), &shown);
                if !line.is_empty() {
                    let mut tail = tail.lock().unwrap();
                    tail.push_back(line);
                    while tail.len() > TAIL {
                        tail.pop_front();
                    }
                }
                true
            });
        });
    }
    let child = Arc::new(Mutex::new(Some(child)));
    *zheus.gateway.lock().unwrap() = Some(Running {
        child: child.clone(),
        #[cfg(windows)]
        job,
    });
    log::line("hermes: gateway started by Coucou");
    changed(app);

    // Watches it: a stop that was not asked for is followed by a new start, a few times.
    let app = app.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        let status = loop {
            if let Some(c) = child.lock().unwrap().as_mut() {
                if let Ok(Some(status)) = c.try_wait() {
                    break Some(status);
                }
            } else {
                break None; // stopped on purpose
            }
            std::thread::sleep(Duration::from_millis(500));
        };
        let zheus = app.state::<Zheus>();
        {
            // Only its own: a gateway started since (after a stop and a start) is not this one.
            let mut current = zheus.gateway.lock().unwrap();
            if current.as_ref().is_some_and(|r| Arc::ptr_eq(&r.child, &child)) {
                *current = None;
            }
        }
        let Some(status) = status else { return };
        log::line(format!("hermes: gateway ended ({status})"));
        if !zheus.wanted.load(Ordering::SeqCst) {
            return;
        }
        let reason = zheus.tail.lock().unwrap().iter().rev().take(3).rev().cloned().collect::<Vec<_>>().join(" · ");
        let quick = started.elapsed() < Duration::from_secs(60);
        let failures = RESTARTS.with_failure(quick);
        if failures > 3 {
            zheus.wanted.store(false, Ordering::SeqCst);
            let reason = if reason.is_empty() { t("Hermes stopped.") } else { reason };
            *zheus.state.lock().unwrap() = GatewayState::Stopped { reason };
            changed(&app);
            return;
        }
        std::thread::sleep(Duration::from_secs(if quick { 5 * failures as u64 } else { 2 }));
        if zheus.wanted.load(Ordering::SeqCst) {
            let _ = start_gateway(&app);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = wait_ready(&app).await;
            });
        }
    });
    Ok(())
}

/// Recent quick stops of the gateway: more than three in ten minutes and Coucou stops starting it again.
struct Restarts(Mutex<Vec<Instant>>);
static RESTARTS: Restarts = Restarts(Mutex::new(Vec::new()));

impl Restarts {
    /// Counts a stop (only a quick one is a failure); how many failures in the last ten minutes.
    fn with_failure(&self, failure: bool) -> usize {
        let mut list = self.0.lock().unwrap();
        list.retain(|at| at.elapsed() < Duration::from_secs(600));
        if failure {
            list.push(Instant::now());
        }
        list.len()
    }

    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

/// Waits until the gateway answers with the key (its first start can take a minute or two); what it answers as.
async fn wait_ready(app: &AppHandle) -> Result<String, String> {
    let url = Url::parse(&address()).map_err(|e| e.to_string())?;
    let key = hermes::key_for_address(&address()).ok_or_else(|| t("Hermes' key is missing."))?;
    let client = hermes::Client::new(url, key);
    let started = Instant::now();
    loop {
        match client.capabilities().await {
            Ok(caps) => {
                *app.state::<Zheus>().state.lock().unwrap() = GatewayState::Running;
                log::line("hermes: gateway answers");
                changed(app);
                remember_address(app, &caps.model);
                return Ok(caps.model);
            }
            Err(e) if started.elapsed() > READY_TIMEOUT => {
                let tail = app.state::<Zheus>().tail.lock().unwrap().iter().rev().take(2).rev().cloned().collect::<Vec<_>>().join(" · ");
                return Err(if tail.is_empty() { e } else { tail });
            }
            Err(_) => {}
        }
        if !app.state::<Zheus>().wanted.load(Ordering::SeqCst) {
            return Err(t("Stopped."));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Stops Coucou's gateway, and everything it started.
pub fn stop_gateway(app: &AppHandle) {
    let zheus = app.state::<Zheus>();
    zheus.wanted.store(false, Ordering::SeqCst);
    let running = zheus.gateway.lock().unwrap().take();
    if let Some(running) = running {
        if let Some(mut child) = running.child.lock().unwrap().take() {
            kill_tree(&mut child);
            let _ = child.wait();
        }
        #[cfg(windows)]
        drop(running.job);
        log::line("hermes: gateway stopped by Coucou");
    }
    *zheus.state.lock().unwrap() = GatewayState::Off;
    changed(app);
}

/// Another Hermes gateway already answers on this computer (Coucou's own, from before, or the user's).
async fn someone_answers() -> bool {
    let Ok(client) = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(2)).build() else { return false };
    client.get(format!("{}/v1/capabilities", address())).send().await.is_ok()
}

/// At Coucou's start: its gateway comes back if the user let Coucou run Hermes.
pub fn start_if_managed(app: &AppHandle) {
    if !app.state::<Shared>().settings.lock().unwrap().hermes_managed {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        RESTARTS.clear();
        if let Err(e) = bring_up(&app).await {
            log::line("hermes: gateway not up at start");
            let elsewhere = *app.state::<Zheus>().state.lock().unwrap() == GatewayState::Elsewhere;
            if !elsewhere {
                let stopped = GatewayState::Stopped { reason: e };
                *app.state::<Zheus>().state.lock().unwrap() = stopped;
                changed(&app);
            }
        }
    });
}

pub fn stop_on_exit(app: &AppHandle) {
    if app.try_state::<Zheus>().is_some() {
        stop_gateway(app);
    }
}

// ── Commands (Settings → Hermes Agent) ────────────────────────────────────────

#[tauri::command]
pub fn hermes_setup_status(window: WebviewWindow, app: AppHandle) -> Result<SetupStatus, String> {
    crate::phone_link::only_settings(&window)?;
    Ok(status_of(&app))
}

/// "Install Hermes": the click after the plan was shown. `configure`: also give an existing Hermes this brain.
#[tauri::command]
pub fn hermes_setup_start(app: AppHandle, window: WebviewWindow, brain: String, house: bool, configure: bool) -> Result<SetupStatus, String> {
    crate::phone_link::only_settings(&window)?;
    let option = {
        let settings = app.state::<Shared>().settings.lock().unwrap().clone();
        brain_options(&settings, secrets::present).into_iter().find(|b| b.id == brain)
    };
    if !option.is_some_and(|b| b.ready) {
        return Err(t("Coucou has no key or address for this AI: add it in Settings first."));
    }
    {
        let zheus = app.state::<Zheus>();
        let mut progress = zheus.progress.lock().unwrap();
        if progress.running {
            return Err(t("Hermes is already being set up."));
        }
        *progress = Progress { running: true, ..Progress::default() };
        zheus.cancel.store(false, Ordering::SeqCst);
    }
    log::line(format!("hermes setup: started (brain {brain})"));
    let run = app.clone();
    tauri::async_runtime::spawn(async move {
        RESTARTS.clear();
        let result = setup(run.clone(), brain, house, configure).await;
        if let Err(e) = &result {
            log::line(format!("hermes setup: not finished: {}", redact(e, &[])));
        }
        set_progress(&run, |p| {
            p.running = false;
            p.finished = result.is_ok();
            p.error = result.err();
        });
    });
    Ok(status_of(&app))
}

/// Stops the installation where it is.
#[tauri::command]
pub fn hermes_setup_cancel(app: AppHandle, window: WebviewWindow) -> Result<SetupStatus, String> {
    crate::phone_link::only_settings(&window)?;
    app.state::<Zheus>().cancel.store(true, Ordering::SeqCst);
    Ok(status_of(&app))
}

/// Coucou runs Hermes (on), or not (off: its gateway stops; Hermes stays installed).
#[tauri::command]
pub fn hermes_gateway_set(app: AppHandle, window: WebviewWindow, shared: State<Shared>, on: bool) -> Result<SetupStatus, String> {
    crate::phone_link::only_settings(&window)?;
    let updated = {
        let mut current = shared.settings.lock().unwrap();
        current.hermes_managed = on;
        if let Err(err) = settings::save(&current) {
            log::line(format!("could not save settings: {err}"));
        }
        current.clone()
    };
    let _ = app.emit("settings-changed", updated);
    if on {
        start_if_managed(&app);
    } else {
        stop_gateway(&app);
    }
    Ok(status_of(&app))
}

// ── Windows: the gateway's processes end with Coucou ──────────────────────────

#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// A job holding the gateway and everything it starts: when Coucou ends, however it ends, they end too.
    pub struct Job(HANDLE);

    // The handle is only closed (Drop) or terminated, both safe from any thread.
    unsafe impl Send for Job {}

    impl Job {
        pub fn holding(child: &Child) -> Option<Job> {
            unsafe {
                let job = CreateJobObjectW(None, windows::core::PCWSTR::null()).ok()?;
                let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let set = SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const core::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                let process = HANDLE(child.as_raw_handle() as *mut core::ffi::c_void);
                if set.is_err() || AssignProcessToJobObject(job, process).is_err() {
                    let _ = CloseHandle(job);
                    return None;
                }
                Some(Job(job))
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                let _ = TerminateJobObject(self.0, 1);
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installer_runs_only_when_it_is_exactly_the_pinned_one() {
        let text = b"Write-Host hello".to_vec();
        let good = sha256_hex(&text);
        assert_eq!(verified(text.clone(), &good).unwrap(), text);
        let err = verified(b"Write-Host evil".to_vec(), &good).unwrap_err();
        assert!(err.contains("nothing was run"), "{err}");
        assert!(installer_url().starts_with("https://raw.githubusercontent.com/NousResearch/hermes-agent/") && installer_url().contains(COMMIT));
        assert_eq!(COMMIT.len(), 40);
        assert!([PS1_SHA256, SH_SHA256].iter().all(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())));
    }

    #[test]
    fn the_installers_steps_and_how_each_ended_are_read_from_its_json() {
        // As scripts/install.ps1 prints them (-Manifest, then Emit-Frame).
        let manifest = r#"{"protocol_version":1,"stages":[{"name":"prerequisites","title":"System prerequisites","category":"runtime","needs_user_input":false},{"name":"python-deps","title":"Install Python dependencies","category":"runtime","needs_user_input":false},{"name":"setup","title":"Configure API keys and settings","category":"configuration","needs_user_input":true},{"name":"bad name;rm","title":"x"}]}"#;
        let stages = parse_manifest(&format!("[hermes] banner\n{manifest}\n")).unwrap();
        assert_eq!(stages.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["prerequisites", "python-deps", "setup"], "an odd name is dropped");
        assert!(stages[2].needs_input && !stages[0].needs_input);
        assert_eq!(stages[1].title, "Install Python dependencies");
        assert!(parse_manifest("not json").is_err());
        let lines: Vec<String> = ["✓ venv created", r#"{"ok":true,"stage":"venv","skipped":false}"#].iter().map(|s| s.to_string()).collect();
        assert_eq!(parse_frame(&lines, "venv"), Some(Frame { ok: true, skipped: false, reason: String::new() }));
        let failed = vec![r#"{"ok":false,"stage":"repository","skipped":false,"reason":"git clone failed"}"#.to_string()];
        assert_eq!(parse_frame(&failed, "repository").unwrap().reason, "git clone failed");
        assert_eq!(parse_frame(&lines, "repository"), None, "another stage's frame is not this one's");
        let skipped = vec![r#"{"ok":true,"stage":"setup","skipped":true,"reason":"needs user input"}"#.to_string()];
        assert!(parse_frame(&skipped, "setup").unwrap().skipped);
    }

    #[test]
    fn hermes_is_set_up_with_the_brain_its_model_and_mochis_home_and_never_a_secret() {
        let anthropic = brain("anthropic").unwrap();
        let hook = Path::new(r"C:\Users\Léa O'Neil\AppData\Local\Coucou\bin\coucou-hook.exe");
        let commands = config_commands(anthropic, "claude-opus-5", None, Some(hook));
        assert_eq!(commands[0], ["config", "set", "model.provider", "anthropic"]);
        assert_eq!(commands[1], ["config", "set", "model.default", "claude-opus-5"]);
        assert_eq!(commands[2], ["config", "set", "mcp_servers.coucou", r"{command: 'C:\Users\Léa O''Neil\AppData\Local\Coucou\bin\coucou-hook.exe', args: ['--mcp']}"]);
        // No model chosen in Coucou: Hermes' own default for the provider, not the example config's.
        let plain = config_commands(brain("google").unwrap(), "", None, None);
        assert_eq!(plain, vec![vec!["config", "set", "model.provider", "gemini"], vec!["config", "unset", "model.default"]]);
        // A local server: its address, the OpenAI-compatible way.
        let mut settings = Settings::default();
        settings.ollama_url = "http://192.168.1.5:11434/".into();
        assert_eq!(base_url(&settings, brain("ollama").unwrap()).as_deref(), Some("http://192.168.1.5:11434/v1"));
        assert_eq!(base_url(&Settings::default(), brain("lmstudio").unwrap()).as_deref(), Some("http://127.0.0.1:1234/v1"));
        assert_eq!(base_url(&settings, anthropic), None);
        let local = config_commands(brain("ollama").unwrap(), "llama3.2", Some("http://127.0.0.1:11434/v1"), None);
        assert_eq!(local[0][3], "custom");
        assert!(local.contains(&vec!["config".to_string(), "set".into(), "model.base_url".into(), "http://127.0.0.1:11434/v1".into()]));
        for args in commands.iter().chain(&plain).chain(&local) {
            assert!(!args.iter().any(|a| a.contains("API_KEY") || a.contains("sk-")), "{args:?}");
        }
    }

    #[test]
    fn the_gateway_gets_its_key_this_computer_only_and_the_one_brain_key() {
        let env = gateway_env("k".repeat(64).as_str(), PORT, Some((brain("google").unwrap(), "AIza-secret")));
        let get = |name: &str| env.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
        assert_eq!(get("API_SERVER_HOST"), Some("127.0.0.1"), "never the network");
        assert_eq!(get("API_SERVER_PORT"), Some("8642"));
        assert_eq!(get("API_SERVER_KEY").map(str::len), Some(64));
        assert_eq!((get("GOOGLE_API_KEY"), get("GEMINI_API_KEY")), (Some("AIza-secret"), Some("AIza-secret")));
        assert!(get("ANTHROPIC_API_KEY").is_none() && get("OPENAI_API_KEY").is_none(), "only the brain's own key");
        let local = gateway_env("x".repeat(64).as_str(), 9000, None);
        assert!(local.contains(&("API_SERVER_PORT".to_string(), "9000".to_string())));
        assert!(!local.iter().any(|(n, _)| n.ends_with("_API_KEY")));
        let key = new_api_key().unwrap();
        assert!(key.len() == 64 && key != new_api_key().unwrap());
    }

    #[test]
    fn brains_are_offered_only_where_coucou_has_a_key_or_an_address() {
        let mut settings = Settings::default();
        settings.chat_models.insert("anthropic".into(), "claude-opus-5".into());
        settings.lmstudio_url = "http://127.0.0.1:1234".into();
        let options = brain_options(&settings, |key| key == "anthropic-api-key");
        let ready: Vec<&str> = options.iter().filter(|b| b.ready).map(|b| b.id.as_str()).collect();
        assert_eq!(ready, ["anthropic", "lmstudio"]);
        assert_eq!(options[0].model, "claude-opus-5");
        assert_eq!(options.len(), BRAINS.len());
    }

    #[test]
    fn the_gateways_port_follows_hermes_own_env() {
        let dir = std::env::temp_dir().join(format!("coucou-hermes-port-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(port_in(&dir), PORT, "no .env: the usual port");
        std::fs::write(dir.join(".env"), "API_SERVER_PORT=8650\n").unwrap();
        assert_eq!(port_in(&dir), 8650);
        std::fs::write(dir.join(".env"), "API_SERVER_PORT=not-a-port\n").unwrap();
        assert_eq!(port_in(&dir), PORT);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hermes_own_env_key_is_read_as_python_dotenv_reads_it() {
        let text = "\u{feff}# API server\n# API_SERVER_KEY=commented\nAPI_SERVER_KEY=first\nexport API_SERVER_KEY=\"0123456789abcdef-quoted\" # mine\nOTHER=1\n";
        assert_eq!(env_value(text, "API_SERVER_KEY").as_deref(), Some("0123456789abcdef-quoted"));
        assert_eq!(env_value("API_SERVER_KEY='single' \n", "API_SERVER_KEY").as_deref(), Some("single"));
        assert_eq!(env_value("API_SERVER_KEY=plain # note\n", "API_SERVER_KEY").as_deref(), Some("plain"));
        assert_eq!(env_value("API_SERVER_KEY=\n", "API_SERVER_KEY"), None);
        assert_eq!(env_value("API_SERVER_KEYS=x\n", "API_SERVER_KEY"), None);
    }

    #[test]
    fn the_hermes_command_is_found_where_its_installer_puts_it() {
        let dir = std::env::temp_dir().join(format!("coucou-hermes-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let home = dir.join("hermes");
        assert_eq!(launcher_in(&home, None), None);
        let name = if cfg!(windows) { "hermes.exe" } else { "hermes" };
        let inner = home.join("hermes-agent").join(".hermes").join("bin");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join(name), b"").unwrap();
        assert_eq!(launcher_in(&home, None), Some(inner.join(name)), "the checkout's own");
        let published = home.join("bin");
        std::fs::create_dir_all(&published).unwrap();
        std::fs::write(published.join(name), b"").unwrap();
        assert_eq!(launcher_in(&home, None), Some(published.join(name)), "the published one first");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_installation_coucou_started_and_did_not_finish_runs_again() {
        let dir = std::env::temp_dir().join(format!("coucou-hermes-pending-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let launcher = dir.join("bin").join("hermes");
        assert!(!installed_in(&dir, None, false), "no command, not installed");
        assert!(installed_in(&dir, Some(&launcher), false), "the user's own Hermes, whatever its marks");
        assert!(!installed_in(&dir, Some(&launcher), true), "Coucou's, stopped halfway: installed again");
        std::fs::create_dir_all(dir.join("hermes-agent")).unwrap();
        std::fs::write(dir.join("hermes-agent").join(".hermes-bootstrap-complete"), "{}").unwrap();
        assert!(installed_in(&dir, Some(&launcher), true), "its installer finished: whole");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_the_gateway_says_never_shows_a_key() {
        let secrets = vec!["0123456789abcdef0123".to_string(), "sk-ant-api03-SECRET".to_string(), "short".to_string()];
        let line = "auth ok with sk-ant-api03-SECRET and key 0123456789abcdef0123, short stays";
        assert_eq!(redact(line, &secrets), "auth ok with ••• and key •••, short stays");
        assert_eq!(redact(&"x".repeat(1000), &[]).len(), 400);
    }

    #[test]
    fn a_line_in_another_encoding_never_ends_the_reading() {
        // "✓ venv" in code page 437 (0xFB is not UTF-8), then the stage's result.
        let bytes: &[u8] = b"\xfb venv created\r\nplain\n{\"ok\":true,\"stage\":\"venv\",\"skipped\":false}";
        let mut lines = Vec::new();
        for_each_line(bytes, |l| {
            lines.push(l);
            true
        });
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1], "plain");
        assert!(parse_frame(&lines, "venv").unwrap().ok, "the result is still read");
    }

    #[test]
    fn a_gateway_that_keeps_stopping_is_left_stopped() {
        let restarts = Restarts(Mutex::new(Vec::new()));
        assert_eq!(restarts.with_failure(false), 0, "a stop after a long run is no failure");
        assert_eq!(restarts.with_failure(true), 1);
        assert_eq!(restarts.with_failure(true), 2);
        assert_eq!(restarts.with_failure(true), 3);
        assert_eq!(restarts.with_failure(true), 4, "the fourth quick stop in ten minutes: given up");
        restarts.clear();
        assert_eq!(restarts.with_failure(false), 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_step_is_read_line_by_line_and_a_cancelled_one_is_ended() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", r#"echo one; echo two >&2; echo '{"ok":true,"stage":"x","skipped":false}'"#]);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut seen = Vec::new();
        let (ok, lines) = run_lines(cmd, Duration::from_secs(10), &AtomicBool::new(false), |l| seen.push(l.to_string())).unwrap();
        assert!(ok && lines.len() == 3 && seen.contains(&"two".to_string()));
        assert!(parse_frame(&lines, "x").unwrap().ok);
        let mut slow = Command::new("sh");
        slow.args(["-c", "sleep 30"]).stdout(Stdio::piped()).stderr(Stdio::piped());
        {
            use std::os::unix::process::CommandExt;
            slow.process_group(0);
        }
        let started = Instant::now();
        let err = run_lines(slow, Duration::from_millis(300), &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(err.contains("too long") && started.elapsed() < Duration::from_secs(5), "{err}");
    }
}
