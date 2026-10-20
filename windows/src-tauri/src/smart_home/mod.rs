// Mochi's home: the devices of the house Mochi may use from the chat — through Home Assistant's official REST API,
// or straight over the local network for Philips TVs — with the user's rooms, routines and rules. See
// docs/SMART_HOME.md.
//
// Off by default (Settings → Mochi's home). While it is off, nothing is read and nothing runs. When it is on:
//
//   * The devices come from Home Assistant (media players, lights, switches) and from the TVs the user added: Philips,
//     Samsung, and Android TVs, projectors and boxes over ADB (android.rs).
//     What each can do is what it reported (model.rs), never assumed from its brand.
//   * Mochi only sees the devices the user ticked, and acts through three tools of his hands (agent/tools.rs):
//     home_devices, home_control, home_routine. Each action is checked against the device before anything is sent,
//     and becomes one call of a fixed list; plugs, switches and the devices set to ask wait for a click (Allow on
//     the island, or on the phone behind Face ID), and "Ask before every action" makes all of them wait.
//   * After each action the device is asked again, and Mochi says what it reports now ("the TV is on, YouTube is
//     on screen"), not what was hoped.
//   * Every action is written in the home's record (journal.rs). Tokens and pairings live in the OS keystore
//     (config.rs). Only addresses on the local network are ever contacted.
//
// The pieces: model.rs (devices, actions, checks), home_assistant.rs and philips.rs (the two ways in), digest.rs
// (the TVs' authentication), discovery.rs (mDNS and SSDP), routines.rs, journal.rs, config.rs. This file holds the
// state, the reading of the house, the carrying out of an action, Mochi's way in, and Settings' commands.

pub mod android;
pub mod config;
pub mod digest;
pub mod discovery;
pub mod home_assistant;
pub mod journal;
pub mod model;
pub mod philips;
pub mod routines;
pub mod samsung;
pub mod upnp;

use std::collections::{BTreeMap, HashMap};
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::Url;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::agent::{BoxFuture, HomeEnv, HomeRunner, HomeResult, RoutineInfo};
use crate::i18n::{t, tf};
use crate::phone_link::only_settings;
use crate::{log, net, settings, Shared};
use config::{DevicePrefs, TvConf, ANDROID, SAMSUNG};
use home_assistant::HomeAssistant;
use model::{Action, Capability, Device, DeviceState, Kind};
use philips::{Tv, TvError};
use routines::Routine;

/// Devices read less than this ago serve Mochi's turns.
const FRESH: Duration = Duration::from_secs(60);
/// A TV's apps and channels are read again after this.
const LISTS: Duration = Duration::from_secs(10 * 60);
/// How long a TV may take to start before the answer says it did not confirm.
const TURN_ON_WAIT: Duration = Duration::from_secs(25);
const CONFIRM_WAIT: Duration = Duration::from_secs(8);

/// The home's state: what was last read, the TVs' lists, the pairings in progress.
#[derive(Default)]
pub struct Home {
    cache: Mutex<Option<(Instant, Vec<Device>)>>,
    lists: Mutex<HashMap<String, TvLists>>,
    /// When each Samsung TV was last asked for its apps.
    samsung_asked: Mutex<HashMap<String, Instant>>,
    pairings: Mutex<HashMap<String, philips::PairingState>>,
    /// One reading of the house at a time.
    reading: tokio::sync::Mutex<()>,
}

#[derive(Clone)]
struct TvLists {
    at: Instant,
    apps: Vec<philips::App>,
    channels: Vec<philips::TvChannel>,
    volume: bool,
}

/// The settings this module reads, taken at once.
#[derive(Clone, Default)]
struct Setup {
    enabled: bool,
    ask_all: bool,
    ha_url: String,
    tvs: Vec<TvConf>,
    prefs: BTreeMap<String, DevicePrefs>,
    routines: Vec<Routine>,
}

fn setup(app: &AppHandle) -> Setup {
    let shared = app.state::<Shared>();
    let s = shared.settings.lock().unwrap();
    Setup {
        enabled: s.home_enabled,
        ask_all: s.home_ask_all,
        ha_url: s.home_assistant_url.clone(),
        tvs: s.home_tvs.clone(),
        prefs: s.home_devices.clone(),
        routines: s.home_routines.clone(),
    }
}

fn forget_cache(app: &AppHandle) {
    if let Some(home) = app.try_state::<Home>() {
        *home.cache.lock().unwrap() = None;
    }
}

// ── Reading the house ─────────────────────────────────────────────────────────

/// Every device, with its state. `fresh`: ask the devices now, else what was read in the last minute will do.
pub async fn read(app: &AppHandle, fresh: bool) -> Vec<Device> {
    let home = app.state::<Home>();
    let cached = || home.cache.lock().unwrap().as_ref().filter(|(at, _)| at.elapsed() < FRESH).map(|(_, d)| d.clone());
    if !fresh {
        if let Some(d) = cached() {
            return d;
        }
    }
    let _one = home.reading.lock().await;
    let s = setup(app);
    let mut devices = Vec::new();
    if let (false, Some(token)) = (s.ha_url.is_empty(), config::ha_token()) {
        match HomeAssistant::new(&s.ha_url, &token) {
            Ok(ha) => match ha.states().await {
                Ok(states) => {
                    let areas = ha.areas().await;
                    for state in &states {
                        let id = state.get("entity_id").and_then(Value::as_str).unwrap_or("");
                        let prefs = ha_prefs(s.prefs.get(&format!("ha:{id}")));
                        if let Some(mut d) = home_assistant::device(state, &prefs, areas.get(id).map(String::as_str)) {
                            d.ask |= s.ask_all;
                            devices.push(d);
                        }
                    }
                }
                Err(e) => log::line(format!("home: Home Assistant not read: {e}")),
            },
            Err(e) => log::line(format!("home: Home Assistant not usable: {e}")),
        }
    }
    let mut reads = tokio::task::JoinSet::new();
    for conf in s.tvs.clone() {
        let prefs = s.prefs.get(&conf.id).cloned().unwrap_or_default();
        if conf.samsung() {
            reads.spawn(samsung_device(app.clone(), conf, prefs, s.ask_all));
        } else if conf.android() {
            reads.spawn(android_device(app.clone(), conf, prefs, s.ask_all));
        } else {
            reads.spawn(tv_device(app.clone(), conf, prefs, s.ask_all));
        }
    }
    while let Some(Ok(d)) = reads.join_next().await {
        devices.push(d);
    }
    devices.sort_by(|a, b| (model::normalize(&a.room), model::normalize(&a.name)).cmp(&(model::normalize(&b.room), model::normalize(&b.name))));
    *home.cache.lock().unwrap() = Some((Instant::now(), devices.clone()));
    devices
}

fn ha_prefs(p: Option<&DevicePrefs>) -> home_assistant::Prefs {
    let p = p.cloned().unwrap_or_default();
    home_assistant::Prefs { name: p.name, room: p.room, exposed: p.exposed.unwrap_or(false), ask: p.ask }
}

fn tv_of(conf: &TvConf) -> Tv {
    Tv { endpoint: conf.endpoint(), creds: config::credentials(&conf.id) }
}

/// The TV's apps and channels: from the last read when recent, else from the TV.
async fn lists(app: &AppHandle, conf: &TvConf, tv: &Tv, renew: bool) -> TvLists {
    let home = app.state::<Home>();
    let old = home.lists.lock().unwrap().get(&conf.id).cloned();
    if let Some(l) = old.as_ref().filter(|l| !renew && l.at.elapsed() < LISTS) {
        return l.clone();
    }
    let system = conf.system();
    let apps = if system.apps() { tv.apps().await.ok() } else { Some(Vec::new()) };
    let channels = tv.channels().await.ok();
    let volume = tv.volume().await.is_ok();
    let fresh = TvLists {
        at: Instant::now(),
        apps: apps.or_else(|| old.as_ref().map(|o| o.apps.clone())).unwrap_or_default(),
        channels: channels.or_else(|| old.as_ref().map(|o| o.channels.clone())).unwrap_or_default(),
        volume: volume || old.as_ref().is_some_and(|o| o.volume),
    };
    home.lists.lock().unwrap().insert(conf.id.clone(), fresh.clone());
    fresh
}

async fn tv_device(app: AppHandle, conf: TvConf, prefs: DevicePrefs, ask_all: bool) -> Device {
    let tv = tv_of(&conf);
    let system = conf.system();
    let power = tv.power().await;
    let answers = !matches!(power, Err(TvError::Unreachable(_) | TvError::CertificateChanged));
    let allowed = !matches!(power, Err(TvError::Auth));
    let cached = app.state::<Home>().lists.lock().unwrap().get(&conf.id).cloned();
    let lists = if answers && allowed { Some(lists(&app, &conf, &tv, false).await) } else { cached };
    let volume = if answers && allowed { tv.volume().await.ok() } else { None };
    let on = power.as_ref().ok().copied();

    let mut caps = Vec::new();
    let can_turn_on = match &power {
        Ok(_) | Err(TvError::Auth) => true,
        Err(TvError::NotSupported) => conf.mac().is_some(),
        Err(_) => system.version.0 >= 5 || conf.mac().is_some(),
    };
    if can_turn_on {
        caps.push(Capability::TurnOn);
    }
    if system.keys() {
        caps.extend([Capability::TurnOff, Capability::VolumeStep, Capability::ChannelStep]);
    }
    if volume.is_some() || lists.as_ref().is_some_and(|l| l.volume) {
        caps.extend([Capability::VolumeSet, Capability::Mute]);
    }
    let apps: Vec<String> = lists.as_ref().map(|l| l.apps.iter().map(|a| a.label.clone()).collect()).unwrap_or_default();
    if system.apps() && !apps.is_empty() {
        caps.push(Capability::Apps);
    }
    let channels: Vec<model::Channel> = lists.as_ref().map(|l| l.channels.iter().map(|c| c.channel.clone()).collect()).unwrap_or_default();
    if !channels.is_empty() {
        caps.push(Capability::Channels);
    }
    caps.sort();
    caps.dedup();

    let mut state = DeviceState { online: answers, on, ..DeviceState::default() };
    if let Some(v) = volume {
        state.volume = Some(v.percent());
        state.muted = Some(v.muted);
    }
    if on == Some(true) && allowed {
        if let Some(l) = &lists {
            state.app = tv.current_app(&l.apps).await;
        }
        state.channel = tv.current_channel().await;
    }
    let model = (!conf.model.is_empty()).then(|| conf.model.clone()).or(system.model.clone());
    Device {
        id: conf.id.clone(),
        name: if prefs.name.trim().is_empty() { conf.name.clone() } else { prefs.name.trim().to_string() },
        room: prefs.room.trim().to_string(),
        kind: Kind::Tv,
        source: "Philips TV".into(),
        model,
        caps,
        apps,
        channels,
        state,
        exposed: prefs.exposed.unwrap_or(true),
        ask: ask_all || prefs.ask.unwrap_or(false),
    }
}

