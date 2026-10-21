// Preferences, stored as plain JSON in settings.json under platform::config_dir().
// No secret ever lands here — API keys live in the OS keychain (see secrets.rs).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// A field the file lacks takes its value from `Default`, so a settings.json
/// written by an older build still loads, whatever has been added since.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub sound_enabled: bool,
    pub sound_volume: f64,
    pub auto_close_interval: f64,
    /// Hovering the island opens it all the way, and it folds again shortly
    /// after the pointer leaves (the Mac's "Open on hover"). Off by default.
    pub open_on_hover: bool,
    pub absence_interval: f64,
    pub active_integrations: Vec<String>,
    /// The always-on workspace pill (src/core/pills.ts checks it is one).
    pub main_pill: String,
    /// "primary" = the main display, "cursor" = whichever display the mouse is on.
    pub screen: String,
    pub autostart: bool,
    pub hooks_installed: bool,
    /// Claude model used by the chat. Changeable in the settings window.
    pub model: String,
    /// Show the Claude plan pill (5 h and weekly limits) in the island's header.
    /// Off until the user turns it on, so the header stays as it shipped.
    pub show_plan_in_notch: bool,
    /// Coucou's status line relay is the one in Claude Code's settings.json.
    /// Like `hooks_installed`, the real state wins at launch over what was stored.
    pub plan_relay_installed: bool,
    /// Show the Codex plan pill (5 h / weekly limits from `codex app-server`).
    /// Off by default; nothing is installed for it.
    pub show_codex_plan_in_notch: bool,
    /// Who the chat talks to: "anthropic", a cloud provider of
    /// openai_compat.rs ("openai", "google", "openrouter"), or a model server
    /// of local_chat.rs ("ollama", "lmstudio", "custom"). Picked in the chat view.
    pub chat_provider: String,
    /// The model picked for each provider other than Anthropic (whose model is
    /// `model`), by provider id.
    pub chat_models: BTreeMap<String, String>,
    /// Addresses of the model servers once connected; empty means not connected.
    pub ollama_url: String,
    pub lmstudio_url: String,
    /// Any other OpenAI-compatible server; its key, if any, is in the keychain.
    pub custom_url: String,
    /// Hermes Agent's API server once connected (hermes.rs); empty means not connected. Its key is in the keychain,
    /// bound to this address. Owned by Rust: only Settings → Hermes Agent's commands change it.
    pub hermes_url: String,
    /// Global shortcuts the user changed, by action id; the others keep their
    /// default (see shortcuts.rs).
    pub shortcuts: crate::shortcuts::Bindings,
    /// Mochi's outfit, picked in the wardrobe: "auto" (dresses for the
    /// season), "none" or an outfit id — the Mac's raw values. The island reads
    /// anything it doesn't know as "auto", so the value is stored as it comes.
    pub mochi_outfit: String,
    /// A colour of the user's own for a pill's Mochi, by pill ID ("#RRGGBB"),
    /// picked in Settings → Active pills. Empty means the catalog's colours.
    /// Kept as it comes, like `mochi_outfit`: src/core/pill-colors.ts reads
    /// whatever is not a colour as "no choice".
    pub pill_colors: BTreeMap<String, String>,
    /// Interface language: "" follows the system, else one of i18n::LANGUAGES
    /// ("fr", "pt-BR", "zh-Hans"…). Kept as it comes, like `mochi_outfit`: a
    /// code this build doesn't know reads as "".
    pub language: String,
    /// Mochi on the desktop: whether he lives there, and his spot. Owned by
    /// the Rust side (desktop.rs) — what a webview sends back is ignored.
    pub desktop_mochi: DesktopMochiPref,
    /// The phone link (phone_link/) is on: a TLS server for the Android app on
    /// the local network. Off until the user turns it on in Settings → Phone;
    /// owned by the Rust side, so what a webview sends back is ignored.
    pub phone_link: bool,
    /// The phone may chat through this computer's API keys (phone_link/chat.rs).
    /// Off until the user turns it on in Settings → Android phone; owned by the
    /// Rust side like `phone_link`, so what a webview sends back is ignored.
    pub phone_chat: bool,
    /// Which models the phone may use, as "provider/model". Empty allows none.
    /// Also owned by the Rust side (phone_link::set_chat_models cleans it).
    pub phone_chat_models: Vec<String>,
    /// The phone may see session details (steps, last line, project folder name, colour). Off until the
    /// user turns it on in Settings → Android phone; owned by the Rust side like `phone_link`.
    pub phone_details: bool,
    /// Let the phone answer the questions Claude Code asks (cap `answers`). Off by default; owned by Rust like the other phone switches.
    pub phone_answers: bool,
    /// Show the files an agent changed on the phone, and their diffs on request (cap `diffs`). Off by default; owned by Rust like the other phone switches.
    pub phone_diffs: bool,
    /// Show the Claude and Codex plan usage on the phone (cap `usage`). Off by default; owned by Rust like the other phone switches.
    pub phone_usage: bool,
    /// The service pills whose cards the phone may see, by pill id (cap `services`). None by default; owned by Rust like the other phone switches.
    pub phone_services: Vec<String>,
    /// Reach the phone through the user's own relay when it is not on this network (phone_link/relay_client.rs).
    /// Off by default; owned by Rust like the other phone switches. The access key, the room and the pairing key live
    /// in the OS keystore, never here.
    pub phone_relay: bool,
    /// The relay's address (`wss://…`), empty until the user enters it. Not a secret. Owned by Rust.
    pub phone_relay_url: String,
    /// Mochi's hands (agent/): the chat may read files, change them and run commands on this computer, each change
    /// after a click. Off until the user turns it on in Settings → Mochi's hands; owned by Rust like the phone
    /// switches, so what a webview sends back is ignored.
    pub agent_enabled: bool,
    /// The folder Mochi works in: reading there needs no click. Empty until the user picks one. Owned by Rust.
    pub agent_folder: String,
    /// "read" (look only), "ask" (every change waits for a click, the default) or "edits" (file edits inside the
    /// folder are applied without a click; commands still ask). Owned by Rust.
    pub agent_level: String,
    /// The chat's hands toggle, next to the model button: use Mochi's hands in this chat when they are on.
    /// The island's own choice (it is saved with the other island settings).
    pub chat_hands: bool,
    /// The paired phone may use Mochi's hands through its chat (cap `agent`). Off by default; owned by Rust like
    /// the other phone switches.
    pub phone_agent: bool,
    /// Notifications with Coucou closed (phone_link/push.rs): a request still waiting while no phone is connected is
    /// announced through ntfy. Off by default; owned by Rust like the other phone switches. The topic and an access
    /// token live in the OS keystore, never here.
    pub phone_push: bool,
    /// The ntfy server ("" is https://ntfy.sh). Not a secret. Owned by Rust.
    pub phone_push_server: String,
    /// Away from home, directly (phone_link/remote.rs): the router is asked to open the link's port and the phones
    /// are told the home's internet addresses. Off by default; owned by Rust like the other phone switches.
    pub phone_remote: bool,
    /// Mochi's home (smart_home/): from the chat, Mochi may use the devices of the house the user picked. Off until the
    /// user turns it on in Settings → Mochi's home; owned by Rust like the hands, so what a webview sends back is ignored.
    pub home_enabled: bool,
    /// Every action on the house waits for a click, not only those on plugs, switches and the devices set to ask.
    /// Owned by Rust.
    pub home_ask_all: bool,
    /// Other agents on this computer (Hermes Agent, through `coucou-hook --mcp`) may use Mochi's home, with the same
    /// rules as Mochi. Off by default; owned by Rust (agent::mcp::home_mcp_set).
    pub home_mcp: bool,
    /// Home Assistant's address on the local network ("" none). Its token is in the OS keystore. Owned by Rust.
    pub home_assistant_url: String,
    /// The Philips TVs added by their address; their pairings are in the OS keystore. Owned by Rust.
    pub home_tvs: Vec<crate::smart_home::config::TvConf>,
    /// The user's rules for each device (name, room, may Mochi use it, ask first), by device id. Owned by Rust.
    pub home_devices: BTreeMap<String, crate::smart_home::config::DevicePrefs>,
    /// The routines ("Good night", "Movie mode"…). Owned by Rust.
    pub home_routines: Vec<crate::smart_home::routines::Routine>,
    /// The chat on this computer reads Mochi's answers aloud, with the system's own voices (Settings → Mochi's home →
    /// Voice). Off by default. The island's own choice, like the hands toggle: nothing leaves the computer.
    pub read_aloud: bool,
    /// Coucou installed Hermes Agent or runs it (hermes_setup.rs): its gateway starts with Coucou. Off by default;
    /// owned by Rust.
    pub hermes_managed: bool,
    /// The provider Coucou handed to Hermes as its brain ("anthropic", "google"…): its key goes to the gateway's
    /// environment, never to a file. Owned by Rust.
    pub hermes_brain: String,
    /// The name the user gives their assistant ("Zheus"; "" shows as that): Hermes is told it answers to it, in its
    /// SOUL.md and with each message. Owned by Rust: only Settings → Hermes Agent's Save (hermes_setup::set_assistant),
    /// the installer's page (first_run.rs) and the phone change it.
    pub assistant_name: String,
    /// Its tone, one of hermes_setup::PERSONAS ("" is "friendly"). Owned by Rust, like the name.
    pub assistant_persona: String,
    /// The user's own tone, for "custom" (at most 1500 characters, kept as typed). Owned by Rust, and never logged.
    pub assistant_persona_text: String,
    /// The skill packs the user wants Hermes to have ("search", "news"; Mochi's home is `home_mcp`). Owned by Rust.
    pub hermes_skills: Vec<String>,
    /// The packs Coucou itself installed in Hermes: only these are ever removed, never a skill the user installed.
    /// Owned by Rust.
    pub hermes_skills_installed: Vec<String>,
    /// Hermes may see and use this computer (screen, mouse, keyboard) with its computer_use tool, on the API server
    /// Coucou talks to. Off by default; owned by Rust (hermes_setup::set_computer_use).
    pub hermes_computer_use: bool,
    /// Coucou turned Hermes' own (hidden) browser off for its API server, with the computer use
    /// (hermes_setup::computer_use_commands). Owned by Rust.
    pub hermes_browser_off: bool,
    /// How quick Hermes is: "fast", "" (Hermes' own) or "deep" (hermes_setup::SPEEDS). Owned by Rust.
    pub hermes_speed: String,
    /// The paired phone may change the assistant (name, tone, brain, skills; cap `assistant`). Off by default; owned
    /// by Rust like the other phone switches.
    pub phone_assistant: bool,
    /// The paired phone may use Mochi's home (cap `home`). Off by default; owned by Rust like the other phone switches.
    pub phone_home: bool,
    /// Zheus One (zheus_one/): missions, the World State, Foresight and the Autopilot, on top of Mochi's home. Off by
    /// default; owned by Rust (zheus_one::zheus_set).
    pub zheus_enabled: bool,
    /// How Zheus prepares the user's arrival. Owned by Rust (zheus_one::zheus_prefs_set).
    pub zheus_arrival: crate::zheus_one::plan::ArrivalPrefs,
    /// The Autopilot's goals and their limits. Owned by Rust, like the arrival.
    pub zheus_goals: crate::zheus_one::autopilot::Goals,
    /// The island's look: "glass" (Liquid Glass, translucent and frosted) or "classic" (black). The island's own choice.
    pub island_look: String,
    /// Zheus' superpowers (powers/): the user's portfolio, studies, English and training, read against the official
    /// data (B3, the CVM, FundosNet, the Banco Central) and given to the brain as tools. Off by default; owned by Rust
    /// (powers::powers_set). The data itself is in powers.json, beside this file.
    pub powers_enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DesktopMochiPref {
    /// He was on the desktop when the app quit: he flies back out at launch.
    pub on_desktop: bool,
    /// Top-left corner of his window where the user last left him.
    pub spot: Option<DesktopSpot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesktopSpot {
    pub x: f64,
    pub y: f64,
    /// What x and y are measured in (`DesktopMode::space`).
    pub space: String,
}

fn default_model() -> String {
    crate::claude::DEFAULT_MODEL.to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sound_enabled: true,
            sound_volume: 0.12,
            auto_close_interval: 15.0,
            open_on_hover: false,
            absence_interval: 180.0,
            active_integrations: vec![
                "integration_resend".into(),
                "integration_n8n".into(),
                "integration_vercel".into(),
                "integration_github".into(),
            ],
            main_pill: "integration_claude".into(),
            screen: "primary".into(),
            autostart: false,
            hooks_installed: false,
            model: default_model(),
            show_plan_in_notch: false,
            plan_relay_installed: false,
            show_codex_plan_in_notch: false,
            chat_provider: crate::chat::ANTHROPIC.into(),
            chat_models: BTreeMap::new(),
            ollama_url: String::new(),
            lmstudio_url: String::new(),
            custom_url: String::new(),
            hermes_url: String::new(),
            shortcuts: Default::default(),
            mochi_outfit: "auto".into(),
            pill_colors: BTreeMap::new(),
            language: String::new(),
            desktop_mochi: DesktopMochiPref::default(),
            phone_link: false,
            phone_chat: false,
            phone_chat_models: Vec::new(),
            phone_details: false,
            phone_answers: false,
            phone_diffs: false,
            phone_usage: false,
            phone_services: Vec::new(),
            phone_relay: false,
            phone_relay_url: String::new(),
            agent_enabled: false,
            agent_folder: String::new(),
            agent_level: "ask".into(),
            chat_hands: true,
            phone_agent: false,
            phone_push: false,
            phone_push_server: String::new(),
            phone_remote: false,
            home_enabled: false,
            home_ask_all: false,
            home_mcp: false,
            home_assistant_url: String::new(),
            home_tvs: Vec::new(),
            home_devices: BTreeMap::new(),
            home_routines: Vec::new(),
            read_aloud: false,
            hermes_managed: false,
            hermes_brain: String::new(),
            assistant_name: String::new(),
            assistant_persona: String::new(),
            assistant_persona_text: String::new(),
            hermes_skills: Vec::new(),
            hermes_skills_installed: Vec::new(),
            hermes_computer_use: false,
            hermes_browser_off: false,
            hermes_speed: String::new(),
            phone_assistant: false,
            phone_home: false,
            zheus_enabled: false,
            zheus_arrival: crate::zheus_one::plan::ArrivalPrefs::default(),
            zheus_goals: crate::zheus_one::autopilot::Goals::default(),
            island_look: "glass".into(),
            powers_enabled: false,
        }
    }
}

