// Zheus: Hermes Agent installed, set up and run by Coucou (src/settings/hermes.ts, src-tauri/src/hermes_setup.rs):
// the plan shown before the click, the brains offered, what the click sends, and the gateway's state in words.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { installFakeDom } from "./fakedom.mjs";
import { internals, sent } from "./tauri.mjs";

installFakeDom();
const { hermesSection, zheusPlan, gatewayText, progressText } = await import("../src/settings/hermes.ts");
const { DEFAULT_SETTINGS } = await import("../src/core/state.ts");

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
const toggle = (on, onChange) => {
  const el = document.createElement("button");
  el.className = on ? "switch on" : "switch";
  el.addEventListener("click", () => onChange(!el.classList.contains("on")));
  return el;
};

const BRAINS = [
  { id: "anthropic", name: "Anthropic", ready: true, model: "claude-opus-5" },
  { id: "openai", name: "OpenAI", ready: false, model: "" },
  { id: "google", name: "Google Gemini", ready: true, model: "" },
  { id: "ollama", name: "Ollama", ready: false, model: "" },
];
const status = (over = {}) => ({
  installed: false,
  managed: false,
  home: "C:\\Users\\Ana\\AppData\\Local\\hermes",
  brains: BRAINS,
  brain: "",
  progress: { running: false, step: "", done: 0, total: 0, last: "", error: null, finished: false },
  gateway: { state: "off" },
  address: "http://127.0.0.1:8642",
  commit: "6e7b83a2f1",
  installer: "https://raw.githubusercontent.com/NousResearch/hermes-agent/6e7b83a2f1c093e992a8d838ed77ea320fbd8e51/scripts/install.ps1",
  ...over,
});
const host = (settings, names = []) => ({ settings: () => settings, useInChat: async () => {}, setName: (n) => names.push(n), toggle });

test("the plan says what will be downloaded, where it goes, what key Hermes gets and where it listens", () => {
  const lines = zheusPlan(status(), BRAINS[0], true);
  assert.equal(lines.length, 4);
  assert.ok(lines[0].includes("6e7b83a2f1") && lines[0].includes("SHA-256"));
  assert.ok(lines[1].includes("C:\\Users\\Ana\\AppData\\Local\\hermes"));
  assert.ok(lines[2].includes("Anthropic") && lines[2].includes("never in a file"));
  assert.ok(lines[3].includes("http://127.0.0.1:8642") && lines[3].includes("Mochi's home"));
  assert.ok(!zheusPlan(status(), BRAINS[0], false)[3].includes("Mochi's home"), "the house only when it is ticked");
  assert.ok(zheusPlan(status(), { id: "ollama", name: "Ollama", ready: true, model: "" }, false)[2].includes("Ollama on this computer"), "a local server has no key");
});

test("the gateway's state and the installation's progress are said in plain words", () => {
  const states = [{ state: "off" }, { state: "starting" }, { state: "running" }, { state: "elsewhere" }, { state: "stopped", reason: "port in use" }];
  const lines = states.map(gatewayText);
  assert.equal(new Set(lines).size, states.length);
  assert.ok(lines[4].includes("port in use"));
  assert.equal(progressText({ running: true, step: "Create Python environment", done: 2, total: 9, last: "", error: null, finished: false }), "Step 3 of 9: Create Python environment");
  assert.equal(progressText({ running: true, step: "Downloading Hermes' installer…", done: 0, total: 0, last: "", error: null, finished: false }), "Downloading Hermes' installer…");
});

