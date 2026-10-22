// Mochi's home from the phone (docs/ANDROID_LINK.md, "Mochi's home"): the devices the user ticked for Mochi, the
// routines and the latest of the home's record, and actions on them through the very calls Mochi's hands make
// (smart_home::control and run_routine, recorded with the origin "phone"), so every rule of Settings → Mochi's home
// still holds there: only a device ticked for Mochi, only an action it reported it can do.
//
// What it enforces, whatever the phone sends:
//   * the connection asked for the capability while both switches were on (Mochi's home, and the phone allowed to use
//     it); both are read again for every action (`off`);
//   * every action carries the phone's own id and is answered once with a `homeResult`; one without a usable id is
//     ignored, and so is a type this computer does not know;
//   * an action is one of the known words with a value that fits, on a device the phone may use that reported it
//     (`invalid`, `unknown`);
//   * a device or a routine that asks first is acted on only with `confirmed: true`, which the phone sends after its
//     own Face ID: without it the answer is `confirm` and nothing reaches the device;
//   * one action (or routine) at a time on the computer (`busy`), at most 30 per 10 minutes per connection (`rate`);
//   * the `home` line stays under 60 KiB, and nothing in it says where a device is or lets anyone in: no TV's or Home
//     Assistant's address, no MAC address, no token (guard.rs).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use super::chat::BoxFuture;
use super::guard::{clean_line, request_id, Rate};
use super::hub::{Hub, Out};
use super::server::Features;
use crate::smart_home::config::TvConf;
use crate::smart_home::journal::Entry;
use crate::smart_home::model::{self, Action, Device, ACTION_WORDS};
use crate::smart_home::routines::{self, Routine, RoutineStep};
use crate::smart_home::Outcome;

pub const MAX_DEVICES: usize = 100;
/// Per device.
pub const MAX_APPS: usize = 40;
pub const MAX_CHANNELS: usize = 60;
pub const MAX_ROUTINES: usize = 50;
pub const MAX_LOG: usize = 30;
/// The whole `home` line stays under this.
pub const LINE_LIMIT: usize = 60 * 1024;
/// Actions and routines, per connection and per RATE_WINDOW_MS.
pub const MAX_ACTIONS: usize = 30;
/// `homeGet`, per connection and per RATE_WINDOW_MS: it has no id to be refused with, so more go unanswered.
pub const MAX_GETS: usize = 60;
pub const RATE_WINDOW_MS: u64 = 10 * 60 * 1000;
const MAX_DEVICE_ID: usize = 200;
const MAX_ROUTINE_ID: usize = 64;
const MAX_DETAIL: usize = 300;

/// The house, and the way to act on it. The app's implementation (mod.rs) is smart_home's; tests use a fake.
pub trait HomeBackend: Send + Sync {
    /// The house as it is (what was read in the last minute will do). Nothing is read while Mochi's home is off.
    fn picture(&self) -> BoxFuture<HomePicture>;
    /// One action, checked and recorded like Mochi's own, with the origin "phone".
    fn control(&self, device: String, action: Action) -> BoxFuture<Outcome>;
    fn run_routine(&self, routine: Routine) -> BoxFuture<Outcome>;
}

/// What the phone may be shown of the house.
#[derive(Debug, Clone, Default)]
pub struct HomePicture {
    /// Mochi's home is on (Settings → Mochi's home).
    pub enabled: bool,
    pub ask_all: bool,
    /// Every device; only the ones ticked for Mochi are shown or used.
    pub devices: Vec<Device>,
    pub routines: Vec<Routine>,
    /// The latest of the home's record, newest first.
    pub log: Vec<Entry>,
    /// What must never reach the phone wherever it turns up: the TVs' addresses, MAC addresses and renderers, Home
    /// Assistant's address.
    pub hidden: Vec<String>,
}

