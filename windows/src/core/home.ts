// Mochi's home (src-tauri/src/smart_home), as Settings shows it: the shapes Rust sends, and the words for them. Pure
// data and helpers, tested without a webview; settings/home.ts draws them.

import { N_, t, tn } from "../i18n/i18n";

export type Kind = "tv" | "speaker" | "light" | "plug" | "switch" | "vacuum" | "climate" | "sensor";
export type Capability =
  | "turnOn" | "turnOff" | "volumeSet" | "volumeStep" | "mute" | "apps" | "channels" | "channelStep" | "brightness"
  | "clean" | "cleanArea" | "pause" | "dock" | "temperature";

/** What a device reports about itself right now (smart_home::model::DeviceState). */
export interface DeviceState {
  online: boolean;
  on?: boolean | null;
  volume?: number | null;
  muted?: boolean | null;
  brightness?: number | null;
  app?: string | null;
  channel?: string | null;
  /** A vacuum's activity ("cleaning", "docked"…) or a climate device's mode ("cool", "off"…). */
  status?: string | null;
  battery?: number | null;
  /** Tenths of °C: the room's temperature, and the one a climate device is set to. */
  temperature?: number | null;
  target?: number | null;
  humidity?: number | null;
}

export interface Channel {
  number: string;
  name: string;
}

/** smart_home::model::Device. */
export interface HomeDevice {
  id: string;
  name: string;
  room: string;
  kind: Kind;
  source: string;
  model?: string | null;
  caps: Capability[];
  apps: string[];
  channels: Channel[];
  state: DeviceState;
  exposed: boolean;
  ask: boolean;
}

/** The user's rules for one device (smart_home::config::DevicePrefs). */
export interface DevicePrefs {
  name: string;
  room: string;
  exposed: boolean | null;
  ask: boolean | null;
}

export interface TvSummary {
  id: string;
  brand: "philips" | "samsung" | "android";
  name: string;
  host: string;
  model: string;
  version: string;
  /** "Tizen", a Philips TV's OS, or "Android 13". */
  os: string;
  android: boolean;
  https: boolean;
  needsPairing: boolean;
  paired: boolean;
  mac: string;
  /** What its features say: "keys", "apps" (Philips); "volume" when a Samsung TV has a UPnP renderer; what an Android
   * device reported when it was paired ("volume", "mute"). */
  features: string[];
}

export interface RoutineStep {
  device: string;
  action: string;
  value: string;
  wait: number;
}

export interface Routine {
  id: string;
  name: string;
  steps: RoutineStep[];
}

export interface HomeStatus {
  enabled: boolean;
  askAll: boolean;
  haUrl: string;
  haToken: boolean;
  tvs: TvSummary[];
  routines: Routine[];
  prefs: Record<string, DevicePrefs>;
}

export interface Found {
  kind: "homeAssistant" | "philipsTv" | "samsungTv" | "otherTv";
  name: string;
  host: string;
  url?: string | null;
  model?: string | null;
  maker?: string | null;
  version?: string | null;
}

export interface LogEntry {
  at: string;
  origin: "island" | "phone" | "settings" | "routine" | string;
  device: string;
  action: string;
  ok: boolean;
  detail: string;
}

export interface ServerInfo {
  version: string;
  location: string;
}

export interface TestResult {
  ok: boolean;
  detail: string;
}

/** What a step's value is: a percent, an app's name, a channel, rooms, degrees, or nothing. */
export type ValueKind = "percent" | "app" | "channel" | "rooms" | "degrees" | null;

