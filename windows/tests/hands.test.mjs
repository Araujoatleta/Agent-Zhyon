// Mochi's hands on the island: the words of a step and of a card (src/core/hands.ts), the chat view that draws
// them (src/views/chat.ts) and the listeners that bring them from Rust (src/island/hands.ts), on a fake DOM and
// through the real bridge.

import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { calls, emit, internals, sent } from "./tauri.mjs";

installFakeDom();
const { changePreview, requestTitle, requestWarning, stepDetail, stepVerb, upsertStep } = await import("../src/core/hands.ts");
const { buildPrompt } = await import("../src/views/chat.ts");
const { registerHands } = await import("../src/island/hands.ts");
const { DEFAULT_SETTINGS, State } = await import("../src/core/state.ts");

/** What the mocked Rust side answers, by command. */
let answers = {};
const plainInvoke = internals.invoke;
internals.invoke = async (cmd, args) => {
  const result = await plainInvoke(cmd, args);
  if (cmd in answers) return typeof answers[cmd] === "function" ? answers[cmd](args) : answers[cmd];
  return result;
};

// The card ignores clicks for a moment after it shows up (a click meant for something else); the tests move the
// clock past it.
const realNow = performance.now.bind(performance);
let skew = 0;
performance.now = () => realNow() + skew;

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
const step = (fields) => ({ index: 0, tool: "run_command", summary: "npm test", status: "ok", ...fields });
const request = (fields) => ({
  id: "agent-1-1",
  run: 1,
  origin: "island",
  tool: "run_command",
  summary: "npm test",
  newFile: false,
  outside: false,
  secret: false,
  ...fields,
});

// ── The words ────────────────────────────────────────────────────────────────

test("a step row says what Mochi did, with its count in words", () => {
  assert.equal(stepVerb("run_command"), "Runs");
  assert.equal(stepVerb("read_file"), "Reads");
  assert.equal(stepVerb("system_info"), "Looks at this computer");
  assert.equal(stepVerb("a_tool_from_later"), "a_tool_from_later");
  assert.equal(stepDetail(step({ tool: "read_file", detail: "1" })), "1 line");
  assert.equal(stepDetail(step({ tool: "write_file", detail: "120" })), "120 lines");
  assert.equal(stepDetail(step({ tool: "list_directory", detail: "2" })), "2 items");
  assert.equal(stepDetail(step({ tool: "find_files", detail: "3" })), "3 files");
  assert.equal(stepDetail(step({ tool: "search_text", detail: "1" })), "1 match");
  assert.equal(stepDetail(step({ detail: "exit 0" })), "exit 0");
  assert.equal(stepDetail(step({ detail: "timeout" })), "time limit");
  assert.equal(stepDetail(step({ detail: "stopped" })), "stopped");
  assert.equal(stepDetail(step({ tool: "edit_file", detail: "+3 −1" })), "+3 −1");
  assert.equal(stepDetail(step({})), "");
});

test("a card says what Mochi wants to do, and why a look needs a click", () => {
  assert.equal(requestTitle(request({})), "Mochi wants to run a command");
  assert.equal(requestTitle(request({ tool: "write_file", after: "hi\n", newFile: true })), "Mochi wants to create a file");
  // Too large to show, still a new file.
  assert.equal(requestTitle(request({ tool: "write_file", newFile: true })), "Mochi wants to create a file");
  assert.equal(requestTitle(request({ tool: "write_file", before: "old\n", after: "new\n" })), "Mochi wants to replace a file");
  assert.equal(requestTitle(request({ tool: "edit_file" })), "Mochi wants to edit a file");
  assert.equal(requestTitle(request({ tool: "fetch_url" })), "Mochi wants to fetch a web page");
  assert.equal(requestTitle(request({ tool: "open_url" })), "Mochi wants to open a web page");
  assert.equal(requestTitle(request({ tool: "read_file", secret: true })), "Mochi wants to read a file that may hold secrets");
  assert.equal(requestTitle(request({ tool: "list_directory", outside: true })), "Mochi wants to look outside his folder");
  assert.equal(requestWarning(request({ secret: true, outside: true })), "This file looks like it holds keys or passwords.");
  assert.equal(requestWarning(request({ outside: true })), "Outside the folder Mochi works in.");
  assert.equal(requestWarning(request({})), null);
});

