// Zheus One's World State: the house as one picture — the devices the user lets agents use, who is home, each room's
// temperature — and the facts that tie them together (the vacuum is cleaning the bedroom, nobody is home with the
// lights on). Pure: built from what smart_home read, tested below; mod.rs reads the house and keeps what the user said.

use serde::Serialize;

use crate::i18n::{t, tf};
use crate::smart_home::home_assistant::Person;
use crate::smart_home::model::{degrees_text, normalize, Device, Kind};
use crate::smart_home::state_text;

/// Where the user is, as far as Zheus knows: what they said ("I'm on my way home"), else Home Assistant when it
/// follows one person only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum Presence {
    Unknown,
    Home,
    Away,
    /// On the way, expected at this minute of the day.
    Arriving { at: u32 },
}

/// One room: its temperature and humidity, from a sensor or a climate device in it, and what is on in it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    pub name: String,
    /// Tenths of °C.
    pub temperature: Option<i16>,
    pub humidity: Option<u8>,
    /// The devices on in it, by name.
    pub on: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct World {
    /// Minutes since local midnight.
    pub now: u32,
    pub user: Presence,
    /// Someone is home, as Home Assistant's people say; None when it says nothing.
    pub occupied: Option<bool>,
    pub people: Vec<Person>,
    /// The devices Zheus may use (those ticked for Mochi), with their state.
    pub devices: Vec<Device>,
    pub rooms: Vec<Room>,
    /// What ties the things together, in the user's words.
    pub facts: Vec<String>,
}

/// A minute of the day as a clock reads it: 1110 → "18:30".
pub fn clock(minute: u32) -> String {
    format!("{:02}:{:02}", (minute / 60) % 24, minute % 60)
}

/// Devices that use power for the user's comfort, which "nobody is home" is about: never plugs and switches (who knows
/// what they power: a fridge, a pump).
pub fn comfort_device(d: &Device) -> bool {
    matches!(d.kind, Kind::Light | Kind::Tv | Kind::Speaker | Kind::Climate)
}

/// The World State from what the house reported. `cleaning`: the rooms Zheus sent the vacuum to, if it did.
pub fn build(now: u32, user: Presence, people: Vec<Person>, devices: Vec<Device>, cleaning: Option<&[String]>) -> World {
    let devices: Vec<Device> = devices.into_iter().filter(|d| d.exposed).collect();
    let occupied = (!people.is_empty()).then(|| people.iter().any(|p| p.home));
    let rooms = rooms_of(&devices);
    let mut world = World { now, user, occupied, people, devices, rooms, facts: Vec::new() };
    world.facts = facts(&world, cleaning);
    world
}

fn rooms_of(devices: &[Device]) -> Vec<Room> {
    let mut rooms: Vec<Room> = Vec::new();
    for d in devices.iter().filter(|d| !d.room.trim().is_empty()) {
        let key = normalize(&d.room);
        let i = match rooms.iter().position(|r| normalize(&r.name) == key) {
            Some(i) => i,
            None => {
                rooms.push(Room { name: d.room.trim().to_string(), temperature: None, humidity: None, on: Vec::new() });
                rooms.len() - 1
            }
        };
        let room = &mut rooms[i];
        // A sensor's reading wins over the one a climate device makes near its own vent.
        let sensor = d.kind == Kind::Sensor;
        if d.state.temperature.is_some() && (sensor || room.temperature.is_none()) {
            room.temperature = d.state.temperature;
        }
        if d.state.humidity.is_some() && (sensor || room.humidity.is_none()) {
            room.humidity = d.state.humidity;
        }
        if d.kind != Kind::Sensor && d.state.on == Some(true) {
            room.on.push(d.name.clone());
        }
    }
    rooms
}

