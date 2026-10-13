// Finding the house's devices on the local network, only when the user presses Search in Settings → Mochi's home:
//
//   * mDNS / DNS-SD (mdns-sd, already here for the phone link): Home Assistant announces `_home-assistant._tcp`, and
//     Philips TVs `_philipstv_s_rpc._tcp` (Android TVs, https) or `_philipstv_rpc._tcp` (as Home Assistant's Philips
//     TV integration looks for them);
//   * SSDP (UPnP): an M-SEARCH for media renderers, whose description (an XML file on the device) names the maker and
//     the model. That is how a TV's model is known: newer Philips TVs encrypt it in their JointSpace description.
//
// What is found is only a list to choose from: nothing is added, and no TV is contacted beyond its description,
// until the user picks it. Nothing runs between searches.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use reqwest::Url;
use serde::Serialize;

use crate::net;

pub const HA_SERVICE: &str = "_home-assistant._tcp.local.";
pub const PHILIPS_SECURE: &str = "_philipstv_s_rpc._tcp.local.";
pub const PHILIPS_PLAIN: &str = "_philipstv_rpc._tcp.local.";
const SSDP: (Ipv4Addr, u16) = (Ipv4Addr::new(239, 255, 255, 250), 1900);
const MAX_DESCRIPTION: usize = 256 * 1024;

/// One thing found on the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// "homeAssistant", "philipsTv" or "otherTv" (a TV of another maker: not controlled directly).
    pub kind: String,
    pub name: String,
    pub host: String,
    /// Home Assistant's address to connect to.
    pub url: Option<String>,
    pub model: Option<String>,
    pub maker: Option<String>,
    /// Home Assistant's version.
    pub version: Option<String>,
}

// ── mDNS ──────────────────────────────────────────────────────────────────────

/// A service seen through mDNS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdnsService {
    pub service_type: String,
    pub instance: String,
    pub ipv4: Vec<Ipv4Addr>,
    pub port: u16,
    pub txt: BTreeMap<String, String>,
}

/// Browses `types` for `wait`, then stops: the daemon exists only during the search. Blocking (run it off the async
/// runtime).
pub fn browse(types: &[&str], wait: Duration) -> Vec<MdnsService> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};
    let Ok(daemon) = ServiceDaemon::new() else { return Vec::new() };
    let receivers: Vec<(String, _)> = types.iter().filter_map(|ty| daemon.browse(ty).ok().map(|r| (ty.to_string(), r))).collect();
    let mut found: Vec<MdnsService> = Vec::new();
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        let mut idle = true;
        for (ty, receiver) in &receivers {
            while let Ok(event) = receiver.try_recv() {
                idle = false;
                if let ServiceEvent::ServiceResolved(info) = event {
                    let fullname = info.get_fullname();
                    let instance = fullname.strip_suffix(&format!(".{ty}")).unwrap_or(fullname).to_string();
                    let txt = ["location_name", "version", "internal_url", "base_url", "uuid"]
                        .iter()
                        .filter_map(|k| info.get_property_val_str(k).map(|v| (k.to_string(), v.to_string())))
                        .collect();
                    let mut ipv4: Vec<Ipv4Addr> = info.get_addresses_v4().into_iter().collect();
                    ipv4.sort();
                    if !found.iter().any(|f| f.service_type == *ty && f.instance == instance) {
                        found.push(MdnsService { service_type: ty.clone(), instance, ipv4, port: info.get_port(), txt });
                    }
                }
            }
        }
        if idle {
            std::thread::sleep(Duration::from_millis(60));
        }
    }
    for ty in types {
        let _ = daemon.stop_browse(ty);
    }
    if let Ok(done) = daemon.shutdown() {
        let _ = done.recv_timeout(Duration::from_millis(500));
    }
    found
}

