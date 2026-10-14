// Settings → Phone: the switches that let the phone change the assistant and use Mochi's home (src/settings/phone.ts),
// drawn on a fake DOM and through the real bridge; and the Rust side they call (src-tauri/src/phone_link).

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { installFakeDom } from "./fakedom.mjs";
import { internals, sent } from "./tauri.mjs";

installFakeDom();
const { assistantBlock, homeBlock } = await import("../src/settings/phone.ts");

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

const toggle = (on, onChange) => {
  const el = document.createElement("button");
  el.className = on ? "switch on" : "switch";
  el.addEventListener("click", () => onChange(!el.classList.contains("on")));
  return el;
};
const switchOf = (block) => [...block.walk()].find((e) => e.tagName === "BUTTON");
const texts = (block) => [block, ...block.walk()].map((e) => e.textContent ?? "");

const blocks = [
  { name: "assistant", make: assistantBlock, status: "phone_assistant_status", set: "phone_assistant_set_enabled", label: "Let the phone change the assistant (name, tone, brain, skills)" },
  { name: "home", make: homeBlock, status: "phone_home_status", set: "phone_home_set_enabled", label: "Let the phone use Mochi's home" },
];

for (const b of blocks) {
  test(`${b.name}: the switch shows what Rust says, and a click asks Rust to change it`, async () => {
    answers = { [b.status]: { enabled: true }, [b.set]: (args) => ({ enabled: args.enabled }) };
    const block = b.make(toggle);
    assert.ok(texts(block).includes(b.label));
    const el = switchOf(block);
    assert.equal(el.classList.contains("on"), false, "off until Rust has said");
    await flush();
    assert.equal(el.classList.contains("on"), true);
    assert.equal(el.getAttribute("aria-pressed"), "true");
    el.classList.remove("on");
    el.fire("click");
    await flush();
    assert.deepEqual(sent(b.set).at(-1), { enabled: true });
  });

  test(`${b.name}: a refusal puts the switch back off and says why`, async () => {
    answers = { [b.status]: { enabled: false }, [b.set]: new Error("only the settings window may change what the phone can use") };
    const block = b.make(toggle);
    await flush();
    const el = switchOf(block);
    el.classList.add("on");
    el.fire("click");
    await flush();
    await flush();
    assert.equal(el.classList.contains("on"), false);
    assert.equal(el.getAttribute("aria-pressed"), "false");
    assert.ok(texts(block).some((t) => t.includes("only the settings window")), "the reason is shown");
  });
}

test("the hints say what the phone may do and what never leaves the computer", () => {
  answers = {};
  const assistant = texts(assistantBlock(toggle)).join(" ");
  assert.match(assistant, /Face ID/);
  assert.match(assistant, /never installed from the phone/);
  const home = texts(homeBlock(toggle)).join(" ");
  assert.match(home, /needs Mochi's home on/);
  assert.match(home, /Face ID/);
  assert.match(home, /never leave this computer/);
});

test("Rust: both switches are the settings window's to change, reconnect the phones, and are registered", () => {
  const mod = readFileSync(new URL("../src-tauri/src/phone_link/mod.rs", import.meta.url), "utf8").replace(/\r\n/g, "\n");
  for (const name of ["assistant", "home"]) {
    const start = mod.indexOf(`pub fn phone_${name}_set_enabled`);
    const body = mod.slice(start, mod.indexOf("\n}\n", start));
    assert.ok(start > 0, `phone_${name}_set_enabled exists`);
    assert.match(body, /only_settings\(&window\)\?/);
    assert.match(body, /kick_all\("closed"/, "the phones reconnect and learn about it");
    assert.match(body, new RegExp(`current\\.phone_${name} = enabled`));
  }
  const lib = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  for (const cmd of ["phone_assistant_status", "phone_assistant_set_enabled", "phone_home_status", "phone_home_set_enabled"]) {
    assert.ok(lib.includes(`phone_link::${cmd},`), `${cmd} is registered`);
  }
  // Mochi's home from the phone needs Mochi's home itself.
  assert.match(mod, /s\.home_enabled && s\.phone_home/);
  // The two settings are Rust's own: a page saving its settings never turns them on.
  assert.match(lib, /settings\.phone_assistant = current\.phone_assistant;/);
  assert.match(lib, /settings\.phone_home = current\.phone_home;/);
});
