// Zheus One: the assistant as one intelligence for the house and the day, on top of Mochi's home (smart_home/) and the
// assistant's brain (Hermes, or the chat's provider). See docs/ZHEUS_ONE.md.
//
//   * Missions: "I'm on my way home" (from the island, the phone or the voice) becomes one plan for the whole house and
//     the day — the air conditioning on time, the vacuum done before the arrival, the lights after dark, the day's
//     summary — instead of fifteen orders.
//   * The World State (world.rs): the devices, who is home, each room, and the facts that tie them together.
//   * Foresight (plan.rs): the plan is shown first, with what would clash and the ways around it; nothing runs until
//     the user says yes (in the chat, read by Coucou itself, intent.rs) or approves it on the island. Steps on a device
//     set to ask still wait for its own click, on the island or the phone.
//   * The Autopilot (autopilot.rs): standing goals that suggest, or act within the limits the user set; spending,
//     publishing, writing to people and the house's safety are never done alone.
//   * The TV mode: "show our situation" puts the house, the plan and the decisions waiting full screen, on the TV or
//     projector when one is plugged in, and the assistant's overview with them.
//
// Off by default (Settings → Zheus One). Only the devices the user shares with agents are ever used, through
// smart_home's own checks and record.

pub mod autopilot;
pub mod intent;
pub mod plan;
pub mod world;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tokio::sync::Notify;

use crate::agent::HomeResult;
use crate::i18n::{t, tf};
use crate::smart_home::model::Action;
use crate::{island, log, settings, Shared};
use autopilot::{Goals, Memory, Suggestion};
use intent::Intent;
use plan::{ArrivalPrefs, Do, Plan};
use world::{clock, Presence, World};

/// How often the Autopilot looks at the house.
const LOOK_EVERY: Duration = Duration::from_secs(5 * 60);
/// "On my way" counts this long past the expected arrival; "I'm home" this long at all.
const ARRIVING_GRACE: u32 = 30;
const HOME_FOR: u32 = 12 * 60;
/// Suggestions kept on the island at most.
const MAX_SUGGESTIONS: usize = 6;
/// The question for the arrival: read only.
const ARRIVAL_ASK: &str = "I am arriving home. In a few short lines, tell me how my day went and the important things \
waiting for me (calendar, tasks, messages, business), with the tools and connectors you have. Read only: do not send, \
accept or change anything. Answer in my language.";

/// What one step of a running plan gave.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepOutcome {
    pub ok: bool,
    pub detail: String,
}

/// The assistant's last overview, for the TV mode.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Briefing {
    pub at: String,
    pub text: String,
}

