// Hermes Agent in Coucou: the Settings section that connects it and shares Mochi's home with it
// (src/settings/hermes.ts), what its steps and cards say on the island (src/core/hands.ts, src/core/home.ts), on a
// fake DOM and through the real bridge.

import { test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { internals, sent } from "./tauri.mjs";

installFakeDom();
const { hermesSection, mcpSnippet } = await import("../src/settings/hermes.ts");
const { requestTitle, requestWarning, stepVerb } = await import("../src/core/hands.ts");
const { originLabel } = await import("../src/core/home.ts");
const { DEFAULT_SETTINGS } = await import("../src/core/state.ts");

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
const toggle = (on, onChange) => {
  const el = document.createElement("button");
  el.className = on ? "switch on" : "switch";
  el.addEventListener("click", () => onChange(!el.classList.contains("on")));
  return el;
};
const HOOK = "C:\\Users\\Ana\\AppData\\Local\\AXYON\\bin\\coucou-hook.exe";

test("AXYON Core is connected with the address and key typed, and nothing is sent before the click", async () => {
  answers = {
    home_mcp_status: { on: false, command: HOOK, homeOn: true },
    hermes_connect: { url: "http://127.0.0.1:8642", model: "zheus", loopback: true },
  };
  let settings = { ...DEFAULT_SETTINGS };
  const section = hermesSection({ settings: () => settings, useInChat: async () => void (settings = { ...settings, chatProvider: "hermes" }), toggle });
  await flush();
  assert.equal(sent("hermes_connect").length, 0);
  const address = all(section.el).find((e) => e.tagName === "INPUT" && e.getAttribute("placeholder") === "http://127.0.0.1:8642");
  const key = all(section.el).find((e) => e.tagName === "INPUT" && e.getAttribute("type") === "password");
  assert.equal(address.getAttribute("placeholder"), "http://127.0.0.1:8642");
  assert.equal(key.getAttribute("type"), "password");
  address.value = "127.0.0.1:8642";
  key.value = "um-segredo-longo";
  byText(section.el, "BUTTON", "Connect").fire("click");
  await flush();
  assert.deepEqual(sent("hermes_connect").at(-1), { url: "127.0.0.1:8642", key: "um-segredo-longo" });
  assert.equal(key.value, "", "the key does not stay in the page");

  // Rust stored the address and said so (settings-changed): the section shows it connected.
  settings = { ...settings, hermesUrl: "http://127.0.0.1:8642", chatModels: { hermes: "zheus" } };
  section.redraw();
  assert.ok(byText(section.el, "SPAN", "http://127.0.0.1:8642"));
  assert.ok(all(section.el).some((e) => e.textContent?.startsWith("AXYON Core answers as zheus.")));
  byText(section.el, "BUTTON", "Use in chat").fire("click");
  await flush();
  assert.equal(settings.chatProvider, "hermes");
  section.redraw();
  assert.ok(byText(section.el, "BUTTON", "In use"));
  byText(section.el, "BUTTON", "Disconnect").fire("click");
  await flush();
  assert.equal(sent("hermes_disconnect").length, 1);
});

test("a refused connection says why, in AXYON Core's words", async () => {
  answers = { home_mcp_status: { on: false, command: HOOK, homeOn: true }, hermes_connect: new Error("AXYON Core refused the key: use the API_SERVER_KEY of its .env file.") };
  const section = hermesSection({ settings: () => ({ ...DEFAULT_SETTINGS }), useInChat: async () => {}, toggle });
  await flush();
  byText(section.el, "BUTTON", "Connect").fire("click");
  await flush();
  await flush();
  assert.ok(byText(section.el, "DIV", "AXYON Core refused the key: use the API_SERVER_KEY of its .env file."));
  assert.equal(byText(section.el, "BUTTON", "Connect").disabled, false, "one can try again");
});

test("AXYON's home is shared with AXYON Core only after the switch, with what to tell AXYON Core", async () => {
  answers = {
    home_mcp_status: { on: false, command: HOOK, homeOn: false },
    home_mcp_set: (args) => ({ on: args.on, command: HOOK, homeOn: false }),
  };
  const section = hermesSection({ settings: () => ({ ...DEFAULT_SETTINGS }), useInChat: async () => {}, toggle });
  await flush();
  assert.equal(sent("home_mcp_set").length, 0);
  assert.ok(!all(section.el).some((e) => e.textContent === '    args: ["--mcp"]'), "no snippet while it is off");
  const sw = all(section.el).find((e) => e.tagName === "BUTTON" && String(e.className).includes("switch"));
  sw.fire("click");
  await flush();
  assert.deepEqual(sent("home_mcp_set").at(-1), { on: true });
  assert.ok(all(section.el).some((e) => e.textContent === '    args: ["--mcp"]'));
  assert.ok(all(section.el).some((e) => e.textContent === `    command: '${HOOK}'`));
  assert.ok(all(section.el).some((e) => e.textContent?.startsWith("AXYON's home is off")), "the house itself is off: it says so");
});

test("the path in AXYON Core's config is quoted for YAML, whatever it holds", () => {
  assert.equal(
    mcpSnippet("C:\\Users\\D'Ávila\\coucou-hook.exe"),
    "mcp_servers:\n  coucou:\n    command: 'C:\\Users\\D''Ávila\\coucou-hook.exe'\n    args: [\"--mcp\"]",
  );
});

test("AXYON Core's steps and cards say who acts and why", () => {
  assert.equal(stepVerb("hermes"), "AXYON Core");
  const card = { id: "agent-3", run: 1, origin: "island", tool: "hermes", summary: "rm -rf build", newFile: false, outside: false, secret: false, reason: "recursive delete" };
  assert.equal(requestTitle(card), "AXYON Core asks before doing this");
  assert.equal(requestWarning(card), "recursive delete");
  // An action Hermes asks for through Mochi's home keeps Mochi's title and says who asked.
  assert.equal(requestWarning({ ...card, tool: "home_control", reason: "Asked by AXYON Core" }), "Asked by AXYON Core");
  assert.equal(originLabel("hermes"), "AXYON Core");
  assert.equal(originLabel("agent"), "Another agent");
});
