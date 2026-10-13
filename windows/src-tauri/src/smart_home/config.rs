// What Mochi's home keeps: the TVs added by their address and the user's rules for each device (in settings.json, no
// secret there), and the credentials (in the OS keystore: Windows Credential Manager, the Secret Service on Linux).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::philips::{self, Credentials, Endpoint};
use super::samsung;
use crate::secrets;

/// Keystore entries (secrets.rs, internal: no webview can read them).
pub const HA_TOKEN_KEY: &str = "home-assistant-token";
/// The TVs' pairings, one JSON object by TV id: a Philips TV's {"user": device id, "key": auth key}, a Samsung TV's
/// {"user": "samsung", "key": its token, or "-" when it allowed Coucou without giving one}. (The name is older than the
/// Samsung TVs, and stays: stored keys are found by it.)
pub const PHILIPS_KEY: &str = "home-philips";
/// `TvConf::brand` of a Samsung TV ("" is a Philips TV, as every TV was before).
pub const SAMSUNG: &str = "samsung";

/// A TV added in Settings → Mochi's home (Philips or Samsung), as its probe found it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TvConf {
    /// `philips:` or `samsung:` and 8 hex digits, made when it is added.
    pub id: String,
    /// "" (Philips) or "samsung".
    pub brand: String,
    pub host: String,
    /// Philips: the JointSpace API (1, 5, 6). Samsung: 2.
    pub api: u8,
    /// Philips: https on 1926. Samsung: the remote wants a token, over wss on 8002.
    pub https: bool,
    /// The SHA-256 of its certificate, hex ("" over http, or not met yet).
    pub pin: String,
    /// For waking it from deep standby; "" unknown.
    pub mac: String,
    /// As the TV calls itself.
    pub name: String,
    /// From its UPnP description (or API 1), or a Samsung TV's own; "" unknown.
    pub model: String,
    pub os: String,
    /// "6.4.0" (JointSpace) or "2.0.25" (Samsung's API).
    pub version: String,
    pub features: BTreeMap<String, Vec<String>>,
    /// Commands need pairing first (a PIN on a Philips TV, Allow on a Samsung TV's screen).
    pub needs_pairing: bool,
    /// A Samsung Frame TV: its power key is held to turn it off.
    pub frame: bool,
    /// The TV's UPnP RenderingControl (volume and mute as numbers), found when it was added; "" none.
    pub renderer: String,
    /// A Samsung TV's apps as it last listed them (when it was paired, then whenever it is on): kept, so that Mochi
    /// can open one on a TV he has just turned on.
    pub apps: Vec<samsung::App>,
}

impl TvConf {
    pub fn samsung(&self) -> bool {
        self.brand == SAMSUNG
    }

    pub fn samsung_tv(&self) -> samsung::Tv {
        samsung::Tv::new(&self.host, self.https, pin_bytes(&self.pin))
    }

    pub fn endpoint(&self) -> Endpoint {
        Endpoint::new(&self.host, self.api.clamp(1, 6), self.https, pin_bytes(&self.pin))
    }

    pub fn system(&self) -> philips::SystemInfo {
        let v: Vec<u8> = self.version.split('.').map(|p| p.parse().unwrap_or(0)).collect();
        philips::SystemInfo {
            name: self.name.clone(),
            version: (v.first().copied().unwrap_or(self.api), v.get(1).copied().unwrap_or(0), v.get(2).copied().unwrap_or(0)),
            os_type: (!self.os.is_empty()).then(|| self.os.clone()),
            model: (!self.model.is_empty()).then(|| self.model.clone()),
            features: self.features.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            needs_pairing: self.needs_pairing,
            secured: self.https,
        }
    }

    pub fn mac(&self) -> Option<[u8; 6]> {
        philips::parse_mac(&self.mac)
    }
}

pub fn pin_hex(pin: &[u8; 32]) -> String {
    pin.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn pin_bytes(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
    }
    Some(out)
}

/// The user's rules for one device (Settings → Mochi's home), by device id. Empty means the device's own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DevicePrefs {
    pub name: String,
    pub room: String,
    /// Mochi may use it. None: the default (yes for a TV added here, no for what comes from Home Assistant).
    pub exposed: Option<bool>,
    /// Ask before acting. None: the default (plugs and switches ask).
    pub ask: Option<bool>,
}

// ── The keystore ──────────────────────────────────────────────────────────────

pub fn ha_token() -> Option<String> {
    secrets::get_internal(HA_TOKEN_KEY)
}

pub fn set_ha_token(token: &str) -> Result<(), String> {
    secrets::set_internal(HA_TOKEN_KEY, token.trim())
}

fn all_credentials() -> serde_json::Map<String, serde_json::Value> {
    secrets::get_internal(PHILIPS_KEY)
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

pub fn credentials(tv: &str) -> Option<Credentials> {
    let all = all_credentials();
    let c = all.get(tv)?;
    let s = |k: &str| c.get(k).and_then(|v| v.as_str()).map(str::to_string).filter(|v| !v.is_empty());
    Some(Credentials { user: s("user")?, key: s("key")? })
}

/// A Samsung TV's token ("-": allowed without one). None: not paired.
pub fn samsung_token(tv: &str) -> Option<String> {
    credentials(tv).filter(|c| c.user == SAMSUNG).map(|c| c.key)
}

pub fn set_samsung_token(tv: &str, token: Option<&str>) -> Result<(), String> {
    let key = token.map(str::trim).filter(|t| !t.is_empty()).unwrap_or("-");
    set_credentials(tv, Some(&Credentials { user: SAMSUNG.into(), key: key.into() }))
}

/// Stores (Some) or forgets (None) a TV's pairing.
pub fn set_credentials(tv: &str, creds: Option<&Credentials>) -> Result<(), String> {
    let mut all = all_credentials();
    match creds {
        Some(c) => {
            all.insert(tv.to_string(), serde_json::json!({ "user": c.user, "key": c.key }));
        }
        None => {
            all.remove(tv);
        }
    }
    let text = if all.is_empty() { String::new() } else { serde_json::Value::Object(all).to_string() };
    secrets::set_internal(PHILIPS_KEY, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tv_conf_gives_its_endpoint_and_its_description_back() {
        let pin = [7u8; 32];
        let conf = TvConf {
            id: "philips:0a1b2c3d".into(),
            host: "192.168.1.20".into(),
            api: 6,
            https: true,
            pin: pin_hex(&pin),
            mac: "a4:b1:c2:d3:e4:f5".into(),
            name: "Philips TV".into(),
            version: "6.4.0".into(),
            features: [("inputkey".to_string(), vec!["key".to_string()])].into_iter().collect(),
            needs_pairing: true,
            ..TvConf::default()
        };
        let e = conf.endpoint();
        assert_eq!((e.port, e.api, e.https, e.pin), (philips::HTTPS_PORT, 6, true, Some(pin)));
        let s = conf.system();
        assert_eq!(s.version, (6, 4, 0));
        assert!(s.keys() && s.needs_pairing);
        assert!(conf.mac().is_some());
        assert_eq!(pin_bytes("zz"), None);
        assert_eq!(pin_bytes(&"g".repeat(64)), None);
    }
}
