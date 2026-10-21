// Zheus Foresight: before a mission runs, its plan — what each device does and when, worked out from the World State
// and the user's preferences — with what would clash (the vacuum needs longer than the way home, its battery is low,
// someone is home) and the ways around it. The user sees it, and nothing runs until they say yes or approve it on the
// island. Pure: tested below; mod.rs shows it and runs it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::world::{clock, World};
use crate::i18n::{t, tf};
use crate::smart_home::model::{self, Action, Capability, Device, Kind};

/// Minutes the vacuum keeps between its end and the user's arrival.
pub const MARGIN: u32 = 3;
/// The longest way home Zheus plans for, in minutes.
pub const MAX_ETA: u32 = 240;
pub const MAX_ROOMS: usize = 12;

/// A room the vacuum cleans (a Home Assistant area, by name) and the minutes it takes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoomTime {
    pub room: String,
    pub minutes: u32,
}

/// How Zheus prepares the user's arrival (Settings → Zheus One).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ArrivalPrefs {
    /// Minutes the way home takes when the user does not say.
    pub eta: u32,
    /// °C the air conditioning is set to for the arrival; 0 leaves it alone.
    pub comfort: u8,
    /// Minutes before the arrival the air conditioning starts: not earlier, it would cool an empty house.
    pub ac_lead: u32,
    /// The vacuum cleans before the arrival.
    pub vacuum: bool,
    /// The rooms the vacuum cleans, in the order they matter, with the minutes each takes. Empty: the whole house.
    pub rooms: Vec<RoomTime>,
    /// Minutes the whole house takes.
    pub whole: u32,
    /// Below this battery (percent) the vacuum stays on its dock.
    pub min_battery: u8,
    /// The lights turned on for an arrival after dark (device ids); empty: the living room's.
    pub lights: Vec<String>,
    /// From and until which minute of the day it is dark.
    pub dark_from: u32,
    pub dark_until: u32,
    /// The TV turned on at the arrival ("" none), and the app opened on it ("" none).
    pub tv: String,
    pub tv_app: String,
    /// The assistant prepares how the day went, told at the arrival.
    pub summary: bool,
}

impl Default for ArrivalPrefs {
    fn default() -> Self {
        ArrivalPrefs {
            eta: 30,
            comfort: 23,
            ac_lead: 15,
            vacuum: true,
            rooms: Vec::new(),
            whole: 40,
            min_battery: 30,
            lights: Vec::new(),
            dark_from: 18 * 60,
            dark_until: 6 * 60,
            tv: String::new(),
            tv_app: String::new(),
            summary: true,
        }
    }
}

impl ArrivalPrefs {
    /// As Settings sent them, within bounds.
    pub fn clean(mut self) -> ArrivalPrefs {
        self.eta = self.eta.clamp(1, MAX_ETA);
        if self.comfort != 0 {
            self.comfort = self.comfort.clamp(16, 30);
        }
        self.ac_lead = self.ac_lead.min(120);
        self.whole = self.whole.clamp(5, MAX_ETA);
        self.min_battery = self.min_battery.min(100);
        self.dark_from = self.dark_from.min(1439);
        self.dark_until = self.dark_until.min(1439);
        self.rooms = self
            .rooms
            .into_iter()
            .map(|r| RoomTime { room: r.room.trim().chars().take(60).collect(), minutes: r.minutes.clamp(1, MAX_ETA) })
            .filter(|r| !r.room.is_empty())
            .take(MAX_ROOMS)
            .collect();
        self.lights = self.lights.into_iter().map(|l| l.trim().chars().take(200).collect::<String>()).filter(|l| !l.is_empty()).take(20).collect();
        self.tv = self.tv.trim().chars().take(200).collect();
        self.tv_app = self.tv_app.trim().chars().take(100).collect();
        self
    }

