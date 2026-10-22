// Zheus: Hermes Agent installed, set up and run by Coucou (src/settings/hermes.ts, src-tauri/src/hermes_setup.rs):
// the plan shown before the click, the brains offered, what the click sends, the gateway's state in words, the ChatGPT
// sign-in, the assistant's name and tone, and the skill packs.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { installFakeDom } from "./fakedom.mjs";
import { internals, sent } from "./tauri.mjs";

installFakeDom();
const { hermesSection, zheusPlan, gatewayText, progressText, loginText, healthApiText, healthAuthText, diagnosisReport } = await import("../src/settings/hermes.ts");
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
  { id: "anthropic", name: "Anthropic", ready: true, model: "claude-opus-5", login: false },
  { id: "openai", name: "OpenAI", ready: false, model: "", login: false },
  { id: "google", name: "Google Gemini", ready: true, model: "", login: false },
  { id: "ollama", name: "Ollama", ready: false, model: "", login: false },
];
const CHATGPT = { id: "chatgpt", name: "ChatGPT", ready: true, model: "gpt-5.6-sol", login: true };
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
  assistant: { name: "Zheus", persona: "friendly", personaText: "" },
  personas: ["friendly", "direct", "professional", "playful", "butler", "custom"],
  skillPacks: ["home", "search", "news"],
  skills: [],
  house: false,
  login: { state: "unknown" },
  model: "",
  models: [],
  computerUse: false,
  speed: "",
  ...over,
});
const host = (settings) => ({ settings: () => settings, useInChat: async () => {}, toggle });
/** The brain picker: the select whose options are brains (the tone has its own). */
const brainSelect = (root) => all(root).find((e) => e.tagName === "SELECT" && e.children.some((o) => o.getAttribute("value") === "google" || o.getAttribute("value") === "chatgpt"));

test("the plan says what will be downloaded, where it goes, what key AXYON Core gets, what it writes and installs, and where it listens", () => {
  const lines = zheusPlan(status(), BRAINS[0], true);
  assert.equal(lines.length, 5);
  assert.ok(lines[0].includes("6e7b83a2f1") && lines[0].includes("SHA-256"));
  assert.ok(lines[1].includes("C:\\Users\\Ana\\AppData\\Local\\hermes"));
  assert.ok(lines[2].includes("Anthropic") && lines[2].includes("never in a file"));
  assert.ok(lines[3].includes("SOUL.md") && lines[3].includes("backed up") && lines[3].includes("Zheus"), "the personality file, and its backup");
  assert.ok(lines[4].includes("http://127.0.0.1:8642") && lines[4].includes("AXYON's home"));
  assert.ok(!zheusPlan(status(), BRAINS[0], false).at(-1).includes("AXYON's home"), "the house only when it is ticked");
  assert.ok(zheusPlan(status(), { id: "ollama", name: "Ollama", ready: true, model: "" }, false)[2].includes("Ollama on this computer"), "a local server has no key");
  // ChatGPT: a sign-in with a code on OpenAI's page, no key.
  const chatgpt = zheusPlan(status(), CHATGPT, false, [], "Athena");
  assert.ok(chatgpt[2].includes("ChatGPT") && chatgpt[2].includes("Plus or Pro") && chatgpt[2].includes("code") && !chatgpt[2].includes("key that AXYON"));
  assert.ok(chatgpt[3].includes("Athena"));
  // The skill packs ticked, by name and with the command that installs them.
  const skills = zheusPlan(status(), BRAINS[0], true, ["search", "news"]);
  assert.equal(skills.length, 6);
  assert.ok(skills[4].includes("hermes skills install") && skills[4].includes("DuckDuckGo") && skills[4].includes("RSS"));
  // A Hermes that is there already: nothing downloaded or installed.
  assert.equal(zheusPlan(status({ installed: true }), BRAINS[0], false).length, 3);
});

