// Zheus Autopilot: standing goals instead of repeated orders. Each goal is off, suggests, or acts within the limits the
// user set (Settings → Zheus One); Zheus looks at the World State every few minutes and offers what fits — or does it,
// when the goal may act. Some things are never done alone, whatever the goal says: spending, publishing, writing to
// people, anything about the house's safety, and an action on a device that asks before every action. Pure: tested
// below; mod.rs looks, shows and runs.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::plan::ArrivalPrefs;
use super::world::{comfort_device, Presence, World};
use crate::i18n::{t, tf};
use crate::smart_home::model::{Action, Capability, Device, Kind};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Off,
    Suggest,
    /// Acts within the limits; what may never be done alone is still only suggested.
    Act,
}

/// The goals and their limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Goals {
    /// Less energy wasted: what was left on while nobody is home is turned off.
    pub energy: Mode,
    /// A tidy house: the vacuum cleans while the house is empty, once a day, and goes back when someone comes home.
    pub tidy: Mode,
    /// The day prepared: the summary of the day in the morning.
    pub agenda: Mode,
    /// The business moving on: campaigns looked at and proposals prepared. Suggestions only, ever.
    pub business: Mode,
    /// The hours the vacuum may clean in (minutes of the day).
    pub tidy_from: u32,
    pub tidy_until: u32,
    /// When the morning summary is offered (minute of the day), and the business review an hour later.
    pub agenda_at: u32,
    /// Minutes the house must have been empty before anything is turned off.
    pub away_minutes: u32,
}

impl Default for Goals {
    fn default() -> Self {
        Goals {
            energy: Mode::Off,
            tidy: Mode::Off,
            agenda: Mode::Off,
            business: Mode::Off,
            tidy_from: 9 * 60,
            tidy_until: 17 * 60,
            agenda_at: 8 * 60,
            away_minutes: 20,
        }
    }
}

impl Goals {
    pub fn clean(mut self) -> Goals {
        self.tidy_from = self.tidy_from.min(1439);
        self.tidy_until = self.tidy_until.min(1439);
        self.agenda_at = self.agenda_at.min(1439);
        self.away_minutes = self.away_minutes.clamp(5, 240);
        // Business never acts alone: "act" is kept as "suggest".
        if self.business == Mode::Act {
            self.business = Mode::Suggest;
        }
        self
    }

    pub fn any_on(&self) -> bool {
        [self.energy, self.tidy, self.agenda, self.business].iter().any(|m| *m != Mode::Off)
    }
}

/// What a suggestion is about: what may be done alone, and what never is. No goal makes the last four yet; the rule
/// is here so that none can act alone the day one does.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    /// The house's comfort devices.
    Home,
    /// Reading and preparing (a summary): nothing leaves, nothing changes.
    Digital,
    /// Money: ads, purchases. Never alone.
    Spend,
    /// Posting, publishing. Never alone.
    Publish,
    /// Writing or calling people, customers included. Never alone.
    People,
    /// Locks, alarms, cameras. Never alone.
    Security,
}

impl Category {
    pub fn may_act_alone(self) -> bool {
        matches!(self, Category::Home | Category::Digital)
    }
}

/// One device action of a suggestion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Planned {
    pub device: String,
    pub device_name: String,
    pub action: String,
    pub value: String,
    pub label: String,
}

impl Planned {
    fn of(d: &Device, action: Action) -> Planned {
        let (word, value) = action.word();
        Planned { device: d.id.clone(), device_name: d.name.clone(), action: word.into(), value: value.unwrap_or_default(), label: format!("{} · {}", d.name, action.label()) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// Shown once a day per key.
    pub key: String,
    pub goal: String,
    pub text: String,
    pub actions: Vec<Planned>,
    /// A question for the assistant instead of device actions (the day's summary, the business review).
    pub ask: Option<String>,
    pub category: Category,
    /// Zheus does it without asking: the goal acts, the category may act alone, and no device asks first.
    pub auto: bool,
}

/// What the Autopilot remembers of today.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Memory {
    /// "2026-10-20": the day the rest is about.
    pub day: String,
    pub cleaned_today: bool,
    /// The minute of the day the house was found empty, while it still is.
    pub away_since: Option<u32>,
    /// The suggestions shown today.
    pub shown: BTreeSet<String>,
}

impl Memory {
    /// A new day forgets the last one.
    pub fn roll(&mut self, day: &str) {
        if self.day != day {
            *self = Memory { day: day.to_string(), away_since: self.away_since, ..Memory::default() };
        }
    }

