// Philips TVs, straight over the local network: the JointSpace API that Philips TVs serve on port 1925 (http) and,
// on the Android TVs since 2016, 1926 (https, paired with a PIN). Written from the protocol as the ha-philipsjs
// library and Home Assistant's Philips TV integration use it (github.com/danielperna84/ha-philipsjs), and checked
// against a simulated TV below. Nothing is assumed from the brand: `probe` asks the TV which API version it speaks
// and which features it has, and `Tv::capabilities` only offers what the TV answered for.
//
//   * Probe: GET /{api}/system over https:1926, then http:1925, for API 6, 5 and 1. It names the TV, its API version,
//     its OS and its features, and says whether it must be paired (systemfeatures.pairing_type).
//   * Pairing (API 6): pair/request shows a PIN on the TV; pair/grant sends it back signed (HMAC-SHA1 with the shared
//     key every JointSpace client uses) under HTTP Digest. The TV then knows this computer by a random device id and
//     an auth key, kept in the OS keystore.
//   * The TV's certificate is self-signed: it is pinned on first contact (its SHA-256) and must stay the same.
//   * Only addresses on the local network are ever contacted.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::{Method, StatusCode, Url};
use ring::rand::{SecureRandom, SystemRandom};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{ring as provider, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde_json::{json, Map, Value};

use super::digest::{self, Challenge};
use super::model::Channel;
use crate::i18n::{t, tf};
use crate::net;

pub const HTTP_PORT: u16 = 1925;
pub const HTTPS_PORT: u16 = 1926;
/// The key every JointSpace client signs the pairing PIN with (ha-philipsjs `AUTH_SHARED_KEY`, base64
/// "ZmVay1EQVFOaZhwQ4Kv81ypLAZNczV9sG4KkseXWn1NEk6cXmPKO/MCa9sryslvLCFMnNe4Z4CPXzToowvhHvA==").
const SHARED_KEY: [u8; 64] = [
    0x66, 0x65, 0x5a, 0xcb, 0x51, 0x10, 0x54, 0x53, 0x9a, 0x66, 0x1c, 0x10, 0xe0, 0xab, 0xfc, 0xd7, 0x2a, 0x4b, 0x01, 0x93,
    0x5c, 0xcd, 0x5f, 0x6c, 0x1b, 0x82, 0xa4, 0xb1, 0xe5, 0xd6, 0x9f, 0x53, 0x44, 0x93, 0xa7, 0x17, 0x98, 0xf2, 0x8e, 0xfc,
    0xc0, 0x9a, 0xf6, 0xca, 0xf2, 0xb2, 0x5b, 0xcb, 0x08, 0x53, 0x27, 0x35, 0xee, 0x19, 0xe0, 0x23, 0xd7, 0xcd, 0x3a, 0x28,
    0xc2, 0xf8, 0x47, 0xbc,
];
const TIMEOUT: Duration = Duration::from_secs(8);
/// A channel list or the apps can be long; nothing a TV sends is read past this.
const MAX_BODY: usize = 4 * 1024 * 1024;
/// What answers a POST on some firmwares instead of JSON (ha-philipsjs `IGNORED_JSON_RESPONSES`).
const EMPTY_ANSWERS: &[&str] = &["", "}", "Context Service not started", "<html><head><title>Ok</title></head><body>Ok</body></html>"];

/// How to reach a TV.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    /// The JointSpace API version in the paths (1, 5 or 6).
    pub api: u8,
    pub https: bool,
    /// The SHA-256 of its certificate, once seen (https only).
    pub pin: Option<[u8; 32]>,
}

impl Endpoint {
    /// The TV at `host`, on the port JointSpace uses for this transport.
    pub fn new(host: &str, api: u8, https: bool, pin: Option<[u8; 32]>) -> Endpoint {
        Endpoint { host: host.to_string(), port: if https { HTTPS_PORT } else { HTTP_PORT }, api, https, pin }
    }

    fn url(&self, path: &str) -> Result<Url, String> {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        let scheme = if self.https { "https" } else { "http" };
        Url::parse(&format!("{scheme}://{host}:{}/{}/{}", self.port, self.api, path.trim_start_matches('/')))
            .map_err(|_| tf("Not a valid address: {address}", &[("address", &self.host)]))
    }
}

/// What pairing gave this computer: the device id it is known by, and its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub user: String,
    pub key: String,
}

/// Why a request to the TV failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TvError {
    /// Nothing answers at that address (off, asleep without network, another address).
    Unreachable(String),
    /// The TV refused the credentials, or it needs pairing.
    Auth,
    /// The TV does not have this endpoint (403 or 404).
    NotSupported,
    /// The TV's certificate is not the one pinned.
    CertificateChanged,
    Other(String),
}

impl TvError {
    /// For the user (Settings) and, in English words, the model gets the same through `describe`.
    pub fn message(&self) -> String {
        match self {
            TvError::Unreachable(_) => t("The TV does not answer. It may be off with its network asleep, or at another address."),
            TvError::Auth => t("The TV refused this computer: pair it again."),
            TvError::NotSupported => t("This TV does not offer that."),
            TvError::CertificateChanged => t("The TV's certificate changed since it was paired: check that it is the same TV, then pair it again."),
            TvError::Other(why) => why.clone(),
        }
    }
}

// ── TLS: the TV's own certificate, pinned ──────────────────────────────────────