struct Running {
    plan: Plan,
    done: Vec<Option<StepOutcome>>,
    stop: Arc<Notify>,
    stopped: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct ZheusOne {
    /// What the user said about where they are, and the minute of the day they said it.
    said: Mutex<Option<(Presence, u32)>>,
    /// The plan shown, waiting for a yes.
    pending: Mutex<Option<Plan>>,
    running: Mutex<Option<Running>>,
    suggestions: Mutex<Vec<Suggestion>>,
    memory: Mutex<Memory>,
    briefing: Mutex<Option<Briefing>>,
    /// The rooms Zheus sent the vacuum to, while it cleans.
    cleaning: Mutex<Option<Vec<String>>>,
    /// The Autopilot's loop is running.
    looking: AtomicBool,
    tv: AtomicBool,
    /// The settings window was showing before the TV mode took it.
    settings_was_visible: AtomicBool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningView {
    pub plan: Plan,
    pub done: Vec<Option<StepOutcome>>,
}

/// Everything the island, Settings and the TV mode show.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    /// Mochi's home is on: without it there is no house to prepare.
    pub home_on: bool,
    pub arrival: ArrivalPrefs,
    pub goals: Goals,
    pub pending: Option<Plan>,
    pub running: Option<RunningView>,
    pub suggestions: Vec<Suggestion>,
    pub briefing: Option<Briefing>,
    pub tv: bool,
}

fn current(app: &AppHandle) -> settings::Settings {
    app.state::<Shared>().settings.lock().unwrap().clone()
}

fn now_minute() -> u32 {
    let t = crate::platform::local_time();
    t.hour * 60 + t.minute
}

fn today() -> String {
    let t = crate::platform::local_time();
    format!("{:04}-{:02}-{:02}", t.year, t.month, t.day)
}

fn minutes_since(since: u32, now: u32) -> u32 {
    (now + 1440 - since % 1440) % 1440
}

fn new_id() -> String {
    let mut bytes = [0u8; 6];
    let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes);
    format!("z-{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// Only Coucou's own windows: the island and Settings (which holds the TV mode).
fn ours(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == island::WINDOW_LABEL || window.label() == "settings" {
        Ok(())
    } else {
        Err("only the island and Settings may do this".into())
    }
}

pub fn status(app: &AppHandle) -> Status {
    let s = current(app);
    let z = app.state::<ZheusOne>();
    let running = z.running.lock().unwrap().as_ref().map(|r| RunningView { plan: r.plan.clone(), done: r.done.clone() });
    let pending = z.pending.lock().unwrap().clone();
    let suggestions = z.suggestions.lock().unwrap().clone();
    let briefing = z.briefing.lock().unwrap().clone();
    let tv = z.tv.load(Ordering::SeqCst);
    Status {
        enabled: s.zheus_enabled,
        home_on: s.home_enabled,
        arrival: s.zheus_arrival.clone(),
        goals: s.zheus_goals.clone(),
        pending,
        running,
        suggestions,
        briefing,
        tv,
    }
}

/// Every window redraws from the new status.
fn changed(app: &AppHandle) {
    let _ = app.emit("zheus-changed", status(app));
}

/// Zheus speaks on the island (and aloud, when the user asked for it): the welcome, a summary, what the Autopilot did.
pub fn say(app: &AppHandle, text: &str) {
    let _ = app.emit_to(island::WINDOW_LABEL, "zheus-say", text.to_string());
}

// ── The World State ───────────────────────────────────────────────────────────

/// Where the user is: what they said, while it holds; else Home Assistant when it follows one person only.
fn user_presence(app: &AppHandle, people: &[crate::smart_home::home_assistant::Person], now: u32) -> Presence {
    let said = *app.state::<ZheusOne>().said.lock().unwrap();
    match said {
        Some((Presence::Arriving { at }, when)) if minutes_since(when, now) <= minutes_since(when, at) + ARRIVING_GRACE => {
            return Presence::Arriving { at };
        }
        Some((Presence::Home, when)) if minutes_since(when, now) <= HOME_FOR => return Presence::Home,
        _ => {}
    }
    match people {
        [one] => {
            if one.home {
                Presence::Home
            } else {
                Presence::Away
            }
        }
        _ => Presence::Unknown,
    }
}

/// The World State now. `fresh`: ask the devices again, else what was read in the last minute will do.
pub async fn world_now(app: &AppHandle, fresh: bool) -> World {
    let devices = crate::smart_home::read(app, fresh).await;
    let people = crate::smart_home::people(app);
    let now = now_minute();
    let user = user_presence(app, &people, now);
    let cleaning = app.state::<ZheusOne>().cleaning.lock().unwrap().clone();
    world::build(now, user, people, devices, cleaning.as_deref())
}

// ── Missions ──────────────────────────────────────────────────────────────────

/// Makes the arrival's plan and shows it, waiting for a yes. `eta`: the minutes the user said, else their usual.
pub async fn propose(app: &AppHandle, eta: Option<u32>) -> Plan {
    let world = world_now(app, true).await;
    let prefs = current(app).zheus_arrival;
    let plan = plan::arrival(&world, &prefs, eta.unwrap_or(prefs.eta), None, new_id());
    *app.state::<ZheusOne>().pending.lock().unwrap() = Some(plan.clone());
    log::line(format!("zheus: arrival planned in {} min, {} steps", plan.eta, plan.steps.len()));
    changed(app);
    let _ = app.emit_to(island::WINDOW_LABEL, "zheus-plan", plan.clone());
    plan
}

/// The waiting plan, made again with another way around its conflict.
pub async fn choose(app: &AppHandle, choice: &str) -> Option<Plan> {
    let waiting = app.state::<ZheusOne>().pending.lock().unwrap().clone()?;
    let world = world_now(app, false).await;
    let left = minutes_since(world.now, waiting.arrive_at).max(1);
    let plan = plan::arrival(&world, &current(app).zheus_arrival, left, Some(choice), waiting.id);
    *app.state::<ZheusOne>().pending.lock().unwrap() = Some(plan.clone());
    changed(app);
    Some(plan)
}

/// Starts the waiting plan (`id`: that one only). The user said yes, or clicked it.
pub fn approve(app: &AppHandle, id: Option<&str>) -> Result<Plan, String> {
    let z = app.state::<ZheusOne>();
    let plan = {
        let mut pending = z.pending.lock().unwrap();
        match pending.as_ref() {
            Some(p) if id.is_none_or(|i| p.id == i) => pending.take().expect("checked"),
            _ => return Err(t("There is no plan waiting.")),
        }
    };
    *z.said.lock().unwrap() = Some((Presence::Arriving { at: plan.arrive_at }, now_minute()));
    stop_running(app);
    let stop = Arc::new(Notify::new());
    let stopped = Arc::new(AtomicBool::new(false));
    *z.running.lock().unwrap() = Some(Running { plan: plan.clone(), done: vec![None; plan.steps.len()], stop: stop.clone(), stopped: stopped.clone() });
    log::line(format!("zheus: plan {} approved", plan.id));
    changed(app);
    let (app2, run) = (app.clone(), plan.clone());
    tauri::async_runtime::spawn(async move { execute(app2, run, stop, stopped).await });
    Ok(plan)
}

fn stop_running(app: &AppHandle) {
    if let Some(r) = app.state::<ZheusOne>().running.lock().unwrap().take() {
        r.stopped.store(true, Ordering::SeqCst);
        // A permit even when the plan is not waiting right now: its next wait ends at once.
        r.stop.notify_one();
    }
}

/// No to the waiting plan, and stop the one running.
pub fn cancel(app: &AppHandle) {
    let z = app.state::<ZheusOne>();
    let had = z.pending.lock().unwrap().take().is_some() | z.running.lock().unwrap().is_some();
    stop_running(app);
    if had {
        log::line("zheus: plan cancelled");
    }
    changed(app);
}

fn record(app: &AppHandle, id: &str, index: usize, outcome: StepOutcome) {
    if let Some(r) = app.state::<ZheusOne>().running.lock().unwrap().as_mut().filter(|r| r.plan.id == id) {
        if let Some(slot) = r.done.get_mut(index) {
            *slot = Some(outcome);
        }
    }
    changed(app);
}

/// Runs the plan's steps at their minutes (counted from when it was made: the way home did not wait for the yes).
async fn execute(app: AppHandle, plan: Plan, stop: Arc<Notify>, stopped: Arc<AtomicBool>) {
    let late = minutes_since(plan.made_at, now_minute());
    let started = Instant::now();
    let mut summary = None;
    for (i, step) in plan.steps.iter().enumerate() {
        let due = Duration::from_secs(u64::from(step.at.saturating_sub(late)) * 60);
        let wait = due.saturating_sub(started.elapsed());
        if !wait.is_zero() && !stopped.load(Ordering::SeqCst) {
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = stop.notified() => {}
            }
        }
        if stopped.load(Ordering::SeqCst) {
            log::line(format!("zheus: plan {} stopped at step {}", plan.id, i + 1));
            return;
        }
        let outcome = match step.kind {
            Do::Device => device_step(&app, step).await,
            Do::Summary => match ask_brain(&app, ARRIVAL_ASK).await {
                Ok(text) => {
                    summary = Some(text);
                    StepOutcome { ok: true, detail: t("Ready") }
                }
                Err(e) => StepOutcome { ok: false, detail: e },
            },
            Do::Welcome => {
                welcome(&app, &plan, summary.take());
                StepOutcome { ok: true, detail: String::new() }
            }
        };
        record(&app, &plan.id, i, outcome);
    }
    log::line(format!("zheus: plan {} done", plan.id));
    let z = app.state::<ZheusOne>();
    let mut running = z.running.lock().unwrap();
    if running.as_ref().is_some_and(|r| r.plan.id == plan.id) {
        *running = None;
    }
    drop(running);
    changed(&app);
}

/// One device action of a plan or a suggestion, through smart_home's own checks and record. A device set to ask
/// waits for its click first.
async fn act(app: &AppHandle, device: &str, label: &str, action: &Action, asks: bool, origin: &str) -> StepOutcome {
    if asks && !crate::agent::mcp::ask_click(app, "home_control", label.to_string(), t("Zheus One")).await {
        return StepOutcome { ok: false, detail: t("Not allowed.") };
    }
    let out = crate::smart_home::control(app, device, action, origin, false).await;
    if out.ok {
        let z = app.state::<ZheusOne>();
        match action {
            Action::CleanArea(rooms) => {
                *z.cleaning.lock().unwrap() = Some(rooms.clone());
                z.memory.lock().unwrap().cleaned_today = true;
            }
            Action::Clean => {
                *z.cleaning.lock().unwrap() = Some(Vec::new());
                z.memory.lock().unwrap().cleaned_today = true;
            }
            Action::Dock => *z.cleaning.lock().unwrap() = None,
            _ => {}
        }
    }
    StepOutcome { ok: out.ok, detail: out.detail }
}

async fn device_step(app: &AppHandle, step: &plan::Step) -> StepOutcome {
    match step.action() {
        Some(action) => act(app, &step.device, &step.label, &action, step.asks, "zheus").await,
        None => StepOutcome { ok: false, detail: t("This step is not valid.") },
    }
}

fn welcome(app: &AppHandle, plan: &Plan, summary: Option<String>) {
    let done: Vec<String> = {
        let z = app.state::<ZheusOne>();
        let running = z.running.lock().unwrap();
        let outcomes = running.as_ref().filter(|r| r.plan.id == plan.id).map(|r| r.done.clone()).unwrap_or_default();
        plan.steps
            .iter()
            .zip(outcomes)
            .filter(|(s, o)| s.kind == Do::Device && o.as_ref().is_some_and(|o| o.ok))
            .map(|(s, _)| s.label.clone())
            .collect()
    };
    let mut text = t("Welcome home!");
    if !done.is_empty() {
        text.push(' ');
        text.push_str(&tf("Done: {list}.", &[("list", &done.join("; "))]));
    }
    if let Some(s) = summary.filter(|s| !s.trim().is_empty()) {
        text.push_str("\n\n");
        text.push_str(s.trim());
    }
    text.push_str("\n\n");
    text.push_str(&t("Shall we start with business, or would you rather relax first?"));
    *app.state::<ZheusOne>().said.lock().unwrap() = Some((Presence::Home, now_minute()));
    say(app, &text);
}

/// A question for the assistant's brain, outside the chat's turn: Hermes when it is the chat's brain, else the
/// chat's provider. Its answer comes back as text.
pub async fn ask_brain(app: &AppHandle, question: &str) -> Result<String, String> {
    let s = current(app);
    let chat = app.state::<crate::chat::Chat>();
    if s.chat_provider == crate::hermes::PROVIDER {
        crate::agent::hermes::run_background(app, &chat, &s, question.to_string()).await
    } else {
        crate::chat::send(app, &chat, &s, question.to_string(), None).await.map(|r| r.text)
    }
}

// ── What the user's message asks ──────────────────────────────────────────────

/// A message on its way to the assistant, once Zheus One has read it: the text to send, with what Zheus did added
/// so the assistant knows; the answer to give if the assistant fails (the plan, in the user's words); and whether
/// the answer is the situation's overview, shown on the TV too.
pub struct Routed {
    pub text: String,
    pub fallback: Option<String>,
    pub briefing: bool,
}

/// Reads the user's own message (the island's chat or the phone's) for Zheus One. Unchanged when Zheus One is off or
/// the message asks nothing of it.
pub async fn route(app: &AppHandle, message: &str) -> Routed {
    let unchanged = Routed { text: message.to_string(), fallback: None, briefing: false };
    let s = current(app);
    if !s.zheus_enabled {
        return unchanged;
    }
    let (waiting, running) = {
        let z = app.state::<ZheusOne>();
        let waiting = z.pending.lock().unwrap().is_some();
        let running = z.running.lock().unwrap().is_some();
        (waiting, running)
    };
    let Some(what) = intent::intent(message, waiting || running) else { return unchanged };
    let with = |note: String, fallback: Option<String>| Routed { text: format!("{message}\n\n(Coucou — Zheus One: {note})"), fallback, briefing: false };
    match what {
        Intent::Arrival { eta } if s.home_enabled => {
            let plan = propose(app, eta).await;
            with(
                format!(
                    "{} Tell the user this plan in their language, in a few short lines, and ask whether to go ahead (they answer yes here, or approve it on the island). In their words: {}",
                    plan.for_model, plan.narration
                ),
                Some(plan.narration),
            )
        }
        Intent::Approve { choice } if waiting => {
            if let Some(c) = choice {
                choose(app, c).await;
            }
            match approve(app, None) {
                Ok(plan) => {
                    let done = tf("Plan started: {steps} steps until {time}.", &[("steps", &plan.steps.len().to_string()), ("time", &clock(plan.arrive_at))]);
                    with(
                        "The user approved Zheus One's arrival plan, and Coucou is running it now. Do not act on these devices yourself; answer in one or two short sentences.".into(),
                        Some(done),
                    )
                }
                Err(_) => unchanged,
            }
        }
        Intent::Cancel => {
            cancel(app);
            with("The user cancelled Zheus One's plan: nothing more of it runs. Answer in one short sentence.".into(), Some(t("Plan cancelled.")))
        }
        Intent::Situation => {
            let world = world_now(app, false).await;
            open_tv(app);
            Routed {
                text: format!(
                    "{message}\n\n(Coucou — Zheus One: the user's situation is on their TV now. {} Give them a short spoken overview: the house first, then their projects, campaigns and the decisions waiting, with your tools if you have them. Read only.)",
                    world.for_model()
                ),
                fallback: Some(world.facts.join("\n")).filter(|f| !f.is_empty()),
                briefing: true,
            }
        }
        Intent::Arrived => {
            *app.state::<ZheusOne>().said.lock().unwrap() = Some((Presence::Home, now_minute()));
            changed(app);
            unchanged
        }
        _ => unchanged,
    }
}

/// The assistant's answer to a routed message: its fallback when it failed, and the TV's overview when it was one.
pub fn answered<E>(app: &AppHandle, routed: &Routed, result: Result<String, E>) -> Result<String, E> {
    let result = match (result, &routed.fallback) {
        (Err(_), Some(fallback)) => Ok(fallback.clone()),
        (other, _) => other,
    };
    if routed.briefing {
        if let Ok(text) = &result {
            *app.state::<ZheusOne>().briefing.lock().unwrap() = Some(Briefing { at: clock(now_minute()), text: text.clone() });
            changed(app);
        }
    }
    result
}

/// For the assistant's own tools (zheus_mission): an arrival's plan shown for the user's yes, or the situation on
/// the TV.
pub async fn mission_for_agent(app: &AppHandle, mission: &str, eta: Option<u32>) -> HomeResult {
    let s = current(app);
    if !s.zheus_enabled {
        return HomeResult { ok: false, text: "Zheus One is off: the user turns it on in Coucou, Settings → Zheus One.".into(), detail: String::new() };
    }
    match mission {
        "arrival" => {
            let plan = propose(app, eta).await;
            HomeResult {
                ok: true,
                text: format!("{} Tell the user the plan briefly; they approve it by answering yes, or on the island. In their words: {}", plan.for_model, plan.narration),
                detail: t("Plan shown for approval"),
            }
        }
        "situation" => {
            let world = world_now(app, false).await;
            open_tv(app);
            HomeResult { ok: true, text: format!("The situation is on the user's TV now. {}", world.for_model()), detail: t("On the TV") }
        }
        other => HomeResult { ok: false, text: format!("\"{other}\" is not a mission. Use arrival or situation."), detail: String::new() },
    }
}

// ── The Autopilot ─────────────────────────────────────────────────────────────

/// Starts the Autopilot's look every few minutes, while Zheus One and a goal are on. Nothing runs otherwise.
pub fn start_looking(app: &AppHandle) {
    let s = current(app);
    if !(s.zheus_enabled && s.home_enabled && s.zheus_goals.any_on()) {
        return;
    }
    if app.state::<ZheusOne>().looking.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        loop {
            let s = current(&app);
            if !(s.zheus_enabled && s.home_enabled && s.zheus_goals.any_on()) {
                app.state::<ZheusOne>().looking.store(false, Ordering::SeqCst);
                log::line("zheus: autopilot resting");
                return;
            }
            look(&app).await;
            tokio::time::sleep(LOOK_EVERY).await;
        }
    });
    log::line("zheus: autopilot looking");
}