    /// Keeps when the house was found empty: `occupied` as the World State says it.
    pub fn track(&mut self, occupied: Option<bool>, now: u32) {
        match occupied {
            Some(false) => {
                self.away_since.get_or_insert(now);
            }
            _ => self.away_since = None,
        }
    }
}

/// The question for the morning summary: read only.
pub const AGENDA_ASK: &str = "Prepare a short summary of my day with the tools and connectors you have: my calendar, \
my pending tasks and the messages that matter. Read only: do not send, accept, delete or change anything. Answer in my \
language, in a few short lines.";
/// The question for the business review: read only, never acting.
pub const BUSINESS_ASK: &str = "Look at how my business is doing with the tools and connectors you have (my Instagram \
and Meta Ads campaigns, my sales): which campaigns need attention, and three concrete proposals to improve them. Read \
only: never publish, spend, change a campaign or write to anyone. Answer in my language, briefly.";

fn minutes_since(since: u32, now: u32) -> u32 {
    (now + 1440 - since) % 1440
}

fn in_window(now: u32, from: u32, until: u32) -> bool {
    if from <= until {
        now >= from && now < until
    } else {
        now >= from || now < until
    }
}

/// What the Autopilot offers now, given the World State, the goals and what it remembers. Nothing shown today twice.
pub fn evaluate(world: &World, goals: &Goals, mem: &Memory, prefs: &ArrivalPrefs) -> Vec<Suggestion> {
    let mut out = Vec::new();
    let empty = world.occupied == Some(false) && !matches!(world.user, Presence::Arriving { .. } | Presence::Home);
    let away = mem.away_since.map(|s| minutes_since(s, world.now)).unwrap_or(0);

    // ── Energy: what is on in an empty house ──
    if goals.energy != Mode::Off && empty && away >= goals.away_minutes {
        let on: Vec<&Device> = world
            .devices
            .iter()
            .filter(|d| d.state.online && comfort_device(d) && d.state.on == Some(true) && d.can(Capability::TurnOff))
            .collect();
        let (free, asking): (Vec<&Device>, Vec<&Device>) = on.into_iter().partition(|d| !d.ask);
        let text = |list: &[&Device]| {
            tf(
                "Nobody has been home for {minutes} min, and these are on: {devices}. Turn them off?",
                &[("minutes", &away.to_string()), ("devices", &list.iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", "))],
            )
        };
        if !free.is_empty() {
            out.push(Suggestion {
                key: "energy".into(),
                goal: "energy".into(),
                text: text(&free),
                actions: free.iter().map(|d| Planned::of(d, Action::TurnOff)).collect(),
                ask: None,
                category: Category::Home,
                auto: goals.energy == Mode::Act,
            });
        }
        if !asking.is_empty() {
            out.push(Suggestion {
                key: "energy-ask".into(),
                goal: "energy".into(),
                text: text(&asking),
                actions: asking.iter().map(|d| Planned::of(d, Action::TurnOff)).collect(),
                ask: None,
                category: Category::Home,
                auto: false,
            });
        }
    }

    // ── A tidy house: the vacuum while it is empty; back on its dock when someone comes home ──
    let vacuum = world.devices.iter().find(|d| d.kind == Kind::Vacuum && d.state.online);
    if let (true, Some(v)) = (goals.tidy != Mode::Off, vacuum) {
        let status = v.state.status.as_deref().unwrap_or("");
        let charged = v.state.battery.is_none_or(|b| b >= prefs.min_battery);
        let start = if v.can(Capability::CleanArea) && !prefs.rooms.is_empty() {
            Some(Action::CleanArea(prefs.rooms.iter().map(|r| r.room.clone()).collect()))
        } else {
            v.can(Capability::Clean).then_some(Action::Clean)
        };
        if let Some(start) = start.filter(|_| {
            empty && !mem.cleaned_today && matches!(status, "docked" | "idle") && charged && in_window(world.now, goals.tidy_from, goals.tidy_until)
        }) {
            let minutes = if matches!(start, Action::CleanArea(_)) { prefs.rooms.iter().map(|r| r.minutes).sum() } else { prefs.whole };
            out.push(Suggestion {
                key: "tidy".into(),
                goal: "tidy".into(),
                text: tf("The house is empty: {device} can clean now (about {minutes} min).", &[("device", &v.name), ("minutes", &minutes.to_string())]),
                actions: vec![Planned::of(v, start)],
                ask: None,
                category: Category::Home,
                auto: goals.tidy == Mode::Act && !v.ask,
            });
        }
        let someone = world.occupied == Some(true) || world.user == Presence::Home;
        if status == "cleaning" && someone && v.can(Capability::Dock) {
            out.push(Suggestion {
                key: "dock".into(),
                goal: "tidy".into(),
                text: tf("Someone is home: send {device} back to its dock?", &[("device", &v.name)]),
                actions: vec![Planned::of(v, Action::Dock)],
                ask: None,
                category: Category::Home,
                auto: goals.tidy == Mode::Act && !v.ask,
            });
        }
    }