test("a file change shows its changed lines, at most a few, and counts the rest", () => {
  const before = Array.from({ length: 40 }, (_, i) => `line ${i}`).join("\n") + "\n";
  const after = before.replace("line 3\n", "line three\n").replace("line 30\n", "line thirty\n");
  const preview = changePreview(request({ tool: "edit_file", summary: "notes.txt", before, after }), 6);
  assert.equal(preview.added, 2);
  assert.equal(preview.removed, 2);
  assert.equal(preview.lines.length, 6);
  assert.ok(preview.more > 0);
  assert.ok(preview.lines.some((l) => l.kind === "removed" && l.text === "line 3"));
  // A new file is all added; nothing to show without its text.
  const created = changePreview(request({ tool: "write_file", after: "a\nb\n", newFile: true }));
  assert.deepEqual([created.added, created.removed, created.more], [2, 0, 0]);
  assert.equal(changePreview(request({ tool: "write_file", newFile: true })), null);
  // Two changes far apart are two hunks, with a gap between them.
  const both = changePreview(request({ tool: "edit_file", before, after }), 40);
  assert.equal(both.lines.filter((l) => l.kind === "gap").length, 1);
});

test("a step's later states replace its row, in step order", () => {
  let steps = upsertStep([], step({ index: 1, status: "running" }));
  steps = upsertStep(steps, step({ index: 0, status: "ok" }));
  steps = upsertStep(steps, step({ index: 1, status: "error", detail: "exit 1" }));
  assert.deepEqual(steps.map((s) => [s.index, s.status]), [[0, "ok"], [1, "error"]]);
});

// ── The chat view ────────────────────────────────────────────────────────────

let view;
beforeEach(() => {
  calls.length = 0;
  answers = {};
  skew = 0;
  State.settings = { ...DEFAULT_SETTINGS, chatModels: {} };
  State.chatHistory = [];
  State.stateOverride = null;
  State.handsRequest = null;
  State.handsRun = null;
  State.pendingApproval = null;
  State.view = "prompt";
  State.droppedFile = null;
  view = buildPrompt(() => {});
  view.sync();
});

const $ = (cls) => view.el.querySelector(cls);

test("the hands toggle only shows while the hands are on in Settings, and the chat asks for them", async () => {
  assert.equal($(".hands-btn").hidden, true);
  State.settings = { ...State.settings, agentEnabled: true };
  view.sync();
  assert.equal($(".hands-btn").hidden, false);
  assert.ok($(".hands-btn").classList.contains("on"));

  answers.chat_send = { text: "Done." };
  $(".chat-input").value = "list my files";
  $(".send-btn").fire("click");
  await flush();
  assert.deepEqual(sent("chat_send").at(-1), { query: "list my files", context: null, hands: true });

  // Off for this chat: saved, and the next turn is a plain chat turn.
  $(".hands-btn").fire("click");
  assert.equal(State.settings.chatHands, false);
  assert.equal(sent("save_settings").at(-1).settings.chatHands, false);
  assert.ok(!$(".hands-btn").classList.contains("on"));
  $(".chat-input").value = "hello";
  $(".send-btn").fire("click");
  await flush();
  assert.equal(sent("chat_send").at(-1).hands, false);
});

test("while a turn with hands runs, the send button stops it", async () => {
  State.settings = { ...State.settings, agentEnabled: true };
  view.sync();
  let finish;
  answers.chat_send = () => new Promise((resolve) => (finish = resolve));
  $(".chat-input").value = "run the tests";
  $(".send-btn").fire("click");
  await flush();
  assert.ok($(".send-btn").classList.contains("stop"));
  assert.equal($(".hands-btn").disabled, true);
  $(".send-btn").fire("click");
  assert.equal(sent("agent_cancel").length, 1);
  assert.equal(sent("chat_send").length, 1);
  finish({ text: "Stopped." });
  await flush();
  assert.ok(!$(".send-btn").classList.contains("stop"));
});

test("the steps of a turn are rows in the chat, and a command's output opens on a click", () => {
  State.chatHistory.push({ id: 1, role: "user", content: "run the tests" });
  State.chatHistory.push({ id: 1_000_001, role: "action", content: "", action: step({ status: "error", detail: "exit 1", output: "1 failing" }) });
  view.sync();
  const row = $(".hands-step");
  assert.ok(row.classList.contains("error"));
  assert.equal($(".hands-verb").textContent, "Runs");
  assert.equal($(".hands-what").textContent, "npm test");
  assert.equal($(".hands-detail").textContent, "exit 1");
  assert.equal($(".hands-output").hidden, true);
  row.fire("click");
  assert.equal($(".hands-output").hidden, false);
  assert.equal($(".hands-output").textContent, "1 failing");
});