/// What an mDNS service means here.
pub fn from_mdns(service: &MdnsService) -> Option<Found> {
    let ip = service.ipv4.iter().find(|ip| net::is_local_ip(IpAddr::V4(**ip)))?;
    let txt = |k: &str| service.txt.get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    if service.service_type == HA_SERVICE {
        // Its own idea of its address on the house's network, if it is a local one; else the address it answered from.
        let url = txt("internal_url")
            .filter(|u| Url::parse(u).ok().and_then(|u| u.host_str().map(net::is_local_host)).unwrap_or(false))
            .unwrap_or_else(|| format!("http://{ip}:{}", service.port));
        return Some(Found {
            kind: "homeAssistant".into(),
            name: txt("location_name").unwrap_or_else(|| "Home Assistant".into()),
            host: ip.to_string(),
            url: Some(url),
            model: None,
            maker: None,
            version: txt("version"),
        });
    }
    if service.service_type == PHILIPS_SECURE || service.service_type == PHILIPS_PLAIN {
        return Some(Found {
            kind: "philipsTv".into(),
            name: if service.instance.trim().is_empty() { "Philips TV".into() } else { service.instance.trim().to_string() },
            host: ip.to_string(),
            url: None,
            model: None,
            maker: Some("Philips".into()),
            version: None,
        });
    }
    None
}

// ── SSDP ──────────────────────────────────────────────────────────────────────

/// The M-SEARCH for media renderers (TVs, speakers), to the multicast group or to one address.
pub fn m_search(host: &str) -> String {
    format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: {host}:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n"
    )
}

/// The headers of an SSDP answer, by lower-case name. None if it is not an answer.
pub fn parse_reply(text: &str) -> Option<BTreeMap<String, String>> {
    let mut lines = text.lines();
    if !lines.next()?.trim().to_ascii_uppercase().starts_with("HTTP/1.1 200") {
        return None;
    }
    Some(
        lines
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
            .collect(),
    )
}

/// SSDP answers for `wait`: the description address of each device that answered, by its IP. `target`: ask one
/// address only (unicast), else the whole network.
pub async fn search(target: Option<Ipv4Addr>, wait: Duration) -> BTreeMap<Ipv4Addr, String> {
    let mut out = BTreeMap::new();
    let Ok(socket) = tokio::net::UdpSocket::bind(("0.0.0.0", 0)).await else { return out };
    let (to, host) = match target {
        Some(ip) => (SocketAddr::from((ip, 1900)), ip.to_string()),
        None => (SocketAddr::from(SSDP), SSDP.0.to_string()),
    };
    let _ = socket.set_multicast_ttl_v4(2);
    let request = m_search(&host);
    // UDP may drop one: ask twice.
    for _ in 0..2 {
        let _ = socket.send_to(request.as_bytes(), to).await;
    }
    let deadline = tokio::time::Instant::now() + wait;
    let mut buf = vec![0u8; 4096];
    while let Ok(Ok((n, from))) = tokio::time::timeout_at(deadline, socket.recv_from(&mut buf)).await {
        let IpAddr::V4(ip) = from.ip() else { continue };
        if !net::is_local_ip(from.ip()) {
            continue;
        }
        let Some(headers) = parse_reply(&String::from_utf8_lossy(&buf[..n])) else { continue };
        if let Some(location) = headers.get("location") {
            // The description must be on the device that answered.
            let same_host = Url::parse(location).ok().and_then(|u| u.host_str().map(|h| h == ip.to_string())).unwrap_or(false);
            if same_host {
                out.entry(ip).or_insert_with(|| location.clone());
            }
        }
    }
    out
}