    // ── The day prepared: the morning summary ──
    if goals.agenda != Mode::Off && in_window(world.now, goals.agenda_at, (goals.agenda_at + 120) % 1440) {
        out.push(Suggestion {
            key: "agenda".into(),
            goal: "agenda".into(),
            text: t("Good morning! Shall I prepare the summary of your day?"),
            actions: Vec::new(),
            ask: Some(AGENDA_ASK.into()),
            category: Category::Digital,
            auto: goals.agenda == Mode::Act,
        });
    }

    // ── The business: suggestions only, ever ──
    let review_at = (goals.agenda_at + 60) % 1440;
    if goals.business != Mode::Off && in_window(world.now, review_at, (review_at + 180) % 1440) {
        out.push(Suggestion {
            key: "business".into(),
            goal: "business".into(),
            text: t("Shall I look at your campaigns and sales and prepare proposals? Nothing is published, spent or sent without you."),
            actions: Vec::new(),
            ask: Some(BUSINESS_ASK.into()),
            category: Category::Digital,
            auto: false,
        });
    }

    // What may never be done alone is only ever a suggestion, whatever the goal says.
    for s in &mut out {
        s.auto &= s.category.may_act_alone() && s.goal != "business";
    }
    out.retain(|s| !mem.shown.contains(&s.key));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smart_home::home_assistant::Person;
    use crate::smart_home::model::tests::{light, tv};
    use crate::smart_home::model::DeviceState;
    use crate::zheus_one::world::build;
    use crate::zheus_one::world::tests::{ac, vacuum};

    fn on(mut d: Device) -> Device {
        d.state = DeviceState { online: true, on: Some(true), ..d.state };
        d
    }

    fn away(devices: Vec<Device>, now: u32) -> World {
        build(now, Presence::Away, vec![Person { name: "Ana".into(), home: false }], devices, None)
    }

    fn mem(away_since: u32) -> Memory {
        Memory { day: "2026-10-20".into(), away_since: Some(away_since), ..Memory::default() }
    }

    #[test]
    fn energy_turns_off_what_is_left_on_but_never_a_plug_nor_a_device_that_asks() {
        let mut plug = on(light("ha:switch.geladeira", "Geladeira", "Cozinha"));
        plug.kind = Kind::Plug;
        let mut asking = on(tv("philips:aa", "TV", "Sala"));
        asking.ask = true;
        let w = away(vec![on(light("ha:light.sala", "Luz da Sala", "Sala")), on(ac("Quarto", true, 240)), plug, asking], 14 * 60);
        let goals = Goals { energy: Mode::Act, ..Goals::default() };
        // Not empty for long enough yet.
        assert!(evaluate(&w, &goals, &mem(14 * 60 - 5), &ArrivalPrefs::default()).is_empty());
        let s = evaluate(&w, &goals, &mem(13 * 60 + 30), &ArrivalPrefs::default());
        assert_eq!(s.len(), 2, "{s:?}");
        assert_eq!(s[0].text, "Nobody has been home for 30 min, and these are on: Luz da Sala, Ar do Quarto. Turn them off?");
        assert!(s[0].auto, "acting is allowed for the lights and the air conditioning");
        assert_eq!(s[0].actions.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(), ["Luz da Sala · Turn off", "Ar do Quarto · Turn off"]);
        assert_eq!((s[1].key.as_str(), s[1].auto), ("energy-ask", false), "the TV asks first: only a suggestion");
        assert!(!s.iter().any(|x| x.text.contains("Geladeira")), "a plug may power anything");
        // On the way home, the lights may be for the arrival.
        let arriving = build(14 * 60, Presence::Arriving { at: 14 * 60 + 20 }, vec![Person { name: "Ana".into(), home: false }], vec![on(light("ha:light.sala", "Luz da Sala", "Sala"))], None);
        assert!(evaluate(&arriving, &goals, &mem(13 * 60), &ArrivalPrefs::default()).is_empty());
        // Shown once a day.
        let mut seen = mem(13 * 60);
        seen.shown.insert("energy".into());
        seen.shown.insert("energy-ask".into());
        assert!(evaluate(&w, &goals, &seen, &ArrivalPrefs::default()).is_empty());
        // Suggest only.
        let s = evaluate(&w, &Goals { energy: Mode::Suggest, ..Goals::default() }, &mem(13 * 60), &ArrivalPrefs::default());
        assert!(s.iter().all(|x| !x.auto));
    }