test("an action waiting for a click is a card: Allow and Deny answer it, never a click too soon", () => {
  State.handsRequest = request({ tool: "edit_file", summary: "notes.txt", before: "a\nb\n", after: "a\nc\n", origin: "phone", cwd: undefined });
  view.sync();
  assert.equal($(".hands-card-title").textContent, "Mochi wants to edit a file");
  assert.ok(view.el.find(".hands-note").some((n) => n.textContent === "Asked from your phone"));
  assert.ok($(".hands-diff"));
  const [deny, allow] = $(".hands-card").querySelector(".actions").children;
  allow.fire("click");
  assert.deepEqual(sent("agent_decide"), []); // too soon after it showed up
  skew = 1000;
  allow.fire("click");
  assert.deepEqual(sent("agent_decide"), [{ id: "agent-1-1", allow: true }]);
  deny.fire("click");
  assert.deepEqual(sent("agent_decide").at(-1), { id: "agent-1-1", allow: false });
});

test("a command's card shows the folder it runs in and warns about a path outside it", () => {
  State.handsRequest = request({ cwd: "C:\\Users\\me\\work", outside: true });
  view.sync();
  assert.ok(view.el.find(".hands-note").some((n) => n.textContent === "in C:\\Users\\me\\work"));
  assert.equal($(".hands-warn").textContent, "Outside the folder Mochi works in.");
  assert.equal($(".hands-diff"), null);
});

// ── From Rust to the island ──────────────────────────────────────────────────

/** An island that records what it was asked to do. Registered once, as the app does. */
const island = {
  did: [],
  alert: (view) => island.did.push(["alert", view]),
  setPinned: (on) => island.did.push(["pinned", on]),
  dropPin: () => island.did.push(["dropPin"]),
};
registerHands(island);
await flush();
beforeEach(() => {
  island.did = [];
  State.isPinned = false;
});

test("the island's turn shows its steps; the phone's stay on the phone", () => {
  emit("agent-run", { run: 7, origin: "island", running: true });
  assert.equal(State.handsRun, 7);
  emit("agent-step", { run: 7, origin: "island", step: step({ status: "running" }) });
  assert.equal(State.stateOverride, "working");
  emit("agent-step", { run: 7, origin: "island", step: step({ status: "ok", detail: "exit 0" }) });
  emit("agent-step", { run: 9, origin: "phone", step: step({ index: 1 }) });
  const rows = State.chatHistory.filter((m) => m.role === "action");
  assert.equal(rows.length, 1);
  assert.equal(rows[0].action.status, "ok");
  assert.equal(State.stateOverride, "thinking");
  emit("agent-run", { run: 7, origin: "island", running: false });
  assert.equal(State.handsRun, null);
});

test("an action waiting for a click opens the chat and keeps it open until it is answered", () => {
  State.view = "overview";
  State.mode = "compact";
  emit("agent-approval", request({ origin: "phone" }));
  assert.equal(State.handsRequest.id, "agent-1-1");
  assert.deepEqual(island.did, [["alert", "prompt"]]);
  assert.equal(State.isPinned, true);
  assert.equal(State.defaultView(), "prompt");
  // Someone else's answer (another card) changes nothing.
  emit("agent-approval-resolved", { id: "agent-1-2", allowed: true });
  assert.ok(State.handsRequest);
  emit("agent-approval-resolved", { id: "agent-1-1", allowed: false });
  assert.equal(State.handsRequest, null);
  assert.equal(State.isPinned, false);
  assert.deepEqual(island.did.at(-1), ["dropPin"]);
});

test("a Claude Code card on screen keeps its place; Mochi's waits behind it", () => {
  State.pendingApproval = { requestId: "cc-1" };
  emit("agent-approval", request({}));
  assert.ok(State.handsRequest);
  assert.deepEqual(island.did, []);
  emit("agent-approval-resolved", { id: "agent-1-1", allowed: true });
  assert.deepEqual(island.did, []); // the Claude Code card still holds the island
  State.pendingApproval = null;
});
