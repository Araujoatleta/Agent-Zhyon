// A media renderer's volume and mute, the UPnP way (RenderingControl), as Samsung TVs — and many other TVs and
// speakers — answer it on the local network. The renderer is found by SSDP, asked to its own address only
// (discovery.rs); its control address is read from its description; then each call is one small SOAP request.
// Only local addresses are contacted, and only on the device that described itself.

use std::net::Ipv4Addr;
use std::time::Duration;

use reqwest::Url;

use super::discovery;
use crate::i18n::{t, tf};
use crate::net;

pub const RENDERING_CONTROL: &str = "urn:schemas-upnp-org:service:RenderingControl:1";
const TIMEOUT: Duration = Duration::from_secs(4);
const MAX_ANSWER: usize = 64 * 1024;

/// The control address of `service` in the description `xml` read at `location`. None when the device does not offer
/// it, or names an address on another device.
pub fn control_url(xml: &str, location: &Url, service: &str) -> Option<Url> {
    let base = discovery::tag(xml, "URLBase")
        .and_then(|b| Url::parse(&b).ok())
        .filter(|b| b.host_str() == location.host_str())
        .unwrap_or_else(|| location.clone());
    let mut rest = xml;
    while let Some(start) = rest.find("<service>") {
        let after = &rest[start + "<service>".len()..];
        let end = after.find("</service>")?;
        let block = &after[..end];
        if discovery::tag(block, "serviceType").as_deref() == Some(service) {
            let url = base.join(&discovery::tag(block, "controlURL")?).ok()?;
            return (url.scheme() == "http" && url.host_str() == location.host_str()).then_some(url);
        }
        rest = &after[end..];
    }
    None
}

/// The renderer at `ip`, asked directly, and its RenderingControl's address. None when it has none.
pub async fn rendering_control(ip: Ipv4Addr) -> Option<Url> {
    let found = discovery::search(Some(ip), Duration::from_millis(1500)).await;
    let location = Url::parse(found.get(&ip)?).ok()?;
    let xml = discovery::fetch_description(location.as_str()).await?;
    control_url(&xml, &location, RENDERING_CONTROL)
}

/// A SOAP request for `action` with its arguments, in order.
pub fn envelope(action: &str, args: &[(&str, String)]) -> String {
    let body: String = args.iter().map(|(name, value)| format!("<{name}>{value}</{name}>")).collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} xmlns:u=\"{RENDERING_CONTROL}\">\
         {body}</u:{action}></s:Body></s:Envelope>"
    )
}

