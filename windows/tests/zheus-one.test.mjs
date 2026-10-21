// Zheus One (src-tauri/src/zheus_one) as the island, Settings and the TV mode show it: the words and tiles of
// src/core/zheus.ts, the island's view (src/views/zheus.ts) and the section of Settings (src/settings/zheus.ts), on a
// fake DOM and through the real bridge — a plan approved with a click, a way around a conflict picked, the minutes of
// the way home, the preferences saved, and nothing sent before a click.

import { test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { internals, sent } from "./tauri.mjs";

installFakeDom();
const { clock, minuteOf, modeChoices, presenceLine, stepStates, tiles, when, DEFAULT_ARRIVAL, DEFAULT_GOALS } = await import("../src/core/zheus.ts");
const { degreesText, originLabel, stateLine, statusWords, valueHint, kindLabel } = await import("../src/core/home.ts");
const { State } = await import("../src/core/state.ts");
const { buildZheus } = await import("../src/views/zheus.ts");
const { zheusSection } = await import("../src/settings/zheus.ts");

/** What the mocked Rust side answers, by command. */
let answers = {};
const plainInvoke = internals.invoke;
internals.invoke = async (cmd, args) => {
  const result = await plainInvoke(cmd, args);
  if (cmd in answers) {
    const a = typeof answers[cmd] === "function" ? answers[cmd](args) : answers[cmd];
    if (a instanceof Error) throw a;
    return a;
  }
  return result;
};
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
const all = (root) => [root, ...root.walk()];
const byText = (root, tag, text) => all(root).find((e) => e.tagName === tag && e.textContent === text);
const byClass = (root, cls) => all(root).filter((e) => (e.className ?? "").split(" ").includes(cls));

const device = (fields) => ({
  id: "x",
  name: "X",
  room: "Sala",
  kind: "light",
  source: "Home Assistant",
  model: null,
  caps: ["turnOn", "turnOff"],
  apps: [],
  channels: [],
  state: { online: true, on: false },
  exposed: true,
  ask: false,
  ...fields,
});

const robo = (status, battery = 80) =>
  device({ id: "ha:vacuum.robo", name: "Robô", kind: "vacuum", caps: ["clean", "cleanArea", "pause", "dock"], state: { online: true, on: status === "cleaning", status, battery } });
const ar = (on) =>
  device({ id: "ha:climate.quarto", name: "Ar do Quarto", room: "Quarto", kind: "climate", caps: ["turnOn", "turnOff", "temperature"], state: { online: true, on, status: on ? "cool" : "off", temperature: 275, target: 230 } });

const world = (devices, user = { state: "arriving", at: 18 * 60 + 35 }) => ({
  now: 18 * 60 + 10,
  user,
  occupied: false,
  people: [],
  devices,
  rooms: [],
  facts: ["You arrive around 18:35."],
});

const plan = (fields = {}) => ({
  id: "z-0a1b2c3d4e5f",
  mission: "arrival",
  eta: 25,
  arriveAt: 18 * 60 + 35,
  madeAt: 18 * 60 + 10,
  steps: [
    { at: 0, kind: "device", device: "ha:vacuum.robo", deviceName: "Robô", action: "clean_area", value: "Quarto", label: "Robô · Clean Quarto", why: "", asks: false },
    { at: 10, kind: "device", device: "ha:climate.quarto", deviceName: "Ar do Quarto", action: "set_temperature", value: "23", label: "Ar do Quarto · Set to 23 °C", why: "", asks: true },
    { at: 25, kind: "welcome", device: "", deviceName: "", action: "", value: "", label: "Welcome home", why: "", asks: false },
  ],
  conflicts: ["You arrive in about 25 min, but the cleaning takes 35."],
  choices: [
    { id: "rooms", label: "Clean only Quarto now (about 15 min), and Sala another time" },
    { id: "dock", label: "Start now, and send it back to its dock just before you arrive" },
    { id: "skip", label: "No cleaning today" },
  ],
  chosen: "rooms",
  narration: "",
  forModel: "",
  ...fields,
});

const status = (fields = {}) => ({
  enabled: true,
  homeOn: true,
  arrival: { ...DEFAULT_ARRIVAL },
  goals: { ...DEFAULT_GOALS },
  pending: null,
  running: null,
  suggestions: [],
  briefing: null,
  tv: false,
  ...fields,
});

// ── The words ────────────────────────────────────────────────────────────────

test("times, the way home and where the user is are said plainly", () => {
  assert.equal(clock(1110), "18:30");
  assert.equal(clock(1440 + 5), "00:05");
  assert.equal(minuteOf("18:30"), 1110);
  assert.equal(minuteOf("7:05"), 425);
  assert.equal(minuteOf("25:00"), null);
  assert.equal(minuteOf("seis"), null);
  const p = plan();
  assert.equal(when(p, p.steps[0]), "now");
  assert.equal(when(p, p.steps[1]), "18:20");
  assert.equal(presenceLine(world([])), "You arrive around 18:35");
  assert.equal(presenceLine(world([], { state: "home" })), "You are home");
  assert.equal(presenceLine({ ...world([], { state: "unknown" }), occupied: true }), "Someone is home");
  assert.equal(presenceLine({ ...world([], { state: "unknown" }), occupied: null }), "The house");
  assert.deepEqual(modeChoices("business").map((m) => m.id), ["off", "suggest"], "business never acts alone");
  assert.deepEqual(modeChoices("energy").map((m) => m.id), ["off", "suggest", "act"]);
});

test("a vacuum, an air conditioner and a sensor are said in words", () => {
  assert.equal(stateLine(robo("docked").state), "On its dock · Battery 80%");
  assert.equal(stateLine(ar(true).state), "Cooling · 27.5 °C · Set to 23 °C");
  assert.equal(stateLine(ar(false).state), "Off · 27.5 °C · Set to 23 °C", "an air conditioner off says Off");
  assert.equal(stateLine({ online: true, temperature: 248, humidity: 61 }), "24.8 °C · Humidity 61%");
  assert.equal(degreesText(235, "pt-BR"), "23,5 °C");
  assert.equal(degreesText(230, "pt-BR"), "23 °C");
  assert.equal(statusWords("returning"), "Going back to its dock");
  assert.equal(statusWords("off"), null);
  assert.equal(valueHint("rooms"), "Quarto, Sala");
  assert.equal(valueHint("degrees"), "10–32");
  assert.equal(kindLabel("vacuum"), "Vacuum");
  assert.equal(originLabel("zheus"), "Zheus One");
  assert.equal(originLabel("autopilot"), "Autopilot");
});

test("the tiles show the house and offer what each device can do", () => {
  const lamps = [device({ id: "ha:light.a", name: "Luz A", state: { online: true, on: true } }), device({ id: "ha:light.b", name: "Luz B" })];
  const tv = device({ id: "philips:aa", name: "TV", kind: "tv", state: { online: true, on: false } });
  const t = tiles(world([robo("cleaning", 64), ar(true), ...lamps, tv]));
  assert.deepEqual(t.map((x) => x.kind), ["vacuum", "climate", "light", "tv"]);
  assert.equal(t[0].line, "Cleaning · 64%");
  assert.equal(t[0].toggle.action, "return_to_base", "the switch of a vacuum at work sends it home");
  assert.deepEqual(t[0].actions.map((a) => a.action), ["pause"]);
  assert.equal(t[1].line, "Set to 23 °C · Room 27.5 °C");
  assert.equal(t[1].toggle.action, "turn_off");
  assert.deepEqual(t[1].actions.map((a) => [a.label, a.value ?? ""]), [["−", "22"], ["+", "24"]]);
  assert.equal(t[2].title, "Lights");
  assert.equal(t[2].line, "1 of 2 on");
  assert.deepEqual(t[2].toggle.targets, ["ha:light.a"], "only the lights that are on are turned off");
  assert.equal(t[3].toggle.action, "turn_on");
  assert.deepEqual(t[3].actions, []);
  const docked = tiles(world([robo("docked")]))[0];
  assert.equal(docked.toggle.action, "clean");
  assert.deepEqual(docked.actions, []);
  assert.equal(docked.on, false);
  const away = tiles(world([robo("returning")]))[0];
  assert.equal(away.toggle, null, "a vacuum on its way back has nothing to switch");
  const silent = tiles(world([{ ...ar(true), state: { ...ar(true).state, online: false } }]))[0];
  assert.equal(silent.toggle, null, "a device not answering can't be switched");
  assert.deepEqual(silent.actions, []);
  const hidden = tiles(world([{ ...robo("docked"), exposed: false }]));
  assert.equal(hidden.length, 0, "a device not shared with Zheus is not shown");
});

test("a running plan's steps are done, failed or still to come", () => {
  const p = plan();
  assert.deepEqual(stepStates({ plan: p, done: [{ ok: true, detail: "" }, { ok: false, detail: "Not allowed." }, null] }), ["done", "failed", "waiting"]);
  assert.deepEqual(stepStates(null), []);
});

// ── The island's view ────────────────────────────────────────────────────────

const actions = () => {
  const calls = [];
  return { calls, blip: () => calls.push("blip"), openSettingsWindow: () => calls.push("settings") };
};

test("a plan waits on the island: its steps, what clashes, a way around it, and Approve sends its id", async () => {
  answers = { zheus_world: world([robo("docked"), ar(false)]), zheus_approve: plan(), zheus_choose: plan({ chosen: "dock" }) };
  State.zheus = status({ pending: plan() });
  State.view = "zheus";
  State.mode = "expanded";
  const host = buildZheus(actions());
  host.sync();
  await flush();
  await flush();
  assert.ok(byText(host.el, "SPAN", "Your arrival: the plan"));
  assert.ok(byText(host.el, "SPAN", "Robô · Clean Quarto"));
  assert.ok(byText(host.el, "SPAN", "asks first"), "a step on a device that asks says so");
  assert.ok(byText(host.el, "SPAN", "You arrive in about 25 min, but the cleaning takes 35."));
  assert.equal(sent("zheus_approve").length, 0, "nothing runs before the click");
  byText(host.el, "BUTTON", "Start now, and send it back to its dock just before you arrive").fire("click");
  await flush();
  assert.deepEqual(sent("zheus_choose").at(-1), { choice: "dock" });
  byText(host.el, "SPAN", "Approve").parentElement.fire("click");
  await flush();
  assert.deepEqual(sent("zheus_approve").at(-1), { id: "z-0a1b2c3d4e5f" });
});

test("the tiles act on a click, and the minutes of the way home make a plan", async () => {
  answers = { zheus_world: world([robo("docked"), ar(false)]), zheus_device: { ok: true, detail: "Cleaning" }, zheus_plan: plan() };
  State.zheus = status();
  State.view = "overview";
  const host = buildZheus(actions());
  host.sync();
  State.view = "zheus";
  host.sync();
  await flush();
  await flush();
  const vacuumTile = byClass(host.el, "zheus-tile").find((e) => (e.className ?? "").includes("vacuum"));
  assert.ok(vacuumTile, "the vacuum has its tile");
  const vacuumSwitch = byClass(vacuumTile, "zheus-switch")[0];
  assert.equal(vacuumSwitch.getAttribute("aria-checked"), "false");
  assert.equal(sent("zheus_device").length, 0, "nothing reaches a device before the click");
  vacuumSwitch.fire("click");
  await flush();
  assert.deepEqual(sent("zheus_device").at(-1), { id: "ha:vacuum.robo", action: "clean", value: null });
  byText(host.el, "BUTTON", "30").fire("click");
  await flush();
  assert.deepEqual(sent("zheus_plan").at(-1), { eta: 30 });
});

test("a kind not shared with Zheus has its empty place, which opens Settings", async () => {
  answers = { zheus_world: world([robo("docked"), ar(false)]) };
  State.zheus = status();
  State.view = "zheus";
  State.mode = "expanded";
  const a = actions();
  const host = buildZheus(a);
  host.sync();
  await flush();
  await flush();
  const ghosts = byClass(host.el, "ghost");
  assert.deepEqual(ghosts.map((g) => g.className.split(" ").at(-1)), ["light", "tv"], "the lights and the TV are not shared");
  assert.ok(byText(ghosts[0], "SPAN", "Not shared yet"));
  ghosts[1].fire("click");
  assert.deepEqual(a.calls.filter((c) => c === "settings"), ["settings"]);
  assert.equal(sent("zheus_device").filter((c) => c.action === "turn_on").length, 0, "an empty place turns nothing on");
});

test("off, the view says how to turn Zheus One on", () => {
  State.zheus = status({ enabled: false });
  State.view = "zheus";
  State.mode = "expanded";
  const a = actions();
  const host = buildZheus(a);
  host.sync();
  byText(host.el, "SPAN", "Open Settings").parentElement.fire("click");
  assert.deepEqual(a.calls.filter((c) => c === "settings"), ["settings"]);
});

// ── Settings → Zheus One ─────────────────────────────────────────────────────

const toggle = (on, onChange) => {
  const el = document.createElement("button");
  el.className = on ? "switch on" : "switch";
  el.addEventListener("click", () => {
    const next = !el.classList.contains("on");
    el.classList.toggle("on", next);
    onChange(next);
  });
  return el;
};

test("Settings saves the arrival and the goals, and the look is the island's own choice", async () => {
  answers = {
    zheus_status: status({ arrival: { ...DEFAULT_ARRIVAL, rooms: [{ room: "Quarto", minutes: 15 }] } }),
    home_devices: [device({ id: "ha:light.sala", name: "Luz da Sala" }), device({ id: "philips:aa", name: "TV", kind: "tv" })],
    zheus_prefs_set: status(),
  };
  let look = "glass";
  const section = zheusSection(toggle, { get: () => look, set: (v) => (look = v) });
  await flush();
  await flush();
  byText(section, "BUTTON", "Classic").fire("click");
  assert.equal(look, "classic");
  const rooms = all(section).filter((e) => e.tagName === "INPUT" && e.getAttribute("placeholder") === "Room");
  assert.equal(rooms.length, 1);
  assert.equal(rooms[0].value, "Quarto");
  // The goals: Off · Suggest · Act, and business has no Act.
  const goal = (name) => all(section).find((e) => (e.className ?? "") === "zheus-goal" && byText(e, "SPAN", name));
  const energy = goal("Less energy wasted");
  const business = goal("Your business moving on");
  assert.deepEqual(byClass(business, "zheus-seg")[0].children.map((b) => b.textContent), ["Off", "Suggest"], "business only ever suggests");
  byText(energy, "BUTTON", "Act").fire("click");
  byText(business, "BUTTON", "Suggest").fire("click");
  assert.equal(byText(energy, "BUTTON", "Act").getAttribute("aria-checked"), "true");
  byText(section, "BUTTON", "Save").fire("click");
  await flush();
  const saved = sent("zheus_prefs_set").at(-1);
  assert.deepEqual(saved.arrival.rooms, [{ room: "Quarto", minutes: 15 }]);
  assert.equal(saved.arrival.eta, 30);
  assert.equal(saved.goals.energy, "act");
  assert.equal(saved.goals.business, "suggest");
  assert.equal(saved.goals.agendaAt, 8 * 60);
  assert.equal(sent("zheus_set").length, 0, "the switch is not touched by a save");
});
