// Samsung TVs since 2016 (Tizen), straight over the local network: the remote control API Samsung's phone apps use, as
// the samsungtvws library and Home Assistant's Samsung TV integration speak it (github.com/xchwarze/samsung-tv-ws-api;
// github.com/home-assistant/core, components/samsungtv), checked against a simulated TV below. Nothing is assumed from
// the brand: `info` asks the TV what it is and what it supports, and only what it answered for is offered.
//
//   * Info: GET http://<tv>:8001/api/v2/ names the TV and its model, says whether it is on ("PowerState", on the
//     models since 2018), whether its remote wants a token ("TokenAuthSupport") and gives its Wi-Fi MAC address.
//   * Remote: a WebSocket, wss://<tv>:8002/api/v2/channels/samsung.remote.control (ws:// on 8001 for the TVs without
//     tokens). The first time, the TV asks on its screen whether to allow "Coucou"; once allowed it gives a token, kept
//     in the OS keystore, and does not ask again. Keys are the remote's (KEY_POWER, KEY_VOLUP…); apps are listed
//     (ed.installedApp.get) and opened (ed.apps.launch) on the same socket.
//   * The TV's certificate is self-signed: it is pinned on first contact (philips::pinned_tls).
//   * A number on the volume, and mute: the TV's UPnP renderer (upnp.rs).
//   * Only addresses on the local network are ever contacted.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use reqwest::Url;
use rustls::pki_types::ServerName;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use super::digest;
use super::philips::{self, TvError};
use crate::i18n::{t, tf};
use crate::net;

pub const REST_PORT: u16 = 8001;
pub const SECURE_PORT: u16 = 8002;
/// The name the TV shows when it asks whether to allow this computer.
const NAME: &str = "Coucou";
const TIMEOUT: Duration = Duration::from_secs(5);
/// How long the user has to answer the TV's question (Home Assistant waits 31 s too).
pub const PAIR_WAIT: Duration = Duration::from_secs(31);
/// Opening the remote of a TV that knows this computer.
pub const OPEN_WAIT: Duration = Duration::from_secs(8);
const MAX_INFO: usize = 256 * 1024;
/// Events a TV sends by itself when the remote opens, before it says whether this computer is allowed.
const STARTUP_EVENTS: &[&str] = &["ed.edenTV.update", "ms.voiceApp.hide"];

/// What GET /api/v2/ says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Info {
    /// "Samsung 7 Series (43)" (the "[TV]" in front taken off).
    pub name: String,
    /// "UN43AU7700GXZD"; "" when the TV does not say.
    pub model: String,
    /// "Tizen".
    pub os: String,
    /// The API's version ("2.0.25").
    pub version: String,
    /// "on" or "standby", on the TVs that say (2018 and later). None: not said.
    pub power: Option<bool>,
    /// The remote needs a token (wss on 8002).
    pub token_auth: bool,
    pub mac: Option<[u8; 6]>,
    /// A Frame TV: a short press of the power key only turns it to art.
    pub frame: bool,
}

/// The answer of GET /api/v2/, read; None when it is not a Samsung TV's.
pub fn parse_info(v: &Value) -> Option<Info> {
    let device = v.get("device")?;
    let s = |key: &str| device.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let yes = |key: &str| s(key).is_some_and(|x| x.eq_ignore_ascii_case("true"));
    let kind = s("type").or_else(|| v.get("type").and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
    if !kind.to_ascii_lowercase().contains("samsung") {
        return None;
    }
    let name = s("name").or_else(|| v.get("name").and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
    let name = name.trim().trim_start_matches("[TV]").trim().to_string();
    Some(Info {
        name: if name.is_empty() { "Samsung TV".into() } else { name },
        model: s("modelName").unwrap_or_default(),
        os: s("OS").unwrap_or_default(),
        version: v.get("version").and_then(Value::as_str).unwrap_or("").trim().to_string(),
        power: s("PowerState").map(|p| p.eq_ignore_ascii_case("on")),
        token_auth: yes("TokenAuthSupport"),
        mac: s("wifiMac").and_then(|m| philips::parse_mac(&m)),
        frame: yes("FrameTVSupport"),
    })
}

/// A Samsung TV and how to reach it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tv {
    pub host: String,
    pub rest_port: u16,
    /// 8002 (wss, with a token) or 8001 (ws).
    pub remote_port: u16,
    pub secure: bool,
    /// The SHA-256 of its certificate, once seen (wss only).
    pub pin: Option<[u8; 32]>,
}

impl Tv {
    pub fn new(host: &str, secure: bool, pin: Option<[u8; 32]>) -> Tv {
        Tv {
            host: host.trim().trim_start_matches('[').trim_end_matches(']').to_string(),
            rest_port: REST_PORT,
            remote_port: if secure { SECURE_PORT } else { REST_PORT },
            secure,
            pin,
        }
    }

    fn host_in_url(&self) -> String {
        if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() }
    }

    fn rest_url(&self, path: &str) -> Result<Url, TvError> {
        Url::parse(&format!("http://{}:{}/api/v2/{path}", self.host_in_url(), self.rest_port))
            .map_err(|_| TvError::Other(tf("Not a valid address: {address}", &[("address", &self.host)])))
    }
}