test("nothing is installed before the click, and the click sends the brain picked and the house", async () => {
  answers = {
    home_mcp_status: { on: false, command: "x", homeOn: true },
    hermes_setup_status: status(),
    hermes_setup_start: (args) => status({ progress: { running: true, step: "System prerequisites", done: 0, total: 9, last: "git ok", error: null, finished: false }, brain: args.brain }),
  };
  const names = [];
  const section = hermesSection(host({ ...DEFAULT_SETTINGS }, names));
  await flush();
  assert.equal(sent("hermes_setup_start").length, 0, "not before the click");
  const select = all(section.el).find((e) => e.tagName === "SELECT");
  const options = select.children;
  assert.equal(options.length, BRAINS.length);
  assert.deepEqual(options.filter((o) => o.disabled).map((o) => o.getAttribute("value")), ["openai", "ollama"], "no key, no choice");
  assert.equal(select.value, "anthropic", "the first one ready");
  assert.ok(all(section.el).some((e) => e.tagName === "LI" && e.textContent.includes("SHA-256")), "the plan is on screen");
  byText(section.el, "BUTTON", "Install Hermes").fire("click");
  await flush();
  assert.deepEqual(sent("hermes_setup_start").at(-1), { brain: "anthropic", house: true, configure: true });
  assert.ok(all(section.el).some((e) => e.textContent === "Step 1 of 9: System prerequisites"), "progress follows");
  assert.ok(byText(section.el, "BUTTON", "Stop"));
  // The assistant's name is kept when it changes.
  const name = all(section.el).find((e) => e.tagName === "INPUT" && e.getAttribute("placeholder") === "Zheus");
  name.value = "  Zheus ";
  name.fire("change");
  assert.deepEqual(names, ["Zheus"]);
});

test("a Hermes already on the computer is run as it is unless asked, and a running one can be turned off", async () => {
  answers = {
    home_mcp_status: { on: false, command: "x", homeOn: true },
    hermes_setup_status: status({ installed: true }),
    hermes_setup_start: () => status({ installed: true, managed: true, brain: "google", gateway: { state: "starting" } }),
    hermes_gateway_set: (args) => status({ installed: true, managed: true, brain: "google", gateway: { state: args.on ? "running" : "off" } }),
  };
  const section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  assert.ok(!all(section.el).some((e) => e.tagName === "LI"), "no installation plan: it is there already");
  const select = all(section.el).find((e) => e.tagName === "SELECT");
  select.value = "google";
  select.fire("change");
  byText(section.el, "BUTTON", "Let Coucou run Hermes").fire("click");
  await flush();
  assert.deepEqual(sent("hermes_setup_start").at(-1), { brain: "google", house: true, configure: false }, "its own settings are left alone");
  assert.ok(all(section.el).some((e) => e.textContent?.startsWith("Starting Hermes…")));
  assert.ok(all(section.el).some((e) => e.textContent === "Its brain: Google Gemini, with the key Coucou keeps."));
  const sw = all(section.el).find((e) => e.tagName === "BUTTON" && String(e.className).includes("switch"));
  sw.fire("click");
  await flush();
  assert.deepEqual(sent("hermes_gateway_set").at(-1), { on: false });
});

test("without any key or server Coucou says what Hermes needs first", async () => {
  answers = { home_mcp_status: { on: false, command: "x", homeOn: true }, hermes_setup_status: status({ brains: BRAINS.map((b) => ({ ...b, ready: false })) }) };
  const section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  assert.ok(all(section.el).some((e) => e.textContent?.startsWith("Add an AI key first")));
  assert.ok(!byText(section.el, "BUTTON", "Install Hermes"));
});

test("Rust installs only the pinned installer and never hands Hermes a key on a command line or in a file", () => {
  const rs = readFileSync(new URL("../src-tauri/src/hermes_setup.rs", import.meta.url), "utf8");
  assert.match(rs, /pub const COMMIT: &str = "[0-9a-f]{40}";/);
  assert.match(rs, /raw\.githubusercontent\.com\/NousResearch\/hermes-agent\/\{COMMIT\}/);
  assert.match(rs, /verified\(crate::net::read_capped/, "the SHA-256 is checked before the file is written");
  assert.ok(!/irm |iex|Invoke-Expression|curl .*\| *bash/.test(rs), "never a piped script");
  assert.match(rs, /"API_SERVER_HOST"\.into\(\), "127\.0\.0\.1"/, "this computer only");
  const commands = rs.slice(rs.indexOf("pub fn config_commands"), rs.indexOf("pub fn gateway_env"));
  assert.ok(commands.length > 200 && !/secrets::|_API_KEY|api_key/.test(commands), "no secret in hermes config set");
});