// ── Carrying out an action ────────────────────────────────────────────────────

/// What became of an action: whether it worked, and the device's state afterwards as it reported it.
struct Done {
    confirmed: bool,
    state: DeviceState,
    /// Already as asked: nothing was sent.
    already: bool,
}

/// The state in English, for the model: "on, volume 20%, YouTube on screen".
pub fn state_text(s: &DeviceState) -> String {
    if !s.online {
        return "not answering".into();
    }
    let mut parts = Vec::new();
    match s.on {
        Some(true) => parts.push("on".to_string()),
        Some(false) => parts.push("off (standby)".to_string()),
        None => {}
    }
    if let Some(v) = s.volume {
        parts.push(format!("volume {v}%"));
    }
    if s.muted == Some(true) {
        parts.push("muted".into());
    }
    if let Some(b) = s.brightness {
        parts.push(format!("brightness {b}%"));
    }
    if let Some(a) = &s.app {
        parts.push(format!("{a} on screen"));
    }
    if let Some(c) = &s.channel {
        parts.push(format!("channel {c}"));
    }
    if parts.is_empty() { "answering".into() } else { parts.join(", ") }
}

/// The state in the user's words, for the island, Settings and the record: "On · volume 20% · YouTube".
pub fn state_words(s: &DeviceState) -> String {
    if !s.online {
        return t("Not answering");
    }
    let mut parts = Vec::new();
    match s.on {
        Some(true) => parts.push(t("On")),
        Some(false) => parts.push(t("Off")),
        None => {}
    }
    if let Some(v) = s.volume {
        parts.push(tf("Volume {percent}%", &[("percent", &v.to_string())]));
    }
    if s.muted == Some(true) {
        parts.push(t("Muted"));
    }
    if let Some(b) = s.brightness {
        parts.push(tf("Brightness {percent}%", &[("percent", &b.to_string())]));
    }
    if let Some(a) = &s.app {
        parts.push(a.clone());
    }
    if let Some(c) = &s.channel {
        parts.push(tf("Channel {channel}", &[("channel", c)]));
    }
    parts.join(" · ")
}