test("the gateway's state and the installation's progress are said in plain words", () => {
  const states = [{ state: "off" }, { state: "starting" }, { state: "running" }, { state: "elsewhere" }, { state: "stopped", reason: "port in use" }];
  const lines = states.map(gatewayText);
  assert.equal(new Set(lines).size, states.length);
  assert.ok(lines[4].includes("port in use"));
  assert.equal(progressText({ running: true, step: "Create Python environment", done: 2, total: 9, last: "", error: null, finished: false }), "Step 3 of 9: Create Python environment");
  assert.equal(progressText({ running: true, step: "Downloading AXYON Core's installer…", done: 0, total: 0, last: "", error: null, finished: false }), "Downloading AXYON Core's installer…");
});

test("nothing is installed before the click, and the click sends the brain picked and the house", async () => {
  answers = {
    home_mcp_status: { on: false, command: "x", homeOn: true },
    hermes_setup_status: status(),
    hermes_setup_start: (args) => status({ progress: { running: true, step: "System prerequisites", done: 0, total: 9, last: "git ok", error: null, finished: false }, brain: args.brain }),
  };
  const section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  assert.equal(sent("hermes_setup_start").length, 0, "not before the click");
  assert.equal(section.el.getAttribute("id"), "hermes", "the installer's page brings Settings here");
  const select = brainSelect(section.el);
  const options = select.children;
  assert.equal(options.length, BRAINS.length);
  assert.deepEqual(options.filter((o) => o.disabled).map((o) => o.getAttribute("value")), ["openai", "ollama"], "no key, no choice");
  assert.equal(select.value, "anthropic", "the first one ready");
  assert.ok(all(section.el).some((e) => e.tagName === "LI" && e.textContent.includes("SHA-256")), "the plan is on screen");
  byText(section.el, "BUTTON", "Install AXYON Core").fire("click");
  await flush();
  assert.deepEqual(sent("hermes_setup_start").at(-1), { brain: "anthropic", house: true, configure: true });
  assert.ok(all(section.el).some((e) => e.textContent === "Step 1 of 9: System prerequisites"), "progress follows");
  assert.ok(byText(section.el, "BUTTON", "Stop"));
});

test("a AXYON Core already on the computer is run as it is unless asked, and a running one can be turned off", async () => {
  answers = {
    home_mcp_status: { on: false, command: "x", homeOn: true },
    hermes_setup_status: status({ installed: true }),
    hermes_setup_start: () => status({ installed: true, managed: true, brain: "google", gateway: { state: "starting" } }),
    hermes_gateway_set: (args) => status({ installed: true, managed: true, brain: "google", gateway: { state: args.on ? "running" : "off" } }),
  };
  const section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  assert.ok(!all(section.el).some((e) => e.tagName === "LI"), "no installation plan: it is there already");
  const select = brainSelect(section.el);
  select.value = "google";
  select.fire("change");
  byText(section.el, "BUTTON", "Let AXYON run AXYON Core").fire("click");
  await flush();
  assert.deepEqual(sent("hermes_setup_start").at(-1), { brain: "google", house: true, configure: false }, "its own settings are left alone");
  assert.ok(all(section.el).some((e) => e.textContent?.startsWith("Starting AXYON Core…")));
  assert.ok(all(section.el).some((e) => e.textContent === "Its brain: Google Gemini, with the key AXYON keeps."));
  const sw = all(section.el).find((e) => e.tagName === "BUTTON" && String(e.className).includes("switch"));
  sw.fire("click");
  await flush();
  assert.deepEqual(sent("hermes_gateway_set").at(-1), { on: false });
});

test("a AXYON Core already on the computer is offered skill packs only when AXYON sets it up too", async () => {
  answers = { home_mcp_status: { on: false, command: "x", homeOn: true }, hermes_setup_status: status({ installed: true }) };
  const section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  const packs = () => all(section.el).filter((e) => e.tagName === "LABEL" && e.textContent === "Free web search (DuckDuckGo)");
  assert.equal(packs().length, 0, "run as it is: the setup installs nothing in it, so nothing is offered to install");
  const configure = all(section.el).find((e) => e.tagName === "LABEL" && e.textContent === "Also give it this brain (its model and provider change)").children[0];
  configure.checked = true;
  configure.fire("change");
  assert.equal(packs().length, 1, "set up again: the packs ticked are installed, as the plan then says");
});