async fn local(host: &str, port: u16) -> Result<(), TvError> {
    if net::resolves_locally(host, port).await {
        Ok(())
    } else {
        Err(TvError::Other(t("Only devices on your local network can be used.")))
    }
}

fn rest_client() -> Result<reqwest::Client, TvError> {
    reqwest::Client::builder().no_proxy().connect_timeout(Duration::from_secs(3)).timeout(TIMEOUT).build().map_err(|e| TvError::Other(e.to_string()))
}

/// What the TV says about itself. NotSupported: something answers there, but not as a Samsung TV.
pub async fn info(tv: &Tv) -> Result<Info, TvError> {
    local(&tv.host, tv.rest_port).await?;
    let response = rest_client()?
        .get(tv.rest_url("")?)
        .send()
        .await
        .map_err(|e| TvError::Unreachable(e.to_string().chars().take(200).collect()))?;
    if !response.status().is_success() {
        return Err(TvError::NotSupported);
    }
    let bytes = net::read_capped(response, MAX_INFO).await.map_err(TvError::Other)?;
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| TvError::NotSupported)?;
    parse_info(&v).ok_or(TvError::NotSupported)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
}

/// Whether the app `id` is on screen, as GET /api/v2/applications/<id> says. None when the TV does not tell (not every
/// model answers it).
pub async fn app_visible(tv: &Tv, id: &str) -> Option<bool> {
    if !valid_id(id) {
        return None;
    }
    local(&tv.host, tv.rest_port).await.ok()?;
    let response = rest_client().ok()?.get(tv.rest_url(&format!("applications/{id}")).ok()?).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = net::read_capped(response, MAX_INFO).await.ok()?;
    serde_json::from_slice::<Value>(&bytes).ok()?.get("visible").and_then(Value::as_bool)
}

/// An app installed on the TV, as it lists them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct App {
    pub id: String,
    pub name: String,
    /// The TV's app_type: 4 opens natively, the others as a link (samsungtvws).
    pub kind: u64,
}

impl App {
    fn action(&self) -> &'static str {
        if self.kind == 4 { "NATIVE_LAUNCH" } else { "DEEP_LINK" }
    }
}

/// The apps of an ed.installedApp.get answer, by name.
pub fn parse_apps(v: &Value) -> Vec<App> {
    let mut apps: Vec<App> = v
        .pointer("/data/data")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    let id = a.get("appId")?.as_str()?.trim().to_string();
                    let name = a.get("name")?.as_str()?.trim().to_string();
                    (valid_id(&id) && !name.is_empty() && name.chars().count() <= 80)
                        .then(|| App { id, name, kind: a.get("app_type").and_then(Value::as_u64).unwrap_or(2) })
                })
                .collect()
        })
        .unwrap_or_default();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    apps
}