/// Accepts the TV's self-signed certificate the first time (recording its SHA-256), then only that one. The handshake
/// signature is still checked, so only the holder of the certificate's key can answer.
#[derive(Debug)]
struct PinnedCertificate {
    expected: Option<[u8; 32]>,
    seen: Arc<Mutex<Option<[u8; 32]>>>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for PinnedCertificate {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let mut fingerprint = [0u8; 32];
        fingerprint.copy_from_slice(ring::digest::digest(&ring::digest::SHA256, end_entity.as_ref()).as_ref());
        *self.seen.lock().unwrap() = Some(fingerprint);
        match self.expected {
            Some(pin) if pin != fingerprint => Err(rustls::Error::General("certificate changed".into())),
            _ => Ok(ServerCertVerified::assertion()),
        }
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// Where the certificate a connection met is written (its SHA-256), for pinning it.
pub type Seen = Arc<Mutex<Option<[u8; 32]>>>;

/// A TLS client configuration that takes the TV's own certificate the first time and then only that one
/// (PinnedCertificate); Samsung TVs are reached the same way (samsung.rs).
pub fn pinned_tls(expected: Option<[u8; 32]>) -> Result<(rustls::ClientConfig, Seen), String> {
    let seen: Seen = Arc::new(Mutex::new(None));
    let provider = Arc::new(provider::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedCertificate { expected, seen: seen.clone(), provider }))
        .with_no_client_auth();
    Ok((config, seen))
}

/// A client for one endpoint, and where the certificate it met is written.
fn client(endpoint: &Endpoint) -> Result<(reqwest::Client, Seen), String> {
    let builder = reqwest::Client::builder().no_proxy().connect_timeout(Duration::from_secs(4)).timeout(TIMEOUT);
    let (builder, seen) = if endpoint.https {
        let (config, seen) = pinned_tls(endpoint.pin)?;
        (builder.use_preconfigured_tls(config), seen)
    } else {
        (builder, Arc::new(Mutex::new(None)))
    };
    Ok((builder.build().map_err(|e| e.to_string())?, seen))
}

/// A TV's answer as JSON: firmware quirks mended (",,", "{,", ",}") and the non-JSON acknowledgements read as `{}`.
pub fn parse_body(text: &str) -> Option<Value> {
    let text = text.trim();
    if EMPTY_ANSWERS.contains(&text) {
        return Some(json!({}));
    }
    if let Ok(v) = serde_json::from_str(text) {
        return Some(v);
    }
    let mut fixed = text.replace("{,", "{").replace(",}", "}");
    while fixed.contains(",,") {
        fixed = fixed.replace(",,", ",");
    }
    serde_json::from_str(&fixed).ok()
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    let _ = SystemRandom::new().fill(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// One request: unauthenticated first; on a Digest challenge, once more with the answer. The certificate met (https)
/// comes back with the result, for pinning.
async fn request(endpoint: &Endpoint, creds: Option<&Credentials>, method: Method, path: &str, body: Option<&Value>) -> (Result<Value, TvError>, Option<[u8; 32]>) {
    let url = match endpoint.url(path) {
        Ok(u) => u,
        Err(e) => return (Err(TvError::Other(e)), None),
    };
    // Checked before every request, as when the TV was added: a name must still point inside the house.
    if !net::resolves_locally(&endpoint.host, endpoint.port).await {
        return (Err(TvError::Other(t("Only devices on your local network can be used."))), None);
    }
    let (client, seen) = match client(endpoint) {
        Ok(c) => c,
        Err(e) => return (Err(TvError::Other(e)), None),
    };
    let send = |authorization: Option<String>| {
        let mut r = client.request(method.clone(), url.clone()).header("Accept", "application/json");
        if let Some(a) = authorization {
            r = r.header("Authorization", a);
        }
        if let Some(b) = body {
            r = r.json(b);
        }
        r.send()
    };
    let result = async {
        let mut response = send(None).await.map_err(|e| unreachable_or(&e))?;
        if response.status() == StatusCode::UNAUTHORIZED {
            let challenge = response
                .headers()
                .get_all("www-authenticate")
                .iter()
                .filter_map(|h| h.to_str().ok())
                .find_map(Challenge::parse);
            let (Some(challenge), Some(creds)) = (challenge, creds) else { return Err(TvError::Auth) };
            let uri = url.path().to_string();
            let answer = challenge.answer(&creds.user, &creds.key, method.as_str(), &uri, &random_hex(8), 1);
            response = send(Some(answer)).await.map_err(|e| unreachable_or(&e))?;
        }
        match response.status() {
            StatusCode::UNAUTHORIZED => Err(TvError::Auth),
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => Err(TvError::NotSupported),
            s if s.is_success() => {
                let bytes = net::read_capped(response, MAX_BODY).await.map_err(TvError::Other)?;
                parse_body(&String::from_utf8_lossy(&bytes)).ok_or_else(|| TvError::Other(t("The TV's answer could not be read.")))
            }
            s => Err(TvError::Other(tf("The TV answered {status}.", &[("status", &s.as_u16().to_string())]))),
        }
    }
    .await;
    let met = *seen.lock().unwrap();
    let result = match result {
        // A TLS failure after a pin is set, with a different certificate seen: say so rather than "unreachable".
        Err(TvError::Unreachable(_)) if endpoint.pin.is_some() && met.is_some() && met != endpoint.pin => Err(TvError::CertificateChanged),
        other => other,
    };
    (result, met)
}

fn unreachable_or(e: &reqwest::Error) -> TvError {
    TvError::Unreachable(e.to_string().chars().take(200).collect())
}

// ── What a TV says about itself ───────────────────────────────────────────────

/// GET /{api}/system, read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemInfo {
    pub name: String,
    /// Major, minor, patch.
    pub version: (u8, u8, u8),
    /// "MSAF_2019_P" (Android TV), "Linux", a Saphi version…
    pub os_type: Option<String>,
    /// Only older TVs send it in clear (API 1); newer ones encrypt it.
    pub model: Option<String>,
    /// featuring.jsonfeatures: each feature and its values ("applications": ["TV_Apps", …]).
    pub features: Vec<(String, Vec<String>)>,
    /// systemfeatures.pairing_type is digest_auth_pairing: commands need pairing first.
    pub needs_pairing: bool,
    /// systemfeatures.secured_transport.
    pub secured: bool,
}

impl SystemInfo {
    pub fn from_json(v: &Value, api_asked: u8) -> SystemInfo {
        let s = |key: &str| v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        let num = |key: &str| v.pointer(&format!("/api_version/{key}")).and_then(Value::as_u64).map(|n| n.min(255) as u8);
        let version = match num("Major") {
            Some(major) => (major, num("Minor").unwrap_or(0), num("Patch").unwrap_or(0)),
            // API 1 TVs do not say.
            None => (api_asked, 0, 0),
        };
        let system_features = v.pointer("/featuring/systemfeatures");
        let feature = |key: &str| system_features.and_then(|f| f.get(key));
        let features = v
            .pointer("/featuring/jsonfeatures")
            .and_then(Value::as_object)
            .map(|o| {
                o.iter()
                    .map(|(k, vals)| (k.clone(), vals.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()))
                    .collect()
            })
            .unwrap_or_default();
        SystemInfo {
            name: s("name").unwrap_or_else(|| "Philips TV".into()),
            version,
            os_type: s("os_type").or_else(|| feature("os_type").and_then(Value::as_str).map(str::to_string)),
            model: s("model"),
            features,
            needs_pairing: feature("pairing_type").and_then(Value::as_str) == Some("digest_auth_pairing"),
            secured: feature("secured_transport").is_some_and(|x| x == "true" || x == &Value::Bool(true)),
        }
    }

    /// The values of a JSON feature (empty when the TV does not list it).
    pub fn feature(&self, name: &str) -> &[String] {
        self.features.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_slice()).unwrap_or(&[])
    }

    /// Android TVs: os_type MSAF_…
    pub fn android(&self) -> bool {
        self.os_type.as_deref().is_some_and(|o| o.starts_with("MSAF_"))
    }

    /// Remote keys (input/key): listed in the features from API 5 on; API 1 has them by its specification.
    pub fn keys(&self) -> bool {
        self.version.0 < 5 || self.feature("inputkey").iter().any(|k| k == "key")
    }

    /// Apps can be listed and opened: the TV lists `applications` and launches `intent`s.
    pub fn apps(&self) -> bool {
        !self.feature("applications").is_empty() && self.feature("activities").iter().any(|a| a == "intent")
    }
}

/// What `probe` found at an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub endpoint: Endpoint,
    pub system: SystemInfo,
}

