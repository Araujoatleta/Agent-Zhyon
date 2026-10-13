// Home Assistant, through its official REST API (developers.home-assistant.io/docs/api/rest), with a long-lived
// access token the user creates in their Home Assistant profile (kept in the OS keystore, never on disk).
//
// Only three kinds of entities become devices: media players (TVs and speakers), lights and switches — what Mochi's
// home controls today. What each can do comes from what Home Assistant reports for it (`supported_features`,
// `supported_color_modes`, `source_list`), and an action becomes exactly one service call from a fixed list
// (`service`): Mochi can never call an arbitrary service (a script, a shell command, a restart).
//
// Only an address on the local network is accepted, so the token never travels past the house.

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::{StatusCode, Url};
use serde_json::{json, Value};

use super::model::{self, Action, Capability, Device, DeviceState, Kind};
use crate::i18n::{t, tf};
use crate::net;

const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BODY: usize = 8 * 1024 * 1024;

/// MediaPlayerEntityFeature (homeassistant/components/media_player/const.py).
mod feature {
    pub const VOLUME_SET: u64 = 4;
    pub const VOLUME_MUTE: u64 = 8;
    pub const TURN_ON: u64 = 128;
    pub const TURN_OFF: u64 = 256;
    pub const VOLUME_STEP: u64 = 1024;
    pub const SELECT_SOURCE: u64 = 2048;
    /// The old LightEntityFeature.BRIGHTNESS, for lights that do not list color modes.
    pub const LIGHT_BRIGHTNESS: u64 = 1;
}

/// The domains that become devices.
pub const DOMAINS: &[&str] = &["media_player", "light", "switch"];

/// What `check` learns about the server.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub version: String,
    pub location: String,
}

pub struct HomeAssistant {
    url: Url,
    token: String,
}

/// The address as the user typed it, cleaned up (`http://homeassistant.local:8123`). Local network only.
pub fn clean_url(raw: &str) -> Result<Url, String> {
    let raw = raw.trim();
    let with_port = if !raw.contains("://") && !raw.contains(':') && !raw.is_empty() { format!("{raw}:8123") } else { raw.to_string() };
    let url = net::normalise_server_url(&with_port)?;
    if !url.host_str().is_some_and(net::is_local_host) {
        // A name of the user's own DNS may still point inside the house: `resolves_locally` checks before each use.
        if url.host_str().is_some_and(|h| h.parse::<std::net::IpAddr>().is_ok()) {
            return Err(t("Only devices on your local network can be used."));
        }
    }
    Ok(url)
}

