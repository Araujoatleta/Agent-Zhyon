// What the house is made of, as Mochi and Settings see it: the devices, what each one can do — as the device itself
// reported it, never assumed from its brand — and the actions asked of them, checked against that before anything is
// sent. Pure data and rules, tested below; the drivers (home_assistant.rs, samsung.rs, philips.rs) fill it and carry it
// out.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::i18n::{t, tf};

/// What a device is, for the words Mochi uses and the confirmation rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Tv,
    Speaker,
    Light,
    Plug,
    Switch,
    /// A robot vacuum (Home Assistant's `vacuum`).
    Vacuum,
    /// An air conditioner or a thermostat (Home Assistant's `climate`).
    Climate,
    /// A temperature or humidity sensor: read, never acted on.
    Sensor,
}

impl Kind {
    /// Plugs and switches power whatever is plugged into them (a heater, a pump…): an action on one always asks first,
    /// unless the user said otherwise for that device.
    pub fn sensitive(self) -> bool {
        matches!(self, Kind::Plug | Kind::Switch)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Tv => "tv",
            Kind::Speaker => "speaker",
            Kind::Light => "light",
            Kind::Plug => "plug",
            Kind::Switch => "switch",
            Kind::Vacuum => "vacuum",
            Kind::Climate => "climate",
            Kind::Sensor => "sensor",
        }
    }
}

/// One thing a device said it can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    TurnOn,
    TurnOff,
    VolumeSet,
    VolumeStep,
    Mute,
    Apps,
    Channels,
    ChannelStep,
    Brightness,
    /// A vacuum starts cleaning.
    Clean,
    /// A vacuum cleans the rooms it is told (Home Assistant 2026.3's `vacuum.clean_area`).
    CleanArea,
    Pause,
    /// A vacuum goes back to its dock.
    Dock,
    /// A climate device is set to a temperature.
    Temperature,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::TurnOn => "turn_on",
            Capability::TurnOff => "turn_off",
            Capability::VolumeSet => "set_volume",
            Capability::VolumeStep => "volume_up/volume_down",
            Capability::Mute => "mute/unmute",
            Capability::Apps => "open_app",
            Capability::Channels => "set_channel",
            Capability::ChannelStep => "channel_up/channel_down",
            Capability::Brightness => "set_brightness",
            Capability::Clean => "clean",
            Capability::CleanArea => "clean_area",
            Capability::Pause => "pause",
            Capability::Dock => "return_to_base",
            Capability::Temperature => "set_temperature",
        }
    }
}

/// A TV channel: its number on the remote (`preset`) and its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub number: String,
    pub name: String,
}

/// What a device reports about itself right now. None: it does not say.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceState {
    /// It answered the last time it was asked.
    pub online: bool,
    pub on: Option<bool>,
    /// Percent.
    pub volume: Option<u8>,
    pub muted: Option<bool>,
    /// Percent.
    pub brightness: Option<u8>,
    pub app: Option<String>,
    pub channel: Option<String>,
    /// What a vacuum is doing ("cleaning", "docked", "returning", "paused", "idle", "error") or the mode a climate
    /// device is in ("cool", "heat", "auto", "off"…), as the device says it.
    pub status: Option<String>,
    /// Percent.
    pub battery: Option<u8>,
    /// Tenths of a degree Celsius: the room's temperature, as a sensor or a climate device reads it.
    pub temperature: Option<i16>,
    /// Tenths of a degree Celsius: the temperature a climate device is set to reach.
    pub target: Option<i16>,
    /// Percent.
    pub humidity: Option<u8>,
}

/// One device, as the house shows it: identity, place, what it can do, its state, and the user's rules for it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// `ha:<entity_id>`, `samsung:<id>` or `philips:<id>`: stable, used by Settings, routines and Mochi.
    pub id: String,
    pub name: String,
    /// The room, "" when none is set.
    pub room: String,
    pub kind: Kind,
    /// "Home Assistant", "Samsung TV" or "Philips TV".
    pub source: String,
    /// The model, when the device or its description says it.
    pub model: Option<String>,
    pub caps: Vec<Capability>,
    /// The apps it can open (Capability::Apps), by name.
    pub apps: Vec<String>,
    pub channels: Vec<Channel>,
    pub state: DeviceState,
    /// Mochi may use it (the user ticked it in Settings → Mochi's home).
    pub exposed: bool,
    /// An action on it waits for a click.
    pub ask: bool,
}