/// The address of the remote: the name the TV shows (base64), and the token when there is one.
pub fn remote_url(tv: &Tv, token: Option<&str>) -> String {
    let scheme = if tv.secure { "wss" } else { "ws" };
    let mut url = format!(
        "{scheme}://{}:{}/api/v2/channels/samsung.remote.control?name={}",
        tv.host_in_url(),
        tv.remote_port,
        digest::base64(NAME.as_bytes())
    );
    if let (true, Some(token)) = (tv.secure, token.filter(|t| !t.is_empty() && t.len() <= 64 && t.chars().all(|c| c.is_ascii_alphanumeric()))) {
        url.push_str("&token=");
        url.push_str(token);
    }
    url
}

trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

/// The remote, open: the TV allowed this computer.
pub struct Remote {
    ws: WebSocketStream<Box<dyn Io>>,
    /// The token the TV gave when it allowed this computer (TVs with tokens; it may give a new one).
    pub token: Option<String>,
    /// The certificate met (wss), to pin.
    pub certificate: Option<[u8; 32]>,
}

fn json_of(message: &Message) -> Option<Value> {
    match message {
        Message::Text(text) => serde_json::from_str(text.as_str()).ok(),
        Message::Binary(bytes) => serde_json::from_slice(bytes).ok(),
        _ => None,
    }
}

fn no_answer(pairing: bool) -> TvError {
    if pairing {
        TvError::Other(t("Nobody answered on the TV in time: press Pair again, then choose Allow on the TV."))
    } else {
        TvError::Other(t("The TV is asking whether to allow Coucou: choose Allow on the TV, or press Pair again in Settings → Mochi's home."))
    }
}

/// Opens the remote. A TV that does not know this computer (no token, or one it forgot) asks on its screen whether to
/// allow "Coucou": `pairing` gives the user PAIR_WAIT to answer, else the remote must open within OPEN_WAIT.
pub async fn connect(tv: &Tv, token: Option<&str>, pairing: bool) -> Result<Remote, TvError> {
    local(&tv.host, tv.remote_port).await?;
    let (config, seen) = if tv.secure {
        let (config, seen) = philips::pinned_tls(tv.pin).map_err(TvError::Other)?;
        (Some(Arc::new(config)), seen)
    } else {
        (None, Default::default())
    };
    let attempt = async {
        let tcp = tokio::time::timeout(Duration::from_secs(4), TcpStream::connect((tv.host.as_str(), tv.remote_port)))
            .await
            .map_err(|_| TvError::Unreachable("no answer".into()))?
            .map_err(|e| TvError::Unreachable(e.to_string()))?;
        let _ = tcp.set_nodelay(true);
        let stream: Box<dyn Io> = match config {
            Some(config) => {
                let name = ServerName::try_from(tv.host.clone()).map_err(|_| TvError::Other(tf("Not a valid address: {address}", &[("address", &tv.host)])))?;
                Box::new(TlsConnector::from(config).connect(name, tcp).await.map_err(|e| TvError::Unreachable(e.to_string()))?)
            }
            None => Box::new(tcp),
        };
        let request = remote_url(tv, token).into_client_request().map_err(|e| TvError::Other(e.to_string()))?;
        let (mut ws, _) = tokio_tungstenite::client_async(request, stream).await.map_err(|e| TvError::Unreachable(e.to_string()))?;
        loop {
            let message = ws.next().await.ok_or_else(|| TvError::Unreachable("closed".into()))?.map_err(|e| TvError::Unreachable(e.to_string()))?;
            let Some(v) = json_of(&message) else { continue };
            match v.get("event").and_then(Value::as_str).unwrap_or("") {
                "ms.channel.connect" => {
                    let token = match v.pointer("/data/token") {
                        Some(Value::String(s)) => Some(s.trim().to_string()),
                        Some(Value::Number(n)) => Some(n.to_string()),
                        _ => None,
                    };
                    return Ok((ws, token.filter(|t| !t.is_empty())));
                }
                "ms.channel.unauthorized" => return Err(TvError::Auth),
                "ms.channel.timeOut" => return Err(no_answer(pairing)),
                event if STARTUP_EVENTS.contains(&event) => continue,
                "ms.error" => {
                    let why = v.pointer("/data/message").and_then(Value::as_str).unwrap_or("ms.error");
                    return Err(TvError::Other(tf("The TV answered {status}.", &[("status", why)])));
                }
                // Anything else before the TV says whether this computer is allowed: wait for that.
                _ => continue,
            }
        }
    };
    let result = tokio::time::timeout(if pairing { PAIR_WAIT } else { OPEN_WAIT }, attempt).await.unwrap_or_else(|_| Err(no_answer(pairing)));
    let met = *seen.lock().unwrap();
    match result {
        Ok((ws, token)) => Ok(Remote { ws, token, certificate: met }),
        // A TLS failure after a pin is set, with a different certificate met: say so rather than "unreachable".
        Err(TvError::Unreachable(_)) if tv.pin.is_some() && met.is_some() && met != tv.pin => Err(TvError::CertificateChanged),
        Err(e) => Err(e),
    }
}