pub use crate::platform::{config_dir, local_dir};

pub fn hook_exe_path() -> PathBuf {
    local_dir().join("bin").join(crate::platform::HOOK_EXE)
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn load() -> Settings {
    load_from(&settings_path())
}

pub fn save(settings: &Settings) -> std::io::Result<()> {
    let dir = config_dir();
    crate::platform::ensure_private_dir(&dir)?;
    save_to(&settings_path(), settings)
}

/// How many copies `set_aside` can make within one second before it gives up.
const MAX_COPIES_PER_SECOND: u32 = 100;

/// settings.json files that are there but whose contents never made it into
/// memory: they could not be read, or could not be used and could not be copied
/// aside either. What is in memory for them is the defaults, so the next save
/// must not quietly replace them — see `keep_what_was_not_loaded`.
static NOT_LOADED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn not_loaded() -> MutexGuard<'static, Vec<PathBuf>> {
    NOT_LOADED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One line in coucou.log. Tests must never write to the real one.
fn note(message: String) {
    #[cfg(not(test))]
    crate::log::line(message);
    #[cfg(test)]
    eprintln!("{message}");
}

/// The settings held in the bytes of a settings.json, or `None` when there is
/// nothing usable in them. Split from `load_from` so it can be tested without a
/// file.
fn parse(bytes: &[u8]) -> Option<Settings> {
    // PowerShell 5 and Notepad write a UTF-8 BOM, and serde_json refuses it.
    let text = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if text.iter().all(u8::is_ascii_whitespace) {
        return Some(Settings::default());
    }
    let Value::Object(fields) = serde_json::from_slice(text).ok()? else {
        return None;
    };
    let whole = serde_json::from_value(Value::Object(fields.clone()));
    Some(whole.unwrap_or_else(|_| salvage(fields)))
}

/// Settings from the fields of a settings.json that does not load as a whole.
/// Each field is taken on its own: one that is not the kind of value expected
/// falls back to its default instead of costing every other setting.
fn salvage(fields: Map<String, Value>) -> Settings {
    let Ok(Value::Object(mut kept)) = serde_json::to_value(Settings::default()) else {
        return Settings::default();
    };
    for (key, value) in fields {
        let mut candidate = kept.clone();
        candidate.insert(key.clone(), value);
        if serde_json::from_value::<Settings>(Value::Object(candidate.clone())).is_ok() {
            kept = candidate;
        } else {
            note(unusable_field(&key));
        }
    }
    serde_json::from_value(Value::Object(kept)).unwrap_or_default()
}

/// The log line for a field `salvage` had to drop. The name comes straight from
/// the file, so it is written escaped: a line break in it must not be able to
/// start what looks like another line of coucou.log.
fn unusable_field(key: &str) -> String {
    format!("settings.json: {key:?} is not usable — its default is used instead")
}

/// `load`, for the file at `path`: tests point it at a temporary directory.
///
/// Only a file that is not there means "start from the defaults" with nothing
/// more to do. One that cannot be read, or cannot be used, also gives the
/// defaults, but it still holds what the user had: it is kept, not written over.
fn load_from(path: &Path) -> Settings {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Settings::default(),
        Err(err) => {
            note(format!("can't read {}: {err} — using the defaults", path.display()));
            protect(path);
            return Settings::default();
        }
    };
    parse(&bytes).unwrap_or_else(|| {
        match set_aside(path, "corrupt", &bytes) {
            Ok(copy) => note(format!(
                "{} is not usable — kept as {}, using the defaults",
                path.display(),
                copy.display()
            )),
            Err(err) => {
                note(format!("{} is not usable and could not be copied: {err}", path.display()));
                protect(path);
            }
        }
        Settings::default()
    })
}