impl Device {
    pub fn can(&self, cap: Capability) -> bool {
        self.caps.contains(&cap)
    }
}

/// An action asked of a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    TurnOn,
    TurnOff,
    /// Percent, 0 to 100.
    SetVolume(u8),
    VolumeUp,
    VolumeDown,
    Mute(bool),
    /// The app's name as asked; matched against the device's list by `resolve_app`.
    OpenApp(String),
    /// A channel number or name as asked; matched by `resolve_channel`.
    SetChannel(String),
    ChannelUp,
    ChannelDown,
    /// Percent, 1 to 100.
    SetBrightness(u8),
    /// A vacuum starts cleaning (the whole house, or as its own app says).
    Clean,
    /// A vacuum cleans these rooms only: Home Assistant areas, by name, matched by the driver.
    CleanArea(Vec<String>),
    Pause,
    /// A vacuum goes back to its dock.
    Dock,
    /// Degrees Celsius, 10 to 32.
    SetTemperature(u8),
}

/// The action names the model and the routines use.
pub const ACTION_WORDS: &[&str] = &[
    "turn_on", "turn_off", "set_volume", "volume_up", "volume_down", "mute", "unmute", "open_app", "set_channel",
    "channel_up", "channel_down", "set_brightness", "clean", "clean_area", "pause", "return_to_base", "set_temperature",
];

/// At most this many rooms in one clean_area.
pub const MAX_ROOMS: usize = 10;