    #[test]
    fn the_vacuum_cleans_an_empty_house_once_a_day_in_its_hours_and_goes_back_when_someone_comes() {
        let goals = Goals { tidy: Mode::Act, ..Goals::default() };
        let prefs = ArrivalPrefs::default();
        let w = away(vec![vacuum("docked", 90)], 10 * 60);
        let s = evaluate(&w, &goals, &mem(9 * 60), &prefs);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].text, "The house is empty: Robô can clean now (about 40 min).");
        assert_eq!(s[0].actions[0].action, "clean");
        assert!(s[0].auto);
        let rooms = ArrivalPrefs { rooms: vec![super::super::plan::RoomTime { room: "Quarto".into(), minutes: 15 }], ..prefs.clone() };
        assert_eq!(evaluate(&w, &goals, &mem(9 * 60), &rooms)[0].actions[0].label, "Robô · Clean Quarto");
        // Not at night, not twice a day, not with a low battery.
        assert!(evaluate(&away(vec![vacuum("docked", 90)], 20 * 60), &goals, &mem(19 * 60), &prefs).is_empty());
        let cleaned = Memory { cleaned_today: true, ..mem(9 * 60) };
        assert!(evaluate(&w, &goals, &cleaned, &prefs).is_empty());
        assert!(evaluate(&away(vec![vacuum("docked", 10)], 10 * 60), &goals, &mem(9 * 60), &prefs).is_empty());
        // Someone came home while it cleans.
        let home = build(11 * 60, Presence::Unknown, vec![Person { name: "Ana".into(), home: true }], vec![vacuum("cleaning", 60)], None);
        let s = evaluate(&home, &goals, &Memory::default(), &prefs);
        assert_eq!((s[0].key.as_str(), s[0].actions[0].action.as_str(), s[0].auto), ("dock", "return_to_base", true));
    }

    #[test]
    fn the_morning_summary_and_the_business_review_and_what_never_acts_alone() {
        let w = away(vec![], 8 * 60 + 10);
        let goals = Goals { agenda: Mode::Act, business: Mode::Act, ..Goals::default() }.clean();
        assert_eq!(goals.business, Mode::Suggest, "business never acts alone");
        let s = evaluate(&w, &goals, &Memory::default(), &ArrivalPrefs::default());
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].key.as_str(), s[0].auto, s[0].ask.as_deref()), ("agenda", true, Some(AGENDA_ASK)));
        let later = away(vec![], 9 * 60 + 30);
        let s = evaluate(&later, &Goals { business: Mode::Act, ..Goals::default() }, &Memory::default(), &ArrivalPrefs::default());
        assert_eq!((s[0].key.as_str(), s[0].auto), ("business", false), "even unclean goals cannot make it act");
        assert!(BUSINESS_ASK.contains("never publish, spend"));
        for c in [Category::Spend, Category::Publish, Category::People, Category::Security] {
            assert!(!c.may_act_alone());
        }
        assert!(Category::Home.may_act_alone() && Category::Digital.may_act_alone());
    }

    #[test]
    fn the_memory_forgets_yesterday_and_follows_the_empty_house() {
        let mut m = Memory { day: "2026-10-19".into(), cleaned_today: true, away_since: Some(600), shown: ["tidy".to_string()].into() };
        m.roll("2026-10-20");
        assert_eq!(m, Memory { day: "2026-10-20".into(), away_since: Some(600), ..Memory::default() });
        m.track(Some(true), 700);
        assert_eq!(m.away_since, None);
        m.track(Some(false), 710);
        m.track(Some(false), 730);
        assert_eq!(m.away_since, Some(710));
        assert_eq!(minutes_since(1430, 10), 20, "across midnight");
        assert!(in_window(23 * 60, 22 * 60, 6 * 60) && !in_window(12 * 60, 22 * 60, 6 * 60));
        assert!(Goals { tidy: Mode::Suggest, ..Goals::default() }.any_on() && !Goals::default().any_on());
    }
}