fn facts(w: &World, cleaning: Option<&[String]>) -> Vec<String> {
    let mut out = Vec::new();
    if let Presence::Arriving { at } = w.user {
        out.push(tf("You arrive around {time}.", &[("time", &clock(at))]));
    }
    let cleaning = cleaning.filter(|r| !r.is_empty());
    for v in w.devices.iter().filter(|d| d.kind == Kind::Vacuum) {
        match v.state.status.as_deref() {
            Some("cleaning") => {
                match cleaning {
                    Some(rooms) => out.push(tf("{device} is cleaning {rooms}.", &[("device", &v.name), ("rooms", &rooms.join(", "))])),
                    None => out.push(tf("{device} is cleaning.", &[("device", &v.name)])),
                }
                // Something playing where it cleans: an outing for the vacuum is no time for a film there.
                if let Some(rooms) = cleaning {
                    let busy = |d: &&Device| rooms.iter().any(|r| normalize(r) == normalize(&d.room));
                    for d in w.devices.iter().filter(|d| matches!(d.kind, Kind::Tv | Kind::Speaker) && d.state.on == Some(true)).filter(busy) {
                        out.push(tf("{device} is on where the vacuum is cleaning.", &[("device", &d.name)]));
                    }
                }
            }
            Some("error") => out.push(tf("{device} needs help.", &[("device", &v.name)])),
            _ => {}
        }
        if let Some(b) = v.state.battery.filter(|b| *b < 20) {
            out.push(tf("{device}'s battery is at {percent}%.", &[("device", &v.name), ("percent", &b.to_string())]));
        }
    }
    if w.occupied == Some(false) && !matches!(w.user, Presence::Arriving { .. }) {
        let on: Vec<&str> = w.devices.iter().filter(|d| comfort_device(d) && d.state.on == Some(true)).map(|d| d.name.as_str()).collect();
        if !on.is_empty() {
            out.push(tf("Nobody is home, and these are on: {devices}.", &[("devices", &on.join(", "))]));
        }
    }
    let silent: Vec<&str> = w.devices.iter().filter(|d| !d.state.online).map(|d| d.name.as_str()).collect();
    if !silent.is_empty() {
        out.push(tf("Not answering: {devices}.", &[("devices", &silent.join(", "))]));
    }
    out
}

impl World {
    /// The World State in English, for the assistant (the zheus_world tool, a mission's context).
    pub fn for_model(&self) -> String {
        let mut out = format!("Now {}. ", clock(self.now));
        out.push_str(&match self.user {
            Presence::Unknown => "Where the user is: not known.".to_string(),
            Presence::Home => "The user is home.".to_string(),
            Presence::Away => "The user is away.".to_string(),
            Presence::Arriving { at } => format!("The user is on the way home, expected around {}.", clock(at)),
        });
        out.push_str(match self.occupied {
            Some(true) => " Home Assistant says someone is home.",
            Some(false) => " Home Assistant says nobody is home.",
            None => "",
        });
        if self.devices.is_empty() {
            out.push_str("\nNo device is shared with Zheus.");
        } else {
            out.push_str("\nDevices:");
            for d in &self.devices {
                let room = if d.room.is_empty() { String::new() } else { format!(", {}", d.room) };
                let can: Vec<&str> = d.caps.iter().map(|c| c.as_str()).collect();
                out.push_str(&format!("\n- {} ({}{room}, id {}): {}", d.name, d.kind.as_str(), d.id, state_text(&d.state)));
                if !can.is_empty() {
                    out.push_str(&format!("; can {}", can.join(", ")));
                }
            }
        }
        let rooms: Vec<String> = self
            .rooms
            .iter()
            .filter_map(|r| {
                let mut parts = Vec::new();
                if let Some(t) = r.temperature {
                    parts.push(format!("{} °C", degrees_text(t)));
                }
                if let Some(h) = r.humidity {
                    parts.push(format!("{h}% humidity"));
                }
                (!parts.is_empty()).then(|| format!("{} {}", r.name, parts.join(", ")))
            })
            .collect();
        if !rooms.is_empty() {
            out.push_str(&format!("\nRooms: {}.", rooms.join("; ")));
        }
        if !self.facts.is_empty() {
            out.push_str(&format!("\nFacts: {}", self.facts.join(" ")));
        }
        out
    }