async fn look(app: &AppHandle) {
    let s = current(app);
    let world = world_now(app, false).await;
    let found = {
        let z = app.state::<ZheusOne>();
        let mut mem = z.memory.lock().unwrap();
        mem.roll(&today());
        mem.track(world.occupied, world.now);
        let found = autopilot::evaluate(&world, &s.zheus_goals, &mem, &s.zheus_arrival);
        for f in &found {
            mem.shown.insert(f.key.clone());
        }
        found
    };
    for f in found {
        if f.auto {
            let lines = carry_out(app, &f, "autopilot").await;
            if !lines.is_empty() {
                say(app, &tf("Autopilot: {what}", &[("what", &lines.join("; "))]));
            }
        } else {
            let z = app.state::<ZheusOne>();
            let mut list = z.suggestions.lock().unwrap();
            list.retain(|x| x.key != f.key);
            list.push(f.clone());
            let extra = list.len().saturating_sub(MAX_SUGGESTIONS);
            list.drain(..extra);
            drop(list);
            let _ = app.emit_to(island::WINDOW_LABEL, "zheus-suggestion", f);
        }
    }
    changed(app);
}

/// Does what a suggestion says: its question to the assistant (the answer is said), its device actions. Lines of
/// what was done, for the island.
async fn carry_out(app: &AppHandle, s: &Suggestion, origin: &str) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(question) = &s.ask {
        match ask_brain(app, question).await {
            Ok(answer) => say(app, &answer),
            Err(e) => lines.push(e),
        }
    }
    let devices = crate::smart_home::read(app, false).await;
    for a in &s.actions {
        let value = (!a.value.is_empty()).then(|| serde_json::Value::String(a.value.clone()));
        let Ok(action) = Action::parse(&a.action, value.as_ref()) else { continue };
        let asks = devices.iter().find(|d| d.id == a.device).is_none_or(|d| d.ask);
        let out = act(app, &a.device, &a.label, &action, asks, origin).await;
        lines.push(if out.ok { a.label.clone() } else { format!("{} — {}", a.label, out.detail) });
    }
    lines
}