/// What must never reach the phone, wherever it turns up: Home Assistant's address and its host, and each TV's address,
/// MAC address, UPnP renderer (and its host) and certificate pin.
pub fn hidden_of(ha_url: &str, tvs: &[TvConf]) -> Vec<String> {
    let mut out = Vec::new();
    let mut add = |text: &str| {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if let Some(host) = reqwest::Url::parse(text).ok().and_then(|u| u.host_str().map(|h| h.trim_matches(['[', ']']).to_string())) {
            out.push(host);
        }
        out.push(text.trim_end_matches('/').to_string());
    };
    add(ha_url);
    for tv in tvs {
        for text in [&tv.host, &tv.mac, &tv.renderer, &tv.pin] {
            add(text);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// What a phone asks of the house, checked for its form.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    Control { device: String, action: Action, confirmed: bool },
    Routine { routine: String, confirmed: bool },
}

/// The job in `msg`: None for a type this computer does not know (ignored), Err for one whose fields are wrong.
pub fn parse(msg: &Value) -> Option<Result<Job, ()>> {
    // Only `true` confirms: the phone sends it after its Face ID.
    let confirmed = msg["confirmed"].as_bool() == Some(true);
    Some(match msg["type"].as_str()? {
        "homeControl" => (|| {
            let device = msg["device"].as_str().filter(|d| !d.is_empty() && d.chars().count() <= MAX_DEVICE_ID)?;
            let word = msg["action"].as_str().filter(|w| ACTION_WORDS.contains(w))?;
            let value = match msg.get("value") {
                None | Some(Value::Null) => None,
                Some(v @ (Value::Number(_) | Value::String(_))) => Some(v),
                Some(_) => return None,
            };
            let action = Action::parse(word, value).ok()?;
            Some(Job::Control { device: device.to_string(), action, confirmed })
        })()
        .ok_or(()),
        "homeRoutine" => msg["routine"]
            .as_str()
            .filter(|r| !r.is_empty() && r.chars().count() <= MAX_ROUTINE_ID)
            .map(|r| Job::Routine { routine: r.to_string(), confirmed })
            .ok_or(()),
        _ => return None,
    })
}

/// What to carry out once the job passed the checks.
#[derive(Debug, Clone, PartialEq)]
pub enum Go {
    Control(String, Action),
    Routine(Routine),
}

/// Checks `job` against the house as it is now (`on`: both switches, read now): what to carry out, or the refusal.
pub fn check(picture: &HomePicture, job: &Job, on: bool) -> Result<Go, &'static str> {
    if !on || !picture.enabled {
        return Err("off");
    }
    match job {
        Job::Control { device, action, confirmed } => {
            let d = picture.devices.iter().find(|d| d.exposed && d.id == *device).ok_or("unknown")?;
            model::check(d, action).map_err(|_| "invalid")?;
            if (d.ask || picture.ask_all) && !confirmed {
                return Err("confirm");
            }
            Ok(Go::Control(device.clone(), action.clone()))
        }
        Job::Routine { routine, confirmed } => {
            let r = picture.routines.iter().find(|r| r.id == *routine).ok_or("unknown")?;
            let shown: Vec<Device> = picture.devices.iter().filter(|d| d.exposed).cloned().collect();
            if (routines::asks(r, &shown) || picture.ask_all) && !confirmed {
                return Err("confirm");
            }
            Ok(Go::Routine(r.clone()))
        }
    }
}

/// The fixed message of each refusal (a failure carries the device's own answer instead).
pub fn reason(code: &str) -> &'static str {
    match code {
        "off" => "AXYON's home from the phone is turned off on the computer.",
        "invalid" => "The computer cannot take this action.",
        "unknown" => "This device or routine is not one the phone may use any more.",
        "confirm" => "This asks first: confirm it on the phone.",
        "busy" => "Another action is still under way on the computer. Try again in a moment.",
        "rate" => "Too many actions. Try again in a few minutes.",
        _ => "It did not go through on the computer.",
    }
}

fn refusal_line(id: &str, code: &str) -> Out {
    Out::Line(json!({ "type": "homeResult", "id": id, "ok": false, "reason": code, "message": reason(code) }).to_string().into())
}

/// The answer to an action carried out: what the device reports now, or why it did not go through, in the user's words.
fn outcome_line(id: &str, outcome: &Outcome, hidden: &[String]) -> Out {
    let detail = clean_line(&outcome.detail, MAX_DETAIL, hidden);
    let v = if outcome.ok {
        json!({ "type": "homeResult", "id": id, "ok": true, "detail": detail })
    } else {
        let message = if detail.is_empty() { reason("failed").to_string() } else { detail };
        json!({ "type": "homeResult", "id": id, "ok": false, "reason": "failed", "message": message })
    };
    Out::Line(v.to_string().into())
}

// ── The `home` line ───────────────────────────────────────────────────────────