/// The order addresses are tried in: what newer TVs need first.
fn attempts(host: &str) -> Vec<Endpoint> {
    [(6, true), (6, false), (5, false), (1, false)].into_iter().map(|(api, https)| Endpoint::new(host, api, https, None)).collect()
}

/// Asks the TV at `host` which API it speaks: GET /{api}/system over https then http. The first answer that is a
/// system description wins; its certificate is pinned from then on.
pub async fn probe(host: &str) -> Result<Probe, String> {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']').to_string();
    if host.is_empty() {
        return Err(t("Enter the TV's address first."));
    }
    if !net::resolves_locally(&host, HTTP_PORT).await {
        return Err(t("Only devices on your local network can be used."));
    }
    probe_endpoints(attempts(&host)).await
}

/// `probe`, over the given endpoints in order.
pub async fn probe_endpoints(endpoints: Vec<Endpoint>) -> Result<Probe, String> {
    let mut last = TvError::Unreachable(String::new());
    for mut endpoint in endpoints {
        let (result, seen) = request(&endpoint, None, Method::GET, "system", None).await;
        match result {
            Ok(v) if v.get("name").is_some() || v.get("api_version").is_some() => {
                let system = SystemInfo::from_json(&v, endpoint.api);
                // Talk on in the API the TV says it has (some answer /6/ but are 5, or /1/ but are 6).
                if (1..=6).contains(&system.version.0) {
                    endpoint.api = system.version.0;
                }
                endpoint.pin = seen.filter(|_| endpoint.https);
                return Ok(Probe { endpoint, system });
            }
            Ok(_) => {}
            Err(e) => last = e,
        }
    }
    Err(match last {
        TvError::Unreachable(_) => t("No Philips TV answers at this address (ports 1925 and 1926). Check that the TV is on, on the same network, and that this is its address."),
        other => other.message(),
    })
}

// ── Pairing (API 6) ───────────────────────────────────────────────────────────

/// A pairing in progress: the TV shows a PIN.
#[derive(Debug, Clone, PartialEq)]
pub struct PairingState {
    pub device: Value,
    pub auth_key: String,
    pub timestamp: Value,
}