/// One call; the answer's text.
async fn call(control: &Url, action: &str, args: &[(&str, String)]) -> Result<String, String> {
    let host = control.host_str().unwrap_or("");
    if !net::resolves_locally(host, control.port_or_known_default().unwrap_or(80)).await {
        return Err(t("Only devices on your local network can be used."));
    }
    let client = reqwest::Client::builder().no_proxy().connect_timeout(Duration::from_secs(3)).timeout(TIMEOUT).build().map_err(|e| e.to_string())?;
    let response = client
        .post(control.clone())
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header("SOAPAction", format!("\"{RENDERING_CONTROL}#{action}\""))
        .body(envelope(action, args))
        .send()
        .await
        .map_err(|_| t("The TV does not answer. It may be off with its network asleep, or at another address."))?;
    let status = response.status();
    let bytes = net::read_capped(response, MAX_ANSWER).await?;
    if !status.is_success() {
        return Err(tf("The TV answered {status}.", &[("status", &status.as_u16().to_string())]));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn master() -> Vec<(&'static str, String)> {
    vec![("InstanceID", "0".into()), ("Channel", "Master".into())]
}

fn unreadable() -> String {
    t("The TV's answer could not be read.")
}

/// The volume, 0 to 100.
pub async fn volume(control: &Url) -> Result<u8, String> {
    let answer = call(control, "GetVolume", &master()).await?;
    discovery::tag(&answer, "CurrentVolume").and_then(|v| v.trim().parse::<u32>().ok()).map(|v| v.min(100) as u8).ok_or_else(unreadable)
}

pub async fn set_volume(control: &Url, volume: u8) -> Result<(), String> {
    let mut args = master();
    args.push(("DesiredVolume", volume.min(100).to_string()));
    call(control, "SetVolume", &args).await.map(|_| ())
}

pub async fn muted(control: &Url) -> Result<bool, String> {
    let answer = call(control, "GetMute", &master()).await?;
    match discovery::tag(&answer, "CurrentMute").as_deref().map(str::trim) {
        Some("1") | Some("true") | Some("True") => Ok(true),
        Some("0") | Some("false") | Some("False") => Ok(false),
        _ => Err(unreadable()),
    }
}

pub async fn set_muted(control: &Url, muted: bool) -> Result<(), String> {
    let mut args = master();
    args.push(("DesiredMute", if muted { "1" } else { "0" }.to_string()));
    call(control, "SetMute", &args).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smart_home::philips::tests::fake_tv;

    fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    /// The shape of a Samsung TV's renderer description (/dmr on port 9197).
    const DESCRIPTION: &str = r#"<?xml version="1.0"?><root xmlns="urn:schemas-upnp-org:device-1-0"><device>
        <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><friendlyName>43" Crystal UHD</friendlyName>
        <manufacturer>Samsung Electronics</manufacturer><modelName>AllShare1.0</modelName><serviceList>
        <service><serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType><controlURL>/upnp/control/ConnectionManager1</controlURL></service>
        <service><serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType><serviceId>urn:upnp-org:serviceId:RenderingControl</serviceId>
        <controlURL>/upnp/control/RenderingControl1</controlURL><eventSubURL>/upnp/event/RenderingControl1</eventSubURL></service>
        </serviceList></device></root>"#;

    #[test]
    fn the_control_address_comes_from_the_description_and_stays_on_the_device() {
        let location = Url::parse("http://192.168.0.9:9197/dmr").unwrap();
        let url = control_url(DESCRIPTION, &location, RENDERING_CONTROL).unwrap();
        assert_eq!(url.as_str(), "http://192.168.0.9:9197/upnp/control/RenderingControl1");
        assert!(control_url(DESCRIPTION, &location, "urn:schemas-upnp-org:service:AVTransport:1").is_none());
        // A control address on another device is not followed.
        let elsewhere = DESCRIPTION.replace("/upnp/control/RenderingControl1", "http://203.0.113.5/x");
        assert!(control_url(&elsewhere, &location, RENDERING_CONTROL).is_none());
        let envelope = envelope("SetVolume", &[("InstanceID", "0".into()), ("Channel", "Master".into()), ("DesiredVolume", "15".into())]);
        assert!(envelope.contains("<u:SetVolume xmlns:u=\"urn:schemas-upnp-org:service:RenderingControl:1\"><InstanceID>0</InstanceID><Channel>Master</Channel><DesiredVolume>15</DesiredVolume></u:SetVolume>"));
    }

    #[test]
    fn volume_and_mute_are_read_and_set_on_the_renderer() {
        let (addr, seen) = fake_tv(|e| {
            let answer = |inner: &str| format!("<s:Envelope><s:Body>{inner}</s:Body></s:Envelope>");
            Some(match () {
                _ if e.body.contains("<u:GetVolume") => (200, answer("<u:GetVolumeResponse><CurrentVolume>12</CurrentVolume></u:GetVolumeResponse>")),
                _ if e.body.contains("<u:GetMute") => (200, answer("<u:GetMuteResponse><CurrentMute>1</CurrentMute></u:GetMuteResponse>")),
                _ if e.body.contains("<u:Set") => (200, answer("<u:SetVolumeResponse/>")),
                _ => (500, String::new()),
            })
        });
        let control = Url::parse(&format!("http://{addr}/upnp/control/RenderingControl1")).unwrap();
        assert_eq!(block_on(volume(&control)), Ok(12));
        assert_eq!(block_on(muted(&control)), Ok(true));
        assert_eq!(block_on(set_volume(&control, 150)), Ok(()));
        assert_eq!(block_on(set_muted(&control, false)), Ok(()));
        let seen = seen.lock().unwrap();
        assert!(seen.iter().all(|e| e.method == "POST" && e.path == "/upnp/control/RenderingControl1"));
        assert!(seen[2].body.contains("<DesiredVolume>100</DesiredVolume>"), "capped at 100");
        assert!(seen[3].body.contains("<DesiredMute>0</DesiredMute>"));
        // An address on the internet is never called.
        let far = Url::parse("http://8.8.8.8:9197/upnp/control/RenderingControl1").unwrap();
        assert!(block_on(volume(&far)).unwrap_err().contains("local network"));
    }
}