/// The user answered a suggestion on the island.
pub async fn answer_suggestion(app: &AppHandle, key: &str, accept: bool) -> Result<Vec<String>, String> {
    let found = {
        let z = app.state::<ZheusOne>();
        let mut list = z.suggestions.lock().unwrap();
        let i = list.iter().position(|s| s.key == key).ok_or_else(|| t("This suggestion is not there any more."))?;
        list.remove(i)
    };
    changed(app);
    if !accept {
        return Ok(Vec::new());
    }
    // A click on the island: the user's own yes for the actions listed (a device set to ask still asks).
    let lines = carry_out(app, &found, "zheus").await;
    changed(app);
    Ok(lines)
}

// ── The TV mode ───────────────────────────────────────────────────────────────

/// The situation full screen in the settings window: on the biggest screen that is not the main one (a TV or a
/// projector plugged in, or cast to), else on the main one.
pub fn open_tv(app: &AppHandle) {
    let Some(win) = app.get_webview_window("settings") else { return };
    let z = app.state::<ZheusOne>();
    if !z.tv.swap(true, Ordering::SeqCst) {
        z.settings_was_visible.store(win.is_visible().unwrap_or(false), Ordering::SeqCst);
    }
    if let (Ok(monitors), Ok(primary)) = (win.available_monitors(), win.primary_monitor()) {
        let other = monitors
            .iter()
            .filter(|m| primary.as_ref().is_none_or(|p| p.position() != m.position()))
            .max_by_key(|m| u64::from(m.size().width) * u64::from(m.size().height));
        if let Some(m) = other {
            let _ = win.set_fullscreen(false);
            let _ = win.set_position(*m.position());
        }
    }
    let _ = win.show();
    let _ = win.set_fullscreen(true);
    let _ = win.set_focus();
    let _ = app.emit_to("settings", "zheus-tv", true);
    log::line("zheus: TV mode on");
    changed(app);
}