/// The HMAC-SHA1 of the timestamp and the PIN with the shared key, in base64 (ha-philipsjs `hmac_signature`).
pub fn signature(timestamp: &Value, pin: &str) -> String {
    let stamp = match timestamp {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, &SHARED_KEY);
    let mut message = stamp.into_bytes();
    message.extend_from_slice(pin.as_bytes());
    digest::base64(ring::hmac::sign(&key, &message).as_ref())
}

/// This computer, as the TV will list it among its paired devices.
fn device(id: &str) -> Value {
    json!({
        "device_name": crate::phone_link::computer_name(),
        "device_os": if cfg!(windows) { "Windows" } else { "Linux" },
        "app_id": "fr.louisraille.coucou",
        "app_name": "Coucou",
        "type": "native",
        "id": id,
    })
}

fn pairing_error(v: &Value) -> String {
    match v.get("error_id").and_then(Value::as_str).unwrap_or("") {
        "INVALID_PIN" => t("Wrong PIN. Check the code on the TV and type it again."),
        "TIMEOUT" | "TIMED_OUT" => t("The PIN expired. Start the pairing again."),
        "CONCURRENT_PAIRING" => t("The TV is already pairing with something else. Close that, then try again."),
        other if !other.is_empty() => tf("The TV refused the pairing ({reason}).", &[("reason", other)]),
        _ => t("The TV refused the pairing."),
    }
}

/// Starts pairing: the TV shows a PIN on screen.
pub async fn pair_request(endpoint: &Endpoint, system: &SystemInfo) -> Result<PairingState, String> {
    let device = device(&random_hex(16));
    let mut access = json!({ "scope": ["read", "write", "control"] });
    if !system.features.is_empty() {
        let features: Map<String, Value> = system.features.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        access["featuring"] = json!({ "jsonfeatures": features });
    }
    let (result, _) = request(endpoint, None, Method::POST, "pair/request", Some(&json!({ "access": access, "device": device }))).await;
    let v = result.map_err(|e| e.message())?;
    if v.get("error_id").and_then(Value::as_str) != Some("SUCCESS") {
        return Err(pairing_error(&v));
    }
    let auth_key = v.get("auth_key").and_then(Value::as_str).filter(|k| !k.is_empty()).ok_or_else(|| t("The TV's answer could not be read."))?;
    let timestamp = v.get("timestamp").cloned().filter(|t| !t.is_null()).ok_or_else(|| t("The TV's answer could not be read."))?;
    Ok(PairingState { device, auth_key: auth_key.to_string(), timestamp })
}

/// Finishes pairing with the PIN the TV shows. Returns the credentials to keep.
pub async fn pair_grant(endpoint: &Endpoint, state: &PairingState, pin: &str) -> Result<Credentials, String> {
    let pin = pin.trim();
    if pin.is_empty() || pin.len() > 12 || !pin.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(t("Type the PIN the TV shows."));
    }
    let user = state.device.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
    let creds = Credentials { user, key: state.auth_key.clone() };
    let body = json!({
        "auth": {
            "auth_appId": "1",
            "auth_timestamp": state.timestamp,
            "auth_signature": signature(&state.timestamp, pin),
            "pin": pin,
        },
        "device": state.device,
    });
    let (result, _) = request(endpoint, Some(&creds), Method::POST, "pair/grant", Some(&body)).await;
    let v = result.map_err(|e| e.message())?;
    if v.get("error_id").and_then(Value::as_str) != Some("SUCCESS") {
        return Err(pairing_error(&v));
    }
    Ok(creds)
}

// ── A paired (or open) TV ─────────────────────────────────────────────────────

/// An app as the TV lists it, with the intent that opens it.
#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub label: String,
    pub intent: Value,
}

/// A channel and the list it is in (the TV wants both to switch to it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TvChannel {
    pub channel: Channel,
    pub ccid: String,
    pub list: String,
}

/// The volume as the TV counts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volume {
    pub current: i64,
    pub min: i64,
    pub max: i64,
    pub muted: bool,
}

impl Volume {
    pub fn percent(&self) -> u8 {
        if self.max <= self.min {
            return 0;
        }
        (((self.current - self.min) as f64 / (self.max - self.min) as f64) * 100.0).round().clamp(0.0, 100.0) as u8
    }

    /// The TV's own step for `percent`.
    pub fn level(&self, percent: u8) -> i64 {
        self.min + ((self.max - self.min) as f64 * f64::from(percent.min(100)) / 100.0).round() as i64
    }
}

pub struct Tv {
    pub endpoint: Endpoint,
    pub creds: Option<Credentials>,
}

