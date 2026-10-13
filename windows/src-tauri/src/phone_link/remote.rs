// Away from home, directly (docs/IPHONE_PC.md): the phone reaches this computer from anywhere with no server of anyone
// in between. Coucou asks the user's own router (UPnP Internet Gateway Device: the way home routers let game consoles
// and video calls in) to forward one TCP port to this computer, reads the home's internet address from that same
// router, and lists this computer's own IPv6 addresses, which a phone can reach when the router lets them in (Coucou
// asks it to, when it offers IPv6 pinholes). The paired phones are told these addresses over the link; away from home
// they connect to them with the same pinned certificate and the same pairing code as on the local network.
//
// Off by default. Turning it off closes the port and the pinholes again. Only the router is contacted — found by SSDP
// on the local network and checked to be a local address — never an address-lookup service, a relay or anyone's DNS.
// What cannot work is said plainly: a provider that shares one internet address between many homes (CGNAT) lets no
// one in over IPv4, and a router that keeps IPv6 closed keeps it closed.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::Url;
use serde::Serialize;

use crate::i18n::{t, tf};
use crate::smart_home::{discovery, upnp};

/// What a router answers to, newest first.
const IGD_TARGETS: [&str; 3] = [
    "urn:schemas-upnp-org:device:InternetGatewayDevice:2",
    "urn:schemas-upnp-org:device:InternetGatewayDevice:1",
    "urn:schemas-upnp-org:service:WANIPConnection:1",
];
/// The services that forward a port, in the order they are preferred.
const WAN_SERVICES: [&str; 3] = [
    "urn:schemas-upnp-org:service:WANIPConnection:2",
    "urn:schemas-upnp-org:service:WANIPConnection:1",
    "urn:schemas-upnp-org:service:WANPPPConnection:1",
];
const FIREWALL6: &str = "urn:schemas-upnp-org:service:WANIPv6FirewallControl:1";
/// How long the router keeps what it opened; renewed well before (`RENEW`).
const LEASE: u32 = 3600;
pub const RENEW: Duration = Duration::from_secs(10 * 60);
/// Ports tried on the router when the first is taken by another device.
const PORT_TRIES: u16 = 10;
const DESCRIPTION: &str = "Coucou phone link";

// ── What the check found ──────────────────────────────────────────────────────

/// Where this computer can be reached from the internet, and why not where it cannot.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reach {
    /// The router's name, when one answered.
    pub router: Option<String>,
    /// The home's internet address with the port opened on the router: `198.51.100.7:47821`.
    pub v4: Option<String>,
    /// Why there is none, in the user's words.
    pub v4_problem: Option<String>,
    /// This computer's own internet addresses (IPv6), with the port: `[2804:…]:47821`.
    pub v6: Vec<String>,
    /// What the router said about letting them in.
    pub v6_note: Option<String>,
    /// The router confirmed they are let in (pinholes opened, or no IPv6 firewall).
    pub v6_confirmed: bool,
    /// The router's IPv6 firewall lets nothing in: they are not given to the phone.
    pub v6_blocked: bool,
}

impl Reach {
    /// Where a phone may try, in order (each way waits a moment for the one before): what the router confirmed
    /// first, IPv6 before IPv4 when both are confirmed (no translation on the way); never what it said is closed.
    pub fn endpoints(&self) -> Vec<String> {
        let v6: Vec<String> = if self.v6_blocked { Vec::new() } else { self.v6.clone() };
        if self.v6_confirmed || self.v4.is_none() {
            v6.into_iter().chain(self.v4.clone()).collect()
        } else {
            self.v4.clone().into_iter().chain(v6).collect()
        }
    }
}

/// What the router's IPv6 firewall did for one address.
#[derive(Debug, Clone, PartialEq)]
pub enum Pinhole {
    /// Let in; the id closes it again.
    Opened(String),
    /// The firewall is off: nothing to open, the address can be reached.
    NoFirewall,
    /// The firewall is on and lets nothing in.
    Closed,
    /// The router did not say: it may or may not let it in.
    Unknown(String),
}