impl HomeAssistant {
    pub fn new(url: &str, token: &str) -> Result<HomeAssistant, String> {
        let token = token.trim();
        if token.is_empty() {
            return Err(t("Enter a long-lived access token from your Home Assistant profile."));
        }
        if token.len() > 1024 || !token.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(t("This access token is not valid."));
        }
        Ok(HomeAssistant { url: clean_url(url)?, token: token.to_string() })
    }

    async fn send(&self, method: reqwest::Method, path: &str, body: Option<&Value>) -> Result<Vec<u8>, String> {
        let host = self.url.host_str().unwrap_or("");
        let port = self.url.port_or_known_default().unwrap_or(8123);
        if !net::resolves_locally(host, port).await {
            return Err(t("Only devices on your local network can be used."));
        }
        let client = reqwest::Client::builder().no_proxy().connect_timeout(Duration::from_secs(4)).timeout(TIMEOUT).build().map_err(|e| e.to_string())?;
        let mut request = client.request(method, net::join(&self.url, path)).bearer_auth(&self.token);
        if let Some(b) = body {
            request = request.json(b);
        }
        let response = request.send().await.map_err(|_| t("Home Assistant does not answer at this address."))?;
        let status = response.status();
        let bytes = net::read_capped(response, MAX_BODY).await?;
        match status {
            s if s.is_success() => Ok(bytes),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Err(t("Home Assistant refused the token. Create a new long-lived access token in your Home Assistant profile."))
            }
            StatusCode::NOT_FOUND => Err(t("Home Assistant does not know this device or service.")),
            s => Err(tf("Home Assistant answered {status}: {message}", &[("status", &s.as_u16().to_string()), ("message", &net::error_detail(&bytes))])),
        }
    }

    async fn get_json(&self, path: &str) -> Result<Value, String> {
        let bytes = self.send(reqwest::Method::GET, path, None).await?;
        serde_json::from_slice(&bytes).map_err(|_| t("Home Assistant's answer could not be read."))
    }

    /// Is the API there, does the token work, which version and which home.
    pub async fn check(&self) -> Result<ServerInfo, String> {
        let api = self.get_json("/api/").await?;
        if api.get("message").and_then(Value::as_str).is_none() {
            return Err(t("This address does not answer like Home Assistant."));
        }
        let config = self.get_json("/api/config").await?;
        let s = |k: &str| config.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        Ok(ServerInfo { version: s("version"), location: s("location_name") })
    }

    /// Every entity of the domains that become devices.
    pub async fn states(&self) -> Result<Vec<Value>, String> {
        let all = self.get_json("/api/states").await?;
        Ok(all
            .as_array()
            .map(|list| list.iter().filter(|s| entity_domain(s).is_some_and(|d| DOMAINS.contains(&d))).cloned().collect())
            .unwrap_or_default())
    }

    pub async fn state(&self, entity_id: &str) -> Result<Value, String> {
        self.get_json(&format!("/api/states/{entity_id}")).await
    }

    /// The area of each entity, through the template API. Best effort: it needs an administrator's token, and without
    /// it the rooms are only the ones set in Settings.
    pub async fn areas(&self) -> BTreeMap<String, String> {
        let template = "{% for d in ['media_player', 'light', 'switch'] %}{% for s in states[d] %}{{ s.entity_id }}\t{{ area_name(s.entity_id) or '' }}\n{% endfor %}{% endfor %}";
        let Ok(bytes) = self.send(reqwest::Method::POST, "/api/template", Some(&json!({ "template": template }))).await else {
            return BTreeMap::new();
        };
        parse_areas(&String::from_utf8_lossy(&bytes))
    }

    /// One service call of the fixed list.
    pub async fn call(&self, domain: &str, service: &str, data: &Value) -> Result<(), String> {
        if !allowed(domain, service) {
            return Err(format!("{domain}.{service} is not a service Mochi may call."));
        }
        self.send(reqwest::Method::POST, &format!("/api/services/{domain}/{service}"), Some(data)).await.map(|_| ())
    }
}

/// `entity\tarea` lines.
pub fn parse_areas(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (entity, area) = line.split_once('\t')?;
            let (entity, area) = (entity.trim(), area.trim());
            (!entity.is_empty() && !area.is_empty() && area != "None").then(|| (entity.to_string(), area.to_string()))
        })
        .collect()
}

fn entity_domain(state: &Value) -> Option<&str> {
    state.get("entity_id").and_then(Value::as_str)?.split_once('.').map(|(d, _)| d)
}

/// The only service calls Mochi can make, whatever the model asks.
const ALLOWED: &[(&str, &str)] = &[
    ("media_player", "turn_on"),
    ("media_player", "turn_off"),
    ("media_player", "volume_set"),
    ("media_player", "volume_up"),
    ("media_player", "volume_down"),
    ("media_player", "volume_mute"),
    ("media_player", "select_source"),
    ("light", "turn_on"),
    ("light", "turn_off"),
    ("switch", "turn_on"),
    ("switch", "turn_off"),
];

pub fn allowed(domain: &str, service: &str) -> bool {
    ALLOWED.contains(&(domain, service))
}

/// How the user set a device up in Settings (none: the defaults).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prefs {
    pub name: String,
    pub room: String,
    pub exposed: bool,
    pub ask: Option<bool>,
}