/// Marks `path` as holding something the next save must not write over.
fn protect(path: &Path) {
    let mut pending = not_loaded();
    if !pending.iter().any(|p| p == path) {
        pending.push(path.to_path_buf());
    }
}

/// Before the first save over a file `load_from` could not load: copies it
/// aside if it can be read by now, and refuses the save if it still cannot.
fn keep_what_was_not_loaded(path: &Path) -> std::io::Result<()> {
    let mut pending = not_loaded();
    let Some(index) = pending.iter().position(|p| p == path) else {
        return Ok(());
    };
    match std::fs::read(path) {
        Ok(bytes) => {
            let copy = set_aside(path, "unread", &bytes)?;
            note(format!("{} was never loaded — kept as {}", path.display(), copy.display()));
        }
        // Gone since: nothing is left to protect.
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => {
            return Err(std::io::Error::new(
                err.kind(),
                format!("{} still can't be read, so it is left as it is: {err}", path.display()),
            ));
        }
    }
    pending.swap_remove(index);
    Ok(())
}

/// Keeps `bytes`, the contents of `path`, in a file beside it named after it
/// with `.<kind>-YYYYMMDD-HHMMSS` appended. Contents an earlier copy of that
/// kind already holds are not copied again: a file nobody repairs would
/// otherwise leave one more copy behind at every launch.
fn set_aside(path: &Path, kind: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    if let Some(copy) = same_copy(path, kind, bytes) {
        return Ok(copy);
    }
    let t = crate::platform::local_time();
    let stamp = format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    );
    set_aside_as(path, kind, &stamp, bytes, write_whole)
}

/// The copy of `path` of this `kind` that already holds exactly `bytes`.
fn same_copy(path: &Path, kind: &str, bytes: &[u8]) -> Option<PathBuf> {
    let prefix = format!("{}.{kind}-", path.file_name()?.to_string_lossy());
    std::fs::read_dir(path.parent()?)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .filter(|entry| entry.metadata().is_ok_and(|m| m.len() == bytes.len() as u64))
        .map(|entry| entry.path())
        .find(|copy| std::fs::read(copy).is_ok_and(|held| held == bytes))
}