/// A device's UPnP description: who made it and which model it is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Description {
    pub friendly_name: Option<String>,
    pub manufacturer: Option<String>,
    pub model_name: Option<String>,
    pub model_number: Option<String>,
    pub device_type: Option<String>,
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// The text of the first `<tag>` (any namespace prefix) in `xml`.
fn tag(xml: &str, name: &str) -> Option<String> {
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        let end = rest.find('>')?;
        let open = &rest[..end];
        let local = open.split_whitespace().next().unwrap_or("").rsplit(':').next().unwrap_or("");
        if local == name && !open.ends_with('/') {
            let body = &rest[end + 1..];
            let close = body.find("</")?;
            let text = unescape(body[..close].trim());
            return (!text.is_empty()).then_some(text);
        }
    }
    None
}

pub fn parse_description(xml: &str) -> Description {
    Description {
        friendly_name: tag(xml, "friendlyName"),
        manufacturer: tag(xml, "manufacturer"),
        model_name: tag(xml, "modelName"),
        model_number: tag(xml, "modelNumber"),
        device_type: tag(xml, "deviceType"),
    }
}

impl Description {
    /// Made by Philips (or TP Vision, who make Philips TVs).
    pub fn philips(&self) -> bool {
        [&self.manufacturer, &self.model_name, &self.friendly_name]
            .iter()
            .any(|f| f.as_deref().is_some_and(|v| v.to_ascii_lowercase().contains("philips") || v.to_ascii_lowercase().contains("tp vision")))
    }

    /// "55PUS8007/12": the model number when it says more than the name.
    pub fn model(&self) -> Option<String> {
        match (&self.model_name, &self.model_number) {
            (Some(name), Some(number)) if number.len() > 3 && !name.contains(number.as_str()) && number.chars().any(|c| c.is_ascii_digit()) => Some(number.clone()),
            (Some(name), _) => Some(name.clone()),
            (None, number) => number.clone(),
        }
    }
}

/// Reads a description from the device (local addresses only, small, quick).
pub async fn describe(location: &str) -> Option<Description> {
    let url = Url::parse(location).ok()?;
    if url.scheme() != "http" || !url.host_str().is_some_and(net::is_local_host) {
        return None;
    }
    let client = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(3)).build().ok()?;
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = net::read_capped(response, MAX_DESCRIPTION).await.ok()?;
    Some(parse_description(&String::from_utf8_lossy(&bytes)))
}

/// The TV's model, from its UPnP description, asked directly (for a TV added by its address).
pub async fn model_of(ip: Ipv4Addr) -> Option<String> {
    let found = search(Some(ip), Duration::from_millis(1500)).await;
    describe(found.get(&ip)?).await.and_then(|d| d.model())
}