/// One device or routine for the line: its fields, and its two lists, which may be cut to make the line fit.
struct Parts {
    base: Value,
    lists: [(&'static str, Vec<Value>); 2],
}

impl Parts {
    fn json(&self, keep: [usize; 2]) -> Value {
        let mut v = self.base.clone();
        for ((key, items), n) in self.lists.iter().zip(keep) {
            v[*key] = Value::Array(items.iter().take(n).cloned().collect());
        }
        v
    }
}

/// How much of the picture goes in.
struct Keep {
    log: usize,
    channels: usize,
    apps: usize,
    problems: usize,
    steps: usize,
    routines: usize,
    devices: usize,
}

impl Keep {
    /// Takes something out of a line `size` bytes long, in this order: the record (its oldest lines, as many as make up
    /// what is over), then the channels, then the apps; should that still not do, the routines' problems and steps,
    /// the routines, then the devices. False when nothing is left to take.
    fn less(&mut self, log: &[Value], size: usize) -> bool {
        let over = (size + 1).saturating_sub(LINE_LIMIT);
        if self.log > 0 {
            let mut freed = 0;
            while self.log > 0 && freed < over {
                self.log -= 1;
                freed += log[self.log].to_string().len() + 1;
            }
            return true;
        }
        for n in [&mut self.channels, &mut self.apps, &mut self.problems, &mut self.steps, &mut self.routines, &mut self.devices] {
            if *n > 0 {
                *n /= 2;
                return true;
            }
        }
        false
    }
}

/// The `home` line for this picture: the devices ticked for Mochi with what they reported, the routines with what
/// would stop them, the latest of the record, cut to the limits and under LINE_LIMIT. Every text the computer did not
/// write is cleaned of addresses, paths and tokens.
pub fn home_line(p: &HomePicture) -> String {
    let shown: Vec<Device> = p
        .devices
        .iter()
        .filter(|d| d.exposed && !d.id.is_empty() && d.id.chars().count() <= MAX_DEVICE_ID)
        .take(MAX_DEVICES)
        .cloned()
        .collect();
    let devices: Vec<Parts> = shown.iter().map(|d| device_parts(d, p)).collect();
    let listed: Vec<Parts> = p
        .routines
        .iter()
        .filter(|r| !r.id.is_empty() && r.id.chars().count() <= MAX_ROUTINE_ID)
        .take(MAX_ROUTINES)
        .map(|r| routine_parts(r, &shown, p))
        .collect();
    let log: Vec<Value> = p.log.iter().take(MAX_LOG).map(|e| entry_json(e, &p.hidden)).collect();
    let mut keep = Keep {
        log: log.len(),
        channels: MAX_CHANNELS,
        apps: MAX_APPS,
        problems: routines::MAX_STEPS,
        steps: routines::MAX_STEPS,
        routines: listed.len(),
        devices: devices.len(),
    };
    loop {
        let line = json!({
            "type": "home",
            "enabled": p.enabled,
            "askAll": p.ask_all,
            "devices": devices.iter().take(keep.devices).map(|d| d.json([keep.channels, keep.apps])).collect::<Vec<_>>(),
            "routines": listed.iter().take(keep.routines).map(|r| r.json([keep.problems, keep.steps])).collect::<Vec<_>>(),
            "log": &log[..keep.log],
        })
        .to_string();
        if line.len() < LINE_LIMIT || !keep.less(&log, line.len()) {
            return line;
        }
    }
}

fn device_parts(d: &Device, p: &HomePicture) -> Parts {
    let hidden = &p.hidden;
    let s = &d.state;
    let base = json!({
        "id": d.id,
        "name": clean_line(&d.name, 60, hidden),
        "room": clean_line(&d.room, 40, hidden),
        "kind": d.kind.as_str(),
        "source": clean_line(&d.source, 40, hidden),
        "caps": d.caps,
        "state": {
            "online": s.online,
            "on": s.on,
            "volume": s.volume,
            "muted": s.muted,
            "brightness": s.brightness,
            "app": s.app.as_deref().map(|a| clean_line(a, 60, hidden)),
            "channel": s.channel.as_deref().map(|c| clean_line(c, 60, hidden)),
        },
        // Already "ask before every action" and the plugs' and switches' own rule (smart_home::read).
        "ask": d.ask || p.ask_all,
    });
    let channels = d
        .channels
        .iter()
        .take(MAX_CHANNELS)
        .map(|c| json!({ "number": clean_line(&c.number, 16, hidden), "name": clean_line(&c.name, 60, hidden) }))
        .collect();
    let apps = d.apps.iter().take(MAX_APPS).map(|a| Value::String(clean_line(a, 60, hidden))).collect();
    Parts { base, lists: [("channels", channels), ("apps", apps)] }
}

/// A routine as Mochi's hands see it: with the devices the phone may use (`shown`), what asks first and what would
/// stop a step (smart_home/routines.rs).
fn routine_parts(r: &Routine, shown: &[Device], p: &HomePicture) -> Parts {
    let hidden = &p.hidden;
    let base = json!({ "id": r.id, "name": clean_line(&r.name, 60, hidden), "ask": routines::asks(r, shown) || p.ask_all });
    let problems = routines::problems(r, shown).iter().take(routines::MAX_STEPS).map(|x| Value::String(clean_line(x, 200, hidden))).collect();
    let steps = r.steps.iter().take(routines::MAX_STEPS).map(|st| Value::String(clean_line(&step_words(st, shown), 120, hidden))).collect();
    Parts { base, lists: [("problems", problems), ("steps", steps)] }
}

/// "Luz da sala · Turn off": the device by its name when the phone may use it (else its id), the action in the user's
/// words.
fn step_words(step: &RoutineStep, shown: &[Device]) -> String {
    let device = shown.iter().find(|d| d.id == step.device).map_or(step.device.as_str(), |d| d.name.as_str());
    let action = step.action().map(|a| a.label()).unwrap_or_else(|_| step.action.clone());
    format!("{device} · {action}")
}

fn entry_json(e: &Entry, hidden: &[String]) -> Value {
    json!({
        "at": clean_line(&e.at, 19, hidden),
        "origin": clean_line(&e.origin, 20, hidden),
        "device": clean_line(&e.device, 100, hidden),
        "action": clean_line(&e.action, 120, hidden),
        "ok": e.ok,
        "detail": clean_line(&e.detail, MAX_DETAIL, hidden),
    })
}

// ── The phones that negotiated the capability ─────────────────────────────────

pub struct HomeLink {
    backend: Arc<dyn HomeBackend>,
    /// The phones' action (or routine) under way: one at a time on the computer.
    running: Mutex<Option<Running>>,
    next: AtomicU64,
}

struct Running {
    token: u64,
    abort: Option<AbortHandle>,
}

/// Frees the "one at a time" place when the action ends, however it ends (done, failed, stopped). Made before the
/// task starts, so a task stopped before it ran frees it too.
struct Slot {
    link: Arc<HomeLink>,
    token: u64,
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut running = self.link.running.lock().unwrap();
        if running.as_ref().is_some_and(|r| r.token == self.token) {
            *running = None;
        }
    }
}

/// One connection's side: how many actions it sent lately (shared with the task that carries one out), how many
/// pictures it asked for, and whether one is on its way.
#[derive(Default)]
pub struct HomeConn {
    actions: Arc<Mutex<Rate>>,
    gets: Rate,
    showing: Arc<AtomicBool>,
}

/// Lowered when the picture for a phone is sent, however that ends.
struct Showing(Arc<AtomicBool>);

impl Drop for Showing {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl HomeLink {
    pub fn new(backend: Arc<dyn HomeBackend>) -> Arc<HomeLink> {
        Arc::new(HomeLink { backend, running: Mutex::new(None), next: AtomicU64::new(1) })
    }