/// What Coucou opened on the router, to renew and to close again.
#[derive(Debug, Clone, Default)]
pub struct Opened {
    wan: Option<(Url, &'static str)>,
    port: Option<u16>,
    firewall: Option<Url>,
    pinholes: Vec<String>,
}

/// The router, as far as Coucou uses it.
#[derive(Debug, Clone, PartialEq)]
pub struct Router {
    pub name: String,
    /// The control address of the service that forwards ports, and which service it is.
    pub wan: Option<(Url, &'static str)>,
    /// The control address of the IPv6 firewall, when the router offers one.
    pub firewall: Option<Url>,
}

/// An internet address a phone can come in by, or why this one cannot be.
pub fn classify_v4(ip: Ipv4Addr) -> Result<(), String> {
    let [a, b, ..] = ip.octets();
    if a == 100 && (64..128).contains(&b) {
        return Err(t("Your internet provider shares one internet address between many homes (CGNAT): nothing can come in over IPv4. IPv6 may still work."));
    }
    if ip.is_private() {
        return Err(t("The router's internet address is a private one: it is behind the internet provider's box, or the provider shares addresses between homes. Put the box in bridge mode, ask the provider for a public IP, or use IPv6."));
    }
    if ip.is_unspecified() || ip.is_loopback() || ip.is_link_local() || ip.is_broadcast() || ip.is_multicast() {
        return Err(t("The router has no internet address right now."));
    }
    Ok(())
}

/// The addresses of `all` a phone on the internet can reach: global unicast IPv6 (not documentation, not 6to4 or
/// Teredo), at most two (the computer's stable one and a temporary one, usually: each one the phone tries in vain costs
/// it a moment).
pub fn global_v6(all: impl IntoIterator<Item = Ipv6Addr>) -> Vec<Ipv6Addr> {
    let mut out: Vec<Ipv6Addr> = Vec::new();
    for ip in all {
        let s = ip.segments();
        let global = (s[0] & 0xe000) == 0x2000;
        let excluded = (s[0] == 0x2001 && s[1] == 0x0db8) || s[0] == 0x2002 || (s[0] == 0x2001 && s[1] == 0);
        if global && !excluded && !out.contains(&ip) && out.len() < 2 {
            out.push(ip);
        }
    }
    out
}

/// This computer's own internet (IPv6) addresses, on the interfaces that are up.
pub fn local_v6() -> Vec<Ipv6Addr> {
    let interfaces = if_addrs::get_if_addrs().unwrap_or_default();
    global_v6(interfaces.into_iter().filter(|i| i.is_oper_up() && !i.is_loopback()).filter_map(|i| match i.ip() {
        IpAddr::V6(ip) => Some(ip),
        IpAddr::V4(_) => None,
    }))
}

// ── The router ────────────────────────────────────────────────────────────────

/// The router described by `xml` (read at `location`): its name and the services Coucou uses.
pub fn router_from(xml: &str, location: &Url) -> Router {
    let name = discovery::tag(xml, "friendlyName")
        .or_else(|| discovery::tag(xml, "modelName"))
        .map(|n| n.trim().chars().take(60).collect::<String>())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| location.host_str().unwrap_or("").to_string());
    let wan = WAN_SERVICES.iter().find_map(|service| upnp::control_url(xml, location, service).map(|url| (url, *service)));
    Router { name, wan, firewall: upnp::control_url(xml, location, FIREWALL6) }
}

/// The router on the local network, by SSDP: the first one that offers to forward ports.
pub async fn find_router(wait: Duration) -> Option<Router> {
    let socket = tokio::net::UdpSocket::bind(("0.0.0.0", 0)).await.ok()?;
    let _ = socket.set_multicast_ttl_v4(2);
    let to = SocketAddr::from((Ipv4Addr::new(239, 255, 255, 250), 1900));
    for target in IGD_TARGETS {
        let request = discovery::m_search_for("239.255.255.250", target);
        // UDP may drop one: ask twice.
        for _ in 0..2 {
            let _ = socket.send_to(request.as_bytes(), to).await;
        }
    }
    let deadline = tokio::time::Instant::now() + wait;
    let mut buf = vec![0u8; 4096];
    let mut seen: Vec<String> = Vec::new();
    let mut fallback: Option<Router> = None;
    while let Ok(Ok((n, from))) = tokio::time::timeout_at(deadline, socket.recv_from(&mut buf)).await {
        if !crate::net::is_local_ip(from.ip()) {
            continue;
        }
        let Some(headers) = discovery::parse_reply(&String::from_utf8_lossy(&buf[..n])) else { continue };
        let Some(location) = headers.get("location").cloned() else { continue };
        // The description must be on the device that answered.
        let Ok(url) = Url::parse(&location) else { continue };
        if url.host_str() != Some(from.ip().to_string().as_str()) || seen.contains(&location) {
            continue;
        }
        seen.push(location.clone());
        let Some(xml) = discovery::fetch_description(&location).await else { continue };
        let router = router_from(&xml, &url);
        if router.wan.is_some() {
            return Some(router);
        }
        fallback.get_or_insert(router);
    }
    fallback
}

fn fault_text(f: &upnp::Fault) -> String {
    match (f.code, f.description.is_empty()) {
        (Some(code), false) => format!("{code} {}", f.description),
        (Some(code), true) => code.to_string(),
        (None, _) => f.status.map(|s| format!("HTTP {s}")).unwrap_or_else(|| t("no answer")),
    }
}

/// The home's internet address, as the router knows it.
pub async fn external_ip(control: &Url, service: &str) -> Result<Ipv4Addr, String> {
    let answer = upnp::soap(control, service, "GetExternalIPAddress", &[]).await.map_err(|f| fault_text(&f))?;
    discovery::tag(&answer, "NewExternalIPAddress").and_then(|ip| ip.trim().parse().ok()).ok_or_else(|| t("no answer"))
}

/// Forwards a port of the router to `port` on `client`: `prefer` first (the one opened before, so a phone away keeps
/// reaching it), then the same port when it is free, else one of the next ones. Returns the router's port.
pub async fn map_port(control: &Url, service: &str, client: Ipv4Addr, port: u16, prefer: Option<u16>) -> Result<u16, String> {
    let mut last = String::new();
    let candidates = prefer.into_iter().chain((port..port.saturating_add(PORT_TRIES)).filter(|p| Some(*p) != prefer));
    for external in candidates {
        let mut lease = LEASE;
        loop {
            let args = [
                ("NewRemoteHost", String::new()),
                ("NewExternalPort", external.to_string()),
                ("NewProtocol", "TCP".to_string()),
                ("NewInternalPort", port.to_string()),
                ("NewInternalClient", client.to_string()),
                ("NewEnabled", "1".to_string()),
                ("NewPortMappingDescription", DESCRIPTION.to_string()),
                ("NewLeaseDuration", lease.to_string()),
            ];
            match upnp::soap(control, service, "AddPortMapping", &args).await {
                Ok(_) => return Ok(external),
                // Only permanent forwards: Coucou closes it itself when the switch goes off.
                Err(f) if f.code == Some(725) && lease != 0 => lease = 0,
                // Taken by another device: the next port.
                Err(f) if matches!(f.code, Some(718) | Some(724) | Some(727)) => {
                    last = fault_text(&f);
                    break;
                }
                Err(f) => return Err(fault_text(&f)),
            }
        }
    }
    Err(last)
}

pub async fn unmap(control: &Url, service: &str, external: u16) {
    let args = [("NewRemoteHost", String::new()), ("NewExternalPort", external.to_string()), ("NewProtocol", "TCP".to_string())];
    let _ = upnp::soap(control, service, "DeletePortMapping", &args).await;
}

/// Lets `addr`:`port` in through the router's IPv6 firewall.
pub async fn open_pinhole(control: &Url, addr: Ipv6Addr, port: u16) -> Pinhole {
    let status = match upnp::soap(control, FIREWALL6, "GetFirewallStatus", &[]).await {
        Ok(status) => status,
        Err(f) => return Pinhole::Unknown(fault_text(&f)),
    };
    // A flag the answer does not carry is not taken for "off".
    let flag = |name: &str| discovery::tag(&status, name).map(|v| matches!(v.trim(), "1" | "true" | "True"));
    match (flag("FirewallEnabled"), flag("InboundPinholeAllowed")) {
        (Some(false), _) => return Pinhole::NoFirewall,
        (Some(true), Some(false)) => return Pinhole::Closed,
        (Some(true), Some(true)) => {}
        _ => return Pinhole::Unknown(t("no answer")),
    }
    let args = [
        ("RemoteHost", String::new()),
        ("RemotePort", "0".to_string()),
        ("InternalClient", addr.to_string()),
        ("InternalPort", port.to_string()),
        ("Protocol", "6".to_string()),
        ("LeaseTime", LEASE.to_string()),
    ];
    match upnp::soap(control, FIREWALL6, "AddPinhole", &args).await {
        Ok(answer) => match discovery::tag(&answer, "UniqueID").map(|id| id.trim().to_string()).filter(|id| !id.is_empty() && id.len() < 16) {
            Some(id) => Pinhole::Opened(id),
            None => Pinhole::Unknown(t("no answer")),
        },
        Err(f) => Pinhole::Unknown(fault_text(&f)),
    }
}

pub async fn close_pinhole(control: &Url, id: &str) {
    let _ = upnp::soap(control, FIREWALL6, "DeletePinhole", &[("UniqueID", id.to_string())]).await;
}

// ── One check ─────────────────────────────────────────────────────────────────

/// Finds the router, forwards the port (again: a renewal), asks the home's address, and lets this computer's IPv6
/// addresses in where the router allows it. `client`: this computer's address on the local network.
pub async fn check(router: Option<Router>, client: Option<Ipv4Addr>, port: u16, v6: Vec<Ipv6Addr>, before: &Opened) -> (Reach, Opened) {
    let mut reach = Reach::default();
    let mut opened = Opened::default();
    reach.v6 = v6.iter().map(|ip| SocketAddr::from((*ip, port)).to_string()).collect();
    let Some(router) = router else {
        reach.v4_problem = Some(t("No router answered on the local network: turn on UPnP (Universal Plug and Play) in your router's settings."));
        if !reach.v6.is_empty() {
            reach.v6_note = Some(t("Reachable if your router lets IPv6 connections in."));
        }
        return (reach, opened);
    };
    reach.router = Some(router.name.clone());

    // IPv4: the router's internet address, and a port forwarded to this computer.
    match (&router.wan, client) {
        (None, _) => reach.v4_problem = Some(t("This router does not offer to open ports (UPnP).")),
        (Some(_), None) => reach.v4_problem = Some(t("This computer has no address on the local network.")),
        (Some((control, service)), Some(client)) => {
            opened.wan = Some((control.clone(), service));
            match external_ip(control, service).await {
                Err(e) => reach.v4_problem = Some(tf("The router did not say its internet address ({error}).", &[("error", &e)])),
                Ok(ip) => match classify_v4(ip) {
                    Err(why) => reach.v4_problem = Some(why),
                    Ok(()) => match map_port(control, service, client, port, before.port).await {
                        Ok(external) => {
                            opened.port = Some(external);
                            reach.v4 = Some(SocketAddr::from((ip, external)).to_string());
                        }
                        Err(e) => reach.v4_problem = Some(tf("The router refused to open the port ({error}).", &[("error", &e)])),
                    },
                },
            }
        }
    }
    // The port opened before on another number, now unused: closed again.
    if let (Some((control, service)), Some(old)) = (&before.wan, before.port) {
        if opened.port != Some(old) {
            unmap(control, service, old).await;
        }
    }

    // IPv6: pinholes for this computer's addresses, when the router has a firewall that takes them.
    if let Some(firewall) = &router.firewall {
        for id in &before.pinholes {
            close_pinhole(&before.firewall.clone().unwrap_or_else(|| firewall.clone()), id).await;
        }
        opened.firewall = Some(firewall.clone());
        let mut note = None;
        for ip in &v6 {
            match open_pinhole(firewall, *ip, port).await {
                Pinhole::Opened(id) => {
                    opened.pinholes.push(id);
                    reach.v6_confirmed = true;
                    note = Some(t("The router lets these addresses in."));
                }
                Pinhole::NoFirewall => {
                    reach.v6_confirmed = true;
                    note = Some(t("The router's IPv6 firewall is off: these addresses can be reached."));
                }
                Pinhole::Closed => {
                    reach.v6_blocked = true;
                    note = Some(t("The router does not let anything in over IPv6: turn on its option for incoming IPv6 connections, or use IPv4."));
                    break;
                }
                Pinhole::Unknown(e) => note = Some(tf("The router did not open IPv6 ({error}): they can be reached only if it lets them in.", &[("error", &e)])),
            }
        }
        reach.v6_note = note;
    } else if !reach.v6.is_empty() {
        reach.v6_note = Some(t("Reachable if your router lets IPv6 connections in."));
    }
    (reach, opened)
}

/// Closes what Coucou opened on the router.
pub async fn close(opened: &Opened) {
    if let (Some((control, service)), Some(port)) = (&opened.wan, opened.port) {
        unmap(control, service, port).await;
    }
    if let Some(firewall) = &opened.firewall {
        for id in &opened.pinholes {
            close_pinhole(firewall, id).await;
        }
    }
}

// ── While the switch is on ────────────────────────────────────────────────────

/// The check, again every few minutes (a renewal, and a new internet address is caught) until stopped; then what was
/// opened is closed. `changed` hears every new reach.
pub struct Handle {
    stop: Arc<tokio::sync::Notify>,
    again: Arc<tokio::sync::Notify>,
    pub reach: Arc<Mutex<Option<Reach>>>,
    opened: Arc<Mutex<Opened>>,
    task: tauri::async_runtime::JoinHandle<()>,
}

impl Handle {
    pub fn check_now(&self) {
        self.again.notify_one();
    }