/// The whole search: mDNS and SSDP at the same time, for about three seconds.
pub async fn scan() -> Vec<Found> {
    let mdns = tokio::task::spawn_blocking(|| browse(&[HA_SERVICE, PHILIPS_SECURE, PHILIPS_PLAIN], Duration::from_millis(3000)));
    let ssdp = search(None, Duration::from_millis(3000)).await;
    let mut found: Vec<Found> = mdns.await.unwrap_or_default().iter().filter_map(from_mdns).collect();
    let mut reads = tokio::task::JoinSet::new();
    for (ip, location) in ssdp {
        reads.spawn(async move { (ip, describe(&location).await) });
    }
    let mut descriptions = Vec::new();
    while let Some(Ok(read)) = reads.join_next().await {
        descriptions.push(read);
    }
    descriptions.sort_by_key(|(ip, _)| *ip);
    for (ip, description) in descriptions {
        let Some(d) = description else { continue };
        let is_tv = d.device_type.as_deref().is_some_and(|t| t.contains("MediaRenderer"));
        if !is_tv {
            continue;
        }
        let host = ip.to_string();
        if let Some(existing) = found.iter_mut().find(|f| f.host == host && f.kind == "philipsTv") {
            existing.model = d.model();
            continue;
        }
        found.push(Found {
            kind: if d.philips() { "philipsTv" } else { "otherTv" }.into(),
            name: d.friendly_name.clone().unwrap_or_else(|| host.clone()),
            host,
            url: None,
            model: d.model(),
            maker: d.manufacturer.clone(),
            version: None,
        });
    }
    found.sort_by(|a, b| (a.kind.as_str(), a.name.to_lowercase()).cmp(&(b.kind.as_str(), b.name.to_lowercase())));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mdns_services_become_home_assistant_or_a_philips_tv() {
        let ha = MdnsService {
            service_type: HA_SERVICE.into(),
            instance: "Casa".into(),
            ipv4: vec![Ipv4Addr::new(192, 168, 1, 10)],
            port: 8123,
            txt: [("location_name", "Casa"), ("version", "2026.10.1"), ("internal_url", "http://homeassistant.local:8123"), ("base_url", "https://x.ui.nabu.casa")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let f = from_mdns(&ha).unwrap();
        assert_eq!((f.kind.as_str(), f.name.as_str(), f.url.as_deref()), ("homeAssistant", "Casa", Some("http://homeassistant.local:8123")));
        assert_eq!(f.version.as_deref(), Some("2026.10.1"));
        // An internal URL on the internet is not taken: the address it answered from is.
        let mut remote = ha.clone();
        remote.txt.insert("internal_url".into(), "https://ha.example.com".into());
        assert_eq!(from_mdns(&remote).unwrap().url.as_deref(), Some("http://192.168.1.10:8123"));
        let tv = MdnsService { service_type: PHILIPS_SECURE.into(), instance: "Philips TV Sala".into(), ipv4: vec![Ipv4Addr::new(192, 168, 1, 20)], port: 1926, txt: BTreeMap::new() };
        let f = from_mdns(&tv).unwrap();
        assert_eq!((f.kind.as_str(), f.name.as_str(), f.host.as_str()), ("philipsTv", "Philips TV Sala", "192.168.1.20"));
        // Nothing on the internet, nothing unknown.
        assert!(from_mdns(&MdnsService { ipv4: vec![Ipv4Addr::new(8, 8, 8, 8)], ..tv.clone() }).is_none());
        assert!(from_mdns(&MdnsService { service_type: "_other._tcp.local.".into(), ..tv }).is_none());
    }

    #[test]
    fn ssdp_answers_and_descriptions_are_read() {
        let reply = "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\nLOCATION: http://192.168.1.20:49153/nmrDescription.xml\r\nSERVER: Linux/4.9 UPnP/1.0 Philips-TV/1.0\r\nST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n";
        let h = parse_reply(reply).unwrap();
        assert_eq!(h["location"], "http://192.168.1.20:49153/nmrDescription.xml");
        assert!(parse_reply("NOTIFY * HTTP/1.1\r\nHOST: x\r\n\r\n").is_none());
        let xml = r#"<?xml version="1.0"?><root xmlns="urn:schemas-upnp-org:device-1-0"><device>
            <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
            <friendlyName>55PUS8007/12</friendlyName><manufacturer>Philips</manufacturer>
            <modelName>Philips TV DMR</modelName><modelNumber>55PUS8007/12</modelNumber>
            <iconList><icon/></iconList></device></root>"#;
        let d = parse_description(xml);
        assert!(d.philips());
        assert_eq!(d.model().as_deref(), Some("55PUS8007/12"));
        assert_eq!(d.friendly_name.as_deref(), Some("55PUS8007/12"));
        let samsung = parse_description("<root><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><friendlyName>[TV] Samsung Q60 &amp; co</friendlyName><manufacturer>Samsung Electronics</manufacturer><modelName>QN55Q60</modelName></device></root>");
        assert!(!samsung.philips());
        assert_eq!(samsung.friendly_name.as_deref(), Some("[TV] Samsung Q60 & co"));
        assert_eq!(samsung.model().as_deref(), Some("QN55Q60"));
        assert_eq!(m_search("239.255.255.250").lines().next(), Some("M-SEARCH * HTTP/1.1"));
        assert!(m_search("192.168.1.20").contains("HOST: 192.168.1.20:1900"));
    }
}