/// `set_aside` for a given time stamp, writing through `write`. `-1`, `-2`… are
/// appended while the name is taken, so a copy that is already there is never
/// written over; and a copy that could not be written whole is removed, never
/// left looking like a faithful one.
fn set_aside_as(
    path: &Path,
    kind: &str,
    stamp: &str,
    bytes: &[u8],
    write: fn(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
) -> std::io::Result<PathBuf> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let base = format!("{name}.{kind}-{stamp}");
    for attempt in 0..MAX_COPIES_PER_SECOND {
        let copy = match attempt {
            0 => path.with_file_name(&base),
            n => path.with_file_name(format!("{base}-{n}")),
        };
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&copy) {
            Ok(mut file) => {
                let written = write(&mut file, bytes);
                drop(file);
                if written.is_err() {
                    let _ = std::fs::remove_file(&copy);
                }
                return written.map(|()| copy);
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(std::io::Error::new(ErrorKind::AlreadyExists, format!("too many copies named {base}")))
}

/// Writes `bytes` to `file` and waits until they are on the disk.
fn write_whole(file: &mut std::fs::File, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    file.write_all(bytes)?;
    file.sync_all()
}

/// `save`, for the file at `path`, whose directory already exists.
fn save_to(path: &Path, settings: &Settings) -> std::io::Result<()> {
    keep_what_was_not_loaded(path)?;
    let json = serde_json::to_vec_pretty(settings)
        .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))?;

    // A dotfiles setup can make settings.json a symlink: write to the file it
    // points at, so the link survives the rename below.
    #[cfg(unix)]
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    #[cfg(unix)]
    let path = resolved.as_path();

    // Write beside the target and rename over it: a crash, a full disk or a
    // power cut leaves the previous settings.json intact rather than half a file.
    let temp = path.with_extension(format!("json.coucou-{}", std::process::id()));
    let written = std::fs::File::create(&temp)
        .and_then(|mut file| write_whole(&mut file, &json))
        .and_then(|()| std::fs::rename(&temp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    /// A settings.json in which no value is the default one.
    // Two #: the colours in it are written "#RRGGBB".
    const CUSTOM: &str = r##"{
  "soundEnabled": false,
  "soundVolume": 0.5,
  "autoCloseInterval": 30.0,
  "openOnHover": true,
  "absenceInterval": 60.0,
  "activeIntegrations": ["integration_notion"],
  "mainPill": "agent_cursor",
  "screen": "cursor",
  "autostart": true,
  "hooksInstalled": true,
  "model": "some-model",
  "showPlanInNotch": true,
  "planRelayInstalled": true,
  "showCodexPlanInNotch": true,
  "chatProvider": "ollama",
  "chatModels": { "ollama": "llama3.2", "openai": "gpt-x" },
  "ollamaUrl": "http://127.0.0.1:11434",
  "lmstudioUrl": "http://127.0.0.1:1234",
  "customUrl": "https://llm.example.com",
  "hermesUrl": "http://127.0.0.1:8642",
  "shortcuts": { "openChat": { "keys": "Ctrl+Shift+K", "enabled": false } },
  "mochiOutfit": "witchHat",
  "pillColors": { "integration_claude": "#2DD4BF" },
  "language": "pt-BR",
  "desktopMochi": { "onDesktop": true, "spot": { "x": 1500.5, "y": -300.0, "space": "screen" } },
  "phoneLink": true,
  "phoneChat": true,
  "phoneChatModels": ["openai/gpt-x"],
  "phoneDetails": true,
  "phoneAnswers": true,
  "phoneDiffs": true,
  "phoneUsage": true,
  "phoneServices": ["integration_stripe"],
  "phoneRelay": true,
  "phoneRelayUrl": "wss://relay.example.workers.dev",
  "agentEnabled": true,
  "agentFolder": "C:\\Users\\me\\work",
  "agentLevel": "edits",
  "chatHands": false,
  "phoneAgent": true,
  "phonePush": true,
  "phonePushServer": "https://ntfy.example.com",
  "phoneRemote": true,
  "homeEnabled": true,
  "homeAskAll": true,
  "homeMcp": true,
  "homeAssistantUrl": "http://homeassistant.local:8123/",
  "homeTvs": [
    { "id": "philips:0a1b2c3d", "brand": "", "host": "192.168.1.20", "api": 6, "https": true, "pin": "", "mac": "A4:B1:C2:D3:E4:F5", "name": "Philips TV", "model": "55PUS8007/12", "os": "MSAF_2019_P", "version": "6.4.0", "features": { "inputkey": ["key"] }, "needsPairing": true, "frame": false, "renderer": "", "apps": [] },
    { "id": "samsung:0e0f1a1b", "brand": "samsung", "host": "192.168.0.9", "api": 2, "https": true, "pin": "", "mac": "A4:30:7A:12:34:56", "name": "Samsung 7 Series (43)", "model": "UN43AU7700GXZD", "os": "Tizen", "version": "2.0.25", "features": {}, "needsPairing": true, "frame": true, "renderer": "http://192.168.0.9:9197/upnp/control/RenderingControl1", "apps": [{ "id": "111299001912", "name": "YouTube", "kind": 2 }] }
  ],
  "homeDevices": { "philips:0a1b2c3d": { "name": "TV da sala", "room": "Sala", "exposed": true, "ask": false } },
  "homeRoutines": [{ "id": "r-0a1b2c3d4e5f", "name": "Boa noite", "steps": [{ "device": "philips:0a1b2c3d", "action": "turn_off", "value": "", "wait": 0 }] }],
  "readAloud": true,
  "hermesManaged": true,
  "hermesBrain": "anthropic",
  "assistantName": "Zheus",
  "assistantPersona": "butler",
  "assistantPersonaText": "Fale como um capitão de navio.\nSempre breve.",
  "hermesSkills": ["search", "news"],
  "hermesSkillsInstalled": ["search"],
  "hermesComputerUse": true,
  "hermesBrowserOff": true,
  "hermesSpeed": "fast",
  "phoneAssistant": true,
  "phoneHome": true,
  "zheusEnabled": true,
  "zheusArrival": {
    "eta": 25,
    "comfort": 22,
    "acLead": 10,
    "vacuum": true,
    "rooms": [{ "room": "Quarto", "minutes": 15 }, { "room": "Sala", "minutes": 20 }],
    "whole": 45,
    "minBattery": 35,
    "lights": ["ha:light.sala"],
    "darkFrom": 1050,
    "darkUntil": 390,
    "tv": "philips:0a1b2c3d",
    "tvApp": "Netflix",
    "summary": false
  },
  "zheusGoals": {
    "energy": "act",
    "tidy": "suggest",
    "agenda": "act",
    "business": "suggest",
    "tidyFrom": 600,
    "tidyUntil": 960,
    "agendaAt": 450,
    "awayMinutes": 30
  },
  "islandLook": "classic",
  "powersEnabled": true
}"##;

    fn custom() -> Value {
        serde_json::from_str(CUSTOM).unwrap()
    }

    /// `CUSTOM` with one key replaced, or removed when `value` is `None`.
    fn custom_with(key: &str, value: Option<Value>) -> Vec<u8> {
        let mut object = custom().as_object().unwrap().clone();
        match value {
            Some(value) => object.insert(key.to_string(), value),
            None => object.remove(key),
        };
        serde_json::to_vec(&object).unwrap()
    }

    /// Settings compare through their JSON: the struct has no `PartialEq`.
    fn shown(settings: &Settings) -> Value {
        serde_json::to_value(settings).unwrap()
    }

    fn defaults() -> Value {
        shown(&Settings::default())
    }

    /// A fresh directory of our own, and the settings.json it will hold.
    fn scratch(name: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir()
            .join(format!("coucou-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        (dir, file)
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    /// What was set aside under `settings.json.<kind>-…`.
    fn kept(dir: &Path, kind: &str) -> Vec<Vec<u8>> {
        let prefix = format!("settings.json.{kind}-");
        names(dir)
            .iter()
            .filter(|name| name.starts_with(&prefix))
            .map(|name| std::fs::read(dir.join(name)).unwrap())
            .collect()
    }

    // ── Reading ───────────────────────────────────────────────────────────────

    #[test]
    fn a_missing_file_is_the_defaults_and_is_not_created() {
        let (dir, file) = scratch("missing");
        assert_eq!(shown(&load_from(&file)), defaults());
        assert!(names(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_file_is_the_defaults_and_nothing_is_set_aside() {
        let (dir, file) = scratch("empty");
        for nothing in [&b""[..], &b" \r\n\t "[..]] {
            std::fs::write(&file, nothing).unwrap();
            assert_eq!(shown(&load_from(&file)), defaults());
        }
        assert_eq!(names(&dir), ["settings.json"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_fields_are_ignored_and_the_known_ones_kept() {
        let bytes = custom_with("somethingFromALaterVersion", Some(json!({ "a": [1, 2] })));
        assert_eq!(shown(&parse(&bytes).unwrap()), custom());
    }

    #[test]
    fn a_file_from_before_the_model_setting_keeps_everything_else() {
        let loaded = shown(&parse(&custom_with("model", None)).unwrap());
        let mut expected = custom();
        expected["model"] = json!(crate::claude::DEFAULT_MODEL);
        assert_eq!(loaded, expected);
    }

    #[test]
    fn a_file_from_before_the_wardrobe_dresses_mochi_for_the_seasons() {
        let loaded = parse(&custom_with("mochiOutfit", None)).unwrap();
        assert_eq!(loaded.mochi_outfit, "auto");
        assert_eq!(loaded.model, "some-model");
        assert!(!loaded.sound_enabled);
    }

    #[test]
    fn a_file_from_before_the_desktop_mochi_keeps_him_in_the_island() {
        let loaded = parse(&custom_with("desktopMochi", None)).unwrap();
        assert_eq!(loaded.desktop_mochi, DesktopMochiPref::default());
        assert!(!loaded.desktop_mochi.on_desktop);
        assert_eq!(loaded.mochi_outfit, "witchHat");
    }

    #[test]
    fn a_file_from_before_the_phone_link_keeps_it_off() {
        // The link opens a port on the local network: it is never on by default,
        // and an older file must not be read as consent.
        assert!(!Settings::default().phone_link);
        let loaded = parse(&custom_with("phoneLink", None)).unwrap();
        assert!(!loaded.phone_link);
        assert_eq!(loaded.mochi_outfit, "witchHat");
        assert!(parse(&custom_with("phoneLink", Some(serde_json::json!(true)))).unwrap().phone_link);

        // A value of the wrong type does not switch it on either, and costs nothing else.
        let (dir, file) = scratch("phone-link");
        std::fs::write(&file, custom_with("phoneLink", Some(serde_json::json!("yes")))).unwrap();
        let odd = load_from(&file);
        assert!(!odd.phone_link);
        assert_eq!(odd.mochi_outfit, "witchHat");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_from_before_phone_chat_keeps_it_off_and_empty() {
        // Chat spends the user's API credit: never on by default, and an older file is not consent.
        let d = Settings::default();
        assert!(!d.phone_chat);
        assert!(d.phone_chat_models.is_empty());
        let loaded = parse(&custom_with("phoneChat", None)).unwrap();
        assert!(!loaded.phone_chat);
        assert!(parse(&custom_with("phoneChat", Some(serde_json::json!(true)))).unwrap().phone_chat);
        let odd = parse(&custom_with("phoneChat", Some(serde_json::json!("yes")))).unwrap();
        assert!(!odd.phone_chat, "a wrong type does not switch it on");
        assert_eq!(odd.mochi_outfit, "witchHat");
        let models = parse(&custom_with("phoneChatModels", Some(serde_json::json!(["openai/gpt-x"])))).unwrap();
        assert_eq!(models.phone_chat_models, vec!["openai/gpt-x".to_string()]);
        let bad = parse(&custom_with("phoneChatModels", Some(serde_json::json!("openai/gpt-x")))).unwrap();
        assert!(bad.phone_chat_models.is_empty());
    }

    #[test]
    fn a_file_from_before_phone_answers_keeps_it_off() {
        // Answering for the agent from the phone is never on by default; an older file is not consent.
        assert!(!Settings::default().phone_answers);
        assert!(!parse(&custom_with("phoneAnswers", None)).unwrap().phone_answers);
        assert!(parse(&custom_with("phoneAnswers", Some(serde_json::json!(true)))).unwrap().phone_answers);
        assert!(!parse(&custom_with("phoneAnswers", Some(serde_json::json!("yes")))).unwrap().phone_answers, "a wrong type does not switch it on");
    }

    #[test]
    fn a_file_from_before_phone_diffs_keeps_it_off() {
        // Showing what an agent changed on the phone is never on by default; an older file is not consent.
        assert!(!Settings::default().phone_diffs);
        assert!(!parse(&custom_with("phoneDiffs", None)).unwrap().phone_diffs);
        assert!(parse(&custom_with("phoneDiffs", Some(serde_json::json!(true)))).unwrap().phone_diffs);
        assert!(!parse(&custom_with("phoneDiffs", Some(serde_json::json!("yes")))).unwrap().phone_diffs, "a wrong type does not switch it on");
    }

    #[test]
    fn a_file_from_before_phone_usage_keeps_it_off() {
        assert!(!Settings::default().phone_usage);
        assert!(!parse(&custom_with("phoneUsage", None)).unwrap().phone_usage);
        assert!(parse(&custom_with("phoneUsage", Some(serde_json::json!(true)))).unwrap().phone_usage);
        assert!(!parse(&custom_with("phoneUsage", Some(serde_json::json!("yes")))).unwrap().phone_usage, "a wrong type does not switch it on");
    }

    #[test]
    fn a_file_from_before_the_relay_keeps_it_off_and_empty() {
        assert!(!Settings::default().phone_relay);
        assert!(Settings::default().phone_relay_url.is_empty());
        let old = parse(&custom_with("phoneRelay", None)).unwrap();
        assert!(!old.phone_relay);
        assert!(parse(&custom_with("phoneRelay", Some(serde_json::json!(true)))).unwrap().phone_relay);
        assert!(!parse(&custom_with("phoneRelay", Some(serde_json::json!("yes")))).unwrap().phone_relay, "a wrong type does not switch it on");
        assert!(parse(&custom_with("phoneRelayUrl", Some(serde_json::json!(5)))).unwrap().phone_relay_url.is_empty());
    }

    #[test]
    fn a_file_from_before_mochis_hands_keeps_them_off() {
        // Mochi acting on the computer is never on by default, and an older file is not consent.
        let d = Settings::default();
        assert!(!d.agent_enabled && !d.phone_agent);
        assert!(d.agent_folder.is_empty());
        assert_eq!(d.agent_level, "ask");
        assert!(d.chat_hands, "the chat uses the hands once the user has turned them on");
        for key in ["agentEnabled", "phoneAgent"] {
            let loaded = parse(&custom_with(key, None)).unwrap();
            assert!(!(if key == "agentEnabled" { loaded.agent_enabled } else { loaded.phone_agent }), "{key}");
            let odd = parse(&custom_with(key, Some(serde_json::json!("yes")))).unwrap();
            assert!(!(if key == "agentEnabled" { odd.agent_enabled } else { odd.phone_agent }), "a wrong type does not switch {key} on");
            assert_eq!(odd.mochi_outfit, "witchHat");
        }
        assert_eq!(parse(&custom_with("agentLevel", None)).unwrap().agent_level, "ask");
        assert!(parse(&custom_with("agentFolder", Some(serde_json::json!(5)))).unwrap().agent_folder.is_empty());
    }

    #[test]
    fn a_file_from_before_mochis_home_keeps_it_off_and_empty() {
        // Mochi acting on the house is never on by default, and an older file is not consent.
        let d = Settings::default();
        assert!(!d.home_enabled && !d.home_ask_all && d.home_assistant_url.is_empty());
        assert!(d.home_tvs.is_empty() && d.home_devices.is_empty() && d.home_routines.is_empty());
        assert!(!parse(&custom_with("homeEnabled", None)).unwrap().home_enabled);
        assert!(!parse(&custom_with("homeEnabled", Some(serde_json::json!("yes")))).unwrap().home_enabled, "a wrong type does not switch it on");
        // A broken TV list costs only itself.
        let odd = parse(&custom_with("homeTvs", Some(serde_json::json!("nope")))).unwrap();
        assert!(odd.home_tvs.is_empty());
        assert_eq!(odd.home_routines.len(), 1);
        // A TV kept before Samsung TVs came is a Philips one.
        let mut older = custom();
        older["homeTvs"][0].as_object_mut().unwrap().remove("brand");
        let tvs = parse(&serde_json::to_vec(&older).unwrap()).unwrap().home_tvs;
        assert!(!tvs[0].samsung() && tvs[1].samsung());
        // Reading answers aloud is a choice too: off until the user turns it on.
        assert!(!d.read_aloud);
        assert!(!parse(&custom_with("readAloud", None)).unwrap().read_aloud);
    }

    #[test]
    fn a_file_from_before_push_notifications_keeps_them_off() {
        // Sending anything to a push service is never on by default; an older file is not consent.
        let d = Settings::default();
        assert!(!d.phone_push && d.phone_push_server.is_empty());
        assert!(!parse(&custom_with("phonePush", None)).unwrap().phone_push);
        assert!(!parse(&custom_with("phonePush", Some(serde_json::json!("yes")))).unwrap().phone_push, "a wrong type does not switch it on");
        assert!(parse(&custom_with("phonePushServer", Some(serde_json::json!(5)))).unwrap().phone_push_server.is_empty());
    }

    #[test]
    fn opening_a_port_on_the_router_is_never_on_by_default() {
        // An older file, or a wrong type, is not consent to be reachable from the internet.
        assert!(!Settings::default().phone_remote);
        assert!(!parse(&custom_with("phoneRemote", None)).unwrap().phone_remote);
        assert!(!parse(&custom_with("phoneRemote", Some(serde_json::json!("yes")))).unwrap().phone_remote);
        assert!(parse(CUSTOM.as_bytes()).is_some_and(|s| s.phone_remote));
    }

    #[test]
    fn a_file_from_before_phone_services_shows_none() {
        // Showing a service on the phone is never on by default; an older file is not consent.
        assert!(Settings::default().phone_services.is_empty());
        assert!(parse(&custom_with("phoneServices", None)).unwrap().phone_services.is_empty());
        assert_eq!(parse(&custom_with("phoneServices", Some(serde_json::json!(["integration_stripe"])))).unwrap().phone_services, vec!["integration_stripe".to_string()]);
        assert!(parse(&custom_with("phoneServices", Some(serde_json::json!("integration_stripe")))).unwrap().phone_services.is_empty(), "a wrong type shows none");
    }

    #[test]
    fn a_file_from_before_the_assistant_and_home_on_the_phone_keeps_them_off() {
        // Changing the assistant or using the house from the phone is never on by default; an older file is not consent.
        let d = Settings::default();
        assert!(!d.phone_assistant && !d.phone_home);
        for key in ["phoneAssistant", "phoneHome"] {
            let old = parse(&custom_with(key, None)).unwrap();
            assert!(!(if key == "phoneAssistant" { old.phone_assistant } else { old.phone_home }), "{key}");
            let odd = parse(&custom_with(key, Some(json!("yes")))).unwrap();
            assert!(!(if key == "phoneAssistant" { odd.phone_assistant } else { odd.phone_home }), "a wrong type does not switch {key} on");
            assert_eq!(odd.assistant_persona, "butler");
        }
    }

    #[test]
    fn a_file_from_before_the_tone_and_skills_gives_the_friendly_tone_and_no_skill() {
        let old = parse(&custom_with("assistantPersona", None)).unwrap();
        assert_eq!(old.assistant_persona, "", "\"\" is the friendly tone");
        assert_eq!(old.assistant_name, "Zheus");
        let loaded = parse(CUSTOM.as_bytes()).unwrap();
        assert_eq!(loaded.assistant_persona_text, "Fale como um capitão de navio.\nSempre breve.", "kept as typed");
        assert_eq!(loaded.hermes_skills, ["search", "news"]);
        assert_eq!(loaded.hermes_skills_installed, ["search"]);
        assert!(loaded.hermes_computer_use);
        assert!(!parse(&custom_with("hermesComputerUse", None)).unwrap().hermes_computer_use, "off by default");
        assert!(loaded.hermes_browser_off && loaded.hermes_speed == "fast");
        let usual = parse(&custom_with("hermesSpeed", None)).unwrap();
        assert!(usual.hermes_speed.is_empty(), "Hermes' own speed by default");
        assert!(!parse(&custom_with("hermesBrowserOff", None)).unwrap().hermes_browser_off);
        let bad = parse(&custom_with("hermesSkills", Some(json!("search")))).unwrap();
        assert!(bad.hermes_skills.is_empty() && bad.hermes_skills_installed == ["search"], "a wrong type costs only itself");
    }

    #[test]
    fn a_file_from_before_phone_details_keeps_them_off() {
        // Details can include commands and folder names: never on by default, an older file is not consent.
        assert!(!Settings::default().phone_details);
        assert!(!parse(&custom_with("phoneDetails", None)).unwrap().phone_details);
        assert!(parse(&custom_with("phoneDetails", Some(serde_json::json!(true)))).unwrap().phone_details);
        let odd = parse(&custom_with("phoneDetails", Some(serde_json::json!("yes")))).unwrap();
        assert!(!odd.phone_details, "a wrong type does not switch it on");
        assert_eq!(odd.mochi_outfit, "witchHat");
    }

    #[test]
    fn a_half_written_desktop_spot_costs_only_the_spot() {
        let loaded = parse(&custom_with(
            "desktopMochi",
            Some(json!({ "onDesktop": true, "spot": { "x": "left" } })),
        ))
        .unwrap();
        // The whole field falls back, and nothing else does.
        assert_eq!(loaded.desktop_mochi, DesktopMochiPref::default());
        assert_eq!(loaded.model, "some-model");

        let loaded = parse(&custom_with("desktopMochi", Some(json!({ "onDesktop": true })))).unwrap();
        assert!(loaded.desktop_mochi.on_desktop);
        assert_eq!(loaded.desktop_mochi.spot, None);
    }

    #[test]
    fn a_file_from_before_the_colours_paints_every_pill_as_the_catalog_says() {
        let loaded = parse(&custom_with("pillColors", None)).unwrap();
        assert!(loaded.pill_colors.is_empty());
        assert_eq!(loaded.mochi_outfit, "witchHat");
    }

    #[test]
    fn pill_colours_are_kept_as_written_and_cost_nothing_else_when_unusable() {
        // A pill a newer build added keeps its colour through a save by this one.
        let loaded = parse(&custom_with("pillColors", Some(json!({ "agent_new": "#abcdef" })))).unwrap();
        assert_eq!(loaded.pill_colors.get("agent_new").map(String::as_str), Some("#abcdef"));
        // Not a map of strings: the colours fall back, and nothing else does.
        let loaded = parse(&custom_with("pillColors", Some(json!(["#2DD4BF"])))).unwrap();
        assert!(loaded.pill_colors.is_empty());
        assert_eq!(loaded.model, "some-model");
    }

    #[test]
    fn an_outfit_this_build_does_not_know_is_kept_as_written() {
        // A newer build may add outfits: the island shows "auto" for it, but
        // the choice must survive a save made by this one.
        let loaded = parse(&custom_with("mochiOutfit", Some(json!("topHat")))).unwrap();
        assert_eq!(loaded.mochi_outfit, "topHat");
        // The language too: "" (follow the system) when absent, as it comes otherwise.
        assert_eq!(parse(&custom_with("language", None)).unwrap().language, "");
        assert_eq!(parse(&custom_with("language", Some(json!("xx")))).unwrap().language, "xx");
    }

    #[test]
    fn a_missing_field_takes_its_default_and_the_rest_are_kept() {
        for key in custom().as_object().unwrap().keys() {
            let loaded = parse(&custom_with(key, None))
                .unwrap_or_else(|| panic!("a file without {key} was thrown away"));
            let mut expected = custom();
            expected[key] = defaults()[key].clone();
            assert_eq!(shown(&loaded), expected, "without {key}");
        }
    }

    #[test]
    fn a_field_of_the_wrong_type_takes_its_default_and_the_rest_are_kept() {
        for (key, wrong) in [
            ("soundVolume", json!("loud")),
            ("soundEnabled", json!(1)),
            ("activeIntegrations", json!("integration_n8n")),
            ("mainPill", json!(["agent_cursor"])),
            ("screen", Value::Null),
            ("shortcuts", json!("Ctrl+Alt+A")),
        ] {
            let loaded = parse(&custom_with(key, Some(wrong)))
                .unwrap_or_else(|| panic!("a file with a bad {key} was thrown away"));
            let mut expected = custom();
            expected[key] = defaults()[key].clone();
            assert_eq!(shown(&loaded), expected, "bad {key}");
        }
    }

    #[test]
    fn a_utf8_bom_is_not_corruption() {
        // What PowerShell 5's `Set-Content -Encoding utf8` and Notepad write.
        let (dir, file) = scratch("bom");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(CUSTOM.as_bytes());
        std::fs::write(&file, &bytes).unwrap();
        assert_eq!(shown(&load_from(&file)), custom());
        assert_eq!(names(&dir), ["settings.json"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unusable_file_is_the_defaults_and_is_set_aside_byte_for_byte() {
        for (name, bad) in [
            ("invalid", &b"{ \"soundVolume\": 0.5, \xff oops"[..]),
            ("not-an-object", &b"[1, 2, 3]"[..]),
        ] {
            let (dir, file) = scratch(name);
            std::fs::write(&file, bad).unwrap();
            assert_eq!(shown(&load_from(&file)), defaults());
            assert_eq!(kept(&dir, "corrupt"), [bad.to_vec()], "{name}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn a_second_unusable_file_never_replaces_the_copy_of_the_first() {
        // Both within the same second: the names must still differ.
        let (dir, file) = scratch("twice");
        let (first, second) = (&b"{ first"[..], &b"{ second"[..]);
        std::fs::write(&file, first).unwrap();
        load_from(&file);
        std::fs::write(&file, second).unwrap();
        load_from(&file);
        let mut copies = kept(&dir, "corrupt");
        copies.sort();
        assert_eq!(copies, [first.to_vec(), second.to_vec()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn something_that_is_not_a_readable_file_is_the_defaults_and_is_left_alone() {
        // A directory in the file's place: a read error that is not "not found".
        let (dir, file) = scratch("unreadable");
        std::fs::create_dir(&file).unwrap();
        assert_eq!(shown(&load_from(&file)), defaults());
        assert_eq!(names(&dir), ["settings.json"]);
        assert!(file.is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Opens `path` the way a process that shares nothing does: nobody else can
    /// read or write it until the handle is dropped.
    #[cfg(windows)]
    fn locked(path: &Path) -> std::fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new().read(true).share_mode(0).open(path).unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn a_file_that_could_not_be_read_is_set_aside_before_it_is_saved_over() {
        let (dir, file) = scratch("locked-then-free");
        std::fs::write(&file, CUSTOM).unwrap();
        let lock = locked(&file);
        let loaded = load_from(&file);
        assert_eq!(shown(&loaded), defaults());
        drop(lock);

        save_to(&file, &loaded).unwrap();
        assert_eq!(kept(&dir, "unread"), [CUSTOM.as_bytes().to_vec()]);
        assert_eq!(shown(&load_from(&file)), defaults());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_file_that_still_cannot_be_read_is_not_saved_over() {
        let (dir, file) = scratch("locked");
        std::fs::write(&file, CUSTOM).unwrap();
        let lock = locked(&file);
        let loaded = load_from(&file);
        assert!(save_to(&file, &loaded).is_err());
        drop(lock);
        assert_eq!(std::fs::read(&file).unwrap(), CUSTOM.as_bytes());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_could_not_be_read_is_never_saved_over_unseen() {
        // The same as the two locked-file tests above, on every platform: a
        // directory in the file's place cannot be read either.
        let (dir, file) = scratch("unread");
        std::fs::create_dir(&file).unwrap();
        let loaded = load_from(&file);

        // Still unreadable: the save is refused and nothing is touched.
        assert!(save_to(&file, &loaded).is_err());
        assert!(file.is_dir());
        assert_eq!(names(&dir), ["settings.json"]);

        // Readable by now: kept aside first, and only then saved over.
        std::fs::remove_dir(&file).unwrap();
        std::fs::write(&file, CUSTOM).unwrap();
        save_to(&file, &loaded).unwrap();
        assert_eq!(kept(&dir, "unread"), [CUSTOM.as_bytes().to_vec()]);
        assert_eq!(shown(&load_from(&file)), defaults());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Copies set aside ──────────────────────────────────────────────────────

    #[test]
    fn an_unusable_file_is_set_aside_once_however_often_it_is_loaded() {
        let (dir, file) = scratch("once");
        std::fs::write(&file, b"{ broken").unwrap();
        for _ in 0..3 {
            load_from(&file);
        }
        assert_eq!(kept(&dir, "corrupt"), [b"{ broken".to_vec()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copies_made_within_the_same_second_are_numbered_and_none_is_written_over() {
        let (dir, file) = scratch("numbered");
        let stamp = "20260102-030405";
        let contents = [&b"one"[..], &b"two"[..], &b"three"[..]];
        let copies: Vec<PathBuf> = contents
            .iter()
            .map(|bytes| set_aside_as(&file, "corrupt", stamp, bytes, write_whole).unwrap())
            .collect();

        let named = |suffix: &str| dir.join(format!("settings.json.corrupt-{stamp}{suffix}"));
        assert_eq!(copies, [named(""), named("-1"), named("-2")]);
        for (copy, bytes) in copies.iter().zip(contents) {
            assert_eq!(std::fs::read(copy).unwrap(), bytes);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_copy_that_could_not_be_written_whole_is_removed() {
        fn half_then_fail(file: &mut std::fs::File, bytes: &[u8]) -> std::io::Result<()> {
            use std::io::Write;
            file.write_all(&bytes[..bytes.len() / 2])?;
            Err(std::io::Error::other("the disk is full"))
        }
        let (dir, file) = scratch("half");
        let copy = set_aside_as(&file, "corrupt", "20260102-030405", b"all of it", half_then_fail);
        assert!(copy.is_err());
        assert!(names(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_field_name_cannot_start_a_line_of_its_own_in_the_log() {
        let forged = "volume\r\n2026-01-02 03:04:05 hook id=1 answered allow";
        let line = unusable_field(forged);
        assert!(!line.contains(['\r', '\n']));
        assert!(line.contains(r"volume\r\n2026-01-02"));
    }

    // ── Writing ───────────────────────────────────────────────────────────────

    /// A dotfiles setup keeps the real file elsewhere and links to it.
    #[cfg(unix)]
    #[test]
    fn a_save_through_a_symlink_keeps_the_link_and_writes_its_target() {
        let (dir, file) = scratch("symlink");
        let real = dir.join("dotfiles-settings.json");
        std::fs::write(&real, CUSTOM).unwrap();
        std::os::unix::fs::symlink(&real, &file).unwrap();

        save_to(&file, &Settings::default()).unwrap();
        assert!(std::fs::symlink_metadata(&file).unwrap().file_type().is_symlink());
        assert_eq!(shown(&parse(&std::fs::read(&real).unwrap()).unwrap()), defaults());
        assert_eq!(names(&dir), ["dotfiles-settings.json", "settings.json"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_field_survives_a_save_and_a_load() {
        let (dir, file) = scratch("round-trip");
        let settings: Settings = serde_json::from_str(CUSTOM).unwrap();
        save_to(&file, &settings).unwrap();
        assert_eq!(shown(&load_from(&file)), custom());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_file_holds_the_camel_case_keys_the_front_end_uses() {
        let (dir, file) = scratch("keys");
        save_to(&file, &Settings::default()).unwrap();
        let written: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        let keys: Vec<&str> = written.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "soundEnabled",
                "soundVolume",
                "autoCloseInterval",
                "openOnHover",
                "absenceInterval",
                "activeIntegrations",
                "mainPill",
                "screen",
                "autostart",
                "hooksInstalled",
                "model",
                "showPlanInNotch",
                "planRelayInstalled",
                "showCodexPlanInNotch",
                "chatProvider",
                "chatModels",
                "ollamaUrl",
                "lmstudioUrl",
                "customUrl",
                "hermesUrl",
                "shortcuts",
                "mochiOutfit",
                "pillColors",
                "language",
                "desktopMochi",
                "phoneLink",
                "phoneChat",
                "phoneChatModels",
                "phoneDetails",
                "phoneAnswers",
                "phoneDiffs",
                "phoneUsage",
                "phoneServices",
                "phoneRelay",
                "phoneRelayUrl",
                "agentEnabled",
                "agentFolder",
                "agentLevel",
                "chatHands",
                "phoneAgent",
                "phonePush",
                "phonePushServer",
                "phoneRemote",
                "homeEnabled",
                "homeAskAll",
                "homeMcp",
                "homeAssistantUrl",
                "homeTvs",
                "homeDevices",
                "homeRoutines",
                "readAloud",
                "hermesManaged",
                "hermesBrain",
                "assistantName",
                "assistantPersona",
                "assistantPersonaText",
                "hermesSkills",
                "hermesSkillsInstalled",
                "hermesComputerUse",
                "hermesBrowserOff",
                "hermesSpeed",
                "phoneAssistant",
                "phoneHome",
                "zheusEnabled",
                "zheusArrival",
                "zheusGoals",
                "islandLook",
                "powersEnabled",
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_save_replaces_the_file_that_was_there() {
        let (dir, file) = scratch("replace");
        std::fs::write(&file, CUSTOM).unwrap();
        save_to(&file, &Settings::default()).unwrap();
        assert_eq!(shown(&load_from(&file)), defaults());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_save_leaves_nothing_but_settings_json_behind() {
        let (dir, file) = scratch("tidy");
        save_to(&file, &Settings::default()).unwrap();
        save_to(&file, &Settings::default()).unwrap();
        assert_eq!(names(&dir), ["settings.json"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_temporary_file_that_cannot_be_written_leaves_the_original_alone() {
        // The new contents go to a file beside the target first. With a
        // directory squatting on that name, the write cannot even start.
        let (dir, file) = scratch("blocked");
        std::fs::write(&file, CUSTOM).unwrap();
        let temp = dir.join(format!("settings.json.coucou-{}", std::process::id()));
        std::fs::create_dir(&temp).unwrap();

        assert!(save_to(&file, &Settings::default()).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), CUSTOM.as_bytes());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
