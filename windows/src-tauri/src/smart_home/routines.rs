// Routines: a name ("Good night", "Movie mode") and the home actions it stands for, in order, set up in Settings →
// Mochi's home. Mochi runs one by its name (home_routine), the user from Settings. Each step goes through the same
// checks as an action asked alone: the device must be one Mochi may use, and must have reported the action.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::model::{self, Action, Capability, Device, Kind};
use crate::i18n::{t, tf};

pub const MAX_STEPS: usize = 20;
/// Seconds a step may wait before it runs (a TV takes a moment to start before it can open an app).
pub const MAX_WAIT: u32 = 120;
const MAX_NAME: usize = 60;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub steps: Vec<RoutineStep>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoutineStep {
    /// The device's id (`ha:light.sala`, `philips:…`).
    pub device: String,
    /// One of model::ACTION_WORDS.
    pub action: String,
    /// The percent, the app or the channel; "" when the action takes none.
    pub value: String,
    /// Seconds to wait before this step.
    pub wait: u32,
}

impl RoutineStep {
    pub fn action(&self) -> Result<Action, String> {
        let value = (!self.value.trim().is_empty()).then(|| Value::String(self.value.trim().to_string()));
        Action::parse(&self.action, value.as_ref())
    }

    fn of(device: &Device, action: Action, wait: u32) -> RoutineStep {
        let (word, value) = action.word();
        RoutineStep { device: device.id.clone(), action: word.into(), value: value.unwrap_or_default(), wait }
    }
}

fn new_id() -> String {
    let mut bytes = [0u8; 6];
    let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes);
    format!("r-{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// A routine as Settings sent it, checked for its form: a name, at most MAX_STEPS steps, each a known action with a
/// usable value. Whether its devices can do it is `problems`' business (devices come and go).
pub fn clean(mut r: Routine) -> Result<Routine, String> {
    r.name = r.name.trim().chars().take(MAX_NAME).collect();
    if r.name.is_empty() {
        return Err(t("Give the routine a name."));
    }
    if r.steps.is_empty() {
        return Err(t("Add at least one step to the routine."));
    }
    if r.steps.len() > MAX_STEPS {
        return Err(tf("A routine has at most {count} steps.", &[("count", &MAX_STEPS.to_string())]));
    }
    for step in &mut r.steps {
        step.device = step.device.trim().to_string();
        step.value = step.value.trim().chars().take(100).collect();
        step.wait = step.wait.min(MAX_WAIT);
        if step.device.is_empty() {
            return Err(t("Pick a device for every step."));
        }
        step.action().map_err(|_| t("A step of this routine has an action or a value that is not valid."))?;
    }
    let id_ok = r.id.len() <= 40 && r.id.starts_with("r-") && r.id[2..].bytes().all(|b| b.is_ascii_hexdigit());
    if !id_ok {
        r.id = new_id();
    }
    Ok(r)
}

/// What stops a routine from running as it is, step by step, for the model and Settings.
pub fn problems(r: &Routine, devices: &[Device]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, step) in r.steps.iter().enumerate() {
        let n = i + 1;
        let Some(device) = devices.iter().find(|d| d.id == step.device) else {
            out.push(format!("step {n}: the device {} is not there any more", step.device));
            continue;
        };
        match step.action() {
            Ok(action) => {
                if let Err(e) = model::check(device, &action) {
                    out.push(format!("step {n}: {e}"));
                }
            }
            Err(e) => out.push(format!("step {n}: {e}")),
        }
    }
    out
}

/// True when one of its steps is on a device that asks before acting.
pub fn asks(r: &Routine, devices: &[Device]) -> bool {
    r.steps.iter().any(|s| devices.iter().find(|d| d.id == s.device).is_none_or(|d| d.ask))
}

/// A device with its room, as a routine's name says it: "TV (Quarto)".
fn shown(d: &Device) -> String {
    if d.room.is_empty() || model::normalize(&d.name).contains(&model::normalize(&d.room)) {
        d.name.clone()
    } else {
        format!("{} ({})", d.name, d.room)
    }
}

/// Words that name a living room, to pick the TV of "Movie mode".
const LIVING_ROOMS: &[&str] = &["sala", "living", "salon", "lounge", "estar", "family"];

