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
  assert.ok(byText(section, "DIV", "Paired. AXYON can now use this TV."));
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

test("the voice block keeps the choice, reads a sentence to try, and says how to add a voice", async () => {
  const spoken = [];
  let voices = [{ name: "Microsoft Zira", lang: "en-US", localService: true }];
  globalThis.speechSynthesis = {
    getVoices: () => voices,
    speak: (u) => spoken.push(u),
    cancel() {},
    addEventListener() {},
    removeEventListener() {},
  };
  globalThis.SpeechSynthesisUtterance = class {
    constructor(text) {
      this.text = text;
    }
  };
  answers = { home_status: status() };
  assert.equal(all(homeSection(toggle)).find((e) => e.tagName === "H3" && e.textContent === "Voice"), undefined, "no block without the choice");

  const chosen = [];
  const section = homeSection(toggle, { on: false, set: (on) => chosen.push(on) });
  await flush();
  assert.ok(byText(section, "H3", "Voice"));
  assert.ok(byText(section, "DIV", "Voice: Microsoft Zira"));
  const row = all(section).find((e) => e.tagName === "DIV" && e.className === "row" && e.children.some((c) => c.textContent === "Read AXYON's answers aloud"));
  row.children[1].fire("click");
  assert.deepEqual(chosen, [true]);
  byText(section, "BUTTON", "Try").fire("click");
  await flush();
  await flush();
  assert.equal(spoken.at(-1)?.text, "Hello! This is how I read my answers.");

  voices = [{ name: "Haruka", lang: "ja-JP", localService: true }];
  const without = homeSection(toggle, { on: true, set: () => {} });
  await flush();
  const advice = "No voice for this language on this computer. On Windows, add one in Settings → Time & language → Speech.";
  assert.ok(byText(without, "DIV", advice));
  byText(without, "BUTTON", "Try").fire("click");
  await flush();
  await flush();
  assert.equal(spoken.length, 1, "nothing read without a voice");
  assert.ok(all(without).some((e) => e.tagName === "DIV" && e.className === "notice err" && e.textContent === advice));
});

const samsung = (fields = {}) => ({
  id: "samsung:0e0f1a1b",
  brand: "samsung",
  name: "Samsung 7 Series (43)",
  host: "192.168.0.9",
  model: "UN43AU7700GXZD",
  version: "2.0.25",
  os: "Tizen",
  android: false,
  https: true,
  needsPairing: true,
  paired: false,
  mac: "A4:30:7A:12:34:56",
  features: ["volume"],
  ...fields,
});

test("a Samsung TV found on the network is added, then paired by choosing Allow on its screen", async () => {
  assert.equal(tvLine(samsung()), "UN43AU7700GXZD · Tizen · API 2.0.25 · encrypted (port 8002) · volume by UPnP");
  assert.equal(foundLine({ kind: "samsungTv", name: "43\" Crystal UHD", host: "192.168.0.9", model: "AllShare1.0" }), "Samsung TV · 192.168.0.9 · AllShare1.0");

  answers = {
    home_status: status({ tvs: [] }),
    home_scan: [{ kind: "samsungTv", name: "43\" Crystal UHD", host: "192.168.0.9", model: "AllShare1.0", maker: "Samsung Electronics" }],
  };
  const section = homeSection(toggle);
  await flush();
  byText(section, "BUTTON", "Search the network").fire("click");
  await flush();
  await flush();
  assert.ok(byText(section, "SPAN", "43\" Crystal UHD"));
  answers.home_tv_add = samsung();
  answers.home_status = status({ tvs: [samsung()] });
  all(section).find((e) => e.tagName === "BUTTON" && e.textContent === "Add").fire("click");
  await flush();
  await flush();
  assert.deepEqual(sent("home_tv_add").at(-1), { host: "192.168.0.9" });
  assert.ok(byText(section, "SPAN", "Needs pairing: press Pair, then choose Allow on the TV."));

  // Pairing: the TV asks on its screen; the page says so while it waits, and asks for no PIN.
  let allow;
  const pinsBefore = sent("home_tv_pin").length;
  answers.home_tv_pair = () => new Promise((resolve) => (allow = resolve));
  byText(section, "BUTTON", "Pair").fire("click");
  await flush();
  // The name the TV knows this computer by stays the earlier one (smart_home/samsung.rs): paired TVs stay paired.
  assert.ok(byText(section, "DIV", "Look at the TV: it asks whether to allow Coucou. Choose Allow."));
  answers.home_status = status({ tvs: [samsung({ paired: true })] });
  allow(samsung({ paired: true }));
  await flush();
  await flush();
  await flush();
  assert.deepEqual(sent("home_tv_pair").at(-1), { id: "samsung:0e0f1a1b" });
  assert.ok(byText(section, "DIV", "Paired. AXYON can now use this TV."));
  assert.equal(all(section).find((e) => e.tagName === "INPUT" && e.getAttribute("placeholder") === "PIN"), undefined, "no PIN for a Samsung TV");
  assert.equal(sent("home_tv_pin").length, pinsBefore);
});