    /// The phone was just offered the capability: the house, after its first `sessions`.
    pub fn greet(&self, conn: &HomeConn, sub: u64, hub: &Arc<Hub>) {
        self.show(conn, sub, hub);
    }

    /// The switch or the link went off: the phones' action under way stops where it is (what a device was already
    /// sent stays done).
    pub fn shutdown(&self) {
        let abort = self.running.lock().unwrap().as_ref().and_then(|r| r.abort.clone());
        if let Some(abort) = abort {
            abort.abort();
        }
    }

    /// One `home…` message of a phone that negotiated the capability (connection `sub`). Never blocks: reading the
    /// house and acting on it run on tasks of their own.
    #[allow(clippy::too_many_arguments)]
    pub fn handle(self: &Arc<Self>, conn: &mut HomeConn, sub: u64, tx: &mpsc::Sender<Out>, hub: &Arc<Hub>, features: &Arc<dyn Features>, msg: &Value, now: u64) {
        if msg["type"] == "homeGet" {
            if features.home() && conn.gets.allow(now, MAX_GETS, RATE_WINDOW_MS) {
                self.show(conn, sub, hub);
            }
            return;
        }
        let Some(parsed) = parse(msg) else { return };
        // Without a usable id there is nothing to answer to.
        let Some(id) = request_id(msg) else { return };
        let refuse = |code: &str| {
            let _ = tx.try_send(refusal_line(id, code));
        };
        if !features.home() {
            return refuse("off");
        }
        let Ok(job) = parsed else { return refuse("invalid") };
        if conn.actions.lock().unwrap().full(now, MAX_ACTIONS, RATE_WINDOW_MS) {
            return refuse("rate");
        }
        let token = self.next.fetch_add(1, Ordering::SeqCst);
        {
            let mut running = self.running.lock().unwrap();
            if running.is_some() {
                drop(running);
                return refuse("busy");
            }
            *running = Some(Running { token, abort: None });
        }
        let slot = Slot { link: self.clone(), token };
        let task = tokio::spawn(act(self.backend.clone(), slot, job, id.to_string(), now, conn.actions.clone(), sub, tx.clone(), hub.clone(), features.clone()));
        if let Some(r) = self.running.lock().unwrap().as_mut().filter(|r| r.token == token) {
            r.abort = Some(task.abort_handle());
        }
    }

