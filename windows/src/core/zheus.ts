// Zheus One (src-tauri/src/zheus_one), as the island, Settings and the TV mode show it: the shapes Rust sends, and the
// pure helpers that turn them into tiles, lines and timelines. Tested without a webview; views/zheus.ts and
// settings/zheus.ts draw them.

import { degreesText, statusWords, stateLine, type HomeDevice } from "./home";
import { t } from "../i18n/i18n";

export type Mode = "off" | "suggest" | "act";

export interface RoomTime {
  room: string;
  minutes: number;
}

/** zheus_one::plan::ArrivalPrefs. */
export interface ArrivalPrefs {
  eta: number;
  comfort: number;
  acLead: number;
  vacuum: boolean;
  rooms: RoomTime[];
  whole: number;
  minBattery: number;
  lights: string[];
  darkFrom: number;
  darkUntil: number;
  tv: string;
  tvApp: string;
  summary: boolean;
}

/** zheus_one::autopilot::Goals. */
export interface Goals {
  energy: Mode;
  tidy: Mode;
  agenda: Mode;
  business: Mode;
  tidyFrom: number;
  tidyUntil: number;
  agendaAt: number;
  awayMinutes: number;
}

export interface PlanStep {
  /** Minutes after the plan was made. */
  at: number;
  kind: "device" | "summary" | "welcome";
  device: string;
  deviceName: string;
  action: string;
  value: string;
  label: string;
  why: string;
  asks: boolean;
}

export interface Choice {
  id: string;
  label: string;
}

export interface Plan {
  id: string;
  mission: "arrival" | "situation";
  eta: number;
  arriveAt: number;
  madeAt: number;
  steps: PlanStep[];
  conflicts: string[];
  choices: Choice[];
  chosen: string;
  narration: string;
  forModel: string;
}

export interface StepOutcome {
  ok: boolean;
  detail: string;
}

export interface Planned {
  device: string;
  deviceName: string;
  action: string;
  value: string;
  label: string;
}

export interface Suggestion {
  key: string;
  goal: string;
  text: string;
  actions: Planned[];
  ask: string | null;
  category: string;
  auto: boolean;
}

export interface Briefing {
  at: string;
  text: string;
}

/** zheus_one::Status. */
export interface ZheusStatus {
  enabled: boolean;
  homeOn: boolean;
  arrival: ArrivalPrefs;
  goals: Goals;
  pending: Plan | null;
  running: { plan: Plan; done: (StepOutcome | null)[] } | null;
  suggestions: Suggestion[];
  briefing: Briefing | null;
  tv: boolean;
}

export type Presence = { state: "unknown" } | { state: "home" } | { state: "away" } | { state: "arriving"; at: number };

export interface Room {
  name: string;
  temperature: number | null;
  humidity: number | null;
  on: string[];
}

/** zheus_one::world::World. */
export interface World {
  now: number;
  user: Presence;
  occupied: boolean | null;
  people: { name: string; home: boolean }[];
  devices: HomeDevice[];
  rooms: Room[];
  facts: string[];
}

/** The ways home offered on the island, in minutes. */
export const ETAS = [15, 30, 45, 60] as const;

/** A minute of the day as a clock reads it: 1110 → "18:30". */
export function clock(minute: number): string {
  const m = ((Math.round(minute) % 1440) + 1440) % 1440;
  return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
}

/** "18:30" → 1110; null when it is not a time. */
export function minuteOf(text: string): number | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(text.trim());
  if (!m) return null;
  const [h, min] = [Number(m[1]), Number(m[2])];
  return h < 24 && min < 60 ? h * 60 + min : null;
}

/** When a step runs, as the user reads it: "now", or the clock. */
export function when(plan: Plan, step: PlanStep): string {
  return step.at === 0 ? t("now") : clock(plan.madeAt + step.at);
}

/** The line at the top of the island's Zheus view: where the user is, as Zheus knows it. */
export function presenceLine(world: World): string {
  switch (world.user.state) {
    case "home":
      return t("You are home");
    case "away":
      return t("You are away");
    case "arriving":
      return t("You arrive around {time}", { time: clock(world.user.at) });
    default:
      return world.occupied === true ? t("Someone is home") : world.occupied === false ? t("Nobody is home") : t("The house");
  }
}

/** One action a tile offers: on one device, or on several (all the lights). */
export interface TileAction {
  action: string;
  value?: string;
  label: string;
  targets: string[];
}

export interface Tile {
  kind: "vacuum" | "climate" | "light" | "tv";
  title: string;
  line: string;
  on: boolean;
  /** What the tile's switch does: turns the device off when it is on, on when it is off (null: it can't now). */
  toggle: TileAction | null;
  /** The rest it offers: the temperature's − and +, the vacuum's pause. */
  actions: TileAction[];
}

/** The kinds the island has a tile for, in their order. */
export const TILE_KINDS: Tile["kind"][] = ["vacuum", "climate", "light", "tv"];

const has = (d: HomeDevice, cap: HomeDevice["caps"][number]) => d.caps.includes(cap);