/// Routines to start from, made of the user's own devices (those Mochi may use): "Good night" turns off the TVs,
/// the speakers and the lights; "Movie mode" turns on the living room's TV and dims its lights; "Turn on …" for each TV
/// that has a room. Shown for review; nothing is saved until the user saves one.
pub fn templates(devices: &[Device]) -> Vec<Routine> {
    let mine: Vec<&Device> = devices.iter().filter(|d| d.exposed).collect();
    let mut out = Vec::new();

    let off: Vec<RoutineStep> = mine
        .iter()
        .filter(|d| matches!(d.kind, Kind::Tv | Kind::Speaker | Kind::Light) && d.can(Capability::TurnOff))
        .map(|d| RoutineStep::of(d, Action::TurnOff, 0))
        .collect();
    if !off.is_empty() {
        out.push(Routine { id: "tpl-good-night".into(), name: t("Good night"), steps: off.into_iter().take(MAX_STEPS).collect() });
    }

    let tvs: Vec<&&Device> = mine.iter().filter(|d| d.kind == Kind::Tv && d.can(Capability::TurnOn)).collect();
    let living = tvs
        .iter()
        .find(|d| LIVING_ROOMS.iter().any(|w| model::normalize(&d.room).contains(w)))
        .or_else(|| tvs.first())
        .copied();
    if let Some(tv) = living {
        let mut steps = vec![RoutineStep::of(tv, Action::TurnOn, 0)];
        for light in mine.iter().filter(|d| d.kind == Kind::Light && (tv.room.is_empty() || model::normalize(&d.room) == model::normalize(&tv.room))) {
            if light.can(Capability::Brightness) {
                steps.push(RoutineStep::of(light, Action::SetBrightness(20), 0));
            } else if light.can(Capability::TurnOff) {
                steps.push(RoutineStep::of(light, Action::TurnOff, 0));
            }
        }
        steps.truncate(MAX_STEPS);
        out.push(Routine { id: "tpl-movie-mode".into(), name: t("Movie mode"), steps });
    }

    for tv in tvs.iter().filter(|d| !d.room.is_empty()) {
        out.push(Routine {
            id: format!("tpl-on-{}", tv.id),
            name: tf("Turn on {device}", &[("device", &shown(tv))]),
            steps: vec![RoutineStep::of(tv, Action::TurnOn, 0)],
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smart_home::model::tests::{light, tv};

    fn house() -> Vec<Device> {
        let mut plug = light("ha:switch.aquecedor", "Aquecedor", "Quarto");
        plug.kind = Kind::Plug;
        plug.caps = vec![Capability::TurnOn, Capability::TurnOff];
        plug.ask = true;
        let mut dim = light("ha:light.sala", "Luz", "Sala");
        dim.caps.retain(|c| *c != Capability::Brightness);
        vec![tv("philips:aa", "TV", "Sala"), tv("philips:bb", "TV", "Quarto"), dim, light("ha:light.teto", "Teto", "Sala"), plug]
    }

    #[test]
    fn templates_are_made_of_the_users_own_devices() {
        let t = templates(&house());
        let names: Vec<&str> = t.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Good night", "Movie mode", "Turn on TV (Sala)", "Turn on TV (Quarto)"]);
        // Good night: TVs and lights off, never the plug (who knows what it powers).
        assert!(t[0].steps.iter().all(|s| s.action == "turn_off" && s.device != "ha:switch.aquecedor"));
        assert_eq!(t[0].steps.len(), 4);
        // Movie mode: the living room's TV on, its dimmable light at 20%, the other light off.
        let movie = &t[1].steps;
        assert_eq!((movie[0].device.as_str(), movie[0].action.as_str()), ("philips:aa", "turn_on"));
        assert!(movie.iter().any(|s| s.device == "ha:light.teto" && s.action == "set_brightness" && s.value == "20"));
        assert!(movie.iter().any(|s| s.device == "ha:light.sala" && s.action == "turn_off"));
        // Nothing Mochi may not use.
        let mut hidden = house();
        for d in &mut hidden {
            d.exposed = false;
        }
        assert!(templates(&hidden).is_empty());
    }

    #[test]
    fn a_routine_is_checked_for_its_form_and_against_the_devices() {
        let mut r = Routine {
            id: String::new(),
            name: "  Boa noite  ".into(),
            steps: vec![RoutineStep { device: "philips:aa".into(), action: "turn_off".into(), value: String::new(), wait: 999 }],
        };
        let clean_r = clean(r.clone()).unwrap();
        assert_eq!(clean_r.name, "Boa noite");
        assert!(clean_r.id.starts_with("r-") && clean_r.id.len() == 14);
        assert_eq!(clean_r.steps[0].wait, MAX_WAIT);
        assert_eq!(clean(clean_r.clone()).unwrap().id, clean_r.id, "a good id is kept");
        r.steps[0].action = "explode".into();
        assert!(clean(r.clone()).is_err());
        r.steps[0].action = "set_volume".into();
        r.steps[0].value = "loud".into();
        assert!(clean(r.clone()).is_err(), "a value that does not parse");
        assert!(clean(Routine { name: " ".into(), ..clean_r.clone() }).is_err());
        assert!(clean(Routine { steps: vec![], ..clean_r.clone() }).is_err());

        let devices = house();
        let ok = Routine { steps: vec![RoutineStep { device: "philips:aa".into(), action: "open_app".into(), value: "youtube".into(), wait: 5 }], ..clean_r.clone() };
        assert!(problems(&ok, &devices).is_empty());
        assert!(!asks(&ok, &devices));
        let bad = Routine {
            steps: vec![
                RoutineStep { device: "ha:light.gone".into(), action: "turn_on".into(), value: String::new(), wait: 0 },
                RoutineStep { device: "ha:light.teto".into(), action: "open_app".into(), value: "x".into(), wait: 0 },
                RoutineStep { device: "ha:switch.aquecedor".into(), action: "turn_on".into(), value: String::new(), wait: 0 },
            ],
            ..clean_r.clone()
        };
        let p = problems(&bad, &devices);
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(p[0].starts_with("step 1") && p[1].starts_with("step 2"));
        assert!(asks(&bad, &devices), "the plug asks, and so does a device that is gone");
    }
}