impl Tv {
    async fn get(&self, path: &str) -> Result<Value, TvError> {
        request(&self.endpoint, self.creds.as_ref(), Method::GET, path, None).await.0
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value, TvError> {
        request(&self.endpoint, self.creds.as_ref(), Method::POST, path, Some(&body)).await.0
    }

    /// On (true) or in standby (false). Err when the TV has no power state (API 1) or does not answer.
    pub async fn power(&self) -> Result<bool, TvError> {
        let v = self.get("powerstate").await?;
        match v.get("powerstate").and_then(Value::as_str) {
            Some(state) => Ok(state.eq_ignore_ascii_case("on")),
            None => Err(TvError::NotSupported),
        }
    }

    pub async fn set_power(&self, on: bool) -> Result<(), TvError> {
        self.post("powerstate", json!({ "powerstate": if on { "On" } else { "Standby" } })).await.map(|_| ())
    }

    /// A key of the remote: "Standby", "VolumeUp", "ChannelStepDown"…
    pub async fn key(&self, key: &str) -> Result<(), TvError> {
        self.post("input/key", json!({ "key": key })).await.map(|_| ())
    }

    pub async fn volume(&self) -> Result<Volume, TvError> {
        let v = self.get("audio/volume").await?;
        let n = |k: &str| v.get(k).and_then(Value::as_i64);
        match (n("current"), n("min"), n("max")) {
            (Some(current), min, Some(max)) if max > 0 => {
                Ok(Volume { current, min: min.unwrap_or(0), max, muted: v.get("muted").and_then(Value::as_bool).unwrap_or(false) })
            }
            _ => Err(TvError::NotSupported),
        }
    }

    /// Sets the volume, keeping or changing the mute.
    pub async fn set_volume(&self, current: i64, muted: bool) -> Result<(), TvError> {
        self.post("audio/volume", json!({ "current": current, "muted": muted })).await.map(|_| ())
    }

    pub async fn apps(&self) -> Result<Vec<App>, TvError> {
        let v = self.get("applications").await?;
        let mut apps: Vec<App> = v
            .get("applications")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|a| {
                        let label = a.get("label").and_then(Value::as_str)?.trim().to_string();
                        let intent = a.get("intent").filter(|i| i.is_object())?.clone();
                        (!label.is_empty()).then_some(App { label, intent })
                    })
                    .collect()
            })
            .unwrap_or_default();
        apps.sort_by_key(|a| a.label.to_lowercase());
        apps.dedup_by(|a, b| a.label == b.label);
        Ok(apps)
    }

    pub async fn launch(&self, intent: &Value) -> Result<(), TvError> {
        self.post("activities/launch", json!({ "intent": intent })).await.map(|_| ())
    }

    /// The app on screen, by the intent's package (matched against `apps` for its label).
    pub async fn current_app(&self, apps: &[App]) -> Option<String> {
        let v = self.get("activities/current").await.ok()?;
        let package = v.pointer("/component/packageName").and_then(Value::as_str)?;
        apps.iter()
            .find(|a| a.intent.pointer("/component/packageName").and_then(Value::as_str) == Some(package))
            .map(|a| a.label.clone())
    }

    /// Every channel the TV lists, by number. API 5 and 6: channeldb/tv; API 1: channels.
    pub async fn channels(&self) -> Result<Vec<TvChannel>, TvError> {
        let mut out: Vec<TvChannel> = Vec::new();
        if self.endpoint.api >= 5 {
            let db = self.get("channeldb/tv").await?;
            let ids: Vec<String> = db
                .get("channelLists")
                .and_then(Value::as_array)
                .map(|l| l.iter().filter_map(|x| x.get("id").and_then(Value::as_str).map(str::to_string)).collect())
                .unwrap_or_default();
            for list in ids.iter().take(6) {
                let Ok(v) = self.get(&format!("channeldb/tv/channelLists/{list}")).await else { continue };
                for c in v.get("Channel").and_then(Value::as_array).into_iter().flatten() {
                    let ccid = match c.get("ccid") {
                        Some(Value::Number(n)) => n.to_string(),
                        Some(Value::String(s)) => s.clone(),
                        _ => continue,
                    };
                    let number = c.get("preset").map(|p| p.as_str().map(str::to_string).unwrap_or_else(|| p.to_string())).unwrap_or_default();
                    let name = c.get("name").and_then(Value::as_str).unwrap_or("").trim().to_string();
                    if number.is_empty() || out.iter().any(|o| o.channel.number == number) {
                        continue;
                    }
                    out.push(TvChannel { channel: Channel { number, name }, ccid, list: list.clone() });
                }
            }
        } else {
            let v = self.get("channels").await?;
            for (id, c) in v.as_object().into_iter().flatten() {
                let number = c.get("preset").and_then(Value::as_str).unwrap_or("").to_string();
                let name = c.get("name").and_then(Value::as_str).unwrap_or("").trim().to_string();
                if !number.is_empty() && !out.iter().any(|o| o.channel.number == number) {
                    out.push(TvChannel { channel: Channel { number, name }, ccid: id.clone(), list: "all".into() });
                }
            }
        }
        out.sort_by_key(|c| (c.channel.number.parse::<u32>().unwrap_or(u32::MAX), c.channel.number.clone()));
        Ok(out)
    }

    pub async fn set_channel(&self, channel: &TvChannel) -> Result<(), TvError> {
        if self.endpoint.api >= 5 {
            let ccid: Value = channel.ccid.parse::<i64>().map(Value::from).unwrap_or_else(|_| Value::from(channel.ccid.clone()));
            self.post("activities/tv", json!({ "channelList": { "id": channel.list }, "channel": { "ccid": ccid } })).await.map(|_| ())
        } else {
            self.post("channels/current", json!({ "id": channel.ccid })).await.map(|_| ())
        }
    }

    /// The channel on screen, by number.
    pub async fn current_channel(&self) -> Option<String> {
        if self.endpoint.api >= 5 {
            let v = self.get("activities/tv").await.ok()?;
            v.pointer("/channel/preset").map(|p| p.as_str().map(str::to_string).unwrap_or_else(|| p.to_string()))
        } else {
            let v = self.get("channels/current").await.ok()?;
            v.get("id").and_then(Value::as_str).map(str::to_string)
        }
    }
}

// ── Waking a TV whose network sleeps ──────────────────────────────────────────