test("without any key or server AXYON says what AXYON Core needs first", async () => {
  answers = { home_mcp_status: { on: false, command: "x", homeOn: true }, hermes_setup_status: status({ brains: BRAINS.map((b) => ({ ...b, ready: false })) }) };
  const section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  assert.ok(all(section.el).some((e) => e.textContent?.startsWith("Add an AI key first")));
  assert.ok(!byText(section.el, "BUTTON", "Install AXYON Core"));
});

test("Rust installs only the pinned installer and never hands AXYON Core a key on a command line or in a file", () => {
  const rs = readFileSync(new URL("../src-tauri/src/hermes_setup.rs", import.meta.url), "utf8");
  assert.match(rs, /pub const COMMIT: &str = "[0-9a-f]{40}";/);
  assert.match(rs, /raw\.githubusercontent\.com\/NousResearch\/hermes-agent\/\{COMMIT\}/);
  assert.match(rs, /verified\(crate::net::read_capped/, "the SHA-256 is checked before the file is written");
  assert.ok(!/irm |iex|Invoke-Expression|curl .*\| *bash/.test(rs), "never a piped script");
  assert.match(rs, /"API_SERVER_HOST"\.into\(\), "127\.0\.0\.1"/, "this computer only");
  const commands = rs.slice(rs.indexOf("pub fn config_commands"), rs.indexOf("pub fn gateway_env"));
  assert.ok(commands.length > 200 && !/secrets::|_API_KEY|api_key/.test(commands), "no secret in hermes config set");
});

test("the assistant's name and tone are saved together, and a tone of one's own gets a box of its own", async () => {
  answers = {
    home_mcp_status: { on: false, command: "x", homeOn: true },
    hermes_setup_status: status(),
    hermes_assistant_set: (args) => status({ assistant: { name: args.name || "Zheus", persona: args.persona, personaText: args.personaText } }),
  };
  const section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  const tone = all(section.el).find((e) => e.tagName === "SELECT" && e.children.some((o) => o.getAttribute("value") === "butler"));
  assert.deepEqual(tone.children.map((o) => o.getAttribute("value")), ["friendly", "direct", "professional", "playful", "butler", "custom"]);
  assert.deepEqual(tone.children.map((o) => o.textContent), ["Friendly", "Direct", "Professional", "Playful", "Butler (formal, witty)", "Custom"]);
  assert.equal(tone.value, "friendly", "the default tone");
  assert.ok(!all(section.el).some((e) => e.tagName === "TEXTAREA"), "no box for a preset tone");
  const name = all(section.el).find((e) => e.tagName === "INPUT" && e.getAttribute("placeholder") === "Zheus");
  assert.equal(name.value, "", "the placeholder says the default name");
  name.value = "  Athena ";
  name.fire("input");
  tone.value = "custom";
  tone.fire("change");
  const area = all(section.el).find((e) => e.tagName === "TEXTAREA");
  assert.ok(area, "a tone of one's own");
  assert.equal(area.getAttribute("maxlength"), "1500");
  area.value = "Fale como um capitão de navio.\nSempre breve.";
  area.fire("input");
  assert.equal(sent("hermes_assistant_set").length, 0, "nothing before Save");
  byText(section.el, "BUTTON", "Save").fire("click");
  await flush();
  assert.deepEqual(sent("hermes_assistant_set").at(-1), { name: "Athena", persona: "custom", personaText: "Fale como um capitão de navio.\nSempre breve." });
  assert.ok(all(section.el).some((e) => e.textContent === "Saved: AXYON Core gets it when AXYON sets it up."));
  assert.equal(name.value, "Athena");
  // Refused by Rust (two lines, say): the reason shows, nothing else changes.
  answers.hermes_assistant_set = new Error("The name must be one line of at most 40 characters.");
  byText(section.el, "BUTTON", "Save").fire("click");
  await flush();
  assert.ok(byText(section.el, "DIV", "The name must be one line of at most 40 characters."));
});

test("a setup with ChatGPT shows its sign-in code large, and Cancel stops the sign-in", async () => {
  const waiting = { state: "waiting", code: "WXYZ-9876", url: "https://auth.openai.com/codex/device", expiresAt: 1_900_000_000_000 };
  answers = {
    home_mcp_status: { on: false, command: "x", homeOn: true },
    hermes_setup_status: status({ brains: [CHATGPT, ...BRAINS] }),
    hermes_setup_start: (args) => status({ brains: [CHATGPT, ...BRAINS], brain: args.brain, progress: { running: true, step: "Sign in with ChatGPT: open the page and type the code WXYZ-9876", done: 0, total: 0, last: "", error: null, finished: false }, login: waiting }),
    hermes_login_cancel: () => status({ brains: [CHATGPT, ...BRAINS], progress: { running: false, step: "", done: 0, total: 0, last: "", error: "Stopped.", finished: false }, login: { state: "signedOut" } }),
  };
  const section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  const select = brainSelect(section.el);
  assert.equal(select.value, "chatgpt", "ChatGPT first");
  assert.equal(select.children[0].textContent, "ChatGPT (your subscription)");
  assert.ok(all(section.el).some((e) => e.textContent?.startsWith("Needs ChatGPT Plus or Pro.")), "the plan it needs");
  assert.ok(all(section.el).some((e) => e.tagName === "LI" && e.textContent.includes("Sign in with your ChatGPT account")), "the plan says it");
  byText(section.el, "BUTTON", "Install AXYON Core").fire("click");
  await flush();
  assert.deepEqual(sent("hermes_setup_start").at(-1), { brain: "chatgpt", house: true, configure: true });
  assert.ok(all(section.el).some((e) => e.textContent === "Sign in with ChatGPT: open the page and type the code WXYZ-9876"));
  assert.ok(byText(section.el, "DIV", "WXYZ-9876"), "the code, large");
  byText(section.el, "BUTTON", "Open the page").fire("click");
  await flush();
  assert.deepEqual(sent("open_url").at(-1), { url: "https://auth.openai.com/codex/device" });
  assert.ok(byText(section.el, "BUTTON", "Copy code"));
  byText(section.el, "BUTTON", "Cancel").fire("click");
  await flush();
  assert.equal(sent("hermes_login_cancel").length, 1);
  assert.ok(byText(section.el, "DIV", "Stopped."));
});

test("a AXYON Core AXYON runs on ChatGPT signs in and out with a click, one sign-in at a time", async () => {
  const managed = (login) => status({ installed: true, managed: true, brain: "chatgpt", brains: [CHATGPT, ...BRAINS], gateway: { state: "running" }, login });
  answers = {
    home_mcp_status: { on: false, command: "x", homeOn: true },
    hermes_setup_status: managed({ state: "signedOut" }),
    hermes_login_start: () => managed({ state: "waiting", code: "ABCD-1234", url: "https://auth.openai.com/codex/device", expiresAt: 1 }),
  };
  const before = sent("hermes_login_start").length;
  let section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  assert.ok(all(section.el).some((e) => e.textContent === "Its brain: ChatGPT, with your subscription."));
  assert.ok(all(section.el).some((e) => e.textContent?.startsWith("Not signed in to ChatGPT")));
  assert.equal(sent("hermes_login_start").length, before, "not before the click");
  byText(section.el, "BUTTON", "Sign in with ChatGPT").fire("click");
  assert.ok(byText(section.el, "BUTTON", "Asking OpenAI for a sign-in code…").disabled, "one sign-in at a time");
  await flush();
  assert.equal(sent("hermes_login_start").length, before + 1);
  assert.ok(byText(section.el, "DIV", "ABCD-1234"));

  answers.hermes_setup_status = managed({ state: "signedIn" });
  answers.hermes_logout = () => managed({ state: "signedOut" });
  section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  assert.ok(byText(section.el, "SPAN", "Signed in with your ChatGPT account."));
  byText(section.el, "BUTTON", "Sign out").fire("click");
  await flush();
  assert.ok(sent("hermes_logout").length >= 1);
  assert.ok(byText(section.el, "BUTTON", "Sign in with ChatGPT"));
  const reasons = [{ state: "unknown" }, { state: "signedOut" }, { state: "signedIn" }, { state: "waiting", code: "A", url: "u", expiresAt: 1 }, { state: "failed", reason: "rate limited" }].map(loginText);
  assert.equal(new Set(reasons).size, 5);
  assert.ok(reasons[4].includes("rate limited"));
});

test("a brain can be changed on a AXYON Core AXYON runs, after the plan of what changes", async () => {
  answers = {
    home_mcp_status: { on: false, command: "x", homeOn: true },
    hermes_setup_status: status({ installed: true, managed: true, brain: "anthropic", brains: [CHATGPT, ...BRAINS], house: true, gateway: { state: "running" } }),
    hermes_setup_start: () => status({ installed: true, managed: true, brain: "chatgpt", brains: [CHATGPT, ...BRAINS], gateway: { state: "starting" } }),
  };
  const section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  const apply = byText(section.el, "BUTTON", "Give it this brain");
  assert.ok(apply.disabled, "the brain it has already");
  const select = brainSelect(section.el);
  assert.equal(select.value, "anthropic");
  select.value = "chatgpt";
  select.fire("change");
  assert.ok(all(section.el).some((e) => e.tagName === "LI" && e.textContent.includes("Sign in with your ChatGPT account")), "what changes, before the click");
  const go = byText(section.el, "BUTTON", "Give it this brain");
  assert.ok(!go.disabled);
  go.fire("click");
  await flush();
  assert.deepEqual(sent("hermes_setup_start").at(-1), { brain: "chatgpt", house: true, configure: true });
});

test("on ChatGPT the model is picked from the subscription's list or typed, and the computer is one switch", async () => {
  const MODELS = ["gpt-6-sol", "gpt-5.6-sol", "gpt-5.5"];
  const running = (over) => status({ installed: true, managed: true, brain: "chatgpt", brains: [CHATGPT, ...BRAINS], house: true, gateway: { state: "running" }, login: { state: "signedIn" }, model: "gpt-5.6-sol", models: MODELS, ...over });
  answers = {
    home_mcp_status: { on: false, command: "x", homeOn: true },
    hermes_setup_status: running(),
    hermes_model_set: (args) => running({ model: args.model }),
    hermes_computer_use_set: (args) => running({ computerUse: args.on }),
    hermes_speed_set: (args) => running({ speed: args.speed }),
  };
  const section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  const modelSelect = () => all(section.el).find((e) => e.tagName === "SELECT" && e.children.some((o) => o.getAttribute("value") === "gpt-6-sol"));
  assert.equal(modelSelect().value, "gpt-5.6-sol", "the model it runs on");
  assert.ok(byText(section.el, "BUTTON", "Use this model").disabled, "nothing to change yet");
  modelSelect().value = "gpt-6-sol";
  modelSelect().fire("change");
  byText(section.el, "BUTTON", "Use this model").fire("click");
  await flush();
  assert.deepEqual(sent("hermes_model_set").at(-1), { model: "gpt-6-sol" });
  // A model of one's own, typed.
  modelSelect().value = "__other";
  modelSelect().fire("change");
  const typed = all(section.el).find((e) => e.tagName === "INPUT" && e.getAttribute("placeholder") === "gpt-…");
  typed.value = "gpt-7-preview";
  typed.fire("input");
  byText(section.el, "BUTTON", "Use this model").fire("click");
  await flush();
  assert.deepEqual(sent("hermes_model_set").at(-1), { model: "gpt-7-preview" });
  // Hermes may use the computer: one switch, off until it is turned on.
  const row = all(section.el).find((e) => e.className === "row" && e.children.some((c) => c.textContent === "Let it use this computer (screen, mouse, keyboard)"));
  const sw = row.children.find((c) => c.classList.contains("switch"));
  assert.ok(!sw.classList.contains("on"));
  sw.fire("click");
  await flush();
  assert.deepEqual(sent("hermes_computer_use_set").at(-1), { on: true });
  // How quick it is: Hermes' own until another is picked, then given with each message.
  const speedSelect = () => all(section.el).find((e) => e.tagName === "SELECT" && e.children.some((o) => o.getAttribute("value") === "fast"));
  assert.equal(speedSelect().value, "", "AXYON Core's own");
  assert.deepEqual(speedSelect().children.map((o) => o.getAttribute("value")), ["fast", "", "deep"]);
  speedSelect().value = "fast";
  speedSelect().fire("change");
  await flush();
  assert.deepEqual(sent("hermes_speed_set").at(-1), { speed: "fast" });
  assert.equal(speedSelect().value, "fast", "redrawn from Rust's answer");
  // Another brain offers no list: its model is the one of its provider.
  answers.hermes_setup_status = running({ brain: "anthropic", models: [] });
  const other = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  assert.ok(!byText(other.el, "BUTTON", "Use this model"));
});

test("skill packs are ticked before the setup, or installed at once in a AXYON Core AXYON runs", async () => {
  answers = {
    home_mcp_status: { on: false, command: "x", homeOn: true },
    hermes_setup_status: status(),
    hermes_skills_set: (args) => status({ skills: args.skills }),
  };
  let section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  assert.ok(all(section.el).some((e) => e.textContent?.startsWith("Already included with AXYON Core:")));
  const tick = (label) => all(section.el).find((e) => e.tagName === "LABEL" && e.textContent === label).children[0];
  assert.equal(tick("Free web search (DuckDuckGo)").checked, false);
  tick("News and feeds (RSS)").checked = true;
  tick("News and feeds (RSS)").fire("change");
  await flush();
  assert.deepEqual(sent("hermes_skills_set").at(-1), { skills: ["news"] });
  assert.ok(all(section.el).some((e) => e.tagName === "LI" && e.textContent.includes("hermes skills install") && e.textContent.includes("RSS")), "the plan says what it installs");
  assert.ok(!all(section.el).some((e) => e.tagName === "LI" && e.textContent.includes("DuckDuckGo")));

  answers.hermes_setup_status = status({ installed: true, managed: true, brain: "google", skills: ["search", "news"], gateway: { state: "running" } });
  section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  assert.ok(all(section.el).some((e) => e.textContent?.startsWith("Skill packs: one you tick is installed in AXYON Core at once")));
  assert.equal(tick("Free web search (DuckDuckGo)").checked, true);
  tick("Free web search (DuckDuckGo)").checked = false;
  tick("Free web search (DuckDuckGo)").fire("change");
  await flush();
  assert.deepEqual(sent("hermes_skills_set").at(-1), { skills: ["news"] });
});

test("Rust signs in, installs skills and writes SOUL.md with fixed commands, and reads no key for the installer's page", () => {
  const rs = readFileSync(new URL("../src-tauri/src/hermes_setup.rs", import.meta.url), "utf8");
  assert.match(rs, /\["auth", action, "openai-codex"\]/, "hermes auth add|status|logout openai-codex, nothing else");
  assert.match(rs, /"official\/research\/duckduckgo-search", "duckduckgo-search"/);
  assert.match(rs, /"official\/research\/rss-feeds", "rss-feeds"/);
  assert.match(rs, /&\["skills", "install", identifier, "--yes"\]/);
  assert.match(rs, /&\["skills", "uninstall", name, "--yes"\]/);
  assert.match(rs, /url\.host_str\(\) == Some\("auth\.openai\.com"\)/, "only OpenAI's own page is opened or shown");
  assert.ok(!/auth\.json"/.test(rs.slice(0, rs.indexOf("#[cfg(test)]"))), "AXYON Core's sign-in file is never read");
  const first = readFileSync(new URL("../src-tauri/src/first_run.rs", import.meta.url), "utf8");
  assert.match(first, /remove_file\(&path\)/, "setup.json is deleted once read");
  assert.ok(!/secrets::/.test(first), "the installer's page never hands AXYON a key through the file");
});

test("Settings comes to AXYON Core after the installer's page, and Rust keeps the assistant's settings its own", () => {
  const main = readFileSync(new URL("../src/settings/main.ts", import.meta.url), "utf8");
  assert.match(main, /onEvent<string>\("settings-focus", focusSection\)/, "the event first_run.rs sends");
  assert.match(main, /if \(boot\?\.focus\) focusSection\(boot\.focus\)/, "or the boot's, when the page was not up yet");
  assert.match(main, /getElementById\("hermes"\)\?\.scrollIntoView/);
  assert.ok(!/assistantName = /.test(main), "the name is saved by hermes_assistant_set only");
  const lib = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  for (const field of ["assistant_name", "assistant_persona", "assistant_persona_text", "hermes_skills", "hermes_skills_installed", "hermes_computer_use", "hermes_browser_off", "hermes_speed", "phone_assistant", "phone_home"]) {
    assert.match(lib, new RegExp(`settings\\.${field} = current\\.${field}`), `${field} is Rust's`);
  }
  for (const command of ["hermes_assistant_set", "hermes_skills_set", "hermes_login_start", "hermes_login_cancel", "hermes_logout", "hermes_model_set", "hermes_computer_use_set", "hermes_speed_set", "hermes_diagnose", "hermes_repair_key", "hermes_detach_service"]) {
    assert.ok(lib.includes(`hermes_setup::${command},`), `${command} is registered`);
  }
});


test("the diagnosis panel checks the integration, repairs the key, and gives a report without secrets", async () => {
  const running = (over) => status({ installed: true, managed: true, brain: "chatgpt", brains: [CHATGPT, ...BRAINS], gateway: { state: "running" }, login: { state: "signedIn" }, model: "gpt-5.6-sol", models: ["gpt-5.6-sol"], ...over });
  answers = {
    home_mcp_status: { on: false, command: "x", homeOn: true },
    hermes_setup_status: running(),
    hermes_diagnose: { installed: true, managed: true, health: { state: "unauthorized" }, hasKey: true, ownService: true, address: "http://127.0.0.1:8642" },
    hermes_repair_key: { installed: true, managed: true, health: { state: "ready", model: "zheus" }, hasKey: true, ownService: false, address: "http://127.0.0.1:8642" },
    hermes_detach_service: { installed: true, managed: true, health: { state: "ready", model: "zheus" }, hasKey: true, ownService: false, address: "http://127.0.0.1:8642" },
  };
  const section = hermesSection(host({ ...DEFAULT_SETTINGS }));
  await flush();
  byText(section.el, "BUTTON", "Check the integration").fire("click");
  await flush();
  await flush();
  assert.equal(sent("hermes_diagnose").length, 1);
  // A wrong key: the auth line says so and Repair + Let only Coucou run it + Copy are offered.
  assert.ok(all(section.el).some((e) => e.textContent === "The key is wrong or missing. Repair it below, or paste the API_SERVER_KEY of AXYON Core's .env."));
  assert.ok(byText(section.el, "BUTTON", "Repair the key"));
  assert.ok(byText(section.el, "BUTTON", "Let only AXYON run AXYON Core"));
  byText(section.el, "BUTTON", "Repair the key").fire("click");
  await flush();
  await flush();
  assert.equal(sent("hermes_repair_key").length, 1);
  assert.ok(all(section.el).some((e) => e.textContent === "The key is valid: AXYON Core answers as zheus."), "after repair the key is valid");
  assert.equal(all(section.el).find((e) => e.tagName === "BUTTON" && e.textContent === "Let only AXYON run AXYON Core"), undefined, "no service fight any more");

  // The health lines read right, and the report carries no key.
  assert.equal(healthApiText({ state: "down" }), "The API does not answer: the gateway is off, or nothing listens on the port yet.");
  assert.equal(healthAuthText({ state: "ready", model: "zheus" }), "The key is valid: AXYON Core answers as zheus.");
  const report = diagnosisReport({ installed: true, managed: true, health: { state: "ready", model: "zheus" }, hasKey: true, ownService: false, address: "http://127.0.0.1:8642" });
  assert.ok(report.includes("127.0.0.1:8642") && !/key=|API_SERVER_KEY=\S/.test(report), "the report has the address but no secret");
});