    fn dark_at(&self, minute: u32) -> bool {
        let m = minute % 1440;
        if self.dark_from > self.dark_until {
            m >= self.dark_from || m < self.dark_until
        } else {
            m >= self.dark_from && m < self.dark_until
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mission {
    /// "I'm on my way home": the house prepared, the day told.
    Arrival,
    /// "Show our situation": the house, projects and decisions on the TV.
    Situation,
}

/// What a step does: an action on a device, or Zheus' own part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Do {
    Device,
    /// The assistant prepares how the day went.
    Summary,
    /// Zheus welcomes the user, with what was done and the day.
    Welcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// Minutes after the plan starts.
    pub at: u32,
    pub kind: Do,
    /// The device's id ("" for Zheus' own steps).
    pub device: String,
    pub device_name: String,
    /// One of model::ACTION_WORDS, and its value.
    pub action: String,
    pub value: String,
    /// In the user's words: "Ar do Quarto · Set to 23 °C".
    pub label: String,
    pub why: String,
    /// The device asks before every action: this step waits for its own click.
    pub asks: bool,
}

impl Step {
    /// The device action, for a device step.
    pub fn action(&self) -> Option<Action> {
        if self.kind != Do::Device {
            return None;
        }
        let value = (!self.value.is_empty()).then(|| Value::String(self.value.clone()));
        Action::parse(&self.action, value.as_ref()).ok()
    }
}

/// A way around a conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: String,
    pub mission: Mission,
    /// Minutes until the arrival, and the minute of the day it is expected.
    pub eta: u32,
    pub arrive_at: u32,
    /// The minute of the day the plan was made: the steps count from it.
    pub made_at: u32,
    pub steps: Vec<Step>,
    pub conflicts: Vec<String>,
    pub choices: Vec<Choice>,
    /// The choice the plan follows ("" when there is none to make).
    pub chosen: String,
    /// For the chat, the phone and the voice, in the user's words.
    pub narration: String,
    /// The same for the assistant, in English.
    pub for_model: String,
}

fn device_step(at: u32, d: &Device, action: Action, why: String) -> Step {
    let (word, value) = action.word();
    Step {
        at,
        kind: Do::Device,
        device: d.id.clone(),
        device_name: d.name.clone(),
        action: word.into(),
        value: value.unwrap_or_default(),
        label: format!("{} · {}", d.name, action.label()),
        why,
        asks: d.ask,
    }
}

fn own_step(at: u32, kind: Do, label: String) -> Step {
    Step { at, kind, device: String::new(), device_name: String::new(), action: String::new(), value: String::new(), label, why: String::new(), asks: false }
}

/// Words that name a living room or an entrance: where the lights go on for an arrival.
const LIVING: &[&str] = &["sala", "living", "salon", "lounge", "estar", "entrada", "hall", "entree"];

fn living(room: &str) -> bool {
    let r = model::normalize(room);
    LIVING.iter().any(|w| r.split(' ').any(|word| word == *w))
}

/// The rooms, in order, that fit in `window` minutes, and those left for later.
fn fit(rooms: &[RoomTime], window: u32) -> (Vec<&RoomTime>, Vec<&RoomTime>) {
    let (mut used, mut take, mut leave) = (0, Vec::new(), Vec::new());
    for r in rooms {
        if used + r.minutes <= window {
            used += r.minutes;
            take.push(r);
        } else {
            leave.push(r);
        }
    }
    (take, leave)
}

fn names(rooms: &[&RoomTime]) -> Vec<String> {
    rooms.iter().map(|r| r.room.clone()).collect()
}

fn minutes(rooms: &[&RoomTime]) -> u32 {
    rooms.iter().map(|r| r.minutes).sum()
}

/// The plan for the user's arrival in `eta` minutes. `choice`: the way around a conflict the user picked (None: the
/// first offered).
pub fn arrival(world: &World, prefs: &ArrivalPrefs, eta: u32, choice: Option<&str>, id: String) -> Plan {
    let eta = eta.clamp(1, MAX_ETA);
    let now = world.now;
    let arrive_at = (now + eta) % 1440;
    let usable = |d: &&Device| d.exposed && d.state.online;
    let mut steps = Vec::new();
    let mut conflicts = Vec::new();
    let mut choices = Vec::new();

    // ── Climate: on with the comfort temperature, `ac_lead` minutes before the arrival, not earlier ──
    if prefs.comfort > 0 {
        let start = eta.saturating_sub(prefs.ac_lead);
        let why = if start > 0 {
            tf("starts {minutes} min before you arrive, not earlier", &[("minutes", &prefs.ac_lead.to_string())])
        } else {
            t("starts now, so the room is ready when you arrive")
        };
        for d in world.devices.iter().filter(usable).filter(|d| d.kind == Kind::Climate && d.can(Capability::Temperature)) {
            if d.state.on != Some(true) && d.can(Capability::TurnOn) {
                steps.push(device_step(start, d, Action::TurnOn, why.clone()));
            }
            steps.push(device_step(start, d, Action::SetTemperature(prefs.comfort), why.clone()));
        }
    }

    // ── The vacuum: done before the arrival, or a way around it ──
    let vacuum = world.devices.iter().filter(usable).find(|d| d.kind == Kind::Vacuum && (d.can(Capability::Clean) || d.can(Capability::CleanArea)));
    let mut chosen = String::new();
    if let (true, Some(v)) = (prefs.vacuum, vacuum) {
        let status = v.state.status.as_deref().unwrap_or("");
        if world.occupied == Some(true) {
            conflicts.push(tf("Someone is home: {device} stays on its dock.", &[("device", &v.name)]));
        } else if status == "error" {
            conflicts.push(tf("{device} needs help: it is not started.", &[("device", &v.name)]));
        } else if v.state.battery.is_some_and(|b| b < prefs.min_battery) && status != "cleaning" {
            conflicts.push(tf(
                "{device}'s battery is at {percent}%: it stays on its dock to charge.",
                &[("device", &v.name), ("percent", &v.state.battery.unwrap_or(0).to_string())],
            ));
        } else if status == "cleaning" {
            if v.can(Capability::Dock) {
                steps.push(device_step(eta.saturating_sub(2), v, Action::Dock, t("it is already cleaning: it goes back to its dock just before you arrive")));
            }
        } else {
            let window = eta.saturating_sub(MARGIN);
            let by_rooms = v.can(Capability::CleanArea) && !prefs.rooms.is_empty();
            let all: Vec<&RoomTime> = prefs.rooms.iter().collect();
            let total = if by_rooms { minutes(&all) } else { prefs.whole };
            let everything = || if by_rooms { Action::CleanArea(names(&all)) } else { Action::Clean };
            let can_start = if by_rooms { true } else { v.can(Capability::Clean) };
            if !can_start {
                // Rooms only, and none set: nothing it can be told.
                conflicts.push(tf("Tell Zheus which rooms {device} cleans, in Settings → Zheus One.", &[("device", &v.name)]));
            } else if total <= window {
                steps.push(device_step(0, v, everything(), tf("about {minutes} min: done before you arrive", &[("minutes", &total.to_string())])));
            } else {
                conflicts.push(tf(
                    "You arrive in about {eta} min, but the cleaning takes {minutes}.",
                    &[("eta", &eta.to_string()), ("minutes", &total.to_string())],
                ));
                let (take, leave) = if by_rooms { fit(&prefs.rooms, window) } else { (Vec::new(), Vec::new()) };
                if !take.is_empty() {
                    choices.push(Choice {
                        id: "rooms".into(),
                        label: tf(
                            "Clean only {rooms} now (about {minutes} min), and {rest} another time",
                            &[("rooms", &names(&take).join(", ")), ("minutes", &minutes(&take).to_string()), ("rest", &names(&leave).join(", "))],
                        ),
                    });
                }
                if v.can(Capability::Dock) {
                    choices.push(Choice { id: "dock".into(), label: t("Start now, and send it back to its dock just before you arrive") });
                }
                choices.push(Choice { id: "skip".into(), label: t("No cleaning today") });
                chosen = choice.filter(|c| choices.iter().any(|x| x.id == *c)).unwrap_or(&choices[0].id).to_string();
                match chosen.as_str() {
                    "rooms" => steps.push(device_step(0, v, Action::CleanArea(names(&take)), tf("about {minutes} min: done before you arrive", &[("minutes", &minutes(&take).to_string())]))),
                    "dock" => {
                        steps.push(device_step(0, v, everything(), t("starts now")));
                        steps.push(device_step(eta.saturating_sub(2), v, Action::Dock, t("back on its dock before you arrive")));
                    }
                    _ => {}
                }
            }
        }
    }

    // ── Lights, for an arrival after dark ──
    if prefs.dark_at(arrive_at) {
        let lights: Vec<&Device> = if prefs.lights.is_empty() {
            world.devices.iter().filter(usable).filter(|d| d.kind == Kind::Light && living(&d.room)).collect()
        } else {
            world.devices.iter().filter(usable).filter(|d| d.kind == Kind::Light && prefs.lights.contains(&d.id)).collect()
        };
        for l in lights.into_iter().filter(|l| l.state.on != Some(true) && l.can(Capability::TurnOn)) {
            steps.push(device_step(eta, l, Action::TurnOn, t("it is dark when you arrive")));
        }
    }

    // ── The TV, if the user wants it on at the arrival ──
    if !prefs.tv.is_empty() {
        match world.devices.iter().filter(usable).find(|d| d.id == prefs.tv) {
            Some(tv) if tv.can(Capability::TurnOn) => {
                if tv.state.on != Some(true) {
                    steps.push(device_step(eta, tv, Action::TurnOn, t("on for your arrival")));
                }
                if !prefs.tv_app.is_empty() && tv.can(Capability::Apps) {
                    match model::resolve_app(tv, &prefs.tv_app) {
                        Ok(app) => steps.push(device_step(eta, tv, Action::OpenApp(app), t("on for your arrival"))),
                        Err(_) => conflicts.push(tf("{device} has no app called {app}.", &[("device", &tv.name), ("app", &prefs.tv_app)])),
                    }
                }
            }
            Some(tv) => conflicts.push(tf("{device} cannot be turned on from here.", &[("device", &tv.name)])),
            None => {
                let name = world.devices.iter().find(|d| d.id == prefs.tv).map_or_else(|| prefs.tv.clone(), |d| d.name.clone());
                conflicts.push(tf("{device} is not answering: it is left out.", &[("device", &name)]));
            }
        }
    }

    // ── What is not answering is left out, and said ──
    for d in world.devices.iter().filter(|d| d.exposed && !d.state.online && matches!(d.kind, Kind::Climate | Kind::Vacuum)) {
        conflicts.push(tf("{device} is not answering: it is left out.", &[("device", &d.name)]));
    }

    // ── Zheus' own part: the day's summary a few minutes before, the welcome at the arrival ──
    if prefs.summary {
        steps.push(own_step(eta.saturating_sub(5), Do::Summary, t("How your day went, ready for your arrival")));
    }
    steps.push(own_step(eta, Do::Welcome, t("Welcome home")));
    steps.sort_by_key(|s| s.at);

    let mut plan = Plan {
        id,
        mission: Mission::Arrival,
        eta,
        arrive_at,
        made_at: now,
        steps,
        conflicts,
        choices,
        chosen,
        narration: String::new(),
        for_model: String::new(),
    };
    plan.narration = narrate(&plan);
    plan.for_model = for_model(&plan);
    plan
}

/// When a step runs, as the user reads it: "now", or the clock.
pub fn when(plan: &Plan, step: &Step) -> String {
    if step.at == 0 {
        t("now")
    } else {
        clock(plan.made_at + step.at)
    }
}

fn narrate(plan: &Plan) -> String {
    let mut lines = vec![tf(
        "You arrive around {time}, in about {minutes} min. Here is the plan:",
        &[("time", &clock(plan.arrive_at)), ("minutes", &plan.eta.to_string())],
    )];
    for s in &plan.steps {
        lines.push(format!("• {} — {}", when(plan, s), s.label));
    }
    for c in &plan.conflicts {
        lines.push(format!("⚠ {c}"));
    }
    if plan.choices.len() > 1 {
        if let Some(c) = plan.choices.iter().find(|c| c.id == plan.chosen) {
            lines.push(tf("I chose: {choice}. You can pick another way on the island.", &[("choice", &c.label)]));
        }
    }
    lines.push(t("Shall I go ahead? Answer yes, or approve it on the island."));
    lines.join("\n")
}

fn for_model(plan: &Plan) -> String {
    let steps: Vec<String> = plan
        .steps
        .iter()
        .filter_map(|s| match s.kind {
            Do::Device => s.action().map(|a| format!("{} {}: {}", when_en(plan, s), s.device_name, a.describe())),
            Do::Summary => Some(format!("{} the assistant prepares how the user's day went", when_en(plan, s))),
            Do::Welcome => None,
        })
        .collect();
    let mut out = format!(
        "Zheus One prepared the user's arrival home (in about {} min, around {}) and is showing them the plan; nothing \
runs until they answer yes or approve it in Coucou. The plan: {}.",
        plan.eta,
        clock(plan.arrive_at),
        if steps.is_empty() { "nothing to do in the house".to_string() } else { steps.join("; ") }
    );
    if !plan.conflicts.is_empty() {
        out.push_str(&format!(" What clashes: {}", plan.conflicts.join(" ")));
    }
    out.push_str(" Do not act on these devices yourself for it: Coucou runs the plan once approved.");
    out
}

fn when_en(plan: &Plan, step: &Step) -> String {
    if step.at == 0 {
        "now".into()
    } else {
        format!("at {}", clock(plan.made_at + step.at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smart_home::home_assistant::Person;
    use crate::smart_home::model::tests::{light, tv};
    use crate::zheus_one::world::tests::{ac, vacuum};
    use crate::zheus_one::world::{build, Presence};

    fn prefs_with_rooms() -> ArrivalPrefs {
        ArrivalPrefs {
            rooms: vec![RoomTime { room: "Quarto".into(), minutes: 15 }, RoomTime { room: "Sala".into(), minutes: 20 }],
            ..ArrivalPrefs::default()
        }
    }

    fn world(devices: Vec<Device>, now: u32) -> World {
        build(now, Presence::Away, vec![Person { name: "Ana".into(), home: false }], devices, None)
    }

    fn labels(plan: &Plan) -> Vec<String> {
        plan.steps.iter().map(|s| format!("{}|{}", s.at, s.label)).collect()
    }

    #[test]
    fn the_vacuum_cleans_only_what_fits_before_the_arrival_as_in_the_example() {
        // "You arrive in about 25 minutes, but the vacuum needs 35: may I clean only the bedroom now?"
        let w = world(vec![vacuum("docked", 80), ac("Quarto", false, 275)], 18 * 60 + 10);
        let plan = arrival(&w, &prefs_with_rooms(), 25, None, "p1".into());
        assert_eq!(plan.arrive_at, 18 * 60 + 35);
        assert_eq!(plan.conflicts, vec!["You arrive in about 25 min, but the cleaning takes 35."]);
        assert_eq!(plan.choices.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["rooms", "dock", "skip"]);
        assert_eq!(plan.chosen, "rooms");
        assert_eq!(plan.choices[0].label, "Clean only Quarto now (about 15 min), and Sala another time");
        let l = labels(&plan);
        assert!(l.contains(&"0|Robô · Clean Quarto".to_string()), "{l:?}");
        // The air conditioning: 15 min before the arrival, not now.
        assert!(l.contains(&"10|Ar do Quarto · Turn on".to_string()), "{l:?}");
        assert!(l.contains(&"10|Ar do Quarto · Set to 23 °C".to_string()), "{l:?}");
        assert!(l.contains(&"20|How your day went, ready for your arrival".to_string()), "{l:?}");
        assert_eq!(l.last().unwrap(), "25|Welcome home");
        assert!(plan.narration.starts_with("You arrive around 18:35, in about 25 min. Here is the plan:\n• now — Robô · Clean Quarto"), "{}", plan.narration);
        assert!(plan.narration.contains("• 18:20 — Ar do Quarto · Set to 23 °C"), "{}", plan.narration);
        assert!(plan.narration.ends_with("Shall I go ahead? Answer yes, or approve it on the island."));
        assert!(plan.for_model.contains("now Robô: clean Quarto; at 18:20 Ar do Quarto: turn on"), "{}", plan.for_model);
        assert!(plan.for_model.contains("Do not act on these devices yourself"));

        // The user picks another way: the whole house now, back on the dock two minutes before.
        let dock = arrival(&w, &prefs_with_rooms(), 25, Some("dock"), "p1".into());
        let l = labels(&dock);
        assert!(l.contains(&"0|Robô · Clean Quarto, Sala".to_string()) && l.contains(&"23|Robô · Back to the dock".to_string()), "{l:?}");
        let skip = arrival(&w, &prefs_with_rooms(), 25, Some("skip"), "p1".into());
        assert!(!labels(&skip).iter().any(|s| s.contains("Robô")));
        let bogus = arrival(&w, &prefs_with_rooms(), 25, Some("explode"), "p1".into());
        assert_eq!(bogus.chosen, "rooms", "an unknown choice is the first one");
    }

    #[test]
    fn enough_time_cleans_everything_and_the_whole_house_without_rooms() {
        let w = world(vec![vacuum("docked", 80)], 17 * 60);
        let plan = arrival(&w, &prefs_with_rooms(), 50, None, "p".into());
        assert!(plan.conflicts.is_empty() && plan.choices.is_empty() && plan.chosen.is_empty());
        assert!(labels(&plan).contains(&"0|Robô · Clean Quarto, Sala".to_string()));
        let whole = arrival(&w, &ArrivalPrefs::default(), 50, None, "p".into());
        assert!(labels(&whole).contains(&"0|Robô · Start cleaning".to_string()), "{:?}", labels(&whole));
        // Too short for the whole house, and no rooms: back on the dock, or not today.
        let short = arrival(&w, &ArrivalPrefs::default(), 20, None, "p".into());
        assert_eq!(short.choices.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["dock", "skip"]);
    }

    #[test]
    fn what_should_not_run_is_said_not_done() {
        let mut someone = world(vec![vacuum("docked", 80)], 17 * 60);
        someone.occupied = Some(true);
        let plan = arrival(&someone, &prefs_with_rooms(), 60, None, "p".into());
        assert_eq!(plan.conflicts, vec!["Someone is home: Robô stays on its dock."]);
        assert!(!labels(&plan).iter().any(|s| s.contains("Robô")));
        let low = arrival(&world(vec![vacuum("docked", 12)], 17 * 60), &prefs_with_rooms(), 60, None, "p".into());
        assert_eq!(low.conflicts, vec!["Robô's battery is at 12%: it stays on its dock to charge."]);
        let broken = arrival(&world(vec![vacuum("error", 90)], 17 * 60), &prefs_with_rooms(), 60, None, "p".into());
        assert_eq!(broken.conflicts, vec!["Robô needs help: it is not started."]);
        let busy = arrival(&world(vec![vacuum("cleaning", 50)], 17 * 60), &prefs_with_rooms(), 30, None, "p".into());
        assert!(labels(&busy).contains(&"28|Robô · Back to the dock".to_string()), "{:?}", labels(&busy));
        let mut off = vacuum("docked", 80);
        off.state.online = false;
        let gone = arrival(&world(vec![off], 17 * 60), &prefs_with_rooms(), 30, None, "p".into());
        assert_eq!(gone.conflicts, vec!["Robô is not answering: it is left out."]);
        let mut shy = ac("Quarto", false, 275);
        shy.exposed = false;
        let hidden = arrival(&world(vec![shy], 17 * 60), &ArrivalPrefs::default(), 30, None, "p".into());
        assert!(!labels(&hidden).iter().any(|s| s.contains("Ar do")), "a device not shared with Zheus is never in a plan");
    }

    #[test]
    fn lights_after_dark_and_the_tv_with_its_app() {
        let mut sala = light("ha:light.sala", "Luz da Sala", "Sala de estar");
        sala.state.on = Some(false);
        sala.state.online = true;
        let mut quarto = light("ha:light.quarto", "Luz do Quarto", "Quarto");
        quarto.state.online = true;
        let mut tv_sala = tv("philips:aa", "TV", "Sala");
        tv_sala.state.online = true;
        tv_sala.state.on = Some(false);
        let w = world(vec![sala.clone(), quarto.clone(), tv_sala], 18 * 60);
        let prefs = ArrivalPrefs { comfort: 0, vacuum: false, summary: false, tv: "philips:aa".into(), tv_app: "netflix".into(), ..ArrivalPrefs::default() };
        let plan = arrival(&w, &prefs, 30, None, "p".into());
        assert_eq!(labels(&plan), ["30|Luz da Sala · Turn on", "30|TV · Turn on", "30|TV · Open Netflix", "30|Welcome home"]);
        // In daylight, no lights; the user's own list of lights wins over the living room.
        let day = world(vec![sala.clone(), quarto.clone()], 9 * 60);
        assert!(!labels(&arrival(&day, &prefs, 30, None, "p".into())).iter().any(|s| s.contains("Luz")));
        let mine = ArrivalPrefs { lights: vec!["ha:light.quarto".into()], ..prefs.clone() };
        let plan = arrival(&world(vec![sala, quarto], 18 * 60), &mine, 30, None, "p".into());
        assert!(labels(&plan).contains(&"30|Luz do Quarto · Turn on".to_string()) && !labels(&plan).iter().any(|s| s.contains("Sala")));
        let missing = arrival(&world(vec![], 18 * 60), &prefs, 30, None, "p".into());
        assert_eq!(missing.conflicts, vec!["philips:aa is not answering: it is left out."]);
    }

    #[test]
    fn preferences_are_kept_within_bounds() {
        let p = ArrivalPrefs {
            eta: 0,
            comfort: 40,
            ac_lead: 999,
            whole: 1,
            rooms: vec![RoomTime { room: "  ".into(), minutes: 5 }, RoomTime { room: " Quarto ".into(), minutes: 0 }],
            lights: vec![" ".into(), "ha:light.a".into()],
            dark_from: 5000,
            ..ArrivalPrefs::default()
        }
        .clean();
        assert_eq!((p.eta, p.comfort, p.ac_lead, p.whole, p.dark_from), (1, 30, 120, 5, 1439));
        assert_eq!(p.rooms, vec![RoomTime { room: "Quarto".into(), minutes: 1 }]);
        assert_eq!(p.lights, vec!["ha:light.a"]);
        assert_eq!(ArrivalPrefs { comfort: 0, ..ArrivalPrefs::default() }.clean().comfort, 0, "0 leaves the air conditioning alone");
        let d = ArrivalPrefs::default();
        assert!(d.dark_at(19 * 60) && d.dark_at(2 * 60) && !d.dark_at(12 * 60));
        assert!(living("Sala de estar") && living("Living room") && !living("Quarto"));
    }
}