pub fn close_tv(app: &AppHandle) {
    let z = app.state::<ZheusOne>();
    if !z.tv.swap(false, Ordering::SeqCst) {
        return;
    }
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.set_fullscreen(false);
        let _ = app.emit_to("settings", "zheus-tv", false);
        if !z.settings_was_visible.load(Ordering::SeqCst) {
            let _ = win.hide();
        } else {
            let _ = win.center();
        }
    }
    log::line("zheus: TV mode off");
    changed(app);
}

// ── Commands ──────────────────────────────────────────────────────────────────

fn save(app: &AppHandle, change: impl FnOnce(&mut settings::Settings)) {
    let shared = app.state::<Shared>();
    let saved = {
        let mut s = shared.settings.lock().unwrap();
        change(&mut s);
        s.clone()
    };
    if let Err(err) = settings::save(&saved) {
        log::line(format!("could not save settings: {err}"));
    }
    let _ = app.emit("settings-changed", saved);
}

#[tauri::command]
pub fn zheus_status(app: AppHandle) -> Status {
    status(&app)
}

/// Settings → Zheus One: on or off. Off stops the plan waiting or running, and the Autopilot.
#[tauri::command]
pub fn zheus_set(app: AppHandle, window: WebviewWindow, enabled: bool) -> Result<Status, String> {
    crate::phone_link::only_settings(&window)?;
    save(&app, |s| s.zheus_enabled = enabled);
    log::line(format!("zheus: {}", if enabled { "on" } else { "off" }));
    if enabled {
        start_looking(&app);
    } else {
        cancel(&app);
        app.state::<ZheusOne>().suggestions.lock().unwrap().clear();
    }
    changed(&app);
    Ok(status(&app))
}