/// The rooms of a clean_area: a list, or one text with commas ("Quarto, Sala").
fn rooms(value: Option<&Value>) -> Result<Vec<String>, String> {
    let items: Vec<String> = match value {
        Some(Value::Array(list)) => list.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        Some(Value::String(s)) => s.split([',', ';', '+']).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    let rooms: Vec<String> = items.iter().map(|r| r.trim().to_string()).filter(|r| !r.is_empty()).collect();
    if rooms.is_empty() {
        return Err("clean_area needs the rooms in \"value\", like \"Quarto, Sala\".".into());
    }
    if rooms.len() > MAX_ROOMS || rooms.iter().any(|r| r.chars().count() > 60) {
        return Err(format!("clean_area takes at most {MAX_ROOMS} rooms, each at most 60 characters."));
    }
    Ok(rooms)
}

/// A temperature in whole degrees Celsius, 10 to 32.
fn degrees(value: Option<&Value>) -> Result<u8, String> {
    let n = match value {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().trim_end_matches('C').trim_end_matches('°').trim().replace(',', ".").parse::<f64>().ok(),
        _ => None,
    }
    .ok_or("set_temperature needs a temperature in °C from 10 to 32 in \"value\".")?;
    if !(10.0..=32.0).contains(&n) {
        return Err("set_temperature must be from 10 to 32 °C.".into());
    }
    Ok(n.round() as u8)
}

fn percent(value: Option<&Value>, what: &str, min: u8) -> Result<u8, String> {
    let n = match value {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().trim_end_matches('%').trim().replace(',', ".").parse::<f64>().ok(),
        _ => None,
    }
    .ok_or_else(|| format!("{what} needs a number from {min} to 100 in \"value\"."))?;
    if !(min as f64..=100.0).contains(&n) {
        return Err(format!("{what} must be from {min} to 100."));
    }
    Ok(n.round() as u8)
}

fn text(value: Option<&Value>, what: &str) -> Result<String, String> {
    let s = match value {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    if s.is_empty() || s.chars().count() > 100 {
        return Err(format!("{what} needs a name in \"value\" (at most 100 characters)."));
    }
    Ok(s)
}

impl Action {
    /// An action from its name and value, as the model or a routine gives them.
    pub fn parse(word: &str, value: Option<&Value>) -> Result<Action, String> {
        let value = value.filter(|v| !v.is_null() && v.as_str().is_none_or(|s| !s.trim().is_empty()));
        Ok(match word.trim() {
            "turn_on" => Action::TurnOn,
            "turn_off" => Action::TurnOff,
            "set_volume" => Action::SetVolume(percent(value, "set_volume", 0)?),
            "volume_up" => Action::VolumeUp,
            "volume_down" => Action::VolumeDown,
            "mute" => Action::Mute(true),
            "unmute" => Action::Mute(false),
            "open_app" => Action::OpenApp(text(value, "open_app")?),
            "set_channel" => Action::SetChannel(text(value, "set_channel")?),
            "channel_up" => Action::ChannelUp,
            "channel_down" => Action::ChannelDown,
            "set_brightness" => Action::SetBrightness(percent(value, "set_brightness", 1)?),
            "clean" => Action::Clean,
            "clean_area" => Action::CleanArea(rooms(value)?),
            "pause" => Action::Pause,
            "return_to_base" => Action::Dock,
            "set_temperature" => Action::SetTemperature(degrees(value)?),
            other => return Err(format!("\"{other}\" is not an action. Use one of: {}.", ACTION_WORDS.join(", "))),
        })
    }

    /// The capability the device must have reported.
    pub fn needs(&self) -> Capability {
        match self {
            Action::TurnOn => Capability::TurnOn,
            Action::TurnOff => Capability::TurnOff,
            Action::SetVolume(_) => Capability::VolumeSet,
            Action::VolumeUp | Action::VolumeDown => Capability::VolumeStep,
            Action::Mute(_) => Capability::Mute,
            Action::OpenApp(_) => Capability::Apps,
            Action::SetChannel(_) => Capability::Channels,
            Action::ChannelUp | Action::ChannelDown => Capability::ChannelStep,
            Action::SetBrightness(_) => Capability::Brightness,
            Action::Clean => Capability::Clean,
            Action::CleanArea(_) => Capability::CleanArea,
            Action::Pause => Capability::Pause,
            Action::Dock => Capability::Dock,
            Action::SetTemperature(_) => Capability::Temperature,
        }
    }

    /// The action's name and value, as stored in a routine.
    pub fn word(&self) -> (&'static str, Option<String>) {
        match self {
            Action::TurnOn => ("turn_on", None),
            Action::TurnOff => ("turn_off", None),
            Action::SetVolume(p) => ("set_volume", Some(p.to_string())),
            Action::VolumeUp => ("volume_up", None),
            Action::VolumeDown => ("volume_down", None),
            Action::Mute(true) => ("mute", None),
            Action::Mute(false) => ("unmute", None),
            Action::OpenApp(app) => ("open_app", Some(app.clone())),
            Action::SetChannel(c) => ("set_channel", Some(c.clone())),
            Action::ChannelUp => ("channel_up", None),
            Action::ChannelDown => ("channel_down", None),
            Action::SetBrightness(p) => ("set_brightness", Some(p.to_string())),
            Action::Clean => ("clean", None),
            Action::CleanArea(rooms) => ("clean_area", Some(rooms.join(", "))),
            Action::Pause => ("pause", None),
            Action::Dock => ("return_to_base", None),
            Action::SetTemperature(t) => ("set_temperature", Some(t.to_string())),
        }
    }

    /// In plain English, for the model and the log: "turn on", "set the volume to 20%".
    pub fn describe(&self) -> String {
        match self {
            Action::TurnOn => "turn on".into(),
            Action::TurnOff => "turn off".into(),
            Action::SetVolume(p) => format!("set the volume to {p}%"),
            Action::VolumeUp => "turn the volume up".into(),
            Action::VolumeDown => "turn the volume down".into(),
            Action::Mute(true) => "mute".into(),
            Action::Mute(false) => "unmute".into(),
            Action::OpenApp(app) => format!("open {app}"),
            Action::SetChannel(c) => format!("switch to channel {c}"),
            Action::ChannelUp => "go to the next channel".into(),
            Action::ChannelDown => "go to the previous channel".into(),
            Action::SetBrightness(p) => format!("set the brightness to {p}%"),
            Action::Clean => "start cleaning".into(),
            Action::CleanArea(rooms) => format!("clean {}", rooms.join(", ")),
            Action::Pause => "pause".into(),
            Action::Dock => "go back to its dock".into(),
            Action::SetTemperature(t) => format!("set the temperature to {t} °C"),
        }
    }

    /// In the user's language, for the island's card, the steps and the phone: "Turn on", "Volume 20%".
    pub fn label(&self) -> String {
        match self {
            Action::TurnOn => t("Turn on"),
            Action::TurnOff => t("Turn off"),
            Action::SetVolume(p) => tf("Volume {percent}%", &[("percent", &p.to_string())]),
            Action::VolumeUp => t("Volume up"),
            Action::VolumeDown => t("Volume down"),
            Action::Mute(true) => t("Mute"),
            Action::Mute(false) => t("Unmute"),
            Action::OpenApp(app) => tf("Open {app}", &[("app", app)]),
            Action::SetChannel(c) => tf("Channel {channel}", &[("channel", c)]),
            Action::ChannelUp => t("Next channel"),
            Action::ChannelDown => t("Previous channel"),
            Action::SetBrightness(p) => tf("Brightness {percent}%", &[("percent", &p.to_string())]),
            Action::Clean => t("Start cleaning"),
            Action::CleanArea(rooms) => tf("Clean {rooms}", &[("rooms", &rooms.join(", "))]),
            Action::Pause => t("Pause"),
            Action::Dock => t("Back to the dock"),
            Action::SetTemperature(d) => tf("Set to {degrees} °C", &[("degrees", &d.to_string())]),
        }
    }
}

/// Tenths of a degree as people read them: 235 → "23.5", 230 → "23".
pub fn degrees_text(tenths: i16) -> String {
    if tenths % 10 == 0 {
        (tenths / 10).to_string()
    } else {
        format!("{:.1}", f64::from(tenths) / 10.0)
    }
}

// ── Names ─────────────────────────────────────────────────────────────────────

/// Lower case, accents off, punctuation as spaces, one space between words: "TV da Sala!" → "tv da sala".
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        let base = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
            'ç' | 'ć' | 'č' => 'c',
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
            'ñ' | 'ń' | 'ň' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => 'o',
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' => 'u',
            'ý' | 'ÿ' => 'y',
            'ś' | 'š' | 'ß' => 's',
            'ź' | 'ż' | 'ž' => 'z',
            'ł' => 'l',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        out.push(base);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Small words that join a device to its room: "TV da sala", "luz do quarto", "lamp in the kitchen".
const JOINERS: &[&str] = &["", "da", "do", "de", "das", "dos", "na", "no", "em", "in", "the", "in the", "of the", "del", "de la", "du", "de l"];

/// The ways a device may be called: its name, its id, and its name with its room.
fn names_of(d: &Device) -> Vec<String> {
    let name = normalize(&d.name);
    let mut names = vec![name.clone(), normalize(&d.id)];
    let room = normalize(&d.room);
    if !room.is_empty() && !name.contains(&room) {
        for j in JOINERS {
            names.push(normalize(&format!("{name} {j} {room}")));
            names.push(normalize(&format!("{room} {name}")));
        }
    }
    names
}

/// How well `name` matches the query `q` (both normalized): 0 not at all; the length of the name when the query holds
/// it as whole words ("a tv do quarto" holds "tv do quarto"), so the most specific name wins; the length of the query
/// when the name holds it ("luz" in "luz do teto").
fn score(name: &str, q: &str) -> usize {
    if name.is_empty() {
        return 0;
    }
    let (padded_q, padded_n) = (format!(" {q} "), format!(" {name} "));
    if padded_q.contains(&padded_n) {
        name.len()
    } else if padded_n.contains(&format!(" {q} ")) || name.contains(q) {
        q.len()
    } else {
        0
    }
}

/// The one device `query` names: its id, its name (with or without its room), or the one whose name matches it best.
/// An Err tells the model which devices exist, or which ones the query could mean.
pub fn find<'a>(devices: &'a [Device], query: &str) -> Result<&'a Device, String> {
    let q = normalize(query);
    if q.is_empty() {
        return Err("Say which device in \"device\".".into());
    }
    if let Some(d) = devices.iter().find(|d| d.id == query.trim()) {
        return Ok(d);
    }
    let exact: Vec<&Device> = devices.iter().filter(|d| names_of(d).contains(&q)).collect();
    let best: Vec<&Device> = if exact.is_empty() {
        let scored: Vec<(usize, &Device)> = devices
            .iter()
            .map(|d| (names_of(d).iter().map(|n| score(n, &q)).max().unwrap_or(0), d))
            .filter(|(s, _)| *s > 0)
            .collect();
        let top = scored.iter().map(|(s, _)| *s).max().unwrap_or(0);
        scored.into_iter().filter(|(s, _)| *s == top).map(|(_, d)| d).collect()
    } else {
        exact
    };
    match best.len() {
        1 => Ok(best[0]),
        0 => Err(format!(
            "No device is called \"{query}\". The devices Mochi may use: {}.",
            list_names(devices.iter())
        )),
        _ => Err(format!("\"{query}\" could be {}. Use the device's id.", list_names(best.into_iter()))),
    }
}