    /// The house, to this phone only: its first picture, or the one it asked for. One at a time per phone.
    fn show(&self, conn: &HomeConn, sub: u64, hub: &Arc<Hub>) {
        if conn.showing.swap(true, Ordering::SeqCst) {
            return;
        }
        let (backend, hub, showing) = (self.backend.clone(), hub.clone(), Showing(conn.showing.clone()));
        tokio::spawn(async move {
            let _showing = showing;
            let picture = backend.picture().await;
            hub.send_home(sub, home_line(&picture).into());
        });
    }
}

/// One action from a phone, holding the computer's one place for it: checked against the house as it is now, carried
/// out, answered; then every phone that has `home` is sent the house as it is afterwards.
#[allow(clippy::too_many_arguments)]
async fn act(
    backend: Arc<dyn HomeBackend>,
    slot: Slot,
    job: Job,
    id: String,
    now: u64,
    actions: Arc<Mutex<Rate>>,
    sub: u64,
    tx: mpsc::Sender<Out>,
    hub: Arc<Hub>,
    features: Arc<dyn Features>,
) {
    let picture = backend.picture().await;
    let go = match check(&picture, &job, features.home()) {
        Ok(go) => go,
        Err(code) => {
            drop(slot);
            let _ = tx.send(refusal_line(&id, code)).await;
            if code == "unknown" {
                // What this phone was shown is out of date.
                hub.send_home(sub, home_line(&picture).into());
            }
            return;
        }
    };
    // Counted only now: a `confirm` or an `unknown` sent nothing to a device.
    actions.lock().unwrap().count(now);
    let outcome = match go {
        Go::Control(device, action) => backend.control(device, action).await,
        Go::Routine(routine) => backend.run_routine(routine).await,
    };
    drop(slot);
    let _ = tx.send(outcome_line(&id, &outcome, &picture.hidden)).await;
    let after = backend.picture().await;
    hub.publish_home(home_line(&after).into());
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::smart_home::model::{Capability, Channel, DeviceState, Kind};

    pub fn light(id: &str, name: &str) -> Device {
        Device {
            id: id.into(),
            name: name.into(),
            room: "Sala".into(),
            kind: Kind::Light,
            source: "Home Assistant".into(),
            model: None,
            caps: vec![Capability::TurnOn, Capability::TurnOff, Capability::Brightness],
            apps: vec![],
            channels: vec![],
            state: DeviceState { online: true, on: Some(true), brightness: Some(80), ..DeviceState::default() },
            exposed: true,
            ask: false,
        }
    }

    pub fn tv(id: &str, name: &str) -> Device {
        Device {
            kind: Kind::Tv,
            source: "Samsung TV".into(),
            caps: vec![Capability::TurnOn, Capability::TurnOff, Capability::VolumeSet, Capability::Apps, Capability::Channels],
            apps: vec!["YouTube".into(), "Netflix".into()],
            channels: vec![Channel { number: "4".into(), name: "Globo".into() }],
            state: DeviceState { online: true, on: Some(false), volume: Some(12), muted: Some(false), ..DeviceState::default() },
            ..light(id, name)
        }
    }

    pub fn routine(id: &str, steps: &[(&str, &str)]) -> Routine {
        Routine {
            id: id.into(),
            name: "Boa noite".into(),
            steps: steps.iter().map(|(device, action)| RoutineStep { device: device.to_string(), action: action.to_string(), value: String::new(), wait: 0 }).collect(),
        }
    }

    pub fn house() -> HomePicture {
        let mut plug = light("ha:switch.aquecedor", "Aquecedor");
        plug.kind = Kind::Plug;
        plug.caps = vec![Capability::TurnOn, Capability::TurnOff];
        plug.ask = true;
        let mut hidden_tv = tv("samsung:0badc0de", "TV do quarto");
        hidden_tv.exposed = false;
        HomePicture {
            enabled: true,
            ask_all: false,
            devices: vec![light("ha:light.sala", "Luz da sala"), tv("samsung:1a2b3c4d", "TV da sala"), plug, hidden_tv],
            routines: vec![routine("r-0a1b2c3d4e5f", &[("ha:light.sala", "turn_off")]), routine("r-aaaaaaaaaaaa", &[("ha:switch.aquecedor", "turn_off")])],
            log: vec![Entry { at: "2026-10-13 21:05:12".into(), origin: "phone".into(), device: "TV da sala".into(), action: "Turn on".into(), ok: true, detail: "On".into() }],
            hidden: Vec::new(),
        }
    }

    fn line(p: &HomePicture) -> Value {
        serde_json::from_str(&home_line(p)).unwrap()
    }

    #[test]
    fn the_line_has_the_ticked_devices_the_routines_and_the_record_in_the_contracts_shape() {
        let v = line(&house());
        assert_eq!((v["type"].as_str(), v["enabled"].as_bool(), v["askAll"].as_bool()), (Some("home"), Some(true), Some(false)));
        let ids: Vec<&str> = v["devices"].as_array().unwrap().iter().map(|d| d["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["ha:light.sala", "samsung:1a2b3c4d", "ha:switch.aquecedor"], "never a device the user did not tick for Mochi");
        let lamp = &v["devices"][0];
        assert_eq!(lamp["kind"], "light");
        assert_eq!(lamp["source"], "Home Assistant");
        assert_eq!(lamp["caps"], json!(["turnOn", "turnOff", "brightness"]));
        assert_eq!(lamp["state"], json!({ "online": true, "on": true, "volume": null, "muted": null, "brightness": 80, "app": null, "channel": null }));
        assert_eq!(lamp["ask"], false);
        let mut keys: Vec<&str> = lamp.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["apps", "ask", "caps", "channels", "id", "kind", "name", "room", "source", "state"], "no model, no address, nothing else");
        assert_eq!(v["devices"][1]["channels"], json!([{ "number": "4", "name": "Globo" }]));
        assert_eq!(v["devices"][1]["caps"], json!(["turnOn", "turnOff", "volumeSet", "apps", "channels"]));
        assert_eq!(v["devices"][2]["ask"], true, "a plug asks first");
        assert_eq!(v["routines"][0], json!({ "id": "r-0a1b2c3d4e5f", "name": "Boa noite", "steps": ["Luz da sala · Turn off"], "ask": false, "problems": [] }));
        assert_eq!(v["routines"][1]["ask"], true, "a step on a device that asks first");
        assert_eq!(v["log"][0], json!({ "at": "2026-10-13 21:05:12", "origin": "phone", "device": "TV da sala", "action": "Turn on", "ok": true, "detail": "On" }));
        // "Ask before every action" makes everything ask.
        let mut all = house();
        all.ask_all = true;
        let v = line(&all);
        assert!(v["devices"].as_array().unwrap().iter().all(|d| d["ask"] == true));
        assert!(v["routines"].as_array().unwrap().iter().all(|r| r["ask"] == true));
    }

    #[test]
    fn a_routine_on_a_device_the_phone_may_not_use_says_so_and_asks() {
        let mut p = house();
        p.routines = vec![routine("r-bbbbbbbbbbbb", &[("samsung:0badc0de", "turn_on")])];
        let v = line(&p);
        assert_eq!(v["routines"][0]["steps"], json!(["samsung:0badc0de · Turn on"]), "a device the phone does not have is named by its id");
        assert_eq!(v["routines"][0]["problems"].as_array().unwrap().len(), 1);
        assert_eq!(v["routines"][0]["ask"], true);
    }

    #[test]
    fn the_limits_hold_and_the_line_fits_by_dropping_the_record_then_the_channels_then_the_apps() {
        let many = |i: usize| Device {
            apps: (0..80).map(|n| format!("App {n} {}", "x".repeat(40))).collect(),
            channels: (0..90).map(|n| Channel { number: n.to_string(), name: format!("Canal {n} {}", "y".repeat(40)) }).collect(),
            ..tv(&format!("samsung:{i:08x}"), &format!("TV {i}"))
        };
        let entry = |n: usize| Entry { at: "2026-10-13 21:05:12".into(), origin: "phone".into(), device: format!("TV {n}"), action: "Turn on".into(), ok: true, detail: "z".repeat(250) };
        let mut p = house();
        p.devices = (0..130).map(many).collect();
        p.routines = (0..70).map(|i| Routine { id: format!("r-{i:012x}"), ..routine("x", &[("ha:light.sala", "turn_off")]) }).collect();
        p.log = (0..45).map(entry).collect();
        let text = home_line(&p);
        assert!(text.len() < LINE_LIMIT, "{} bytes", text.len());
        let v: Value = serde_json::from_str(&text).unwrap();
        assert!(v["devices"].as_array().unwrap().len() <= MAX_DEVICES);
        assert!(v["routines"].as_array().unwrap().len() <= MAX_ROUTINES);
        assert_eq!(v["log"].as_array().unwrap().len(), 0, "the record goes first");
        for d in v["devices"].as_array().unwrap() {
            assert!(d["apps"].as_array().unwrap().len() <= MAX_APPS && d["channels"].as_array().unwrap().len() <= MAX_CHANNELS);
        }

        // A few TVs with full lists fit as they are, limits applied: only the record is cut, from its oldest line.
        let mut p = house();
        p.devices = (0..3).map(many).collect();
        p.log = (0..MAX_LOG + 10).map(entry).collect();
        let v = line(&p);
        assert_eq!(v["devices"][0]["apps"].as_array().unwrap().len(), MAX_APPS);
        assert_eq!(v["devices"][0]["channels"].as_array().unwrap().len(), MAX_CHANNELS);
        assert_eq!(v["log"].as_array().unwrap().len(), MAX_LOG);

        // More TVs: the record is dropped, then the channels are cut before any app is.
        p.devices = (0..20).map(many).collect();
        let text = home_line(&p);
        assert!(text.len() < LINE_LIMIT);
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["log"].as_array().unwrap().len(), 0);
        assert!(v["devices"][0]["channels"].as_array().unwrap().len() < MAX_CHANNELS);
        assert_eq!(v["devices"][0]["apps"].as_array().unwrap().len(), MAX_APPS, "apps go only after the channels");
        assert_eq!(v["devices"].as_array().unwrap().len(), 20, "every device is still there");

        // When only the record is too long, a few of its oldest lines go and the rest stays.
        let mut p = house();
        p.devices = (0..9).map(many).collect();
        p.log = (0..MAX_LOG).map(entry).collect();
        let full = home_line(&HomePicture { log: Vec::new(), ..p.clone() }).len();
        if full + 8 * 1024 < LINE_LIMIT && full + MAX_LOG * 400 > LINE_LIMIT {
            let v = line(&p);
            let kept = v["log"].as_array().unwrap();
            assert!(!kept.is_empty() && kept.len() < MAX_LOG, "{}", kept.len());
            assert_eq!(kept[0]["device"], "TV 0", "the newest lines stay");
        }
    }

    #[test]
    fn nothing_that_says_where_a_device_is_or_lets_anyone_in_reaches_the_phone() {
        let mut p = house();
        let mut tv = tv("samsung:1a2b3c4d", "TV 192.168.1.20");
        tv.room = "Sala (tv.lan)".into();
        tv.apps = vec!["http://192.168.1.20:8000/stream".into(), "YouTube".into()];
        tv.channels = vec![Channel { number: "1".into(), name: "A4:B1:C2:D3:E4:F5".into() }];
        tv.state.app = Some("http://homeassistant.local:8123/api/camera_proxy/camera.porta?token=eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9abc123".into());
        tv.state.channel = Some("Tv.Lan".into());
        p.devices = vec![tv];
        p.routines = vec![Routine { name: "Boa noite (192.168.1.20)".into(), ..routine("r-0a1b2c3d4e5f", &[("samsung:1a2b3c4d", "turn_off")]) }];
        p.log = vec![Entry {
            at: "2026-10-13 21:05:12".into(),
            origin: "phone".into(),
            device: "TV".into(),
            action: "Turn on".into(),
            ok: false,
            detail: "Not a valid address: tv.lan, MAC a4-b1-c2-d3-e4-f5, token sk-test-0123456789abcdefghijkl".into(),
        }];
        p.hidden = vec!["tv.lan".into(), "A4:B1:C2:D3:E4:F5".into(), "http://homeassistant.local:8123".into(), "homeassistant.local".into()];
        let text = home_line(&p);
        for secret in ["http://", "https://", "192.168.", "tv.lan", "Tv.Lan", "A4:B1", "a4-b1", "homeassistant.local", "eyJ0", "sk-test", "8123"] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["devices"][0]["apps"][1], "YouTube", "the rest of the list stays");
        assert_eq!(v["devices"][0]["id"], "samsung:1a2b3c4d", "the id the phone sends back is kept exactly");
    }

