// Mochi's home in Settings: the words for devices, states and routines (src/core/home.ts), and the section that
// draws them (src/settings/home.ts), on a fake DOM and through the real bridge — the pairing of a TV, a test from
// Settings, a routine saved from a suggestion.

import { test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { internals, sent } from "./tauri.mjs";

installFakeDom();
const {
  actionsFor, capsLine, foundLine, kindLabel, originLabel, roomsOf, stateLine, stepLine, tvLine,
} = await import("../src/core/home.ts");
const { homeSection } = await import("../src/settings/home.ts");

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

const tv = (fields = {}) => ({
  id: "philips:aa",
  name: "TV",
  room: "Sala",
  kind: "tv",
  source: "Philips TV",
  model: "55PUS8007/12",
  caps: ["turnOn", "turnOff", "volumeSet", "volumeStep", "mute", "apps", "channels", "channelStep"],
  apps: ["Netflix", "YouTube"],
  channels: [{ number: "4", name: "Globo" }, { number: "5", name: "SBT" }],
  state: { online: true, on: true, volume: 20, muted: false, app: "YouTube" },
  exposed: true,
  ask: false,
  ...fields,
});

const status = (fields = {}) => ({
  enabled: true,
  askAll: false,
  haUrl: "",
  haToken: false,
  tvs: [{ id: "philips:aa", name: "Philips TV", host: "192.168.1.20", model: "55PUS8007/12", version: "6.4.0", android: true, https: true, needsPairing: true, paired: false, mac: "", features: ["keys", "apps"] }],
  routines: [],
  prefs: {},
  ...fields,
});

/** Every element under `root` (depth first). */
const all = (root) => [root, ...root.walk()];
const byText = (root, tag, text) => all(root).find((e) => e.tagName === tag && e.textContent === text);
const toggle = (on, onChange) => {
  const el = document.createElement("button");
  el.className = on ? "switch on" : "switch";
  el.addEventListener("click", () => onChange(!el.classList.contains("on")));
  return el;
};

// ── The words ────────────────────────────────────────────────────────────────

test("a device's state and abilities are said in words", () => {
  assert.equal(stateLine(tv().state), "On · Volume 20% · YouTube");
  assert.equal(stateLine({ online: false }), "Not answering");
  assert.equal(stateLine({ online: true, on: false, brightness: 30 }), "Off · Brightness 30%");
  assert.equal(capsLine(tv()), "Turn on, Turn off, Set the volume, Volume up, Volume down, Mute, Unmute, 2 apps, 2 channels, Next channel, Previous channel");
  assert.equal(capsLine(tv({ caps: [] })), "Reported nothing it can do");
  assert.deepEqual(actionsFor(tv({ caps: ["turnOn", "brightness"] })).map((a) => a.id), ["turn_on", "set_brightness"]);
  assert.deepEqual(actionsFor(undefined), []);
  assert.equal(kindLabel("plug"), "Plug");
});

test("rooms, finds, TVs, steps and the record read naturally", () => {
  assert.deepEqual(roomsOf([tv(), tv({ id: "b", room: "Quarto" }), tv({ id: "c", room: " " })], { x: { name: "", room: "Cozinha", exposed: null, ask: null } }), ["Cozinha", "Quarto", "Sala"]);
  assert.equal(foundLine({ kind: "philipsTv", name: "TV", host: "192.168.1.20", model: "55PUS8007/12" }), "Philips TV · 192.168.1.20 · 55PUS8007/12");
  assert.equal(foundLine({ kind: "homeAssistant", name: "Casa", host: "192.168.1.10", version: "2026.10.1" }), "Home Assistant · 192.168.1.10 · 2026.10.1");
  assert.equal(foundLine({ kind: "otherTv", name: "Q60", host: "192.168.1.30", maker: "Samsung" }), "Samsung · 192.168.1.30");
  assert.equal(tvLine(status().tvs[0]), "55PUS8007/12 · JointSpace 6.4.0 · Android TV · encrypted (port 1926)");
  assert.equal(stepLine({ device: "philips:aa", action: "open_app", value: "YouTube", wait: 5 }, [tv()]), "TV (Sala): Open an app YouTube (after 5 s)");
  assert.equal(stepLine({ device: "gone", action: "turn_off", value: "", wait: 0 }, []), "gone: Turn off");
  assert.equal(originLabel("phone"), "Phone");
});

// ── The section ──────────────────────────────────────────────────────────────

test("a TV is paired with the PIN it shows, and nothing is sent before the clicks", async () => {
  answers = { home_status: status() };
  const section = homeSection(toggle);
  await flush();
  assert.ok(byText(section, "SPAN", "Philips TV"), "the TV is listed");
  assert.equal(sent("home_tv_pair").length, 0);
  byText(section, "BUTTON", "Pair").fire("click");
  await flush();
  assert.deepEqual(sent("home_tv_pair").at(-1), { id: "philips:aa" });
  const pin = all(section).find((e) => e.tagName === "INPUT" && e.getAttribute("placeholder") === "PIN");
  assert.ok(pin, "the PIN field shows up");
  pin.value = "1234";
  answers.home_tv_pin = { ...status().tvs[0], paired: true };
  answers.home_status = status({ tvs: [{ ...status().tvs[0], paired: true }] });
  byText(section, "BUTTON", "Confirm").fire("click");
  await flush();
  await flush();
  assert.deepEqual(sent("home_tv_pin").at(-1), { id: "philips:aa", pin: "1234" });
  assert.ok(byText(section, "DIV", "Paired. Mochi can now use this TV."));
});

test("a device is tested from Settings with the action and value picked", async () => {
  answers = { home_status: status(), home_devices: [tv()], home_test: { ok: true, detail: "On · YouTube" } };
  const section = homeSection(toggle);
  await flush();
  byText(section, "BUTTON", "Read the devices").fire("click");
  await flush();
  await flush();
  const select = all(section).find((e) => e.tagName === "SELECT");
  assert.ok(select, "the actions it reported are offered");
  assert.ok(select.children.some((o) => o.getAttribute("value") === "open_app"));
  select.value = "open_app";
  select.fire("change");
  const value = all(section).find((e) => e.tagName === "INPUT" && e.getAttribute("list") === "home-values-philips:aa");
  assert.equal(value.hidden, false);
  value.value = "YouTube";
  byText(section, "BUTTON", "Test").fire("click");
  await flush();
  await flush();
  assert.deepEqual(sent("home_test").at(-1), { id: "philips:aa", action: "open_app", value: "YouTube" });
  assert.ok(byText(section, "DIV", "On · YouTube"));
  // An action that failed says why.
  answers.home_test = { ok: false, detail: "The TV does not answer." };
  byText(section, "BUTTON", "Test").fire("click");
  await flush();
  await flush();
  assert.ok(byText(section, "DIV", "The TV does not answer."));
});

test("a suggested routine is reviewed and saved, and the switches go through Rust", async () => {
  const goodNight = { id: "tpl-good-night", name: "Good night", steps: [{ device: "philips:aa", action: "turn_off", value: "", wait: 0 }] };
  answers = { home_status: status({ enabled: false }), home_devices: [tv()], home_templates: [goodNight], home_routine_save: goodNight };
  const section = homeSection(toggle);
  await flush();
  byText(section, "BUTTON", "Suggestions").fire("click");
  await flush();
  await flush();
  assert.ok(byText(section, "SPAN", "TV (Sala): Turn off"));
  byText(section, "BUTTON", "Edit and save").fire("click");
  answers.home_status = status({ enabled: false, routines: [goodNight] });
  all(section).find((e) => e.tagName === "BUTTON" && e.textContent === "Save" && e.className === "primary").fire("click");
  await flush();
  await flush();
  const saved = sent("home_routine_save").at(-1).routine;
  assert.equal(saved.id, "", "a suggestion is saved as a new routine");
  assert.equal(saved.name, "Good night");
  assert.deepEqual(saved.steps, goodNight.steps);
  // The switch asks Rust, which owns it.
  const switches = all(section).filter((e) => e.className.startsWith("switch"));
  answers.home_set = status();
  switches[0].fire("click");
  await flush();
  assert.deepEqual(sent("home_set").at(-1), { enabled: true, askAll: false });
});