/// Settings → Zheus One: the arrival's preferences and the Autopilot's goals.
#[tauri::command]
pub fn zheus_prefs_set(app: AppHandle, window: WebviewWindow, arrival: ArrivalPrefs, goals: Goals) -> Result<Status, String> {
    crate::phone_link::only_settings(&window)?;
    let (arrival, goals) = (arrival.clean(), goals.clean());
    save(&app, |s| {
        s.zheus_arrival = arrival;
        s.zheus_goals = goals;
    });
    start_looking(&app);
    changed(&app);
    Ok(status(&app))
}

#[tauri::command]
pub async fn zheus_world(app: AppHandle, window: WebviewWindow, fresh: Option<bool>) -> Result<World, String> {
    ours(&window)?;
    Ok(world_now(&app, fresh.unwrap_or(false)).await)
}

/// The island's "Prepare my arrival" (`eta`: the minutes picked).
#[tauri::command]
pub async fn zheus_plan(app: AppHandle, window: WebviewWindow, eta: Option<u32>) -> Result<Plan, String> {
    ours(&window)?;
    if !current(&app).zheus_enabled {
        return Err(t("Turn Zheus One on first, in Settings → Zheus One."));
    }
    Ok(propose(&app, eta).await)
}

#[tauri::command]
pub async fn zheus_choose(app: AppHandle, window: WebviewWindow, choice: String) -> Result<Plan, String> {
    ours(&window)?;
    choose(&app, &choice).await.ok_or_else(|| t("There is no plan waiting."))
}