fn list_names<'a>(devices: impl Iterator<Item = &'a Device>) -> String {
    let names: Vec<String> = devices
        .map(|d| if d.room.is_empty() { format!("{} ({})", d.name, d.id) } else { format!("{} in {} ({})", d.name, d.room, d.id) })
        .collect();
    if names.is_empty() { "none".into() } else { names.join(", ") }
}

/// The one entry of `options` that `wanted` names: the same name, else the only one that starts with it, else the
/// only one that contains it.
fn pick<'a>(options: &'a [String], wanted: &str) -> Result<&'a String, Vec<&'a String>> {
    let w = normalize(wanted);
    let by = |f: &dyn Fn(&str) -> bool| options.iter().filter(|o| f(&normalize(o))).collect::<Vec<_>>();
    for found in [by(&|o| o == w), by(&|o| o.starts_with(&w)), by(&|o| o.contains(&w))] {
        match found.len() {
            1 => return Ok(found[0]),
            0 => continue,
            _ => return Err(found),
        }
    }
    Err(Vec::new())
}

/// The app of the device's own list that `wanted` names ("youtube" → "YouTube").
pub fn resolve_app(device: &Device, wanted: &str) -> Result<String, String> {
    match pick(&device.apps, wanted) {
        Ok(app) => Ok(app.clone()),
        Err(several) if !several.is_empty() => Err(format!(
            "\"{wanted}\" could be {} on {}. Say which.",
            several.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
            device.name
        )),
        Err(_) => Err(format!(
            "{} has no app called \"{wanted}\". Its apps: {}.",
            device.name,
            if device.apps.is_empty() { "none listed".into() } else { device.apps.join(", ") }
        )),
    }
}