/// A MAC address from "AA:BB:CC:DD:EE:FF" or "aa-bb-cc-dd-ee-ff".
pub fn parse_mac(text: &str) -> Option<[u8; 6]> {
    let parts: Vec<&str> = text.trim().split([':', '-']).collect();
    if parts.len() != 6 {
        return None;
    }
    let mut mac = [0u8; 6];
    for (i, p) in parts.iter().enumerate() {
        if p.len() != 2 {
            return None;
        }
        mac[i] = u8::from_str_radix(p, 16).ok()?;
    }
    (mac != [0; 6] && mac != [0xff; 6]).then_some(mac)
}

pub fn format_mac(mac: &[u8; 6]) -> String {
    mac.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

/// Six 0xFF, then the MAC sixteen times: the Wake-on-LAN "magic packet".
pub fn magic_packet(mac: &[u8; 6]) -> Vec<u8> {
    let mut packet = vec![0xffu8; 6];
    for _ in 0..16 {
        packet.extend_from_slice(mac);
    }
    packet
}

/// Sends the magic packet on the local network (broadcast, ports 9 and 7). The TV must have "Switch on with Wi-Fi"
/// (or with the network) on.
pub async fn wake(mac: &[u8; 6]) -> Result<(), String> {
    let socket = tokio::net::UdpSocket::bind(("0.0.0.0", 0)).await.map_err(|e| e.to_string())?;
    socket.set_broadcast(true).map_err(|e| e.to_string())?;
    let packet = magic_packet(mac);
    let mut sent = false;
    for port in [9u16, 7] {
        sent |= socket.send_to(&packet, ("255.255.255.255", port)).await.is_ok();
    }
    if sent { Ok(()) } else { Err(t("The wake-up signal could not be sent on the network.")) }
}

/// The MAC of a device this computer talked to recently, from the system's neighbour table (`arp -a <ip>` on Windows,
/// `ip neigh show <ip>` on Linux). None when it is not there. The address is checked to be an IP first, and passed
/// as an argument, never through a shell.
pub fn learn_mac(ip: &str) -> Option<[u8; 6]> {
    let ip: std::net::IpAddr = ip.trim().parse().ok()?;
    let ip = ip.to_string();
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("arp");
        c.args(["-a", &ip]);
        c
    } else {
        let mut c = std::process::Command::new("ip");
        c.args(["neigh", "show", &ip]);
        c
    };
    let out = crate::platform::no_console(&mut cmd).output().ok()?;
    mac_in(&String::from_utf8_lossy(&out.stdout), &ip)
}