/// A device from one entity's state, with what it reports it can do. None for a domain that is not a device here.
pub fn device(state: &Value, prefs: &Prefs, area: Option<&str>) -> Option<Device> {
    let entity_id = state.get("entity_id").and_then(Value::as_str)?;
    let domain = entity_domain(state)?;
    let attrs = state.get("attributes").cloned().unwrap_or_else(|| json!({}));
    let raw_state = state.get("state").and_then(Value::as_str).unwrap_or("unknown");
    let features = attrs.get("supported_features").and_then(Value::as_u64).unwrap_or(0);
    let has = |bit: u64| features & bit == bit;
    let online = !matches!(raw_state, "unavailable" | "unknown");
    let on = match raw_state {
        "off" | "standby" => Some(false),
        "unavailable" | "unknown" => None,
        _ => Some(true),
    };
    let mut caps = Vec::new();
    let mut apps = Vec::new();
    let mut state_out = DeviceState { online, on, ..DeviceState::default() };
    let kind = match domain {
        "media_player" => {
            for (bit, cap) in [
                (feature::TURN_ON, Capability::TurnOn),
                (feature::TURN_OFF, Capability::TurnOff),
                (feature::VOLUME_SET, Capability::VolumeSet),
                (feature::VOLUME_STEP, Capability::VolumeStep),
                (feature::VOLUME_MUTE, Capability::Mute),
            ] {
                if has(bit) {
                    caps.push(cap);
                }
            }
            if has(feature::SELECT_SOURCE) {
                apps = attrs
                    .get("source_list")
                    .and_then(Value::as_array)
                    .map(|l| l.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                if !apps.is_empty() {
                    caps.push(Capability::Apps);
                }
            }
            state_out.volume = attrs.get("volume_level").and_then(Value::as_f64).map(|v| (v * 100.0).round().clamp(0.0, 100.0) as u8);
            state_out.muted = attrs.get("is_volume_muted").and_then(Value::as_bool);
            state_out.app = attrs.get("source").and_then(Value::as_str).map(str::to_string);
            if attrs.get("device_class").and_then(Value::as_str) == Some("tv") { Kind::Tv } else { Kind::Speaker }
        }
        "light" => {
            caps.extend([Capability::TurnOn, Capability::TurnOff]);
            let modes: Vec<&str> = attrs.get("supported_color_modes").and_then(Value::as_array).map(|m| m.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let dims = if modes.is_empty() { has(feature::LIGHT_BRIGHTNESS) } else { modes.iter().any(|m| *m != "onoff") };
            if dims {
                caps.push(Capability::Brightness);
            }
            state_out.brightness = attrs.get("brightness").and_then(Value::as_f64).map(|b| (b / 255.0 * 100.0).round().clamp(0.0, 100.0) as u8);
            Kind::Light
        }
        "switch" => {
            caps.extend([Capability::TurnOn, Capability::TurnOff]);
            if attrs.get("device_class").and_then(Value::as_str) == Some("outlet") { Kind::Plug } else { Kind::Switch }
        }
        _ => return None,
    };
    caps.sort();
    let source_name = attrs.get("friendly_name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty()).unwrap_or(entity_id);
    Some(Device {
        id: format!("ha:{entity_id}"),
        name: if prefs.name.trim().is_empty() { source_name.to_string() } else { prefs.name.trim().to_string() },
        room: if prefs.room.trim().is_empty() { area.unwrap_or("").to_string() } else { prefs.room.trim().to_string() },
        kind,
        source: "Home Assistant".into(),
        model: None,
        caps,
        apps,
        channels: Vec::new(),
        state: state_out,
        exposed: prefs.exposed,
        ask: prefs.ask.unwrap_or(kind.sensitive()),
    })
}

/// The one service call that carries `action` out on `device` (already checked against it by model::check).
pub fn service(device: &Device, action: &Action) -> Result<(String, &'static str, Value), String> {
    let entity_id = device.id.strip_prefix("ha:").ok_or("not a Home Assistant device")?;
    let domain = entity_id.split_once('.').map(|(d, _)| d).ok_or("not an entity id")?.to_string();
    let base = json!({ "entity_id": entity_id });
    let with = |extra: Value| {
        let mut data = base.clone();
        if let (Some(d), Some(e)) = (data.as_object_mut(), extra.as_object()) {
            d.extend(e.clone());
        }
        data
    };
    let (service, data) = match (domain.as_str(), action) {
        (_, Action::TurnOn) => ("turn_on", base.clone()),
        (_, Action::TurnOff) => ("turn_off", base.clone()),
        ("media_player", Action::SetVolume(p)) => ("volume_set", with(json!({ "volume_level": f64::from(*p) / 100.0 }))),
        ("media_player", Action::VolumeUp) => ("volume_up", base.clone()),
        ("media_player", Action::VolumeDown) => ("volume_down", base.clone()),
        ("media_player", Action::Mute(m)) => ("volume_mute", with(json!({ "is_volume_muted": m }))),
        ("media_player", Action::OpenApp(app)) => ("select_source", with(json!({ "source": model::resolve_app(device, app)? }))),
        ("light", Action::SetBrightness(p)) => ("turn_on", with(json!({ "brightness_pct": p }))),
        _ => return Err(format!("{} cannot {} through Home Assistant.", device.name, action.describe())),
    };
    if !allowed(&domain, service) {
        return Err(format!("{domain}.{service} is not a service Mochi may call."));
    }
    Ok((domain, service, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exposed() -> Prefs {
        Prefs { exposed: true, ..Prefs::default() }
    }

    #[test]
    fn a_tv_reports_what_it_can_do_through_its_features() {
        let state = json!({
            "entity_id": "media_player.tv_sala",
            "state": "on",
            "attributes": {
                "friendly_name": "TV da Sala",
                "device_class": "tv",
                // TURN_ON | TURN_OFF | VOLUME_SET | VOLUME_MUTE | VOLUME_STEP | SELECT_SOURCE | PLAY_MEDIA
                "supported_features": 128 | 256 | 4 | 8 | 1024 | 2048 | 512,
                "source_list": ["Watch TV", "HDMI 1", "YouTube"],
                "source": "YouTube",
                "volume_level": 0.25,
                "is_volume_muted": false,
            },
        });
        let d = device(&state, &exposed(), Some("Sala")).unwrap();
        assert_eq!(d.id, "ha:media_player.tv_sala");
        assert_eq!((d.name.as_str(), d.room.as_str(), d.kind), ("TV da Sala", "Sala", Kind::Tv));
        assert_eq!(d.caps, vec![Capability::TurnOn, Capability::TurnOff, Capability::VolumeSet, Capability::VolumeStep, Capability::Mute, Capability::Apps]);
        assert_eq!(d.state, DeviceState { online: true, on: Some(true), volume: Some(25), muted: Some(false), app: Some("YouTube".into()), ..DeviceState::default() });
        assert!(!d.ask, "a TV does not ask by default");
        // A speaker that only plays: no power, no apps.
        let speaker = device(&json!({"entity_id":"media_player.cozinha","state":"idle","attributes":{"supported_features":4}}), &exposed(), None).unwrap();
        assert_eq!((speaker.kind, speaker.caps.clone()), (Kind::Speaker, vec![Capability::VolumeSet]));
        assert_eq!(speaker.name, "media_player.cozinha");
    }

    #[test]
    fn lights_dim_only_when_a_color_mode_says_so_and_plugs_ask() {
        let dimmable = device(&json!({"entity_id":"light.teto","state":"on","attributes":{"supported_color_modes":["brightness"],"brightness":128}}), &exposed(), None).unwrap();
        assert!(dimmable.can(Capability::Brightness));
        assert_eq!(dimmable.state.brightness, Some(50));
        let plain = device(&json!({"entity_id":"light.abajur","state":"off","attributes":{"supported_color_modes":["onoff"]}}), &exposed(), None).unwrap();
        assert!(!plain.can(Capability::Brightness));
        assert_eq!(plain.state.on, Some(false));
        let plug = device(&json!({"entity_id":"switch.cafeteira","state":"unavailable","attributes":{"device_class":"outlet"}}), &Prefs::default(), None).unwrap();
        assert_eq!(plug.kind, Kind::Plug);
        assert!(plug.ask, "a plug asks by default");
        assert!(!plug.exposed, "nothing is Mochi's until the user ticks it");
        assert!(!plug.state.online);
        let overridden = device(&json!({"entity_id":"switch.cafeteira","state":"on"}), &Prefs { name: "Café".into(), room: "Cozinha".into(), exposed: true, ask: Some(false) }, Some("Area")).unwrap();
        assert_eq!((overridden.name.as_str(), overridden.room.as_str(), overridden.ask), ("Café", "Cozinha", false));
        assert!(device(&json!({"entity_id":"sensor.temperature","state":"21"}), &exposed(), None).is_none());
        assert!(device(&json!({"entity_id":"script.reboot","state":"off"}), &exposed(), None).is_none());
    }

    #[test]
    fn each_action_is_one_service_of_the_fixed_list() {
        let tv = device(
            &json!({"entity_id":"media_player.tv","state":"on","attributes":{"device_class":"tv","supported_features":128|256|4|8|1024|2048,"source_list":["YouTube","Netflix"]}}),
            &exposed(),
            None,
        )
        .unwrap();
        let s = |a: Action| service(&tv, &a).map(|(d, s, data)| (format!("{d}.{s}"), data));
        assert_eq!(s(Action::TurnOn).unwrap(), ("media_player.turn_on".into(), json!({"entity_id":"media_player.tv"})));
        assert_eq!(s(Action::SetVolume(30)).unwrap().1, json!({"entity_id":"media_player.tv","volume_level":0.3}));
        assert_eq!(s(Action::Mute(true)).unwrap().1["is_volume_muted"], true);
        assert_eq!(s(Action::OpenApp("youtube".into())).unwrap(), ("media_player.select_source".into(), json!({"entity_id":"media_player.tv","source":"YouTube"})));
        assert!(s(Action::OpenApp("Spotify".into())).is_err());
        assert!(s(Action::SetBrightness(40)).is_err());
        assert!(s(Action::SetChannel("5".into())).is_err(), "channels are not offered through Home Assistant");
        let lamp = device(&json!({"entity_id":"light.teto","state":"on","attributes":{"supported_color_modes":["hs"]}}), &exposed(), None).unwrap();
        assert_eq!(service(&lamp, &Action::SetBrightness(40)).unwrap().2, json!({"entity_id":"light.teto","brightness_pct":40}));
        // Whatever comes in, only the list goes out.
        assert!(allowed("light", "turn_on"));
        assert!(!allowed("homeassistant", "restart") && !allowed("shell_command", "anything") && !allowed("script", "turn_on"));
    }

    #[test]
    fn areas_come_from_the_template_lines() {
        let areas = parse_areas("media_player.tv\tSala\nlight.teto\t\nswitch.x\tNone\n\nlight.cozinha\tCozinha\n");
        assert_eq!(areas.len(), 2);
        assert_eq!(areas["media_player.tv"], "Sala");
        assert_eq!(areas["light.cozinha"], "Cozinha");
    }

    #[test]
    fn the_address_is_cleaned_and_kept_on_the_local_network() {
        assert_eq!(clean_url("homeassistant.local").unwrap().as_str(), "http://homeassistant.local:8123/");
        assert_eq!(clean_url("http://192.168.1.10:8123/").unwrap().as_str(), "http://192.168.1.10:8123/");
        assert_eq!(clean_url("https://ha.lan/api").unwrap().as_str(), "https://ha.lan/");
        assert!(clean_url("http://8.8.8.8:8123").is_err());
        assert!(HomeAssistant::new("http://192.168.1.10:8123", "  ").is_err());
        assert!(HomeAssistant::new("http://192.168.1.10:8123", "two words").is_err());
    }

    fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    #[test]
    fn a_service_call_goes_out_with_the_token_and_the_data() {
        use crate::smart_home::philips::tests::fake_tv;
        let (addr, seen) = fake_tv(|e| {
            Some(match e.path.as_str() {
                "/api/" => (200, r#"{"message":"API running."}"#.into()),
                "/api/config" => (200, r#"{"version":"2026.10.1","location_name":"Casa"}"#.into()),
                "/api/services/media_player/turn_on" => (200, "[]".into()),
                _ => (404, r#"{"message":"Entity not found."}"#.into()),
            })
        });
        let ha = HomeAssistant::new(&format!("http://{addr}"), "tok.en").unwrap();
        assert_eq!(block_on(ha.check()).unwrap(), ServerInfo { version: "2026.10.1".into(), location: "Casa".into() });
        block_on(ha.call("media_player", "turn_on", &json!({"entity_id":"media_player.tv"}))).unwrap();
        assert!(block_on(ha.call("homeassistant", "restart", &json!({}))).is_err());
        assert!(block_on(ha.state("media_player.nope")).unwrap_err().contains("does not know"));
        let seen = seen.lock().unwrap();
        let call = seen.iter().find(|e| e.path == "/api/services/media_player/turn_on").unwrap();
        assert_eq!(call.method, "POST");
        assert_eq!(call.authorization.as_deref(), Some("Bearer tok.en"));
        assert_eq!(serde_json::from_str::<Value>(&call.body).unwrap(), json!({"entity_id":"media_player.tv"}));
        assert!(seen.iter().all(|e| e.path != "/api/services/homeassistant/restart"), "a service off the list never leaves");
    }
}