    /// Stops renewing and closes what was opened, in the background. The task is returned so a new start waits for
    /// it: the old one's closing must never undo what the new one opens.
    pub fn stop(self) -> tauri::async_runtime::JoinHandle<()> {
        self.stop.notify_one();
        self.task
    }

    /// The app is quitting: closes what was opened before returning, a few seconds at most.
    pub fn stop_now(self) {
        self.task.abort();
        let opened = self.opened.lock().unwrap().clone();
        tauri::async_runtime::block_on(async move {
            let _ = tokio::time::timeout(Duration::from_secs(3), close(&opened)).await;
        });
    }
}

/// `v6`: this computer listens on IPv6 too, so its IPv6 addresses are ways in.
pub fn start(port: u16, v6: bool, after: Option<tauri::async_runtime::JoinHandle<()>>, changed: impl Fn(&Reach) + Send + Sync + 'static) -> Handle {
    let (stop, again) = (Arc::new(tokio::sync::Notify::new()), Arc::new(tokio::sync::Notify::new()));
    let reach: Arc<Mutex<Option<Reach>>> = Arc::new(Mutex::new(None));
    let shared_opened: Arc<Mutex<Opened>> = Arc::new(Mutex::new(Opened::default()));
    let (stop_rx, again_rx, reach_w, opened_w) = (stop.clone(), again.clone(), reach.clone(), shared_opened.clone());
    let task = tauri::async_runtime::spawn(async move {
        let (stop, again, reach) = (stop_rx, again_rx, reach_w);
        if let Some(previous) = after {
            let _ = previous.await;
        }
        let mut opened = Opened::default();
        let mut last_router: Option<Router> = None;
        loop {
            // A router that missed one search (UDP, or busy) is asked at the address it gave before.
            let router = find_router(Duration::from_millis(2500)).await.or_else(|| last_router.clone());
            last_router = router.clone();
            let client = super::pairing::lan_address().and_then(|ip| match ip {
                IpAddr::V4(v4) => Some(v4),
                IpAddr::V6(_) => None,
            });
            let addresses = if v6 { local_v6() } else { Vec::new() };
            let (now, open) = check(router, client, port, addresses, &opened).await;
            opened = open;
            *opened_w.lock().unwrap() = opened.clone();
            let differs = reach.lock().unwrap().as_ref() != Some(&now);
            *reach.lock().unwrap() = Some(now.clone());
            if differs {
                changed(&now);
            }
            tokio::select! {
                _ = tokio::time::sleep(RENEW) => {}
                _ = again.notified() => {}
                _ = stop.notified() => break,
            }
        }
        close(&opened).await;
        *opened_w.lock().unwrap() = Opened::default();
        crate::log::line("phone link: away from home, directly: closed on the router");
    });
    Handle { stop, again, reach, opened: shared_opened, task }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smart_home::philips::tests::fake_tv;

    fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    #[test]
    fn a_shared_or_private_internet_address_lets_no_one_in() {
        assert!(classify_v4("198.51.100.7".parse().unwrap()).is_ok());
        assert!(classify_v4("100.70.1.2".parse().unwrap()).unwrap_err().contains("CGNAT"));
        assert!(classify_v4("192.168.1.10".parse().unwrap()).unwrap_err().contains("private"));
        assert!(classify_v4("0.0.0.0".parse().unwrap()).is_err());
        assert!(classify_v4("100.128.0.1".parse().unwrap()).is_ok(), "just past the shared range");
    }

    #[test]
    fn only_internet_ipv6_addresses_are_given_to_the_phone() {
        let all: Vec<Ipv6Addr> = ["fe80::1", "fd00::5", "::1", "2001:db8::1", "2804:14c:1:2::10", "2002:c000:204::1", "2001:0:4136::1", "2804:14c:1:2::10", "2a02:1::2"]
            .iter()
            .map(|s| s.parse().unwrap())
            .collect();
        assert_eq!(global_v6(all.clone()), vec!["2804:14c:1:2::10".parse::<Ipv6Addr>().unwrap(), "2a02:1::2".parse().unwrap()]);
        let mut more = all;
        more.push("2a02:1::3".parse().unwrap());
        assert_eq!(global_v6(more).len(), 2, "at most two");
    }

    #[test]
    fn what_the_router_confirmed_is_tried_first_and_what_it_closed_never() {
        let v4 = Some("198.51.100.7:47821".to_string());
        let v6 = vec!["[2804:14c:1:2::10]:47821".to_string()];
        let confirmed = Reach { v4: v4.clone(), v6: v6.clone(), v6_confirmed: true, ..Default::default() };
        assert_eq!(confirmed.endpoints(), vec![v6[0].clone(), v4.clone().unwrap()], "both confirmed: IPv6 first");
        let unsure = Reach { v4: v4.clone(), v6: v6.clone(), ..Default::default() };
        assert_eq!(unsure.endpoints(), vec![v4.clone().unwrap(), v6[0].clone()], "IPv4 confirmed, IPv6 maybe: IPv4 first");
        let closed = Reach { v4: v4.clone(), v6: v6.clone(), v6_blocked: true, ..Default::default() };
        assert_eq!(closed.endpoints(), vec![v4.unwrap()], "closed IPv6 is not given");
        let only_v6 = Reach { v6: v6.clone(), ..Default::default() };
        assert_eq!(only_v6.endpoints(), v6, "IPv6 alone is still worth a try");
    }

    const DESCRIPTION_XML: &str = r#"<?xml version="1.0"?><root xmlns="urn:schemas-upnp-org:device-1-0"><device>
        <deviceType>urn:schemas-upnp-org:device:InternetGatewayDevice:2</deviceType><friendlyName>Archer C6</friendlyName>
        <deviceList><device><deviceType>urn:schemas-upnp-org:device:WANDevice:2</deviceType><serviceList>
        <service><serviceType>urn:schemas-upnp-org:service:WANIPv6FirewallControl:1</serviceType><controlURL>/upnp/control/fw6</controlURL></service>
        </serviceList><deviceList><device><deviceType>urn:schemas-upnp-org:device:WANConnectionDevice:2</deviceType><serviceList>
        <service><serviceType>urn:schemas-upnp-org:service:WANIPConnection:1</serviceType><controlURL>/upnp/control/wan</controlURL></service>
        </serviceList></device></deviceList></device></deviceList></device></root>"#;

    #[test]
    fn the_router_says_which_services_open_ports_and_ipv6() {
        let location = Url::parse("http://192.168.0.1:1900/igd.xml").unwrap();
        let r = router_from(DESCRIPTION_XML, &location);
        assert_eq!(r.name, "Archer C6");
        let (wan, service) = r.wan.unwrap();
        assert_eq!((wan.as_str(), service), ("http://192.168.0.1:1900/upnp/control/wan", "urn:schemas-upnp-org:service:WANIPConnection:1"));
        assert_eq!(r.firewall.unwrap().as_str(), "http://192.168.0.1:1900/upnp/control/fw6");
        let bare = router_from("<root><device><modelName>Box</modelName></device></root>", &location);
        assert_eq!((bare.name.as_str(), bare.wan.is_none(), bare.firewall.is_none()), ("Box", true, true));
    }

    /// A router on loopback: its address, a port taken once, then given; the IPv6 firewall on, pinholes allowed.
    fn fake_router() -> (Url, Arc<std::sync::Mutex<Vec<crate::smart_home::philips::tests::Exchange>>>) {
        let (addr, seen) = fake_tv(|e| {
            let ok = |inner: &str| (200, format!("<s:Envelope><s:Body>{inner}</s:Body></s:Envelope>"));
            let fault = |code: u32, text: &str| {
                (500, format!("<s:Envelope><s:Body><s:Fault><detail><UPnPError><errorCode>{code}</errorCode><errorDescription>{text}</errorDescription></UPnPError></detail></s:Fault></s:Body></s:Envelope>"))
            };
            Some(match () {
                _ if e.body.contains("<u:GetExternalIPAddress") => ok("<u:GetExternalIPAddressResponse><NewExternalIPAddress>198.51.100.7</NewExternalIPAddress></u:GetExternalIPAddressResponse>"),
                _ if e.body.contains("<u:AddPortMapping") && e.body.contains("<NewExternalPort>47821<") => fault(718, "ConflictInMappingEntry"),
                _ if e.body.contains("<u:AddPortMapping") && e.body.contains("<NewLeaseDuration>3600<") => fault(725, "OnlyPermanentLeasesSupported"),
                _ if e.body.contains("<u:AddPortMapping") => ok("<u:AddPortMappingResponse/>"),
                _ if e.body.contains("<u:DeletePortMapping") => ok("<u:DeletePortMappingResponse/>"),
                _ if e.body.contains("<u:GetFirewallStatus") => ok("<u:GetFirewallStatusResponse><FirewallEnabled>1</FirewallEnabled><InboundPinholeAllowed>1</InboundPinholeAllowed></u:GetFirewallStatusResponse>"),
                _ if e.body.contains("<u:AddPinhole") => ok("<u:AddPinholeResponse><UniqueID>7</UniqueID></u:AddPinholeResponse>"),
                _ if e.body.contains("<u:DeletePinhole") => ok("<u:DeletePinholeResponse/>"),
                _ => (500, String::new()),
            })
        });
        (Url::parse(&format!("http://{addr}/")).unwrap(), seen)
    }

    #[test]
    fn the_port_is_forwarded_the_address_read_and_ipv6_let_in_then_all_closed_again() {
        let (base, seen) = fake_router();
        let router = Router {
            name: "Archer C6".into(),
            wan: Some((base.join("wan").unwrap(), WAN_SERVICES[1])),
            firewall: Some(base.join("fw6").unwrap()),
        };
        let v6: Vec<Ipv6Addr> = vec!["2804:14c:1:2::10".parse().unwrap()];
        let (reach, opened) = block_on(check(Some(router), Some(Ipv4Addr::new(192, 168, 0, 20)), 47821, v6, &Opened::default()));
        assert_eq!(reach.router.as_deref(), Some("Archer C6"));
        assert_eq!(reach.v4.as_deref(), Some("198.51.100.7:47822"), "the next port, after the first was taken");
        assert_eq!(reach.v6, vec!["[2804:14c:1:2::10]:47821".to_string()]);
        assert_eq!(reach.v6_note.as_deref(), Some("The router lets these addresses in."));
        assert_eq!(reach.endpoints().len(), 2);
        {
            let seen = seen.lock().unwrap();
            let forward = seen.iter().filter(|e| e.body.contains("<u:AddPortMapping")).last().unwrap();
            assert!(forward.body.contains("<NewInternalClient>192.168.0.20</NewInternalClient>") && forward.body.contains("<NewLeaseDuration>0<"), "a permanent forward when the router takes no other");
            assert!(seen.iter().any(|e| e.body.contains("<u:AddPinhole") && e.body.contains("<InternalClient>2804:14c:1:2::10</InternalClient>")));
        }
        // A renewal asks for the port it has first, so a phone away keeps reaching it.
        let router = Router { name: "Archer C6".into(), wan: Some((base.join("wan").unwrap(), WAN_SERVICES[1])), firewall: Some(base.join("fw6").unwrap()) };
        let before = seen.lock().unwrap().len();
        let (again, opened) = block_on(check(Some(router), Some(Ipv4Addr::new(192, 168, 0, 20)), 47821, vec!["2804:14c:1:2::10".parse().unwrap()], &opened));
        assert_eq!(again.v4.as_deref(), Some("198.51.100.7:47822"));
        {
            let seen = seen.lock().unwrap();
            let first = seen[before..].iter().find(|e| e.body.contains("<u:AddPortMapping")).unwrap();
            assert!(first.body.contains("<NewExternalPort>47822<"), "the port it had, not 47821 again");
            assert!(seen[before..].iter().any(|e| e.body.contains("<u:DeletePinhole") && e.body.contains("<UniqueID>7<")), "the old pinhole is replaced");
            assert!(!seen[before..].iter().any(|e| e.body.contains("<u:DeletePortMapping")), "the port kept is not closed");
        }
        block_on(close(&opened));
        let seen = seen.lock().unwrap();
        assert!(seen.iter().any(|e| e.body.contains("<u:DeletePortMapping") && e.body.contains("<NewExternalPort>47822<")));
        assert!(seen.iter().any(|e| e.body.contains("<u:DeletePinhole") && e.body.contains("<UniqueID>7<")));
    }

    #[test]
    fn without_a_router_or_behind_cgnat_it_says_why() {
        let (reach, _) = block_on(check(None, Some(Ipv4Addr::new(192, 168, 0, 20)), 47821, vec![], &Opened::default()));
        assert!(reach.v4_problem.as_deref().unwrap().contains("UPnP"));
        assert!(reach.endpoints().is_empty());
        let (addr, _) = fake_tv(|_| Some((200, "<s:Envelope><s:Body><NewExternalIPAddress>100.70.1.2</NewExternalIPAddress></s:Body></s:Envelope>".into())));
        let base = Url::parse(&format!("http://{addr}/")).unwrap();
        let router = Router { name: "Box".into(), wan: Some((base.join("wan").unwrap(), WAN_SERVICES[1])), firewall: None };
        let (reach, opened) = block_on(check(Some(router), Some(Ipv4Addr::new(192, 168, 0, 20)), 47821, vec!["2804::1".parse().unwrap()], &Opened::default()));
        assert!(reach.v4.is_none() && reach.v4_problem.as_deref().unwrap().contains("CGNAT"));
        assert_eq!(reach.endpoints(), vec!["[2804::1]:47821".to_string()], "IPv6 still offered");
        assert!(opened.port.is_none(), "nothing was opened");
    }
}