/// The MAC on the line of `ip` in a neighbour table's text.
pub fn mac_in(text: &str, ip: &str) -> Option<[u8; 6]> {
    text.lines()
        .filter(|line| line.split_whitespace().any(|w| w == ip))
        .flat_map(|line| line.split_whitespace())
        .find_map(parse_mac)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};

    #[test]
    fn the_pin_is_signed_as_ha_philipsjs_signs_it() {
        // Computed with ha-philipsjs's hmac_signature(AUTH_SHARED_KEY, "1234567890", "4321").
        assert_eq!(signature(&json!(1234567890), "4321"), "no8egn4L49VTnQGMRC3YuAc4S6o=");
        assert_eq!(signature(&json!("1234567890"), "4321"), "no8egn4L49VTnQGMRC3YuAc4S6o=");
    }

    #[test]
    fn a_system_description_tells_the_api_the_os_and_the_features() {
        // The shape of a 2019 Android TV's /6/system (ha-philipsjs fixtures).
        let v = json!({
            "menulanguage": "English", "name": "Philips TV", "country": "Brazil",
            "serialnumber_encrypted": "x", "model_encrypted": "y",
            "api_version": { "Major": 6, "Minor": 4, "Patch": 0 },
            "featuring": {
                "jsonfeatures": {
                    "applications": ["TV_Apps", "TV_Games", "TV_Settings"],
                    "activities": ["intent"],
                    "inputkey": ["key"],
                    "channels": ["preset_string"],
                },
                "systemfeatures": { "pairing_type": "digest_auth_pairing", "secured_transport": "true", "tvtype": "consumer" },
            },
            "os_type": "MSAF_2019_P",
        });
        let s = SystemInfo::from_json(&v, 6);
        assert_eq!(s.version, (6, 4, 0));
        assert!(s.needs_pairing && s.secured && s.android());
        assert!(s.keys() && s.apps());
        assert_eq!(s.model, None, "encrypted, not guessed");
        // A 2012 TV on API 1: no version, no features, the model in clear.
        let old = SystemInfo::from_json(&json!({ "name": "42PFL6007", "model": "42PFL6007T/12", "country": "Netherlands" }), 1);
        assert_eq!(old.version, (1, 0, 0));
        assert!(!old.needs_pairing && !old.apps() && old.keys());
        assert_eq!(old.model.as_deref(), Some("42PFL6007T/12"));
        // API 6 without "activities: intent" cannot open apps.
        let no_launch = SystemInfo::from_json(&json!({ "name": "TV", "api_version": { "Major": 6 }, "featuring": { "jsonfeatures": { "applications": ["TV_Apps"] } } }), 6);
        assert!(!no_launch.apps());
    }

    #[test]
    fn broken_json_from_old_firmware_is_mended() {
        assert_eq!(parse_body(r#"{"a":1,,"b":2,}"#).unwrap(), json!({"a":1,"b":2}));
        assert_eq!(parse_body("Context Service not started").unwrap(), json!({}));
        assert_eq!(parse_body("").unwrap(), json!({}));
        assert!(parse_body("<html>nope").is_none());
    }

    #[test]
    fn the_volume_is_turned_into_percent_and_back() {
        let v = Volume { current: 15, min: 0, max: 60, muted: false };
        assert_eq!(v.percent(), 25);
        assert_eq!(v.level(50), 30);
        assert_eq!(v.level(100), 60);
        assert_eq!(Volume { current: 0, min: 0, max: 0, muted: false }.percent(), 0);
    }

    #[test]
    fn wake_on_lan_and_the_neighbour_table() {
        assert_eq!(parse_mac("a4-b1-c2-d3-e4-f5"), Some([0xa4, 0xb1, 0xc2, 0xd3, 0xe4, 0xf5]));
        assert_eq!(parse_mac("A4:B1:C2:D3:E4:F5"), Some([0xa4, 0xb1, 0xc2, 0xd3, 0xe4, 0xf5]));
        assert_eq!(parse_mac("ff-ff-ff-ff-ff-ff"), None, "the broadcast address is no device");
        assert_eq!(parse_mac("a4:b1:c2:d3:e4"), None);
        assert_eq!(format_mac(&[0xa4, 0xb1, 0xc2, 0xd3, 0xe4, 0xf5]), "A4:B1:C2:D3:E4:F5");
        let packet = magic_packet(&[1, 2, 3, 4, 5, 6]);
        assert_eq!(packet.len(), 102);
        assert_eq!(&packet[..6], &[0xff; 6]);
        assert_eq!(&packet[96..], &[1, 2, 3, 4, 5, 6]);
        // Windows (Portuguese) and Linux tables.
        let windows = "\nInterface: 192.168.1.5 --- 0x7\n  Endereço IP           Endereço físico       Tipo\n  192.168.1.20          a4-b1-c2-d3-e4-f5     dinâmico\n  192.168.1.255         ff-ff-ff-ff-ff-ff     estático\n";
        assert_eq!(mac_in(windows, "192.168.1.20"), parse_mac("a4-b1-c2-d3-e4-f5"));
        assert_eq!(mac_in(windows, "192.168.1.21"), None);
        let linux = "192.168.1.20 dev wlan0 lladdr a4:b1:c2:d3:e4:f5 REACHABLE\n";
        assert_eq!(mac_in(linux, "192.168.1.20"), parse_mac("a4:b1:c2:d3:e4:f5"));
    }

    /// One reply of the simulated TV: what it got, and what it answers.
    pub struct Exchange {
        pub method: String,
        pub path: String,
        pub authorization: Option<String>,
        pub body: String,
    }

    /// A Philips TV on loopback (plain http): answers with `respond` for each request, as long as it returns Some.
    /// The digest challenge is the TV's when `respond` says 401.
    pub fn fake_tv(respond: impl Fn(&Exchange) -> Option<(u16, String)> + Send + 'static) -> (String, Arc<Mutex<Vec<Exchange>>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(conn) = conn else { break };
                let mut reader = BufReader::new(conn.try_clone().unwrap());
                let mut first = String::new();
                if reader.read_line(&mut first).is_err() || first.is_empty() {
                    continue;
                }
                let mut parts = first.split_whitespace();
                let (method, path) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
                let (mut length, mut authorization) = (0usize, None);
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                    if lower.starts_with("authorization:") {
                        authorization = Some(line["authorization:".len()..].trim().to_string());
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let exchange = Exchange { method, path, authorization, body: String::from_utf8_lossy(&body).to_string() };
                let Some((status, text)) = respond(&exchange) else { break };
                log.lock().unwrap().push(exchange);
                let challenge = if status == 401 { "WWW-Authenticate: Digest realm=\"XTV\", nonce=\"abc123\", qop=\"auth\"\r\n" } else { "" };
                let mut conn = conn;
                let _ = write!(
                    conn,
                    "HTTP/1.1 {status} X\r\n{challenge}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                    text.len()
                );
            }
        });
        (format!("127.0.0.1:{port}"), seen)
    }

    fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    /// The simulated TV's endpoint.
    fn endpoint_at(addr: &str, api: u8) -> Endpoint {
        let (host, port) = addr.split_once(':').unwrap();
        Endpoint { host: host.into(), port: port.parse().unwrap(), api, https: false, pin: None }
    }

    async fn call(endpoint: &Endpoint, creds: Option<&Credentials>, method: Method, path: &str, body: Option<&Value>) -> Result<Value, TvError> {
        request(endpoint, creds, method, path, body).await.0
    }

    #[test]
    fn a_paired_tv_is_answered_with_digest_and_the_answer_checks_out() {
        let creds = Credentials { user: "dev123".into(), key: "secretkey".into() };
        let (addr, seen) = fake_tv(|e| {
            if e.authorization.is_none() {
                return Some((401, String::new()));
            }
            Some((200, r#"{"current":15,"min":0,"max":60,"muted":false}"#.into()))
        });
        let endpoint = endpoint_at(&addr, 6);
        let v = block_on(call(&endpoint, Some(&creds), Method::GET, "audio/volume", None)).unwrap();
        assert_eq!(v["max"], 60);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2, "challenge, then the answer");
        assert_eq!(seen[0].path, "/6/audio/volume");
        let auth = seen[1].authorization.clone().unwrap();
        let challenge = Challenge::parse("Digest realm=\"XTV\", nonce=\"abc123\", qop=\"auth\"").unwrap();
        let cnonce = auth.split("cnonce=\"").nth(1).unwrap().split('"').next().unwrap();
        assert_eq!(auth, challenge.answer("dev123", "secretkey", "GET", "/6/audio/volume", cnonce, 1));
        // Without credentials, a 401 is "pair it again".
        let (addr, _) = fake_tv(|_| Some((401, String::new())));
        let endpoint = endpoint_at(&addr, 6);
        assert_eq!(block_on(call(&endpoint, None, Method::GET, "audio/volume", None)), Err(TvError::Auth));
    }

    #[test]
    fn pairing_sends_the_signed_pin_under_digest() {
        let (addr, seen) = fake_tv(|e| match (e.path.as_str(), e.authorization.is_some()) {
            ("/6/pair/request", _) => Some((200, r#"{"error_id":"SUCCESS","auth_key":"k3y","timestamp":1700000000,"timeout":60}"#.into())),
            ("/6/pair/grant", false) => Some((401, String::new())),
            ("/6/pair/grant", true) => {
                let body: Value = serde_json::from_str(&e.body).unwrap();
                let ok = body["auth"]["pin"] == "1234" && body["auth"]["auth_signature"] == signature(&json!(1700000000), "1234");
                Some((200, if ok { r#"{"error_id":"SUCCESS"}"# } else { r#"{"error_id":"INVALID_PIN"}"# }.into()))
            }
            _ => Some((404, String::new())),
        });
        let endpoint = endpoint_at(&addr, 6);
        let system = SystemInfo::from_json(&json!({"name":"TV","api_version":{"Major":6},"featuring":{"jsonfeatures":{"inputkey":["key"]}}}), 6);
        let state = block_on(pair_request(&endpoint, &system)).unwrap();
        assert_eq!(state.auth_key, "k3y");
        assert_eq!(block_on(pair_grant(&endpoint, &state, "9999")).unwrap_err(), "Wrong PIN. Check the code on the TV and type it again.");
        let creds = block_on(pair_grant(&endpoint, &state, " 1234 ")).unwrap();
        assert_eq!(creds.key, "k3y");
        assert_eq!(creds.user.len(), 32);
        let seen = seen.lock().unwrap();
        let request_body: Value = serde_json::from_str(&seen[0].body).unwrap();
        assert_eq!(request_body["access"]["scope"], json!(["read", "write", "control"]));
        assert_eq!(request_body["device"]["app_name"], "Coucou");
        assert_eq!(request_body["device"]["id"], creds.user);
        assert!(seen[0].authorization.is_none(), "pair/request goes without credentials");
        assert!(block_on(pair_grant(&endpoint, &state, "12 34")).is_err(), "only the PIN's characters");
    }

    #[test]
    fn a_tv_lists_its_apps_and_channels_and_switches_to_them() {
        let (addr, seen) = fake_tv(|e| {
            Some(match (e.method.as_str(), e.path.as_str()) {
                ("GET", "/6/applications") => (200, json!({"applications":[
                    {"label":"YouTube","intent":{"component":{"packageName":"com.google.android.youtube.tv","className":"com.google.android.apps.youtube.tv.activity.ShellActivity"},"action":"android.intent.action.MAIN"}},
                    {"label":"Netflix","intent":{"component":{"packageName":"com.netflix.ninja","className":"com.netflix.ninja.MainActivity"}}},
                    {"label":"Broken"}
                ]}).to_string()),
                ("GET", "/6/channeldb/tv") => (200, r#"{"channelLists":[{"id":"allcab","version":"1"}],"favoriteLists":[]}"#.into()),
                ("GET", "/6/channeldb/tv/channelLists/allcab") => (200, r#"{"id":"allcab","Channel":[{"ccid":35,"preset":"5","name":"SBT"},{"ccid":12,"preset":"4","name":"Globo"},{"ccid":13,"preset":"4","name":"Dup"}]}"#.into()),
                ("POST", _) => (200, String::new()),
                _ => (404, String::new()),
            })
        });
        let tv = Tv { endpoint: endpoint_at(&addr, 6), creds: None };
        let apps = block_on(tv.apps()).unwrap();
        assert_eq!(apps.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(), ["Netflix", "YouTube"], "sorted, the broken one left out");
        block_on(tv.launch(&apps[1].intent)).unwrap();
        let channels = block_on(tv.channels()).unwrap();
        assert_eq!(channels.iter().map(|c| c.channel.number.as_str()).collect::<Vec<_>>(), ["4", "5"], "by number, once each");
        block_on(tv.set_channel(&channels[1])).unwrap();
        assert_eq!(block_on(tv.power()), Err(TvError::NotSupported), "no powerstate on this one");
        let seen = seen.lock().unwrap();
        let launch = seen.iter().find(|e| e.path == "/6/activities/launch").unwrap();
        assert_eq!(serde_json::from_str::<Value>(&launch.body).unwrap()["intent"]["component"]["packageName"], "com.google.android.youtube.tv");
        let switch = seen.iter().find(|e| e.path == "/6/activities/tv").unwrap();
        assert_eq!(serde_json::from_str::<Value>(&switch.body).unwrap(), json!({"channelList":{"id":"allcab"},"channel":{"ccid":35}}));
    }

    #[test]
    fn only_local_addresses_are_contacted() {
        let endpoint = Endpoint::new("example.com", 6, false, None);
        let r = block_on(request(&endpoint, None, Method::GET, "system", None)).0;
        assert!(matches!(r, Err(TvError::Other(m)) if m.contains("local network")));
        assert!(block_on(probe("8.8.8.8")).unwrap_err().contains("local network"));
    }
}