/// Asks `read` again until `ok` holds or `wait` is over; the last state either way.
async fn until<F, R>(wait: Duration, mut read: R, ok: F) -> (bool, DeviceState)
where
    R: FnMut() -> BoxFuture<'static, DeviceState>,
    F: Fn(&DeviceState) -> bool,
{
    let deadline = Instant::now() + wait;
    loop {
        let state = read().await;
        if ok(&state) {
            return (true, state);
        }
        if Instant::now() >= deadline {
            return (false, state);
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

async fn tv_state(app: AppHandle, conf: TvConf) -> DeviceState {
    let tv = tv_of(&conf);
    let power = tv.power().await;
    let mut s = DeviceState { online: !matches!(power, Err(TvError::Unreachable(_) | TvError::CertificateChanged)), on: power.as_ref().ok().copied(), ..DeviceState::default() };
    if let Ok(v) = tv.volume().await {
        s.volume = Some(v.percent());
        s.muted = Some(v.muted);
    }
    if s.on == Some(true) {
        let apps = app.state::<Home>().lists.lock().unwrap().get(&conf.id).map(|l| l.apps.clone()).unwrap_or_default();
        s.app = tv.current_app(&apps).await;
        s.channel = tv.current_channel().await;
    }
    s
}

fn tv_error(e: TvError, conf: &TvConf) -> String {
    match e {
        TvError::Unreachable(_) if conf.mac().is_none() => format!(
            "{} {}",
            e.message(),
            t("To turn it on from standby, turn on \"Switch on with Wi-Fi\" (or \"with Chromecast\") in the TV's network settings.")
        ),
        e => e.message(),
    }
}

async fn tv_act(app: &AppHandle, conf: &TvConf, device: &Device, action: &Action) -> Result<Done, String> {
    let tv = tv_of(conf);
    let reader = {
        let (app, conf) = (app.clone(), conf.clone());
        move || -> BoxFuture<'static, DeviceState> { Box::pin(tv_state(app.clone(), conf.clone())) }
    };
    match action {
        Action::TurnOn => {
            match tv.power().await {
                Ok(true) => return Ok(Done { confirmed: true, state: tv_state(app.clone(), conf.clone()).await, already: true }),
                Ok(false) => {
                    if let Err(e) = tv.set_power(true).await {
                        // Some firmwares only take the remote's key, which toggles.
                        tv.key("Standby").await.map_err(|_| tv_error(e, conf))?;
                    }
                }
                Err(TvError::Auth) => return Err(TvError::Auth.message()),
                Err(e) => match conf.mac() {
                    Some(mac) => philips::wake(&mac).await?,
                    None => return Err(tv_error(e, conf)),
                },
            }
            let (confirmed, state) = until(TURN_ON_WAIT, reader, |s| s.on == Some(true)).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::TurnOff => {
            match tv.power().await {
                Ok(false) => return Ok(Done { confirmed: true, state: tv_state(app.clone(), conf.clone()).await, already: true }),
                Err(e @ (TvError::Unreachable(_) | TvError::Auth | TvError::CertificateChanged)) => return Err(tv_error(e, conf)),
                _ => tv.key("Standby").await.map_err(|e| tv_error(e, conf))?,
            }
            let (confirmed, state) = until(CONFIRM_WAIT, reader, |s| s.on != Some(true)).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::SetVolume(p) => {
            let v = tv.volume().await.map_err(|e| tv_error(e, conf))?;
            tv.set_volume(v.level(*p), v.muted).await.map_err(|e| tv_error(e, conf))?;
            let target = *p;
            let (confirmed, state) = until(Duration::from_secs(3), reader, move |s| s.volume.is_some_and(|v| v.abs_diff(target) <= 2)).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::VolumeUp | Action::VolumeDown => {
            tv.key(if *action == Action::VolumeUp { "VolumeUp" } else { "VolumeDown" }).await.map_err(|e| tv_error(e, conf))?;
            Ok(Done { confirmed: true, state: tv_state(app.clone(), conf.clone()).await, already: false })
        }
        Action::Mute(m) => {
            let v = tv.volume().await.map_err(|e| tv_error(e, conf))?;
            if v.muted == *m {
                return Ok(Done { confirmed: true, state: tv_state(app.clone(), conf.clone()).await, already: true });
            }
            tv.set_volume(v.current, *m).await.map_err(|e| tv_error(e, conf))?;
            let want = *m;
            let (confirmed, state) = until(Duration::from_secs(3), reader, move |s| s.muted == Some(want)).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::OpenApp(name) => {
            let label = model::resolve_app(device, name)?;
            let lists = lists(app, conf, &tv, false).await;
            let app_entry = lists.apps.iter().find(|a| a.label == label).ok_or_else(|| format!("{label} is not in the TV's list any more."))?;
            tv.launch(&app_entry.intent).await.map_err(|e| tv_error(e, conf))?;
            let (confirmed, state) = until(CONFIRM_WAIT, reader, move |s| s.app.as_deref() == Some(label.as_str())).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::SetChannel(wanted) => {
            let channel = model::resolve_channel(device, wanted)?;
            let lists = lists(app, conf, &tv, false).await;
            let entry = lists.channels.iter().find(|c| c.channel.number == channel.number).ok_or_else(|| format!("{} has no channel {}.", device.name, channel.number))?;
            tv.set_channel(entry).await.map_err(|e| tv_error(e, conf))?;
            let number = channel.number.clone();
            let (confirmed, state) = until(Duration::from_secs(6), reader, move |s| s.channel.as_deref() == Some(number.as_str())).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::ChannelUp | Action::ChannelDown => {
            tv.key(if *action == Action::ChannelUp { "ChannelStepUp" } else { "ChannelStepDown" }).await.map_err(|e| tv_error(e, conf))?;
            tokio::time::sleep(Duration::from_millis(800)).await;
            Ok(Done { confirmed: true, state: tv_state(app.clone(), conf.clone()).await, already: false })
        }
        Action::SetBrightness(_) => Err(format!("{} has no brightness.", device.name)),
    }
}

// ── Samsung TVs (samsung.rs) ──────────────────────────────────────────────────

/// A Samsung TV as Mochi sees it: what its API says (on or in standby), its volume from its UPnP renderer, its apps
/// from its own list, and only what that allows.
async fn samsung_device(app: AppHandle, conf: TvConf, prefs: DevicePrefs, ask_all: bool) -> Device {
    let info = samsung::info(&conf.samsung_tv()).await;
    let paired = !conf.needs_pairing || config::samsung_token(&conf.id).is_some();
    let mut state = samsung_reading(&conf, info.as_ref().ok()).await;
    // The remote is opened only on a TV that is on: on some models, opening it turns the TV on.
    let apps = if paired && state.on == Some(true) { samsung_apps(&app, &conf, false).await } else { conf.apps.clone() };
    let caps = samsung_caps(paired, state.online, conf.mac().is_some(), Url::parse(&conf.renderer).is_ok(), !apps.is_empty());
    if state.on != Some(true) {
        state.volume = None;
        state.muted = None;
    }
    Device {
        id: conf.id.clone(),
        name: if prefs.name.trim().is_empty() { conf.name.clone() } else { prefs.name.trim().to_string() },
        room: prefs.room.trim().to_string(),
        kind: Kind::Tv,
        source: "Samsung TV".into(),
        model: (!conf.model.is_empty()).then(|| conf.model.clone()),
        caps,
        apps: apps.into_iter().map(|a| a.name).collect(),
        channels: Vec::new(),
        state,
        exposed: prefs.exposed.unwrap_or(true),
        ask: ask_all || prefs.ask.unwrap_or(false),
    }
}

/// What a Samsung TV can do from here: its remote's keys once it allowed Coucou (`paired`); turning it on with the
/// power key while its network is on, or with Wake-on-LAN (`mac`); the volume as a number and mute through its UPnP
/// renderer; apps from its own list.
fn samsung_caps(paired: bool, online: bool, mac: bool, renderer: bool, apps: bool) -> Vec<Capability> {
    let mut caps = Vec::new();
    if mac || (paired && online) {
        caps.push(Capability::TurnOn);
    }
    if paired {
        caps.extend([Capability::TurnOff, Capability::VolumeStep, Capability::ChannelStep]);
    }
    if renderer {
        caps.extend([Capability::VolumeSet, Capability::Mute]);
    }
    if paired && apps {
        caps.push(Capability::Apps);
    }
    caps.sort();
    caps.dedup();
    caps
}

/// The state from what the API said (None: it did not answer), with the volume when the TV is on.
async fn samsung_reading(conf: &TvConf, info: Option<&samsung::Info>) -> DeviceState {
    let Some(info) = info else { return DeviceState::default() };
    // A TV that does not say is on: its API answers only then (or for a few seconds after it is turned off).
    let on = info.power.unwrap_or(true);
    let mut state = DeviceState { online: true, on: Some(on), ..DeviceState::default() };
    if let (true, Ok(renderer)) = (on, Url::parse(&conf.renderer)) {
        state.volume = upnp::volume(&renderer).await.ok();
        state.muted = upnp::muted(&renderer).await.ok();
    }
    state
}

async fn samsung_state(conf: TvConf) -> DeviceState {
    let info = samsung::info(&conf.samsung_tv()).await.ok();
    samsung_reading(&conf, info.as_ref()).await
}

/// The TV's apps: the list kept in its settings, asked again to the TV (on and paired) when it is older than LISTS.
/// A new list is kept.
async fn samsung_apps(app: &AppHandle, conf: &TvConf, renew: bool) -> Vec<samsung::App> {
    let home = app.state::<Home>();
    let recent = home.samsung_asked.lock().unwrap().get(&conf.id).is_some_and(|at| at.elapsed() < LISTS);
    if recent && !renew {
        return conf.apps.clone();
    }
    home.samsung_asked.lock().unwrap().insert(conf.id.clone(), Instant::now());
    let fresh = match open_remote(app, conf).await {
        Ok(mut remote) => {
            let apps = remote.apps().await.ok().flatten();
            remote.close().await;
            apps
        }
        Err(_) => None,
    };
    match fresh {
        Some(apps) if !apps.is_empty() => {
            if apps != conf.apps {
                keep_samsung_apps(app, &conf.id, &apps);
            }
            apps
        }
        _ => conf.apps.clone(),
    }
}

fn keep_samsung_apps(app: &AppHandle, id: &str, apps: &[samsung::App]) {
    save(app, |s| {
        if let Some(tv) = s.home_tvs.iter_mut().find(|tv| tv.id == id) {
            tv.apps = apps.to_vec();
        }
    });
}

/// Opens the TV's remote with the token it gave; a new token, or the certificate met for the first time, is kept.
async fn open_remote(app: &AppHandle, conf: &TvConf) -> Result<samsung::Remote, String> {
    let token = config::samsung_token(&conf.id);
    if conf.needs_pairing && token.is_none() {
        return Err(t("Pair this TV first, in Settings → Mochi's home."));
    }
    let remote = samsung::connect(&conf.samsung_tv(), token.as_deref(), false).await.map_err(|e| samsung_error(e, conf))?;
    if let Some(new) = remote.token.as_deref().filter(|new| token.as_deref() != Some(*new)) {
        let _ = config::set_samsung_token(&conf.id, Some(new));
    }
    if let (true, true, Some(pin)) = (conf.https, conf.pin.is_empty(), remote.certificate) {
        let id = conf.id.clone();
        save(app, |s| {
            if let Some(tv) = s.home_tvs.iter_mut().find(|tv| tv.id == id) {
                tv.pin = config::pin_hex(&pin);
            }
        });
    }
    Ok(remote)
}

/// Presses `key` on the TV's remote (held for a Frame TV's power key).
async fn samsung_key(app: &AppHandle, conf: &TvConf, key: &str, hold: bool) -> Result<(), String> {
    let mut remote = open_remote(app, conf).await?;
    let sent = if hold { remote.hold(key, Duration::from_secs(3)).await } else { remote.key(key).await };
    sent.map_err(|e| samsung_error(e, conf))?;
    remote.close().await;
    Ok(())
}

fn samsung_wake_hint() -> String {
    t("To turn it on from standby, turn on \"Power On with Mobile\" in the TV's network settings (Settings → General → Network → Expert Settings).")
}

fn samsung_error(e: TvError, conf: &TvConf) -> String {
    match e {
        TvError::Auth => t("The TV refused Coucou: press Pair in Settings → Mochi's home, then choose Allow on the TV."),
        TvError::Unreachable(_) if conf.mac().is_none() => format!("{} {}", e.message(), samsung_wake_hint()),
        e => e.message(),
    }
}

async fn samsung_act(app: &AppHandle, conf: &TvConf, device: &Device, action: &Action) -> Result<Done, String> {
    let tv = conf.samsung_tv();
    let reader = {
        let conf = conf.clone();
        move || -> BoxFuture<'static, DeviceState> { Box::pin(samsung_state(conf.clone())) }
    };
    let renderer = || Url::parse(&conf.renderer).map_err(|_| tf("{device} does not offer that.", &[("device", &device.name)]));
    let now = || samsung_state(conf.clone());
    match action {
        Action::TurnOn => {
            match samsung::info(&tv).await {
                Ok(info) if info.power != Some(false) => return Ok(Done { confirmed: true, state: now().await, already: true }),
                // In standby with its network on: the power key wakes it (or Wake-on-LAN, before it is paired).
                Ok(_) => match (config::samsung_token(&conf.id).is_some() || !conf.needs_pairing, conf.mac()) {
                    (false, Some(mac)) => philips::wake(&mac).await?,
                    _ => samsung_key(app, conf, "KEY_POWER", false).await?,
                },
                Err(TvError::Unreachable(_)) => match conf.mac() {
                    Some(mac) => philips::wake(&mac).await?,
                    None => return Err(format!("{} {}", TvError::Unreachable(String::new()).message(), samsung_wake_hint())),
                },
                Err(e) => return Err(samsung_error(e, conf)),
            }
            let (confirmed, state) = until(TURN_ON_WAIT, reader, |s| s.on == Some(true)).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::TurnOff => {
            match samsung::info(&tv).await {
                Err(TvError::Unreachable(_)) => return Ok(Done { confirmed: true, state: DeviceState::default(), already: true }),
                Ok(info) if info.power == Some(false) => return Ok(Done { confirmed: true, state: now().await, already: true }),
                Ok(info) => samsung_key(app, conf, "KEY_POWER", info.frame || conf.frame).await?,
                Err(e) => return Err(samsung_error(e, conf)),
            }
            let (confirmed, state) = until(CONFIRM_WAIT, reader, |s| s.on != Some(true)).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::SetVolume(p) => {
            upnp::set_volume(&renderer()?, *p).await?;
            let target = *p;
            let (confirmed, state) = until(Duration::from_secs(3), reader, move |s| s.volume.is_some_and(|v| v.abs_diff(target) <= 2)).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::VolumeUp | Action::VolumeDown => {
            samsung_key(app, conf, if *action == Action::VolumeUp { "KEY_VOLUP" } else { "KEY_VOLDOWN" }, false).await?;
            Ok(Done { confirmed: true, state: now().await, already: false })
        }
        Action::Mute(m) => {
            let control = renderer()?;
            if upnp::muted(&control).await.ok() == Some(*m) {
                return Ok(Done { confirmed: true, state: now().await, already: true });
            }
            upnp::set_muted(&control, *m).await?;
            let want = *m;
            let (confirmed, state) = until(Duration::from_secs(3), reader, move |s| s.muted == Some(want)).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::OpenApp(name) => {
            let label = model::resolve_app(device, name)?;
            let entry = samsung_apps(app, conf, false)
                .await
                .into_iter()
                .find(|a| a.name == label)
                .ok_or_else(|| format!("{label} is not in the TV's list any more."))?;
            let mut remote = open_remote(app, conf).await?;
            remote.launch(&entry).await.map_err(|e| samsung_error(e, conf))?;
            remote.close().await;
            // Confirmed on the TVs that say which app is on screen; the others are only sent to.
            let deadline = Instant::now() + CONFIRM_WAIT;
            let confirmed = loop {
                match samsung::app_visible(&tv, &entry.id).await {
                    Some(true) => break true,
                    Some(false) if Instant::now() < deadline => tokio::time::sleep(Duration::from_millis(1000)).await,
                    _ => break false,
                }
            };
            let mut state = now().await;
            if confirmed {
                state.app = Some(label);
            }
            Ok(Done { confirmed, state, already: false })
        }
        Action::ChannelUp | Action::ChannelDown => {
            samsung_key(app, conf, if *action == Action::ChannelUp { "KEY_CHUP" } else { "KEY_CHDOWN" }, false).await?;
            Ok(Done { confirmed: true, state: now().await, already: false })
        }
        Action::SetChannel(_) | Action::SetBrightness(_) => Err(tf("{device} does not offer that.", &[("device", &device.name)])),
    }
}

// ── Android devices over ADB (android.rs) ─────────────────────────────────────

/// What an Android device reports, as the house says it: awake or not, its media volume, and the app on screen by its
/// name in the device's list.
fn android_state_of(reading: &android::Reading, apps: &[samsung::App]) -> DeviceState {
    DeviceState {
        online: true,
        on: reading.awake,
        volume: reading.percent(),
        muted: reading.muted,
        app: reading.package.as_deref().filter(|_| reading.awake == Some(true)).and_then(|p| android::app_on_screen(apps, p)),
        ..DeviceState::default()
    }
}

/// The device's reading, through one connection.
async fn android_reading(conf: &TvConf) -> Result<android::Reading, TvError> {
    let mut adb = android::open(&conf.host, false).await?;
    let out = adb.shell(android::state_command(conf.api)).await;
    adb.close().await;
    Ok(android::parse_state(&out?))
}

/// Its state now; a device that does not let Coucou in still answers (online, nothing more).
async fn android_state(conf: TvConf) -> DeviceState {
    match android_reading(&conf).await {
        Ok(reading) => android_state_of(&reading, &conf.apps),
        Err(TvError::Unreachable(_)) => DeviceState::default(),
        Err(_) => DeviceState { online: true, ..DeviceState::default() },
    }
}

/// Runs one of android.rs's fixed commands on the device.
async fn android_run(conf: &TvConf, command: &str) -> Result<String, String> {
    let mut adb = android::open(&conf.host, false).await.map_err(|e| android_error(e, conf))?;
    let out = adb.shell(command).await.map_err(|e| android_error(e, conf));
    adb.close().await;
    out
}

fn android_error(e: TvError, conf: &TvConf) -> String {
    match e {
        TvError::Auth => t("The device does not let Coucou in: press Pair in Settings → Mochi's home, then choose Allow on its screen."),
        TvError::Unreachable(_) if conf.mac().is_none() => t("The device does not answer. A projector or a TV box usually turns its network off when it is switched off: turn it on with its remote. If it is on, check that its network debugging is still on."),
        e => e.message(),
    }
}

/// What an Android device can do from here: its keys once it let Coucou in (`paired`): waking it while its network
/// is on, sleep, the volume; the volume as a number and mute when it reports them (now, or when it was paired); apps
/// from its own list; turning it on from off only through Wake-on-LAN (`mac`).
fn android_caps(paired: bool, online: bool, mac: bool, volume: bool, mute: bool, apps: bool) -> Vec<Capability> {
    let mut caps = Vec::new();
    if mac || (paired && online) {
        caps.push(Capability::TurnOn);
    }
    if paired {
        caps.extend([Capability::TurnOff, Capability::VolumeStep]);
        if volume {
            caps.push(Capability::VolumeSet);
        }
        if mute {
            caps.push(Capability::Mute);
        }
        if apps {
            caps.push(Capability::Apps);
        }
    }
    caps.sort();
    caps.dedup();
    caps
}

/// An Android device as Mochi sees it: what it reports over ADB, its apps from its own list (asked again now and
/// then while it is awake), and only what that allows.
async fn android_device(app: AppHandle, conf: TvConf, prefs: DevicePrefs, ask_all: bool) -> Device {
    let mut apps = conf.apps.clone();
    let mut paired = conf.allowed || !conf.needs_pairing;
    let mut state = DeviceState::default();
    match android::open(&conf.host, false).await {
        Ok(mut adb) => {
            paired = true;
            state = match adb.shell(android::state_command(conf.api)).await {
                Ok(out) => android_state_of(&android::parse_state(&out), &apps),
                Err(_) => DeviceState { online: true, ..DeviceState::default() },
            };
            let home = app.state::<Home>();
            let recent = home.samsung_asked.lock().unwrap().get(&conf.id).is_some_and(|at| at.elapsed() < LISTS);
            if state.on == Some(true) && !recent {
                home.samsung_asked.lock().unwrap().insert(conf.id.clone(), Instant::now());
                if let Ok(out) = adb.shell(android::apps_command(conf.api)).await {
                    let fresh = android::parse_apps(&out);
                    if !fresh.is_empty() && fresh != apps {
                        keep_samsung_apps(&app, &conf.id, &fresh);
                        apps = fresh;
                    }
                }
            }
            adb.close().await;
        }
        Err(TvError::Unreachable(_)) => {}
        Err(e) => {
            state.online = true;
            paired &= !matches!(e, TvError::Auth);
        }
    }
    let reports = |what: &str| conf.features.get("reports").is_some_and(|r| r.iter().any(|x| x == what));
    let caps = android_caps(paired, state.online, conf.mac().is_some(), state.volume.is_some() || reports("volume"), state.muted.is_some() || reports("mute"), !apps.is_empty());
    Device {
        id: conf.id.clone(),
        name: if prefs.name.trim().is_empty() { conf.name.clone() } else { prefs.name.trim().to_string() },
        room: prefs.room.trim().to_string(),
        kind: Kind::Tv,
        source: "Android (ADB)".into(),
        model: (!conf.model.is_empty()).then(|| conf.model.clone()),
        caps,
        apps: apps.into_iter().map(|a| a.name).collect(),
        channels: Vec::new(),
        state,
        exposed: prefs.exposed.unwrap_or(true),
        ask: ask_all || prefs.ask.unwrap_or(false),
    }
}

async fn android_act(conf: &TvConf, device: &Device, action: &Action) -> Result<Done, String> {
    let reader = {
        let conf = conf.clone();
        move || -> BoxFuture<'static, DeviceState> { Box::pin(android_state(conf.clone())) }
    };
    let keys = |keys: &[u16]| android::keys_command(keys);
    let unknown = || tf("{device} does not say its volume.", &[("device", &device.name)]);
    match action {
        Action::TurnOn => {
            match android_reading(conf).await {
                Ok(r) if r.awake == Some(true) => return Ok(Done { confirmed: true, state: android_state_of(&r, &conf.apps), already: true }),
                Ok(_) => {
                    android_run(conf, &keys(&[android::wake_key(conf.api)])).await?;
                }
                Err(TvError::Unreachable(_)) => match conf.mac() {
                    Some(mac) => philips::wake(&mac).await?,
                    None => return Err(android_error(TvError::Unreachable(String::new()), conf)),
                },
                Err(e) => return Err(android_error(e, conf)),
            }
            let (confirmed, state) = until(TURN_ON_WAIT, reader, |s| s.on == Some(true)).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::TurnOff => {
            match android_reading(conf).await {
                Err(TvError::Unreachable(_)) => return Ok(Done { confirmed: true, state: DeviceState::default(), already: true }),
                Ok(r) if r.awake == Some(false) => return Ok(Done { confirmed: true, state: android_state_of(&r, &conf.apps), already: true }),
                Ok(_) => {
                    android_run(conf, &keys(&[android::sleep_key(conf.api)])).await?;
                }
                Err(e) => return Err(android_error(e, conf)),
            }
            let (confirmed, state) = until(CONFIRM_WAIT, reader, |s| s.on != Some(true)).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::VolumeUp | Action::VolumeDown => {
            let key = if *action == Action::VolumeUp { android::KEY_VOLUME_UP } else { android::KEY_VOLUME_DOWN };
            android_run(conf, &keys(&[key])).await?;
            Ok(Done { confirmed: true, state: android_state(conf.clone()).await, already: false })
        }
        Action::SetVolume(p) => {
            // Its volume has steps (15 on most): the keys are pressed as many times as the step asked is away.
            let reading = android_reading(conf).await.map_err(|e| android_error(e, conf))?;
            let (current, max) = reading.volume.filter(|(_, max)| *max > 0).ok_or_else(unknown)?;
            let target = (u32::from(*p) * max + 50) / 100;
            if target == current.min(max) {
                return Ok(Done { confirmed: true, state: android_state_of(&reading, &conf.apps), already: true });
            }
            let key = if target > current { android::KEY_VOLUME_UP } else { android::KEY_VOLUME_DOWN };
            android_run(conf, &keys(&vec![key; target.abs_diff(current).min(100) as usize])).await?;
            let (want, step) = (*p, (100 / max).max(2) as u8);
            let (confirmed, state) = until(Duration::from_secs(4), reader, move |s| s.volume.is_some_and(|v| v.abs_diff(want) <= step)).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::Mute(m) => {
            let reading = android_reading(conf).await.map_err(|e| android_error(e, conf))?;
            if reading.muted == Some(*m) {
                return Ok(Done { confirmed: true, state: android_state_of(&reading, &conf.apps), already: true });
            }
            android_run(conf, &keys(&[android::KEY_MUTE])).await?;
            let want = *m;
            let (confirmed, state) = until(Duration::from_secs(3), reader, move |s| s.muted == Some(want)).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::OpenApp(name) => {
            let label = model::resolve_app(device, name)?;
            let entry = conf.apps.iter().find(|a| a.name == label).ok_or_else(|| format!("{label} is not in the device's list any more."))?;
            let command = android::launch_command(&entry.id).ok_or_else(|| format!("{label} cannot be opened."))?;
            let out = android_run(conf, &command).await?;
            if android::launch_failed(&out) {
                return Err(tf("{device} could not open {app}.", &[("device", &device.name), ("app", &label)]));
            }
            let (confirmed, state) = until(CONFIRM_WAIT, reader, move |s| s.app.as_deref() == Some(label.as_str())).await;
            Ok(Done { confirmed, state, already: false })
        }
        Action::SetChannel(_) | Action::ChannelUp | Action::ChannelDown | Action::SetBrightness(_) => {
            Err(tf("{device} does not offer that.", &[("device", &device.name)]))
        }
    }
}

async fn ha_state(url: String, token: String, entity: String, prefs: home_assistant::Prefs) -> DeviceState {
    let Ok(ha) = HomeAssistant::new(&url, &token) else { return DeviceState::default() };
    match ha.state(&entity).await {
        Ok(v) => home_assistant::device(&v, &prefs, None).map(|d| d.state).unwrap_or_default(),
        Err(_) => DeviceState::default(),
    }
}

async fn ha_act(s: &Setup, device: &Device, action: &Action) -> Result<Done, String> {
    let token = config::ha_token().ok_or_else(|| t("Connect Home Assistant first, in Settings → Mochi's home."))?;
    let ha = HomeAssistant::new(&s.ha_url, &token)?;
    let (domain, service, data) = home_assistant::service(device, action)?;
    ha.call(&domain, service, &data).await?;
    let entity = device.id.trim_start_matches("ha:").to_string();
    let prefs = ha_prefs(s.prefs.get(&device.id));
    let reader = {
        let (url, token) = (s.ha_url.clone(), token.clone());
        move || -> BoxFuture<'static, DeviceState> { Box::pin(ha_state(url.clone(), token.clone(), entity.clone(), prefs.clone())) }
    };
    let action = action.clone();
    let source = match &action {
        Action::OpenApp(app) => model::resolve_app(device, app).ok(),
        _ => None,
    };
    let wait = if matches!(action, Action::TurnOn | Action::TurnOff) { Duration::from_secs(10) } else { Duration::from_secs(3) };
    let (confirmed, state) = until(wait, reader, move |st| match &action {
        Action::TurnOn => st.on == Some(true),
        Action::TurnOff => st.on == Some(false),
        Action::SetVolume(p) => st.volume.is_some_and(|v| v.abs_diff(*p) <= 2),
        Action::Mute(m) => st.muted == Some(*m),
        Action::OpenApp(_) => st.app == source,
        Action::SetBrightness(p) => st.brightness.is_some_and(|b| b.abs_diff(*p) <= 3),
        _ => true,
    })
    .await;
    Ok(Done { confirmed, state, already: false })
}

/// The answer of an action, for the model (`text`, English) and for the user (`detail`).
pub struct Outcome {
    pub ok: bool,
    pub text: String,
    pub detail: String,
}

/// Carries out `action` on the device `id`: checked against what it reported, sent, confirmed, recorded. `origin`:
/// "island", "phone", "settings" or "routine". `user`: the user clicked it in Settings, so any device of theirs may
/// be used, ticked for Mochi or not.
pub async fn control(app: &AppHandle, id: &str, action: &Action, origin: &str, user: bool) -> Outcome {
    let s = setup(app);
    let devices = read(app, false).await;
    let Some(mut device) = devices.iter().find(|d| d.id == id).cloned() else {
        return Outcome { ok: false, text: format!("There is no device {id}. Call home_devices to see them."), detail: t("This device is not there any more.") };
    };
    if user {
        device.exposed = true;
    }
    if let Err(e) = model::check(&device, action) {
        return Outcome { ok: false, detail: e.clone(), text: e };
    }
    let result = if let Some(conf) = s.tvs.iter().find(|c| c.id == device.id) {
        if conf.samsung() {
            samsung_act(app, conf, &device, action).await
        } else if conf.android() {
            android_act(conf, &device, action).await
        } else {
            tv_act(app, conf, &device, action).await
        }
    } else if device.id.starts_with("ha:") {
        ha_act(&s, &device, action).await
    } else {
        Err(t("This device is not there any more."))
    };
    forget_cache(app);
    let label = action.label();
    let outcome = match result {
        Ok(done) => {
            let what = action.describe();
            let text = match (done.already, done.confirmed) {
                (true, _) => format!("{} was already as asked ({what}): it reports {}. Nothing was sent.", device.name, state_text(&done.state)),
                (false, true) => format!("Done: {what} on {}. The device now reports: {}.", device.name, state_text(&done.state)),
                (false, false) => format!(
                    "Sent: {what} on {}, but the device has not confirmed it yet (it reports: {}). Tell the user it may take a moment.",
                    device.name,
                    state_text(&done.state)
                ),
            };
            let detail = if done.confirmed { state_words(&done.state) } else { format!("{} · {}", t("Not confirmed yet"), state_words(&done.state)) };
            Outcome { ok: true, text, detail }
        }
        Err(e) => Outcome { ok: false, text: format!("Could not {} on {}: {e}", action.describe(), device.name), detail: e },
    };
    journal::append(&journal::Entry::now(origin, &device.name, &label, outcome.ok, &outcome.detail));
    log::line(format!("home: {} on {} from {origin}: {}", action.word().0, device.id, if outcome.ok { "ok" } else { "failed" }));
    outcome
}

/// Runs a routine's steps in order, each through `control`. A step that fails does not stop the others.
pub async fn run_routine(app: &AppHandle, routine: &Routine, origin: &str, user: bool) -> Outcome {
    let mut lines = Vec::new();
    let mut ok = 0;
    for (i, step) in routine.steps.iter().enumerate() {
        if step.wait > 0 {
            tokio::time::sleep(Duration::from_secs(u64::from(step.wait.min(routines::MAX_WAIT)))).await;
        }
        let out = match step.action() {
            Ok(action) => control(app, &step.device, &action, if origin == "settings" { "settings" } else { "routine" }, user).await,
            Err(e) => Outcome { ok: false, detail: e.clone(), text: e },
        };
        if out.ok {
            ok += 1;
        }
        lines.push(format!("{}. {}", i + 1, out.text));
    }
    let all = ok == routine.steps.len();
    journal::append(&journal::Entry::now(origin, &routine.name, &t("Routine"), all, &format!("{ok}/{}", routine.steps.len())));
    Outcome {
        ok: ok > 0,
        text: format!("Routine \"{}\": {ok} of {} steps done.\n{}", routine.name, routine.steps.len(), lines.join("\n")),
        detail: tf("{done} of {count} steps done", &[("done", &ok.to_string()), ("count", &routine.steps.len().to_string())]),
    }
}

// ── Mochi's way in (agent/tools.rs) ───────────────────────────────────────────

struct Runner {
    app: AppHandle,
    origin: &'static str,
}

impl HomeRunner for Runner {
    fn devices<'a>(&'a self) -> BoxFuture<'a, Vec<Device>> {
        Box::pin(async move { read(&self.app, true).await.into_iter().filter(|d| d.exposed).collect() })
    }

    fn control<'a>(&'a self, id: &'a str, action: &'a Action) -> BoxFuture<'a, HomeResult> {
        Box::pin(async move {
            let o = control(&self.app, id, action, self.origin, false).await;
            HomeResult { ok: o.ok, text: o.text, detail: o.detail }
        })
    }

    fn routine<'a>(&'a self, id: &'a str) -> BoxFuture<'a, HomeResult> {
        Box::pin(async move {
            let s = setup(&self.app);
            let Some(r) = s.routines.iter().find(|r| r.id == id) else {
                return HomeResult { ok: false, text: format!("There is no routine {id}."), detail: String::new() };
            };
            let o = run_routine(&self.app, r, self.origin, false).await;
            HomeResult { ok: o.ok, text: o.text, detail: o.detail }
        })
    }
}

/// What Mochi's hands get for one turn: the devices he may use and the routines, and the way to act on them. None
/// when the home is off or has nothing for him.
pub async fn agent_env(app: &AppHandle, from_phone: bool) -> Option<HomeEnv> {
    agent_env_for(app, if from_phone { "phone" } else { "island" }).await
}

/// The same, for whoever asks: "island", "phone", or another agent ("hermes", "agent"), which the record names.
pub async fn agent_env_for(app: &AppHandle, origin: &'static str) -> Option<HomeEnv> {
    let s = setup(app);
    if !s.enabled {
        return None;
    }
    let devices: Vec<Device> = read(app, false).await.into_iter().filter(|d| d.exposed).collect();
    if devices.is_empty() {
        return None;
    }
    let routines = s
        .routines
        .iter()
        .map(|r| RoutineInfo {
            id: r.id.clone(),
            name: r.name.clone(),
            asks: routines::asks(r, &devices) || s.ask_all,
            steps: r.steps.iter().map(|st| {
                let device = devices.iter().find(|d| d.id == st.device).map_or(st.device.as_str(), |d| d.name.as_str());
                let action = st.action().map(|a| a.describe()).unwrap_or_else(|e| e);
                format!("{device}: {action}")
            }).collect(),
            problems: routines::problems(r, &devices),
        })
        .collect();
    Some(HomeEnv {
        devices,
        routines,
        runner: Arc::new(Runner { app: app.clone(), origin }),
    })
}

// ── Settings → Mochi's home ───────────────────────────────────────────────────

/// A TV as Settings lists it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TvSummary {
    pub id: String,
    /// "philips" or "samsung".
    pub brand: String,
    pub name: String,
    pub host: String,
    pub model: String,
    pub version: String,
    /// "Tizen", or a Philips TV's OS.
    pub os: String,
    pub android: bool,
    pub https: bool,
    pub needs_pairing: bool,
    pub paired: bool,
    pub mac: String,
    /// What its features say it can do, before asking it anything: "keys", "apps".
    pub features: Vec<String>,
}

fn summary(c: &TvConf) -> TvSummary {
    if c.android() {
        return TvSummary {
            id: c.id.clone(),
            brand: ANDROID.into(),
            name: c.name.clone(),
            host: c.host.clone(),
            model: c.model.clone(),
            version: c.version.clone(),
            os: c.os.clone(),
            android: true,
            https: false,
            needs_pairing: c.needs_pairing,
            paired: c.allowed,
            mac: c.mac.clone(),
            features: c.features.get("reports").cloned().unwrap_or_default(),
        };
    }
    if c.samsung() {
        return TvSummary {
            id: c.id.clone(),
            brand: SAMSUNG.into(),
            name: c.name.clone(),
            host: c.host.clone(),
            model: c.model.clone(),
            version: c.version.clone(),
            os: c.os.clone(),
            android: false,
            https: c.https,
            needs_pairing: c.needs_pairing,
            paired: config::samsung_token(&c.id).is_some(),
            mac: c.mac.clone(),
            features: if Url::parse(&c.renderer).is_ok() { vec!["volume".to_string()] } else { Vec::new() },
        };
    }
    let system = c.system();
    let mut features = Vec::new();
    if system.keys() {
        features.push("keys".to_string());
    }
    if system.apps() {
        features.push("apps".to_string());
    }
    TvSummary {
        id: c.id.clone(),
        brand: "philips".into(),
        name: c.name.clone(),
        host: c.host.clone(),
        model: c.model.clone(),
        version: c.version.clone(),
        os: c.os.clone(),
        android: system.android(),
        https: c.https,
        needs_pairing: c.needs_pairing,
        paired: config::credentials(&c.id).is_some(),
        mac: c.mac.clone(),
        features,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeStatus {
    pub enabled: bool,
    pub ask_all: bool,
    pub ha_url: String,
    /// A token is stored; the token itself never comes back to a window.
    pub ha_token: bool,
    pub tvs: Vec<TvSummary>,
    pub routines: Vec<Routine>,
    pub prefs: BTreeMap<String, DevicePrefs>,
}

#[tauri::command]
pub fn home_status(shared: State<Shared>) -> HomeStatus {
    let s = shared.settings.lock().unwrap();
    HomeStatus {
        enabled: s.home_enabled,
        ask_all: s.home_ask_all,
        ha_url: s.home_assistant_url.clone(),
        ha_token: config::ha_token().is_some(),
        tvs: s.home_tvs.iter().map(summary).collect(),
        routines: s.home_routines.clone(),
        prefs: s.home_devices.clone(),
    }
}

/// The latest actions, newest first.
#[tauri::command]
pub fn home_log() -> Vec<journal::Entry> {
    journal::recent(30)
}

/// Saves a change to the home's settings and tells the windows.
fn save(app: &AppHandle, change: impl FnOnce(&mut settings::Settings)) {
    let updated = {
        let shared = app.state::<Shared>();
        let mut current = shared.settings.lock().unwrap();
        change(&mut current);
        if let Err(err) = settings::save(&current) {
            log::line(format!("could not save settings: {err}"));
        }
        current.clone()
    };
    forget_cache(app);
    let _ = app.emit("settings-changed", updated);
}

/// The switch, and "Ask before every action".
#[tauri::command]
pub fn home_set(app: AppHandle, window: WebviewWindow, shared: State<Shared>, enabled: bool, ask_all: bool) -> Result<HomeStatus, String> {
    only_settings(&window)?;
    let was = shared.settings.lock().unwrap().home_enabled;
    save(&app, |s| {
        s.home_enabled = enabled;
        s.home_ask_all = ask_all;
    });
    log::line(format!("home: {}{}", if enabled { "on" } else { "off" }, if ask_all { ", asks before every action" } else { "" }));
    // A paired phone is offered Mochi's home only while it is on: phones reconnect to be told.
    if was != enabled {
        crate::phone_link::agent_setting_changed(&app);
    }
    Ok(home_status(shared))
}

/// Connects Home Assistant: checks the address and the token, then keeps them. `token` None keeps the stored one;
/// an empty address forgets both.
#[tauri::command]
pub async fn home_ha_connect(app: AppHandle, window: WebviewWindow, url: String, token: Option<String>) -> Result<home_assistant::ServerInfo, String> {
    only_settings(&window)?;
    if url.trim().is_empty() {
        config::set_ha_token("")?;
        save(&app, |s| s.home_assistant_url.clear());
        log::line("home: Home Assistant forgotten");
        return Ok(home_assistant::ServerInfo { version: String::new(), location: String::new() });
    }
    let token = match token.filter(|given| !given.trim().is_empty()) {
        Some(given) => given,
        None => config::ha_token().ok_or_else(|| t("Enter a long-lived access token from your Home Assistant profile."))?,
    };
    let ha = HomeAssistant::new(&url, &token)?;
    let info = ha.check().await?;
    config::set_ha_token(&token)?;
    let clean = home_assistant::clean_url(&url)?.to_string();
    save(&app, |s| s.home_assistant_url = clean);
    log::line(format!("home: Home Assistant {} connected", info.version));
    Ok(info)
}

/// Searches the local network for Home Assistant and TVs (about three seconds).
#[tauri::command]
pub async fn home_scan(window: WebviewWindow) -> Result<Vec<discovery::Found>, String> {
    only_settings(&window)?;
    let found = discovery::scan().await;
    log::line(format!("home: search found {} device(s)", found.len()));
    Ok(found)
}

/// Adds the TV at `host` (or updates it). It is asked what it is: a Samsung TV (its API on 8001), then a Philips TV
/// (JointSpace); what it says is kept, with its model and MAC address. A TV that is neither is not added, and the
/// answer says which known ways in it does answer on (`unknown_tv`), so the next step is chosen from facts.
#[tauri::command]
pub async fn home_tv_add(app: AppHandle, window: WebviewWindow, host: String) -> Result<TvSummary, String> {
    only_settings(&window)?;
    let host = host.trim().trim_start_matches('[').trim_end_matches(']').to_string();
    if host.is_empty() {
        return Err(t("Enter the TV's address first."));
    }
    match samsung::info(&samsung::Tv::new(&host, true, None)).await {
        Ok(info) => return add_samsung(&app, &host, info).await,
        // Not on the local network: nothing else is tried.
        Err(TvError::Other(why)) => return Err(why),
        Err(_) => {}
    }
    match philips::probe(&host).await {
        Ok(probe) => Ok(add_philips(&app, probe).await),
        Err(_) => match android::knock(&host).await {
            Some(android::Knock::Refused(why)) => Err(why),
            Some(knock) => Ok(add_android(&app, &host, knock).await),
            None => Err(unknown_tv(&host).await),
        },
    }
}

/// An Android device answering ADB: kept, and described by itself when it already lets Coucou in (it needs no key, or
/// it allowed this computer's before); else it is to be paired (its user allows this computer on its screen).
async fn add_android(app: &AppHandle, host: &str, knock: android::Knock) -> TvSummary {
    let existing = setup(app).tvs.into_iter().find(|tv| tv.host == host && tv.android());
    let mut conf = existing.unwrap_or_else(|| TvConf {
        id: new_tv_id(ANDROID),
        brand: ANDROID.into(),
        host: host.to_string(),
        name: t("Android device"),
        ..TvConf::default()
    });
    // No MAC address is learned: these devices mostly turn their Wi-Fi off with them, so Wake-on-LAN would be offered
    // only to fail. One typed in Settings is used.
    match knock {
        android::Knock::Open(mut adb) => {
            conf.needs_pairing = adb.signed_in;
            conf.allowed = adb.signed_in;
            describe_android(&mut conf, &mut adb).await;
            adb.close().await;
        }
        _ => {
            conf.needs_pairing = true;
            conf.allowed = false;
        }
    }
    let out = keep_tv(app, &conf);
    log::line(format!("home: Android device added (Android {}, {})", if conf.version.is_empty() { "?" } else { &conf.version }, if conf.needs_pairing && !conf.allowed { "to be allowed" } else { "allowed" }));
    out
}

/// What a device that let Coucou in says it is, what it reports and its apps: kept in its settings.
async fn describe_android(conf: &mut TvConf, adb: &mut android::Adb) {
    if let Some(model) = android::banner_value(&adb.banner, "ro.product.model") {
        conf.model = model;
    }
    if let Ok(out) = adb.shell(android::ABOUT_COMMAND).await {
        let about = android::parse_about(&out);
        if !about.release.is_empty() {
            conf.os = format!("Android {}", about.release);
            conf.version = about.release.clone();
        }
        if about.sdk > 0 {
            conf.api = about.sdk;
        }
        let name = if about.name.is_empty() { about.model.clone() } else { about.name };
        if !name.is_empty() {
            conf.name = name;
        }
        if !about.model.is_empty() {
            conf.model = about.model;
        }
    }
    if let Ok(out) = adb.shell(android::state_command(conf.api)).await {
        let reading = android::parse_state(&out);
        let reports: Vec<String> = [("volume", reading.volume.is_some()), ("mute", reading.muted.is_some())].into_iter().filter(|(_, yes)| *yes).map(|(w, _)| w.to_string()).collect();
        conf.features.insert("reports".into(), reports);
    }
    if let Ok(out) = adb.shell(android::apps_command(conf.api)).await {
        let apps = android::parse_apps(&out);
        if !apps.is_empty() {
            conf.apps = apps;
        }
    }
}

fn new_tv_id(brand: &str) -> String {
    let mut bytes = [0u8; 4];
    let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes);
    format!("{brand}:{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// Keeps a TV's settings (a new one, or the same one again) and forgets what was read of it.
fn keep_tv(app: &AppHandle, conf: &TvConf) -> TvSummary {
    let out = summary(conf);
    save(app, |settings| {
        settings.home_tvs.retain(|tv| tv.id != conf.id);
        settings.home_tvs.push(conf.clone());
    });
    if let Some(home) = app.try_state::<Home>() {
        home.lists.lock().unwrap().remove(&conf.id);
        home.samsung_asked.lock().unwrap().remove(&conf.id);
    }
    out
}

async fn add_philips(app: &AppHandle, probe: philips::Probe) -> TvSummary {
    let ip = probe.endpoint.host.parse::<Ipv4Addr>().ok();
    let model = match ip {
        Some(ip) => discovery::model_of(ip).await,
        None => None,
    };
    let mac = philips::learn_mac(&probe.endpoint.host).map(|m| philips::format_mac(&m));
    let existing = setup(app).tvs.into_iter().find(|tv| tv.host == probe.endpoint.host && !tv.samsung());
    let s = &probe.system;
    let conf = TvConf {
        id: existing.as_ref().map(|e| e.id.clone()).unwrap_or_else(|| new_tv_id("philips")),
        brand: String::new(),
        host: probe.endpoint.host.clone(),
        api: probe.endpoint.api,
        https: probe.endpoint.https,
        pin: probe.endpoint.pin.map(|p| config::pin_hex(&p)).unwrap_or_default(),
        mac: mac.or_else(|| existing.as_ref().map(|e| e.mac.clone())).unwrap_or_default(),
        name: s.name.clone(),
        model: model.or(s.model.clone()).or_else(|| existing.as_ref().map(|e| e.model.clone())).unwrap_or_default(),
        os: s.os_type.clone().unwrap_or_default(),
        version: format!("{}.{}.{}", s.version.0, s.version.1, s.version.2),
        features: s.features.iter().cloned().collect(),
        needs_pairing: s.needs_pairing,
        allowed: false,
        frame: false,
        renderer: String::new(),
        apps: Vec::new(),
    };
    let out = keep_tv(app, &conf);
    log::line(format!("home: Philips TV added (API {}, {})", conf.version, if conf.https { "https" } else { "http" }));
    out
}

/// A Samsung TV: what its API said, its renderer for the volume (asked over SSDP), its MAC address. Every Samsung TV
/// asks once on its screen whether to allow Coucou: that is its pairing.
async fn add_samsung(app: &AppHandle, host: &str, info: samsung::Info) -> Result<TvSummary, String> {
    let existing = setup(app).tvs.into_iter().find(|tv| tv.host == host && tv.samsung());
    let renderer = match host.parse::<Ipv4Addr>() {
        Ok(ip) => upnp::rendering_control(ip).await.map(|u| u.to_string()),
        Err(_) => None,
    };
    let mac = info.mac.or_else(|| philips::learn_mac(host)).map(|m| philips::format_mac(&m));
    // The certificate pinned stays while the TV is reached the same way.
    let pin = existing.as_ref().filter(|e| e.https == info.token_auth).map(|e| e.pin.clone()).unwrap_or_default();
    let conf = TvConf {
        id: existing.as_ref().map(|e| e.id.clone()).unwrap_or_else(|| new_tv_id(SAMSUNG)),
        brand: SAMSUNG.into(),
        host: host.to_string(),
        api: 2,
        https: info.token_auth,
        pin,
        mac: mac.or_else(|| existing.as_ref().map(|e| e.mac.clone())).unwrap_or_default(),
        name: info.name.clone(),
        model: if info.model.is_empty() { existing.as_ref().map(|e| e.model.clone()).unwrap_or_default() } else { info.model.clone() },
        os: info.os.clone(),
        version: info.version.clone(),
        features: BTreeMap::new(),
        needs_pairing: true,
        allowed: false,
        frame: info.frame,
        renderer: renderer.or_else(|| existing.as_ref().map(|e| e.renderer.clone())).unwrap_or_default(),
        apps: existing.as_ref().map(|e| e.apps.clone()).unwrap_or_default(),
    };
    let out = keep_tv(app, &conf);
    log::line(format!(
        "home: Samsung TV added (API {}, {}, {})",
        conf.version,
        if conf.https { "token" } else { "no token" },
        if conf.renderer.is_empty() { "no renderer" } else { "UPnP volume" }
    ));
    Ok(out)
}

/// The ways in a TV may answer on, and what each is, for a TV that is neither a Samsung nor a Philips one.
const KNOWN_PORTS: &[(u16, &str)] = &[
    (80, "HTTP"),
    (1925, "Philips JointSpace"),
    (3000, "LG webOS"),
    (3001, "LG webOS"),
    (5555, "ADB (Android)"),
    (6466, "Android TV Remote"),
    (6467, "Android TV Remote"),
    (8001, "Samsung"),
    (8008, "Google Cast"),
    (8009, "Google Cast"),
    (8060, "Roku"),
    (8080, "HTTP"),
    (9197, "UPnP"),
    (55000, "Samsung (2010–2015)"),
];

/// The ways in found, in words: "ADB (Android) (5555)", "DIAL"… The services every UPnP device has are left out.
fn ways_in(open: &[(u16, &str)], services: &[String]) -> Vec<String> {
    let mut found: Vec<String> = open.iter().map(|(port, what)| format!("{what} ({port})")).collect();
    for service in services {
        let short = match service.as_str() {
            s if s.contains("dial-multiscreen-org") => "DIAL".to_string(),
            s if s.contains("MediaRenderer") => "UPnP MediaRenderer".to_string(),
            s if s.contains("RenderingControl") || s.contains("AVTransport") || s.contains("ConnectionManager") || s == "upnp:rootdevice" || s.starts_with("uuid:") => continue,
            s => s.chars().take(80).collect(),
        };
        if !found.contains(&short) {
            found.push(short);
        }
    }
    found
}

/// What a TV that is neither a Samsung nor a Philips one does answer on: the known ports open, and what it offers over
/// SSDP. Said to the user, and kept in the log.
async fn unknown_tv(host: &str) -> String {
    if !net::resolves_locally(host, 80).await {
        return t("Only devices on your local network can be used.");
    }
    let mut checks = tokio::task::JoinSet::new();
    for &(port, what) in KNOWN_PORTS {
        let host = host.to_string();
        checks.spawn(async move {
            let open = matches!(tokio::time::timeout(Duration::from_millis(1500), tokio::net::TcpStream::connect((host.as_str(), port))).await, Ok(Ok(_)));
            (port, what, open)
        });
    }
    let services = match host.parse::<Ipv4Addr>() {
        Ok(ip) => discovery::services_of(ip, Duration::from_millis(1500)).await,
        Err(_) => Vec::new(),
    };
    let mut open = Vec::new();
    while let Some(checked) = checks.join_next().await {
        if let Ok((port, what, true)) = checked {
            open.push((port, what));
        }
    }
    open.sort();
    let found = ways_in(&open, &services);
    log::line(format!("home: {} is neither Samsung nor Philips; it answers on: {}", host, if found.is_empty() { "nothing known".into() } else { found.join(", ") }));
    let said = if found.is_empty() {
        t("This TV answers neither as a Samsung nor as a Philips TV, and on none of the other known ways in. Check that it is on and that the address is right; it may not be controllable over the network.")
    } else {
        tf(
            "This TV answers neither as a Samsung nor as a Philips TV. It does answer on: {found}. Send this line to whoever helps you with Coucou: it tells how this TV could be controlled.",
            &[("found", &found.join(", "))],
        )
    };
    // Port 5555 is closed: an Android device answers there once its network debugging is on.
    format!("{said} {}", t("If it is an Android device (a projector, a TV box), turn on its network debugging (ADB) in its Developer options, then add it again."))
}

fn tv_conf(app: &AppHandle, id: &str) -> Result<TvConf, String> {
    setup(app).tvs.into_iter().find(|tv| tv.id == id).ok_or_else(|| t("This device is not there any more."))
}

/// Starts pairing: the TV shows a PIN.
#[tauri::command]
pub async fn home_tv_pair(app: AppHandle, window: WebviewWindow, id: String) -> Result<Option<TvSummary>, String> {
    only_settings(&window)?;
    let conf = tv_conf(&app, &id)?;
    if conf.samsung() {
        return pair_samsung(&app, conf).await.map(Some);
    }
    if conf.android() {
        return pair_android(&app, conf).await.map(Some);
    }
    let state = philips::pair_request(&conf.endpoint(), &conf.system()).await?;
    app.state::<Home>().pairings.lock().unwrap().insert(id, state);
    log::line("home: pairing started, the TV shows a PIN");
    Ok(None)
}

/// A Samsung TV asks on its screen whether to allow Coucou (for up to half a minute). Allowed, its token (or "-") is
/// kept in the OS keystore and the certificate met is pinned from then on.
async fn pair_samsung(app: &AppHandle, mut conf: TvConf) -> Result<TvSummary, String> {
    log::line("home: pairing a Samsung TV, it asks on its screen");
    let remote = samsung::connect(&conf.samsung_tv(), None, true).await.map_err(|e| match e {
        TvError::Auth => t("The TV says Coucou was denied. To allow it, remove Coucou from the TV's list of devices (Settings → General → External Device Manager → Device Connection Manager → Device List), then press Pair again."),
        e => e.message(),
    })?;
    let mut remote = remote;
    let token = remote.token.clone();
    let certificate = remote.certificate;
    // The TV is on (its question was answered on it): its apps are read now, for when Mochi turns it on later.
    let apps = remote.apps().await.ok().flatten().unwrap_or_default();
    remote.close().await;
    config::set_samsung_token(&conf.id, token.as_deref())?;
    if let (true, Some(pin)) = (conf.https, certificate) {
        conf.pin = config::pin_hex(&pin);
    }
    if !apps.is_empty() {
        conf.apps = apps;
        app.state::<Home>().samsung_asked.lock().unwrap().insert(conf.id.clone(), Instant::now());
    }
    let out = keep_tv(app, &conf);
    forget_cache(app);
    log::line(format!("home: Samsung TV paired ({})", if token.is_some() { "token kept" } else { "no token given" }));
    Ok(out)
}

/// An Android device asks on its screen whether to allow this computer's key (for up to a minute; this computer's key
/// is made first if there is none). Allowed, the device is described again and kept as paired.
async fn pair_android(app: &AppHandle, mut conf: TvConf) -> Result<TvSummary, String> {
    log::line("home: pairing an Android device, it asks on its screen");
    let mut adb = android::open(&conf.host, true).await.map_err(|e| match e {
        TvError::Auth => t("The device did not allow Coucou, or its question went unanswered. Press Pair again and choose Allow on its screen, with \"Always allow from this computer\" ticked."),
        e => android_error(e, &conf),
    })?;
    conf.needs_pairing = adb.signed_in;
    conf.allowed = true;
    describe_android(&mut conf, &mut adb).await;
    adb.close().await;
    let out = keep_tv(app, &conf);
    app.state::<Home>().samsung_asked.lock().unwrap().insert(conf.id.clone(), Instant::now());
    forget_cache(app);
    log::line("home: Android device allowed Coucou");
    Ok(out)
}

/// Finishes pairing with the PIN the TV shows. The pairing is kept in the OS keystore.
#[tauri::command]
pub async fn home_tv_pin(app: AppHandle, window: WebviewWindow, id: String, pin: String) -> Result<TvSummary, String> {
    only_settings(&window)?;
    let conf = tv_conf(&app, &id)?;
    if conf.samsung() || conf.android() {
        return Err(t("This TV has no PIN: press Pair, then choose Allow on the TV."));
    }
    let state = app.state::<Home>().pairings.lock().unwrap().get(&id).cloned().ok_or_else(|| t("Press Pair first: the TV shows a PIN."))?;
    let creds = philips::pair_grant(&conf.endpoint(), &state, &pin).await?;
    config::set_credentials(&id, Some(&creds))?;
    app.state::<Home>().pairings.lock().unwrap().remove(&id);
    if let Some(home) = app.try_state::<Home>() {
        home.lists.lock().unwrap().remove(&id);
    }
    forget_cache(&app);
    log::line("home: Philips TV paired");
    Ok(summary(&conf))
}

/// The TV's MAC address, to wake it from deep standby. Empty forgets it.
#[tauri::command]
pub fn home_tv_set_mac(app: AppHandle, window: WebviewWindow, id: String, mac: String) -> Result<TvSummary, String> {
    only_settings(&window)?;
    let mac = mac.trim();
    let clean = if mac.is_empty() {
        String::new()
    } else {
        philips::format_mac(&philips::parse_mac(mac).ok_or_else(|| t("This is not a MAC address (like A4:B1:C2:D3:E4:F5)."))?)
    };
    let mut conf = tv_conf(&app, &id)?;
    conf.mac = clean;
    let out = summary(&conf);
    save(&app, |s| {
        if let Some(tv) = s.home_tvs.iter_mut().find(|tv| tv.id == id) {
            *tv = conf.clone();
        }
    });
    Ok(out)
}

/// Forgets a TV, its pairing and its rules.
#[tauri::command]
pub fn home_tv_remove(app: AppHandle, window: WebviewWindow, id: String) -> Result<(), String> {
    only_settings(&window)?;
    config::set_credentials(&id, None)?;
    save(&app, |s| {
        s.home_tvs.retain(|tv| tv.id != id);
        s.home_devices.remove(&id);
    });
    if let Some(home) = app.try_state::<Home>() {
        home.lists.lock().unwrap().remove(&id);
        home.samsung_asked.lock().unwrap().remove(&id);
    }
    log::line("home: TV removed");
    Ok(())
}

/// Every device, asked now.
#[tauri::command]
pub async fn home_devices(app: AppHandle, window: WebviewWindow) -> Result<Vec<Device>, String> {
    only_settings(&window)?;
    Ok(read(&app, true).await)
}

/// The user's rules for one device: its name and room here, whether Mochi may use it, whether it asks first.
#[tauri::command]
pub fn home_device_set(app: AppHandle, window: WebviewWindow, id: String, prefs: DevicePrefs) -> Result<(), String> {
    only_settings(&window)?;
    let clean = DevicePrefs {
        name: prefs.name.trim().chars().take(60).collect(),
        room: prefs.room.trim().chars().take(40).collect(),
        exposed: prefs.exposed,
        ask: prefs.ask,
    };
    save(&app, |s| {
        if clean == DevicePrefs::default() {
            s.home_devices.remove(&id);
        } else {
            s.home_devices.insert(id.clone(), clean);
        }
    });
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub ok: bool,
    pub detail: String,
}

/// A test from Settings: the user's own click, on any of their devices.
#[tauri::command]
pub async fn home_test(app: AppHandle, window: WebviewWindow, id: String, action: String, value: Option<String>) -> Result<TestResult, String> {
    only_settings(&window)?;
    let value = value.filter(|v| !v.trim().is_empty()).map(Value::String);
    let action = Action::parse(&action, value.as_ref())?;
    let o = control(&app, &id, &action, "settings", true).await;
    Ok(TestResult { ok: o.ok, detail: o.detail })
}

/// Saves a routine (new or changed). Returns it as kept.
#[tauri::command]
pub fn home_routine_save(app: AppHandle, window: WebviewWindow, routine: Routine) -> Result<Routine, String> {
    only_settings(&window)?;
    let r = routines::clean(routine)?;
    save(&app, |s| match s.home_routines.iter_mut().find(|x| x.id == r.id) {
        Some(existing) => *existing = r.clone(),
        None => s.home_routines.push(r.clone()),
    });
    Ok(r)
}

#[tauri::command]
pub fn home_routine_delete(app: AppHandle, window: WebviewWindow, id: String) -> Result<(), String> {
    only_settings(&window)?;
    save(&app, |s| s.home_routines.retain(|r| r.id != id));
    Ok(())
}

/// Runs a routine from Settings (the user's own click).
#[tauri::command]
pub async fn home_routine_run(app: AppHandle, window: WebviewWindow, id: String) -> Result<TestResult, String> {
    only_settings(&window)?;
    let r = setup(&app).routines.into_iter().find(|r| r.id == id).ok_or_else(|| t("This routine is not there any more."))?;
    let o = run_routine(&app, &r, "settings", true).await;
    Ok(TestResult { ok: o.ok, detail: o.detail })
}

/// Routines to start from, made of the user's devices.
#[tauri::command]
pub async fn home_templates(app: AppHandle, window: WebviewWindow) -> Result<Vec<Routine>, String> {
    only_settings(&window)?;
    Ok(routines::templates(&read(&app, false).await))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_android_device_offers_what_it_reports_once_it_let_coucou_in() {
        use Capability::*;
        assert_eq!(android_caps(false, true, false, true, true, true), Vec::<Capability>::new(), "not allowed yet: nothing");
        assert_eq!(android_caps(false, false, true, false, false, false), [TurnOn], "Wake-on-LAN alone");
        assert_eq!(android_caps(true, true, false, false, false, false), [TurnOn, TurnOff, VolumeStep]);
        assert_eq!(android_caps(true, false, false, true, true, true), [TurnOff, VolumeSet, VolumeStep, Mute, Apps], "off: no waking it without its network");
        let apps = android::parse_apps("com.netflix.ninja/.MainActivity\n");
        let awake = android::Reading { awake: Some(true), volume: Some((8, 15)), muted: Some(false), package: Some("com.netflix.ninja".into()) };
        let s = android_state_of(&awake, &apps);
        assert_eq!((s.online, s.on, s.volume, s.muted, s.app.as_deref()), (true, Some(true), Some(53), Some(false), Some("Netflix")));
        let asleep = android::Reading { awake: Some(false), ..awake };
        assert_eq!(android_state_of(&asleep, &apps).app, None, "nothing on screen while it sleeps");
        let conf = TvConf { brand: ANDROID.into(), name: "HY300-T".into(), needs_pairing: true, allowed: true, ..TvConf::default() };
        let line = summary(&conf);
        assert_eq!((line.brand.as_str(), line.android, line.needs_pairing, line.paired), ("android", true, true, true));
    }

    #[test]
    fn the_state_is_said_in_words_for_the_model() {
        let s = DeviceState { online: true, on: Some(true), volume: Some(20), muted: Some(false), app: Some("YouTube".into()), ..DeviceState::default() };
        assert_eq!(state_text(&s), "on, volume 20%, YouTube on screen");
        assert_eq!(state_words(&s), "On · Volume 20% · YouTube");
        assert_eq!(state_text(&DeviceState::default()), "not answering");
        assert_eq!(state_text(&DeviceState { online: true, ..DeviceState::default() }), "answering");
    }

    #[test]
    fn a_samsung_tv_offers_only_what_its_pairing_its_network_and_its_renderer_allow() {
        use Capability::*;
        // Added, not paired, no MAC, no renderer: nothing yet.
        assert!(samsung_caps(false, true, false, false, false).is_empty());
        // Its renderer alone: the volume as a number and mute, without pairing.
        assert_eq!(samsung_caps(false, true, false, true, false), [VolumeSet, Mute]);
        // Paired and answering: the remote's keys, and its apps once it listed some.
        assert_eq!(samsung_caps(true, true, false, false, true), [TurnOn, TurnOff, VolumeStep, Apps, ChannelStep]);
        // Asleep: only Wake-on-LAN turns it on, and only with its MAC.
        assert!(!samsung_caps(true, false, false, false, false).contains(&TurnOn));
        assert!(samsung_caps(true, false, true, false, false).contains(&TurnOn));
    }

    #[test]
    fn a_tv_coucou_does_not_know_is_described_by_what_it_answers_on() {
        let found = ways_in(
            &[(5555, "ADB (Android)"), (8008, "Google Cast")],
            &[
                "upnp:rootdevice".into(),
                "urn:dial-multiscreen-org:service:dial:1".into(),
                "urn:schemas-upnp-org:device:MediaRenderer:1".into(),
                "urn:schemas-upnp-org:service:AVTransport:1".into(),
                "uuid:1234".into(),
            ],
        );
        assert_eq!(found, ["ADB (Android) (5555)", "Google Cast (8008)", "DIAL", "UPnP MediaRenderer"]);
        assert!(ways_in(&[], &["upnp:rootdevice".into()]).is_empty());
    }

    /// Reads this file: every command that changes something, or reaches out to the network, checks the calling
    /// window (only Settings may), as in phone_link and agent.
    #[test]
    fn every_command_checks_the_calling_window() {
        let source = include_str!("mod.rs").replace("\r\n", "\n");
        let mut seen = 0;
        for chunk in source.split("#[tauri::command]\n").skip(1) {
            let name = chunk.split("fn ").nth(1).and_then(|r| r.split('(').next()).expect("a command has a name");
            let end = chunk.find("\n}\n").expect("a command ends");
            let body = &chunk[..end];
            seen += 1;
            if ["home_status", "home_log"].contains(&name) {
                assert!(!body.contains("save(") && !body.contains(".await"), "{name} must only read");
                continue;
            }
            assert!(body.contains("window: WebviewWindow") && body.contains("only_settings(&window)?"), "{name} does not check the calling window");
        }
        assert!(seen >= 16, "saw {seen}");
    }

    #[test]
    fn until_stops_as_soon_as_the_state_holds() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let calls = Arc::new(Mutex::new(0));
        let c = calls.clone();
        let read = move || -> BoxFuture<'static, DeviceState> {
            let c = c.clone();
            Box::pin(async move {
                let mut n = c.lock().unwrap();
                *n += 1;
                DeviceState { online: true, on: Some(*n >= 2), ..DeviceState::default() }
            })
        };
        let (ok, state) = rt.block_on(until(Duration::from_secs(5), read, |s| s.on == Some(true)));
        assert!(ok && state.on == Some(true));
        assert_eq!(*calls.lock().unwrap(), 2);
    }
}