test("a TV that is neither Samsung nor Philips is not added, and what it answers on is said", async () => {
  const why = "This TV answers neither as a Samsung nor as a Philips TV. It does answer on: ADB (Android) (5555). Send this line to whoever helps you with AXYON: it tells how this TV could be controlled.";
  answers = { home_status: status({ tvs: [] }), home_tv_add: new Error(why) };
  const section = homeSection(toggle);
  await flush();
  const host = all(section).find((e) => e.tagName === "INPUT" && e.getAttribute("placeholder") === "192.168.1.20");
  host.value = "192.168.0.60";
  all(section).find((e) => e.tagName === "BUTTON" && e.textContent === "Add").fire("click");
  await flush();
  await flush();
  assert.deepEqual(sent("home_tv_add").at(-1), { host: "192.168.0.60" });
  assert.ok(all(section).some((e) => e.className === "notice err" && e.textContent === why));
});

const projector = (fields = {}) => ({
  id: "android:5a6b7c8d",
  brand: "android",
  name: "ATV_99",
  host: "192.168.0.99",
  model: "HY300-T",
  version: "13",
  os: "Android 13",
  android: true,
  https: false,
  needsPairing: true,
  paired: false,
  mac: "",
  features: ["volume", "mute"],
  ...fields,
});

test("an Android projector found on the network is tried over ADB, then allowed on its own screen", async () => {
  assert.equal(tvLine(projector()), "HY300-T · Android 13 · network debugging (ADB, port 5555)");
  answers = {
    home_status: status({ tvs: [] }),
    home_scan: [{ kind: "otherTv", name: "ATV_99", host: "192.168.0.99", model: "WaxRainPlayer", maker: "WaxRain" }],
  };
  const section = homeSection(toggle);
  await flush();
  byText(section, "BUTTON", "Search the network").fire("click");
  await flush();
  await flush();
  // A TV of another maker has its Add button too: Coucou tries ADB before saying what it answers on.
  answers.home_tv_add = projector();
  answers.home_status = status({ tvs: [projector()] });
  all(section).find((e) => e.tagName === "BUTTON" && e.textContent === "Add").fire("click");
  await flush();
  await flush();
  assert.deepEqual(sent("home_tv_add").at(-1), { host: "192.168.0.99" });
  assert.ok(all(section).some((e) => e.textContent === "Found: ATV_99 (HY300-T · Android 13 · network debugging (ADB, port 5555)). Now press Pair, then choose Allow on its screen."));
  assert.ok(byText(section, "SPAN", "Needs pairing: press Pair, then choose Allow on the device's screen."));
  let allow;
  answers.home_tv_pair = () => new Promise((resolve) => (allow = resolve));
  byText(section, "BUTTON", "Pair").fire("click");
  await flush();
  assert.ok(byText(section, "DIV", "Look at the device: it asks whether to allow USB debugging from this computer. Tick \"Always allow from this computer\", then choose Allow."));
  answers.home_status = status({ tvs: [projector({ paired: true })] });
  allow(projector({ paired: true }));
  await flush();
  await flush();
  assert.deepEqual(sent("home_tv_pair").at(-1), { id: "android:5a6b7c8d" });
  assert.ok(byText(section, "DIV", "Paired. AXYON can now use this TV."));
  assert.equal(all(section).find((e) => e.tagName === "INPUT" && e.getAttribute("placeholder") === "PIN"), undefined, "no PIN for an Android device");
});