    #[test]
    fn what_is_hidden_is_every_address_of_the_house_and_its_hosts() {
        let tv = TvConf {
            host: "192.168.1.20".into(),
            mac: "A4:B1:C2:D3:E4:F5".into(),
            renderer: "http://192.168.1.20:9197/upnp/control/RenderingControl1".into(),
            pin: "ab".repeat(32),
            ..TvConf::default()
        };
        let hidden = hidden_of("http://homeassistant.local:8123/", &[tv, TvConf { host: "[fe80::1]".into(), ..TvConf::default() }]);
        for wanted in ["homeassistant.local", "http://homeassistant.local:8123", "192.168.1.20", "A4:B1:C2:D3:E4:F5", "[fe80::1]"] {
            assert!(hidden.iter().any(|h| h == wanted), "{wanted} not in {hidden:?}");
        }
        assert!(hidden.iter().all(|h| !h.is_empty()));
        assert!(hidden_of("", &[]).is_empty());
    }

    #[test]
    fn actions_are_read_from_the_phones_words_and_checked_against_the_house() {
        let control = |extra: Value| {
            let mut msg = json!({ "type": "homeControl", "id": "h1", "device": "ha:light.sala", "action": "set_brightness", "value": 40 });
            for (k, v) in extra.as_object().unwrap() {
                msg[k] = v.clone();
            }
            parse(&msg)
        };
        assert_eq!(control(json!({})), Some(Ok(Job::Control { device: "ha:light.sala".into(), action: Action::SetBrightness(40), confirmed: false })));
        assert_eq!(control(json!({ "value": "40%", "confirmed": true })), Some(Ok(Job::Control { device: "ha:light.sala".into(), action: Action::SetBrightness(40), confirmed: true })));
        assert_eq!(control(json!({ "action": "turn_on", "value": null })), Some(Ok(Job::Control { device: "ha:light.sala".into(), action: Action::TurnOn, confirmed: false })));
        assert!(matches!(control(json!({ "confirmed": "yes" })), Some(Ok(Job::Control { confirmed: false, .. }))), "only true confirms");
        for bad in [
            json!({ "action": "self_destruct" }),
            json!({ "action": "Turn_On" }),
            json!({ "value": 140 }),
            json!({ "value": [40] }),
            json!({ "value": true }),
            json!({ "device": "" }),
            json!({ "device": 7 }),
            json!({ "device": "x".repeat(201) }),
            json!({ "action": "open_app", "value": "" }),
        ] {
            assert_eq!(control(bad.clone()), Some(Err(())), "{bad}");
        }
        assert_eq!(parse(&json!({ "type": "homeRoutine", "id": "h2", "routine": "r1", "confirmed": true })), Some(Ok(Job::Routine { routine: "r1".into(), confirmed: true })));
        assert_eq!(parse(&json!({ "type": "homeRoutine", "id": "h2", "routine": 1 })), Some(Err(())));
        assert_eq!(parse(&json!({ "type": "homeUnlockTheDoor", "id": "h3" })), None);

        let p = house();
        let job = |device: &str, action: Action, confirmed: bool| Job::Control { device: device.into(), action, confirmed };
        assert_eq!(check(&p, &job("ha:light.sala", Action::SetBrightness(40), false), true), Ok(Go::Control("ha:light.sala".into(), Action::SetBrightness(40))));
        assert_eq!(check(&p, &job("ha:light.sala", Action::TurnOn, false), false), Err("off"), "the switch, read now");
        assert_eq!(check(&HomePicture { enabled: false, ..p.clone() }, &job("ha:light.sala", Action::TurnOn, false), true), Err("off"));
        assert_eq!(check(&p, &job("ha:light.gone", Action::TurnOn, false), true), Err("unknown"));
        assert_eq!(check(&p, &job("samsung:0badc0de", Action::TurnOn, true), true), Err("unknown"), "not ticked for Mochi");
        assert_eq!(check(&p, &job("ha:light.sala", Action::OpenApp("YouTube".into()), true), true), Err("invalid"), "a light has no apps");
        assert_eq!(check(&p, &job("ha:switch.aquecedor", Action::TurnOn, false), true), Err("confirm"), "a plug asks first");
        assert!(check(&p, &job("ha:switch.aquecedor", Action::TurnOn, true), true).is_ok());
        let all = HomePicture { ask_all: true, ..p.clone() };
        assert_eq!(check(&all, &job("ha:light.sala", Action::TurnOn, false), true), Err("confirm"), "ask before every action");
        let routine = |id: &str, confirmed: bool| Job::Routine { routine: id.into(), confirmed };
        assert!(matches!(check(&p, &routine("r-0a1b2c3d4e5f", false), true), Ok(Go::Routine(r)) if r.id == "r-0a1b2c3d4e5f"));
        assert_eq!(check(&p, &routine("r-aaaaaaaaaaaa", false), true), Err("confirm"), "its step is on a plug");
        assert!(check(&p, &routine("r-aaaaaaaaaaaa", true), true).is_ok());
        assert_eq!(check(&p, &routine("r-missing", true), true), Err("unknown"));
    }
}