/** The actions, in the order Settings offers them, with the capability each needs (smart_home::model::Action). */
export const ACTIONS: readonly { id: string; label: string; cap: Capability; value: ValueKind }[] = [
  { id: "turn_on", label: N_("Turn on"), cap: "turnOn", value: null },
  { id: "turn_off", label: N_("Turn off"), cap: "turnOff", value: null },
  { id: "set_volume", label: N_("Set the volume"), cap: "volumeSet", value: "percent" },
  { id: "volume_up", label: N_("Volume up"), cap: "volumeStep", value: null },
  { id: "volume_down", label: N_("Volume down"), cap: "volumeStep", value: null },
  { id: "mute", label: N_("Mute"), cap: "mute", value: null },
  { id: "unmute", label: N_("Unmute"), cap: "mute", value: null },
  { id: "open_app", label: N_("Open an app"), cap: "apps", value: "app" },
  { id: "set_channel", label: N_("Change the channel"), cap: "channels", value: "channel" },
  { id: "channel_up", label: N_("Next channel"), cap: "channelStep", value: null },
  { id: "channel_down", label: N_("Previous channel"), cap: "channelStep", value: null },
  { id: "set_brightness", label: N_("Set the brightness"), cap: "brightness", value: "percent" },
  { id: "clean", label: N_("Start cleaning"), cap: "clean", value: null },
  { id: "clean_area", label: N_("Clean some rooms"), cap: "cleanArea", value: "rooms" },
  { id: "pause", label: N_("Pause"), cap: "pause", value: null },
  { id: "return_to_base", label: N_("Back to the dock"), cap: "dock", value: null },
  { id: "set_temperature", label: N_("Set the temperature"), cap: "temperature", value: "degrees" },
];

/** What a step's value field shows as an example. */
export function valueHint(kind: ValueKind): string {
  switch (kind) {
    case "percent":
      return "0–100";
    case "app":
      return "YouTube";
    case "channel":
      return "5";
    case "rooms":
      return "Quarto, Sala";
    case "degrees":
      return "10–32";
    default:
      return "";
  }
}

/** The actions `device` reported it can do. */
export function actionsFor(device: HomeDevice | undefined): typeof ACTIONS[number][] {
  if (!device) return [];
  return ACTIONS.filter((a) => device.caps.includes(a.cap));
}

export function actionLabel(id: string): string {
  const a = ACTIONS.find((x) => x.id === id);
  return a ? t(a.label) : id;
}

const KINDS: Record<Kind, string> = {
  tv: N_("TV"),
  speaker: N_("Speaker"),
  light: N_("Light"),
  plug: N_("Plug"),
  switch: N_("Switch"),
  vacuum: N_("Vacuum"),
  climate: N_("Air conditioning"),
  sensor: N_("Sensor"),
};

/** Tenths of a degree in the user's way of writing numbers: 235 → "23.5 °C" ("23,5 °C" in Portuguese). */
export function degreesText(tenths: number, lang?: string): string {
  const n = tenths / 10;
  const text = Number.isInteger(n) ? String(n) : n.toFixed(1);
  const comma = (lang ?? "").match(/^(pt|es|fr|ru|id)/) ? text.replace(".", ",") : text;
  return `${comma} °C`;
}

/** A vacuum's or a climate device's status in words, as Rust's smart_home::status_words; null for "off". */
export function statusWords(status: string | null | undefined): string | null {
  switch (status) {
    case "cleaning":
      return t("Cleaning");
    case "docked":
      return t("On its dock");
    case "returning":
      return t("Going back to its dock");
    case "paused":
      return t("Paused");
    case "idle":
      return t("Idle");
    case "error":
      return t("Needs help");
    case "cool":
      return t("Cooling");
    case "heat":
      return t("Heating");
    case "heat_cool":
    case "auto":
      return t("Automatic");
    case "dry":
      return t("Drying");
    case "fan_only":
      return t("Fan");
    default:
      return null;
  }
}

export function kindLabel(kind: Kind): string {
  return KINDS[kind] ? t(KINDS[kind]) : kind;
}

/** The state in words: "On · Volume 20% · YouTube", "Not answering". */
export function stateLine(s: DeviceState, lang?: string): string {
  if (!s.online) return t("Not answering");
  const parts: string[] = [];
  // A vacuum's or an air conditioner's own status says more than on or off.
  const status = statusWords(s.status);
  if (!status && s.on === true) parts.push(t("On"));
  if (!status && s.on === false) parts.push(t("Off"));
  if (s.volume != null) parts.push(t("Volume {percent}%", { percent: s.volume }));
  if (s.muted === true) parts.push(t("Muted"));
  if (s.brightness != null) parts.push(t("Brightness {percent}%", { percent: s.brightness }));
  if (s.app) parts.push(s.app);
  if (s.channel) parts.push(t("Channel {channel}", { channel: s.channel }));
  if (status) parts.push(status);
  if (s.battery != null) parts.push(t("Battery {percent}%", { percent: s.battery }));
  if (s.temperature != null) parts.push(degreesText(s.temperature, lang));
  if (s.target != null) parts.push(t("Set to {degrees} °C", { degrees: degreesText(s.target, lang).replace(" °C", "") }));
  if (s.humidity != null) parts.push(t("Humidity {percent}%", { percent: s.humidity }));
  return parts.join(" · ");
}