/// The channel of the device's list that `wanted` names: its number, else its name.
pub fn resolve_channel(device: &Device, wanted: &str) -> Result<Channel, String> {
    let w = wanted.trim();
    if let Some(c) = device.channels.iter().find(|c| c.number == w) {
        return Ok(c.clone());
    }
    let names: Vec<String> = device.channels.iter().map(|c| c.name.clone()).collect();
    match pick(&names, w) {
        Ok(name) => Ok(device.channels.iter().find(|c| &c.name == name).cloned().expect("picked from the list")),
        Err(several) if !several.is_empty() => Err(format!(
            "\"{w}\" could be several channels on {}: {}. Use the number.",
            device.name,
            several.iter().take(8).map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        )),
        Err(_) => Err(format!("{} has no channel \"{w}\" (by number or name).", device.name)),
    }
}

/// Checks `action` against what `device` reported, before anything is sent. The Err is for the model: it says what
/// the device can do instead.
pub fn check(device: &Device, action: &Action) -> Result<(), String> {
    if !device.exposed {
        return Err(format!(
            "{} is not one of the devices the user lets Mochi use (Settings → Mochi's home).",
            device.name
        ));
    }
    if !device.can(action.needs()) {
        let can: Vec<&str> = device.caps.iter().map(|c| c.as_str()).collect();
        return Err(format!(
            "{} did not report that it can {}. What it can do: {}.",
            device.name,
            action.describe(),
            if can.is_empty() { "nothing".to_string() } else { can.join(", ") }
        ));
    }
    match action {
        Action::OpenApp(app) => resolve_app(device, app).map(|_| ()),
        Action::SetChannel(c) => resolve_channel(device, c).map(|_| ()),
        _ => Ok(()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub fn tv(id: &str, name: &str, room: &str) -> Device {
        Device {
            id: id.into(),
            name: name.into(),
            room: room.into(),
            kind: Kind::Tv,
            source: "Philips TV".into(),
            model: None,
            caps: vec![Capability::TurnOn, Capability::TurnOff, Capability::VolumeSet, Capability::Apps, Capability::Channels],
            apps: vec!["YouTube".into(), "YouTube Kids".into(), "Netflix".into(), "Globoplay".into()],
            channels: vec![
                Channel { number: "4".into(), name: "Globo".into() },
                Channel { number: "5".into(), name: "SBT".into() },
                Channel { number: "13".into(), name: "Globo News".into() },
            ],
            state: DeviceState::default(),
            exposed: true,
            ask: false,
        }
    }

    pub fn light(id: &str, name: &str, room: &str) -> Device {
        Device {
            kind: Kind::Light,
            source: "Home Assistant".into(),
            caps: vec![Capability::TurnOn, Capability::TurnOff, Capability::Brightness],
            apps: vec![],
            channels: vec![],
            ..tv(id, name, room)
        }
    }

    #[test]
    fn actions_are_read_from_the_models_words_and_checked() {
        assert_eq!(Action::parse("turn_on", None).unwrap(), Action::TurnOn);
        assert_eq!(Action::parse("set_volume", Some(&json!(20))).unwrap(), Action::SetVolume(20));
        assert_eq!(Action::parse("set_volume", Some(&json!("35%"))).unwrap(), Action::SetVolume(35));
        assert_eq!(Action::parse("set_brightness", Some(&json!("12,6"))).unwrap(), Action::SetBrightness(13));
        assert_eq!(Action::parse("open_app", Some(&json!(" YouTube "))).unwrap(), Action::OpenApp("YouTube".into()));
        assert_eq!(Action::parse("set_channel", Some(&json!(5))).unwrap(), Action::SetChannel("5".into()));
        assert_eq!(Action::parse("unmute", Some(&Value::Null)).unwrap(), Action::Mute(false));
        assert!(Action::parse("set_volume", Some(&json!(140))).unwrap_err().contains("0 to 100"));
        assert!(Action::parse("set_brightness", Some(&json!(0))).is_err(), "0% is turn_off");
        assert!(Action::parse("open_app", None).is_err());
        assert!(Action::parse("open_app", Some(&json!("  "))).is_err());
        assert!(Action::parse("self_destruct", None).unwrap_err().contains("turn_on"));
        assert_eq!(Action::parse("set_temperature", Some(&json!("23 °C"))).unwrap(), Action::SetTemperature(23));
        assert_eq!(Action::parse("set_temperature", Some(&json!(22.6))).unwrap(), Action::SetTemperature(23));
        assert!(Action::parse("set_temperature", Some(&json!(40))).unwrap_err().contains("10 to 32"));
        assert_eq!(Action::parse("clean_area", Some(&json!("Quarto, Sala ;"))).unwrap(), Action::CleanArea(vec!["Quarto".into(), "Sala".into()]));
        assert_eq!(Action::parse("clean_area", Some(&json!(["Cozinha"]))).unwrap(), Action::CleanArea(vec!["Cozinha".into()]));
        assert!(Action::parse("clean_area", Some(&json!(" , "))).is_err());
        assert!(Action::parse("clean_area", None).is_err());
        assert_eq!(Action::parse("return_to_base", None).unwrap(), Action::Dock);
        assert_eq!(Action::CleanArea(vec!["Quarto".into(), "Sala".into()]).word(), ("clean_area", Some("Quarto, Sala".into())));
        assert_eq!(degrees_text(235), "23.5");
        assert_eq!(degrees_text(230), "23");
        assert_eq!(degrees_text(-15), "-1.5");
        // Every word round-trips.
        for word in ACTION_WORDS {
            let value = match *word {
                "set_volume" | "set_brightness" => Some(json!(40)),
                "set_temperature" => Some(json!(22)),
                "open_app" | "set_channel" | "clean_area" => Some(json!("x")),
                _ => None,
            };
            let action = Action::parse(word, value.as_ref()).unwrap();
            assert_eq!(action.word().0, *word);
        }
    }

    #[test]
    fn names_ignore_case_accents_and_punctuation() {
        assert_eq!(normalize("  TV da Sála!! "), "tv da sala");
        assert_eq!(normalize("Luz-do-Quarto"), "luz do quarto");
        assert_eq!(normalize("Ação"), "acao");
    }

    #[test]
    fn a_device_is_found_by_id_name_or_name_with_its_room() {
        let devices = vec![
            tv("philips:aa", "TV", "Sala"),
            tv("philips:bb", "TV", "Quarto"),
            light("ha:light.teto", "Luz do teto", "Sala"),
        ];
        assert_eq!(find(&devices, "philips:bb").unwrap().id, "philips:bb");
        assert_eq!(find(&devices, "TV da sala").unwrap().id, "philips:aa");
        assert_eq!(find(&devices, "tv do quarto").unwrap().id, "philips:bb");
        assert_eq!(find(&devices, "a TV do Quarto").unwrap().id, "philips:bb");
        assert_eq!(find(&devices, "luz").unwrap().id, "ha:light.teto");
        let both = find(&devices, "TV").unwrap_err();
        assert!(both.contains("philips:aa") && both.contains("philips:bb"), "{both}");
        let none = find(&devices, "geladeira").unwrap_err();
        assert!(none.contains("Luz do teto"), "{none}");
        assert!(find(&devices, " ").is_err());
    }

    #[test]
    fn apps_and_channels_come_from_the_devices_own_list() {
        let tv = tv("philips:aa", "TV", "Sala");
        assert_eq!(resolve_app(&tv, "youtube").unwrap(), "YouTube", "the exact name wins over YouTube Kids");
        assert_eq!(resolve_app(&tv, "net").unwrap(), "Netflix");
        assert!(resolve_app(&tv, "Disney+").unwrap_err().contains("Globoplay"));
        assert_eq!(resolve_channel(&tv, "13").unwrap().name, "Globo News");
        assert_eq!(resolve_channel(&tv, "sbt").unwrap().number, "5");
        assert_eq!(resolve_channel(&tv, "globo").unwrap().number, "4", "the exact name");
        assert!(resolve_channel(&tv, "99").is_err());
    }

    #[test]
    fn an_action_the_device_did_not_report_is_refused_before_anything_is_sent() {
        let mut tv = tv("philips:aa", "TV", "Sala");
        assert!(check(&tv, &Action::TurnOn).is_ok());
        assert!(check(&tv, &Action::OpenApp("youtube".into())).is_ok());
        let err = check(&tv, &Action::SetBrightness(50)).unwrap_err();
        assert!(err.contains("did not report") && err.contains("turn_on"), "{err}");
        assert!(check(&tv, &Action::OpenApp("Spotify".into())).is_err());
        tv.exposed = false;
        assert!(check(&tv, &Action::TurnOn).unwrap_err().contains("Settings"));
    }

    #[test]
    fn plugs_and_switches_are_the_sensitive_kinds() {
        assert!(Kind::Plug.sensitive() && Kind::Switch.sensitive());
        assert!(!Kind::Tv.sensitive() && !Kind::Light.sensitive() && !Kind::Speaker.sensitive());
    }
}