/// The user's click on the plan's Approve.
#[tauri::command]
pub fn zheus_approve(app: AppHandle, window: WebviewWindow, id: String) -> Result<Plan, String> {
    ours(&window)?;
    approve(&app, Some(&id))
}

#[tauri::command]
pub fn zheus_cancel(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    ours(&window)?;
    cancel(&app);
    Ok(())
}

#[tauri::command]
pub async fn zheus_suggestion(app: AppHandle, window: WebviewWindow, key: String, accept: bool) -> Result<Vec<String>, String> {
    ours(&window)?;
    answer_suggestion(&app, &key, accept).await
}

/// What a click on a device's tile gave.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TileResult {
    pub ok: bool,
    pub detail: String,
}

/// A click on a device's tile in the island or the TV mode: the user's own action on a device they share.
#[tauri::command]
pub async fn zheus_device(app: AppHandle, window: WebviewWindow, id: String, action: String, value: Option<String>) -> Result<TileResult, String> {
    ours(&window)?;
    let value = value.filter(|v| !v.trim().is_empty()).map(serde_json::Value::String);
    let action = Action::parse(&action, value.as_ref())?;
    let origin = if window.label() == "settings" { "settings" } else { "island" };
    let out = crate::smart_home::control(&app, &id, &action, origin, false).await;
    changed(&app);
    Ok(TileResult { ok: out.ok, detail: out.detail })
}

#[tauri::command]
pub fn zheus_tv(app: AppHandle, window: WebviewWindow, open: bool) -> Result<(), String> {
    ours(&window)?;
    if open {
        open_tv(&app);
    } else {
        close_tv(&app);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minutes_wrap_around_midnight() {
        assert_eq!(minutes_since(23 * 60 + 50, 10), 20);
        assert_eq!(minutes_since(600, 600), 0);
        assert_eq!(minutes_since(600, 615), 15);
        assert!(new_id().starts_with("z-") && new_id().len() == 14);
    }

    #[test]
    fn every_command_is_registered() {
        let source = include_str!("mod.rs");
        let lib = include_str!("../lib.rs");
        for chunk in source.split("#[tauri::command]\n").skip(1) {
            let name = chunk.split("fn ").nth(1).and_then(|s| s.split('(').next()).unwrap();
            assert!(lib.contains(&format!("zheus_one::{name}")), "{name} is not in the invoke handler");
        }
    }
}