/** What a device can do, in words: "Turn on, Turn off, Set the volume, 12 apps, 140 channels". */
export function capsLine(d: HomeDevice): string {
  const words: string[] = [];
  for (const a of actionsFor(d)) {
    if (a.cap === "apps") words.push(tn("{count} app", "{count} apps", d.apps.length));
    else if (a.cap === "channels") words.push(tn("{count} channel", "{count} channels", d.channels.length));
    else if (!words.includes(t(a.label))) words.push(t(a.label));
  }
  return words.length ? words.join(", ") : t("Reported nothing it can do");
}

/** The rooms in use, for the room field's suggestions. */
export function roomsOf(devices: HomeDevice[], prefs: Record<string, DevicePrefs>): string[] {
  const rooms = new Set<string>();
  for (const d of devices) if (d.room.trim()) rooms.add(d.room.trim());
  for (const p of Object.values(prefs)) if (p.room.trim()) rooms.add(p.room.trim());
  return [...rooms].sort((a, b) => a.localeCompare(b));
}

/** A found thing in words: "Philips TV · 192.168.1.20 · 55PUS8007/12". */
export function foundLine(f: Found): string {
  const kind =
    f.kind === "homeAssistant" ? "Home Assistant"
    : f.kind === "philipsTv" ? "Philips TV"
    : f.kind === "samsungTv" ? "Samsung TV"
    : (f.maker ?? t("TV"));
  return [kind, f.host, f.model ?? "", f.version ?? ""].filter(Boolean).join(" · ");
}

/** A TV in words: "JointSpace 6.4.0 · Android TV · encrypted", "UN43AU7700GXZD · Tizen · API 2.0.25 · volume by UPnP". */
export function tvLine(tv: TvSummary): string {
  if (tv.brand === "android") {
    return [tv.model, tv.os || "Android", t("network debugging (ADB, port 5555)")].filter(Boolean).join(" · ");
  }
  if (tv.brand === "samsung") {
    const parts = [tv.model, tv.os || "Tizen", tv.version ? `API ${tv.version}` : ""].filter(Boolean);
    parts.push(tv.https ? t("encrypted (port 8002)") : t("unencrypted (port 8001)"));
    if (tv.features.includes("volume")) parts.push(t("volume by UPnP"));
    return parts.join(" · ");
  }
  const parts = [`JointSpace ${tv.version}`];
  if (tv.android) parts.push("Android TV");
  parts.push(tv.https ? t("encrypted (port 1926)") : t("unencrypted (port 1925)"));
  if (tv.model) parts.unshift(tv.model);
  return parts.join(" · ");
}

/** Who asked, for the record. */
export function originLabel(origin: string): string {
  switch (origin) {
    case "island":
      return t("Chat");
    case "phone":
      return t("Phone");
    case "routine":
      return t("Routine");
    case "settings":
      return t("Settings");
    // Other agents, through coucou-hook --mcp (agent/mcp.rs).
    case "hermes":
      return "Hermes Agent";
    case "agent":
      return t("Another agent");
    // Zheus One (src-tauri/src/zheus_one): a plan the user approved, and the Autopilot's goals.
    case "zheus":
      return "Zheus One";
    case "autopilot":
      return t("Autopilot");
    default:
      return origin;
  }
}

/** A routine step as a line: "TV (Sala): Open YouTube". */
export function stepLine(step: RoutineStep, devices: HomeDevice[]): string {
  const d = devices.find((x) => x.id === step.device);
  const name = d ? (d.room ? `${d.name} (${d.room})` : d.name) : step.device;
  const value = step.value ? ` ${step.value}` : "";
  const wait = step.wait > 0 ? ` (${t("after {seconds} s", { seconds: step.wait })})` : "";
  return `${name}: ${actionLabel(step.action)}${value}${wait}`;
}

/** A routine as Settings edits it: a copy, so a cancel leaves the stored one alone. */
export function cloneRoutine(r: Routine): Routine {
  return { id: r.id, name: r.name, steps: r.steps.map((s) => ({ ...s })) };
}