fn key_message(cmd: &str, key: &str) -> Value {
    json!({ "method": "ms.remote.control", "params": { "Cmd": cmd, "DataOfCmd": key, "Option": "false", "TypeOfRemote": "SendRemoteKey" } })
}

impl Remote {
    async fn send(&mut self, v: Value) -> Result<(), TvError> {
        self.ws.send(Message::Text(v.to_string().into())).await.map_err(|e| TvError::Unreachable(e.to_string()))
    }

    /// The next event named `event` within `wait`, the others skipped. None when it did not come.
    async fn event(&mut self, event: &str, wait: Duration) -> Result<Option<Value>, TvError> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            match tokio::time::timeout_at(deadline, self.ws.next()).await {
                Err(_) => return Ok(None),
                Ok(None) => return Err(TvError::Unreachable("closed".into())),
                Ok(Some(Err(e))) => return Err(TvError::Unreachable(e.to_string())),
                Ok(Some(Ok(message))) => {
                    if let Some(v) = json_of(&message).filter(|v| v.get("event").and_then(Value::as_str) == Some(event)) {
                        return Ok(Some(v));
                    }
                }
            }
        }
    }

    /// One press of a key of the remote ("KEY_VOLUP").
    pub async fn key(&mut self, key: &str) -> Result<(), TvError> {
        self.send(key_message("Click", key)).await
    }

    /// A key held down (the power key of a Frame TV, which a short press only turns to art).
    pub async fn hold(&mut self, key: &str, held: Duration) -> Result<(), TvError> {
        self.send(key_message("Press", key)).await?;
        tokio::time::sleep(held).await;
        self.send(key_message("Release", key)).await
    }

    /// The apps installed, as the TV lists them. None when it does not answer: not every model does.
    pub async fn apps(&mut self) -> Result<Option<Vec<App>>, TvError> {
        self.send(json!({ "method": "ms.channel.emit", "params": { "event": "ed.installedApp.get", "to": "host" } })).await?;
        Ok(self.event("ed.installedApp.get", Duration::from_secs(5)).await?.map(|v| parse_apps(&v)))
    }

    /// Opens an app of the TV's list. The TV acknowledges it (or not, on some models); whether the app came up is
    /// `app_visible`'s to say.
    pub async fn launch(&mut self, app: &App) -> Result<(), TvError> {
        if !valid_id(&app.id) {
            return Err(TvError::NotSupported);
        }
        let data = json!({ "action_type": app.action(), "appId": app.id, "metaTag": "" });
        self.send(json!({ "method": "ms.channel.emit", "params": { "event": "ed.apps.launch", "to": "host", "data": data } })).await?;
        match self.event("ed.apps.launch", Duration::from_secs(3)).await? {
            Some(v) if v.get("data").and_then(Value::as_u64).is_some_and(|code| code >= 400) => Err(TvError::NotSupported),
            _ => Ok(()),
        }
    }

    /// Leaves the keys a moment to be taken, then closes.
    pub async fn close(mut self) {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let _ = self.ws.close(None).await;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::smart_home::philips::tests::fake_tv;
    use std::sync::Mutex;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

    fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    /// GET /api/v2/ of a 2021 Crystal UHD (the shape samsungtvws and Home Assistant read).
    pub fn crystal_uhd() -> Value {
        json!({
            "device": {
                "FrameTVSupport": "false", "GamePadSupport": "true", "OS": "Tizen", "PowerState": "on",
                "TokenAuthSupport": "true", "VoiceSupport": "true", "countryCode": "BR", "description": "Samsung DTV RCR",
                "id": "uuid:0f1e2d3c", "ip": "192.168.0.9", "model": "21_KANTM2_UHD", "modelName": "UN43AU7700GXZD",
                "name": "[TV] Samsung 7 Series (43)", "networkType": "wireless", "type": "Samsung SmartTV",
                "wifiMac": "A4:30:7A:12:34:56"
            },
            "id": "uuid:0f1e2d3c", "name": "[TV] Samsung 7 Series (43)", "remote": "1.0", "type": "Samsung SmartTV",
            "uri": "http://192.168.0.9:8001/api/v2/", "version": "2.0.25"
        })
    }

    #[test]
    fn a_tvs_description_says_what_it_is_and_what_it_wants() {
        let i = parse_info(&crystal_uhd()).unwrap();
        assert_eq!((i.name.as_str(), i.model.as_str(), i.os.as_str(), i.version.as_str()), ("Samsung 7 Series (43)", "UN43AU7700GXZD", "Tizen", "2.0.25"));
        assert_eq!(i.power, Some(true));
        assert!(i.token_auth && !i.frame);
        assert_eq!(i.mac, philips::parse_mac("a4:30:7a:12:34:56"));
        // A 2016 TV: no power state, no token.
        let mut old = crystal_uhd();
        let device = old["device"].as_object_mut().unwrap();
        device.remove("PowerState");
        device.remove("TokenAuthSupport");
        let i = parse_info(&old).unwrap();
        assert_eq!(i.power, None);
        assert!(!i.token_auth);
        let mut standby = crystal_uhd();
        standby["device"]["PowerState"] = json!("standby");
        assert_eq!(parse_info(&standby).unwrap().power, Some(false));
        // Something else answering on 8001 is not a Samsung TV.
        assert!(parse_info(&json!({ "device": { "type": "Speaker" } })).is_none());
        assert!(parse_info(&json!({ "name": "x" })).is_none());
    }

    #[test]
    fn the_remotes_address_names_coucou_and_carries_only_a_clean_token() {
        let tv = Tv::new("192.168.0.9", true, None);
        assert_eq!(remote_url(&tv, Some("12345678")), "wss://192.168.0.9:8002/api/v2/channels/samsung.remote.control?name=Q291Y291&token=12345678");
        assert_eq!(remote_url(&tv, Some("12&x=1")), "wss://192.168.0.9:8002/api/v2/channels/samsung.remote.control?name=Q291Y291");
        let plain = Tv::new("192.168.0.9", false, None);
        assert_eq!(remote_url(&plain, Some("12345678")), "ws://192.168.0.9:8001/api/v2/channels/samsung.remote.control?name=Q291Y291");
    }

    #[test]
    fn the_installed_apps_are_read_by_name() {
        let v = json!({ "event": "ed.installedApp.get", "from": "host", "data": { "data": [
            { "appId": "3201907018807", "app_type": 2, "name": "Netflix", "icon": "/x.png", "is_lock": 0 },
            { "appId": "111299001912", "app_type": 2, "name": "YouTube" },
            { "appId": "org.tizen.browser", "app_type": 4, "name": "Internet" },
            { "appId": "bad id; rm -rf", "app_type": 2, "name": "Evil" },
            { "appId": "111299001913", "app_type": 2, "name": "youtube" }
        ] } });
        let apps = parse_apps(&v);
        assert_eq!(apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Internet", "Netflix", "YouTube"]);
        assert_eq!(apps[0].action(), "NATIVE_LAUNCH");
        assert_eq!(apps[2].action(), "DEEP_LINK");
        assert!(parse_apps(&json!({ "event": "ed.installedApp.get" })).is_empty());
    }

    #[test]
    fn the_rest_api_says_what_the_tv_is_and_which_app_is_on_screen() {
        let (addr, seen) = fake_tv(|e| {
            Some(match e.path.as_str() {
                "/api/v2/" => (200, crystal_uhd().to_string()),
                "/api/v2/applications/111299001912" => (200, json!({ "id": "111299001912", "name": "YouTube", "running": true, "visible": true }).to_string()),
                _ => (404, String::new()),
            })
        });
        let port: u16 = addr.split(':').nth(1).unwrap().parse().unwrap();
        let tv = Tv { rest_port: port, ..Tv::new("127.0.0.1", true, None) };
        assert_eq!(block_on(info(&tv)).unwrap().model, "UN43AU7700GXZD");
        assert_eq!(block_on(app_visible(&tv, "111299001912")), Some(true));
        assert_eq!(block_on(app_visible(&tv, "3201907018807")), None, "the TV does not tell");
        assert_eq!(block_on(app_visible(&tv, "../../x")), None, "never sent");
        assert_eq!(seen.lock().unwrap().len(), 3);
        // Something that is not a Samsung TV.
        let (other, _) = fake_tv(|_| Some((200, json!({ "hello": "world" }).to_string())));
        let port: u16 = other.split(':').nth(1).unwrap().parse().unwrap();
        assert_eq!(block_on(info(&Tv { rest_port: port, ..Tv::new("127.0.0.1", true, None) })), Err(TvError::NotSupported));
        assert!(matches!(block_on(info(&Tv::new("8.8.8.8", true, None))), Err(TvError::Other(m)) if m.contains("local network")));
    }

    /// What the simulated TV's remote got: the address asked for, and each message.
    pub struct Session {
        pub uri: String,
        pub messages: Vec<Value>,
    }

    /// A Samsung TV's remote on loopback, over TLS with its own certificate: `greet` says what it answers when the
    /// remote opens (given the address asked for), `reply` what it answers to each message. Returns the port, the
    /// certificate's SHA-256 and what it got.
    pub fn fake_remote(
        greet: impl Fn(&str) -> Vec<Value> + Send + Sync + 'static,
        reply: impl Fn(&Value) -> Vec<Value> + Send + Sync + 'static,
    ) -> (u16, [u8; 32], Arc<Mutex<Vec<Session>>>) {
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let cert = rcgen::CertificateParams::new(vec!["SmartViewSDK".to_string()]).unwrap().self_signed(&key).unwrap();
        let der = cert.der().clone();
        let mut pin = [0u8; 32];
        pin.copy_from_slice(ring::digest::digest(&ring::digest::SHA256, der.as_ref()).as_ref());
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![der], rustls::pki_types::PrivateKeyDer::Pkcs8(key.serialize_der().into()))
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let sessions = Arc::new(Mutex::new(Vec::new()));
        let log = sessions.clone();
        let (greet, reply) = (Arc::new(greet), Arc::new(reply));
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                while let Ok((tcp, _)) = listener.accept().await {
                    let Ok(tls) = acceptor.accept(tcp).await else { continue };
                    let uri = Arc::new(Mutex::new(String::new()));
                    let asked = uri.clone();
                    let callback = move |request: &Request, response: Response| {
                        *asked.lock().unwrap() = request.uri().to_string();
                        Ok(response)
                    };
                    let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(tls, callback).await else { continue };
                    let uri = uri.lock().unwrap().clone();
                    let index = {
                        let mut all = log.lock().unwrap();
                        all.push(Session { uri: uri.clone(), messages: Vec::new() });
                        all.len() - 1
                    };
                    for event in greet(&uri) {
                        let _ = ws.send(Message::Text(event.to_string().into())).await;
                    }
                    while let Some(Ok(message)) = ws.next().await {
                        let Some(v) = json_of(&message) else { continue };
                        log.lock().unwrap()[index].messages.push(v.clone());
                        for answer in reply(&v) {
                            let _ = ws.send(Message::Text(answer.to_string().into())).await;
                        }
                    }
                }
            });
        });
        (port, pin, sessions)
    }

    fn at(port: u16, pin: Option<[u8; 32]>) -> Tv {
        Tv { remote_port: port, ..Tv::new("127.0.0.1", true, pin) }
    }

    #[test]
    fn the_tv_asks_once_then_knows_coucou_by_its_token() {
        let (port, pin, sessions) = fake_remote(
            |uri| {
                if uri.contains("token=58034573") {
                    vec![json!({ "event": "ms.channel.connect", "data": { "id": "x" } })]
                } else {
                    // The TV shows its question; the user chooses Allow.
                    vec![json!({ "event": "ed.edenTV.update" }), json!({ "event": "ms.channel.connect", "data": { "id": "x", "token": "58034573" } })]
                }
            },
            |message| match message.pointer("/params/event").and_then(Value::as_str) {
                Some("ed.installedApp.get") => vec![
                    json!({ "event": "ms.channel.clientConnect" }),
                    json!({ "event": "ed.installedApp.get", "from": "host", "data": { "data": [{ "appId": "111299001912", "app_type": 2, "name": "YouTube" }] } }),
                ],
                Some("ed.apps.launch") => vec![json!({ "event": "ed.apps.launch", "from": "host", "data": 200 })],
                _ => vec![],
            },
        );
        block_on(async {
            // Pairing: no token yet; the certificate met is the TV's.
            let remote = connect(&at(port, None), None, true).await.unwrap();
            assert_eq!(remote.token.as_deref(), Some("58034573"));
            assert_eq!(remote.certificate, Some(pin));
            remote.close().await;
            // Then with the token and the pin: keys, apps, an app opened.
            let mut remote = connect(&at(port, Some(pin)), Some("58034573"), false).await.unwrap();
            remote.key("KEY_VOLUP").await.unwrap();
            let apps = remote.apps().await.unwrap().unwrap();
            assert_eq!(apps[0].name, "YouTube");
            remote.launch(&apps[0]).await.unwrap();
            remote.close().await;
        });
        let sessions = sessions.lock().unwrap();
        assert_eq!(sessions.len(), 2);
        assert!(sessions[0].uri.ends_with("?name=Q291Y291"), "{}", sessions[0].uri);
        assert!(sessions[1].uri.ends_with("&token=58034573"));
        let m = &sessions[1].messages;
        assert_eq!(m[0], json!({ "method": "ms.remote.control", "params": { "Cmd": "Click", "DataOfCmd": "KEY_VOLUP", "Option": "false", "TypeOfRemote": "SendRemoteKey" } }));
        assert_eq!(m[1]["params"]["event"], "ed.installedApp.get");
        assert_eq!(m[2]["params"]["data"], json!({ "action_type": "DEEP_LINK", "appId": "111299001912", "metaTag": "" }));
    }

    #[test]
    fn a_refusal_a_changed_certificate_and_a_tv_off_are_told_apart() {
        let (port, _pin, _) = fake_remote(|_| vec![json!({ "event": "ms.channel.unauthorized" })], |_| vec![]);
        assert_eq!(block_on(connect(&at(port, None), None, true)).err(), Some(TvError::Auth));
        assert_eq!(block_on(connect(&at(port, Some([9u8; 32])), Some("1"), false)).err(), Some(TvError::CertificateChanged));
        // Nothing listening.
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        assert!(matches!(block_on(connect(&at(closed, None), None, false)), Err(TvError::Unreachable(_))));
        // A TV that never says whether it allows Coucou (its question unanswered): the wait ends.
        let (silent, _, _) = fake_remote(|_| vec![json!({ "event": "ed.edenTV.update" })], |_| vec![]);
        let started = std::time::Instant::now();
        assert!(matches!(block_on(connect(&at(silent, None), None, false)), Err(TvError::Other(m)) if m.contains("Allow")));
        assert!(started.elapsed() >= OPEN_WAIT && started.elapsed() < OPEN_WAIT + Duration::from_secs(5));
        // A launch the TV refuses.
        let (refusing, _, _) = fake_remote(
            |_| vec![json!({ "event": "ms.channel.connect", "data": {} })],
            |_| vec![json!({ "event": "ed.apps.launch", "data": 404 })],
        );
        block_on(async {
            let mut remote = connect(&at(refusing, None), None, false).await.unwrap();
            assert_eq!(remote.token, None);
            let app = App { id: "111299001912".into(), name: "YouTube".into(), kind: 2 };
            assert_eq!(remote.launch(&app).await, Err(TvError::NotSupported));
        });
    }
}