/** The house as the island's tiles: the vacuum, the air conditioning, the lights, the TV, as they are now. */
export function tiles(world: World, lang?: string): Tile[] {
  const out: Tile[] = [];
  const live = world.devices.filter((d) => d.exposed);

  const vacuum = live.find((d) => d.kind === "vacuum");
  if (vacuum) {
    const s = vacuum.state;
    const cleaning = s.status === "cleaning" || s.status === "returning";
    let toggle: TileAction | null = null;
    const actions: TileAction[] = [];
    if (s.online && s.status === "cleaning") {
      if (has(vacuum, "dock")) toggle = { action: "return_to_base", label: t("Dock"), targets: [vacuum.id] };
      if (has(vacuum, "pause")) actions.push({ action: "pause", label: t("Pause"), targets: [vacuum.id] });
    } else if (s.online && has(vacuum, "clean") && s.status !== "returning") {
      toggle = { action: "clean", label: t("Clean"), targets: [vacuum.id] };
    }
    const parts = [statusWords(s.status) ?? (s.online ? "" : t("Not answering")), s.battery != null ? `${s.battery}%` : ""].filter(Boolean);
    out.push({ kind: "vacuum", title: vacuum.name, line: parts.join(" · "), on: cleaning, toggle, actions });
  }

  const climates = live.filter((d) => d.kind === "climate");
  const climate = climates.find((d) => d.state.on === true) ?? climates[0];
  if (climate) {
    const s = climate.state;
    let toggle: TileAction | null = null;
    const actions: TileAction[] = [];
    if (s.online && has(climate, "temperature") && s.target != null) {
      const target = Math.round(s.target / 10);
      if (target > 10) actions.push({ action: "set_temperature", value: String(target - 1), label: "−", targets: [climate.id] });
      if (target < 32) actions.push({ action: "set_temperature", value: String(target + 1), label: "+", targets: [climate.id] });
    }
    if (s.online && s.on === true && has(climate, "turnOff")) toggle = { action: "turn_off", label: t("Turn off"), targets: [climate.id] };
    if (s.online && s.on !== true && has(climate, "turnOn")) toggle = { action: "turn_on", label: t("Turn on"), targets: [climate.id] };
    const parts: string[] = [];
    if (!s.online) parts.push(t("Not answering"));
    else if (s.on === true && s.target != null) parts.push(t("Set to {degrees} °C", { degrees: degreesText(s.target, lang).replace(" °C", "") }));
    else parts.push(statusWords(s.status) ?? (s.on === false ? t("Off") : ""));
    if (s.temperature != null) parts.push(t("Room {degrees}", { degrees: degreesText(s.temperature, lang) }));
    out.push({ kind: "climate", title: climate.name, line: parts.filter(Boolean).join(" · "), on: s.on === true, toggle, actions });
  }

  const lights = live.filter((d) => d.kind === "light" && d.state.online);
  if (lights.length) {
    const on = lights.filter((d) => d.state.on === true);
    const toggle: TileAction = on.length
      ? { action: "turn_off", label: t("Turn off"), targets: on.filter((d) => has(d, "turnOff")).map((d) => d.id) }
      : { action: "turn_on", label: t("Turn on"), targets: lights.filter((d) => has(d, "turnOn")).map((d) => d.id) };
    out.push({
      kind: "light",
      title: lights.length === 1 ? lights[0].name : t("Lights"),
      line: lights.length === 1 ? stateLine(lights[0].state, lang) : t("{on} of {count} on", { on: on.length, count: lights.length }),
      on: on.length > 0,
      toggle: toggle.targets.length > 0 ? toggle : null,
      actions: [],
    });
  }

  const tvs = live.filter((d) => d.kind === "tv");
  const tv = tvs.find((d) => d.state.on === true) ?? tvs[0];
  if (tv) {
    const s = tv.state;
    let toggle: TileAction | null = null;
    if (s.online && s.on === true && has(tv, "turnOff")) toggle = { action: "turn_off", label: t("Turn off"), targets: [tv.id] };
    if (s.on !== true && has(tv, "turnOn")) toggle = { action: "turn_on", label: t("Turn on"), targets: [tv.id] };
    out.push({ kind: "tv", title: tv.name, line: stateLine(s, lang) || t("Off"), on: s.on === true, toggle, actions: [] });
  }
  return out;
}

/** What became of each step of a running plan: done, failed, or still to come. */
export function stepStates(running: ZheusStatus["running"]): ("done" | "failed" | "waiting")[] {
  if (!running) return [];
  return running.plan.steps.map((_, i) => {
    const o = running.done[i];
    return o == null ? "waiting" : o.ok ? "done" : "failed";
  });
}

/** The Autopilot's modes, a word each and a line on hover. Business never acts alone, so it has no "act". */
export function modeChoices(goal: keyof Pick<Goals, "energy" | "tidy" | "agenda" | "business">): { id: Mode; label: string; title: string }[] {
  const all: { id: Mode; label: string; title: string }[] = [
    { id: "off", label: t("Off"), title: t("Off") },
    { id: "suggest", label: t("Suggest"), title: t("Suggest") },
    { id: "act", label: t("Act"), title: t("Act within the limits") },
  ];
  return goal === "business" ? all.slice(0, 2) : all;
}

export const DEFAULT_ARRIVAL: ArrivalPrefs = {
  eta: 30,
  comfort: 23,
  acLead: 15,
  vacuum: true,
  rooms: [],
  whole: 40,
  minBattery: 30,
  lights: [],
  darkFrom: 18 * 60,
  darkUntil: 6 * 60,
  tv: "",
  tvApp: "",
  summary: true,
};

export const DEFAULT_GOALS: Goals = {
  energy: "off",
  tidy: "off",
  agenda: "off",
  business: "off",
  tidyFrom: 9 * 60,
  tidyUntil: 17 * 60,
  agendaAt: 8 * 60,
  awayMinutes: 20,
};