    /// One line for the island and the phone: "Home · 2 rooms · 24 °C in the Bedroom".
    pub fn headline(&self) -> String {
        match self.user {
            Presence::Home => t("You are home"),
            Presence::Away => t("You are away"),
            Presence::Arriving { at } => tf("You arrive around {time}", &[("time", &clock(at))]),
            Presence::Unknown => match self.occupied {
                Some(true) => t("Someone is home"),
                Some(false) => t("Nobody is home"),
                None => t("The house"),
            },
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::smart_home::model::tests::{light, tv};
    use crate::smart_home::model::{Capability, DeviceState};

    pub fn vacuum(status: &str, battery: u8) -> Device {
        Device {
            kind: Kind::Vacuum,
            source: "Home Assistant".into(),
            caps: vec![Capability::Clean, Capability::CleanArea, Capability::Pause, Capability::Dock],
            apps: vec![],
            channels: vec![],
            state: DeviceState { online: true, on: Some(status == "cleaning"), status: Some(status.into()), battery: Some(battery), ..DeviceState::default() },
            ..tv("ha:vacuum.robo", "Robô", "Sala")
        }
    }

    pub fn ac(room: &str, on: bool, temp: i16) -> Device {
        Device {
            kind: Kind::Climate,
            source: "Home Assistant".into(),
            caps: vec![Capability::TurnOn, Capability::TurnOff, Capability::Temperature],
            apps: vec![],
            channels: vec![],
            state: DeviceState {
                online: true,
                on: Some(on),
                status: Some(if on { "cool" } else { "off" }.into()),
                temperature: Some(temp),
                target: Some(240),
                ..DeviceState::default()
            },
            ..tv(&format!("ha:climate.{}", normalize(room)), &format!("Ar do {room}"), room)
        }
    }

    pub fn sensor(room: &str, temp: i16, humidity: u8) -> Device {
        Device {
            kind: Kind::Sensor,
            source: "Home Assistant".into(),
            caps: vec![],
            apps: vec![],
            channels: vec![],
            state: DeviceState { online: true, temperature: Some(temp), humidity: Some(humidity), ..DeviceState::default() },
            ..tv(&format!("ha:sensor.{}", normalize(room)), &format!("Sensor {room}"), room)
        }
    }

    #[test]
    fn the_rooms_take_the_sensors_reading_and_what_is_on() {
        let mut lamp = light("ha:light.sala", "Luz da Sala", "Sala");
        lamp.state = DeviceState { online: true, on: Some(true), ..DeviceState::default() };
        let mut hidden = light("ha:light.x", "Abajur", "Sala");
        hidden.exposed = false;
        hidden.state.on = Some(true);
        let w = build(18 * 60, Presence::Unknown, vec![], vec![ac("Quarto", false, 275), sensor("Quarto", 268, 60), lamp, hidden], None);
        assert_eq!(w.devices.len(), 3, "only what the user shares");
        let quarto = w.rooms.iter().find(|r| r.name == "Quarto").unwrap();
        assert_eq!((quarto.temperature, quarto.humidity), (Some(268), Some(60)), "the sensor wins over the air conditioner's own reading");
        let sala = w.rooms.iter().find(|r| r.name == "Sala").unwrap();
        assert_eq!(sala.on, vec!["Luz da Sala"]);
        assert_eq!(w.occupied, None);
    }

    #[test]
    fn facts_tie_the_house_together() {
        let mut tv_on = tv("philips:aa", "TV", "Sala");
        tv_on.state = DeviceState { online: true, on: Some(true), ..DeviceState::default() };
        let mut silent = light("ha:light.q", "Luz do Quarto", "Quarto");
        silent.state.online = false;
        let people = vec![Person { name: "Ana".into(), home: false }];
        let rooms = vec!["Sala".to_string()];
        let w = build(14 * 60, Presence::Away, people, vec![vacuum("cleaning", 15), tv_on, silent], Some(&rooms));
        assert_eq!(w.occupied, Some(false));
        let all = w.facts.join(" | ");
        assert!(all.contains("Robô is cleaning Sala."), "{all}");
        assert!(all.contains("TV is on where the vacuum is cleaning."), "{all}");
        assert!(all.contains("Robô's battery is at 15%."), "{all}");
        assert!(all.contains("Nobody is home, and these are on: TV."), "{all}");
        assert!(all.contains("Not answering: Luz do Quarto."), "{all}");
        // On the way home, the lights left on are for the arrival, not a waste.
        let arriving = build(14 * 60, Presence::Arriving { at: 14 * 60 + 25 }, vec![Person { name: "Ana".into(), home: false }], vec![tv("philips:aa", "TV", "Sala")], None);
        assert!(arriving.facts[0].contains("14:25"));
        assert!(!arriving.facts.join(" ").contains("Nobody is home"));
    }

    #[test]
    fn the_model_reads_devices_rooms_and_facts() {
        let w = build(18 * 60 + 5, Presence::Arriving { at: 18 * 60 + 30 }, vec![Person { name: "Ana".into(), home: true }], vec![vacuum("docked", 80), ac("Quarto", false, 275)], None);
        let text = w.for_model();
        assert!(text.starts_with("Now 18:05. The user is on the way home, expected around 18:30. Home Assistant says someone is home."), "{text}");
        assert!(text.contains("- Robô (vacuum, Sala, id ha:vacuum.robo): docked, battery 80%; can clean, clean_area, pause, return_to_base"), "{text}");
        assert!(text.contains("Rooms: Quarto 27.5 °C."), "{text}");
        assert_eq!(clock(1110), "18:30");
        assert_eq!(clock(24 * 60 + 5), "00:05");
        assert_eq!(w.headline(), "You arrive around 18:30");
    }
}
