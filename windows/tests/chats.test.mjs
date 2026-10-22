// The island's kept conversations (src/core/chats.ts, the panel in src/views/chat.ts, + in the header): after a
// restart the chat goes on where it was, + starts a new chat and keeps the old one, the list opens an old one again,
// and erasing asks twice. Through the real bridge, against a mocked Rust side.

import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { calls, internals, sent } from "./tauri.mjs";

installFakeDom();
const { buildPrompt } = await import("../src/views/chat.ts");
const { buildHeader } = await import("../src/views/views.ts");
const { canSwitch, newChat, resumeChat } = await import("../src/core/chats.ts");
const { DEFAULT_SETTINGS, State } = await import("../src/core/state.ts");

let answers;
const plainInvoke = internals.invoke;
internals.invoke = async (cmd, args) => {
  const result = await plainInvoke(cmd, args);
  if (cmd in answers) return typeof answers[cmd] === "function" ? answers[cmd](args) : answers[cmd];
  return result;
};

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const now = Math.floor(Date.now() / 1000);
const KEPT = {
  current: "c-1",
  messages: [
    { role: "user", content: "Onde aporto este mês?" },
    { role: "assistant", content: "Em GGRC11, que está abaixo do teto." },
  ],
  chats: [
    { id: "c-1", title: "Onde aporto este mês?", updated: now, turns: 1 },
    { id: "c-2", title: "E a Selic?", updated: now - 86400 * 3, turns: 2 },
  ],
};
const OTHER = {
  current: "c-2",
  messages: [
    { role: "user", content: "E a Selic?" },
    { role: "assistant", content: "Em 10,75%." },
  ],
  chats: KEPT.chats,
};

let view;
beforeEach(() => {
  calls.length = 0;
  answers = {};
  State.settings = { ...DEFAULT_SETTINGS, chatModels: {} };
  State.chatHistory = [];
  State.chatShelf = null;
  State.chatSending = false;
  State.stateOverride = null;
  State.view = "prompt";
  State.droppedFile = null;
  view = buildPrompt(() => {});
  view.sync();
});

const $ = (cls) => view.el.querySelector(cls);
const bubbles = () => view.el.find(".chat-row").map((r) => r.textContent);

test("after a restart the chat goes on with the conversation it was in", async () => {
  answers.chat_shelf = KEPT;
  await resumeChat();
  view.sync();
  assert.deepEqual(State.chatHistory.map((m) => [m.role, m.content]), [
    ["user", "Onde aporto este mês?"],
    ["assistant", "Em GGRC11, que está abaixo do teto."],
  ]);
  assert.equal(bubbles().length, 2);
  assert.equal(State.chatShelf.current, "c-1");
});

test("a conversation begun before the kept one came back is not replaced", async () => {
  State.chatHistory = [{ id: 1, role: "user", content: "já comecei" }];
  answers.chat_shelf = KEPT;
  await resumeChat();
  assert.equal(State.chatHistory.length, 1);
  assert.equal(State.chatShelf.chats.length, 2, "the list is there all the same");
});

test("the list opens an old conversation, which takes the chat's place", async () => {
  answers.chat_shelf = KEPT;
  answers.chat_open = OTHER;
  await resumeChat();
  $(".shelf-btn").fire("click");
  await flush();
  assert.ok($(".chat-body").classList.contains("shelving"));
  const rows = view.el.find(".shelf-chat");
  assert.deepEqual(rows.map((r) => r.querySelector(".picker-model-name").textContent), ["Onde aporto este mês?", "E a Selic?"]);
  assert.ok(rows[0].classList.contains("on"), "the one on the island");
  rows[1].fire("click");
  await flush();
  assert.deepEqual(sent("chat_open"), [{ id: "c-2" }]);
  assert.equal(State.chatHistory[0].content, "E a Selic?");
  assert.ok(!$(".chat-body").classList.contains("shelving"), "back to the chat");
  view.sync();
  assert.match(bubbles().join("|"), /10,75%/);
});

test("erasing a conversation asks twice", async () => {
  answers.chat_shelf = KEPT;
  answers.chat_forget = { ...KEPT, chats: [KEPT.chats[0]] };
  await resumeChat();
  $(".shelf-btn").fire("click");
  await flush();
  const erase = () => view.el.find(".shelf-chat")[1].querySelector(".shelf-erase");
  erase().fire("click");
  assert.deepEqual(sent("chat_forget"), [], "a first click only asks");
  assert.equal(view.el.find(".shelf-chat")[1].querySelector(".picker-model-name").textContent, "Tap again to erase it");
  erase().fire("click");
  await flush();
  assert.deepEqual(sent("chat_forget"), [{ id: "c-2" }]);
  assert.equal(State.chatHistory.length, 2, "the conversation on the island stays");
});

test("resetting the conversation on the island erases it and starts over", async () => {
  answers.chat_shelf = KEPT;
  answers.chat_forget = { current: null, messages: [], chats: [KEPT.chats[1]] };
  await resumeChat();
  view.sync();
  $(".shelf-btn").fire("click");
  await flush();
  $(".shelf-reset").fire("click");
  assert.deepEqual(sent("chat_forget"), []);
  $(".shelf-reset").fire("click");
  await flush();
  await flush();
  assert.deepEqual(sent("chat_forget"), [{ id: "c-1" }]);
  assert.deepEqual(State.chatHistory, []);
});

test("a new chat keeps the old one, and never while an answer is on its way", async () => {
  answers.chat_shelf = KEPT;
  answers.chat_reset = { current: null, messages: [], chats: KEPT.chats };
  await resumeChat();
  State.chatSending = true;
  assert.equal(canSwitch(), false);
  assert.equal(await newChat(), false);
  assert.deepEqual(sent("chat_reset"), []);
  assert.equal(State.chatHistory.length, 2);
  State.chatSending = false;
  assert.equal(await newChat(), true);
  assert.equal(sent("chat_reset").length, 1);
  assert.deepEqual(State.chatHistory, []);
  assert.equal(State.chatShelf.chats.length, 2, "the old conversation is still in the list");
});

test("+ in the chat starts a new chat; elsewhere it is where a file is dropped", () => {
  const did = [];
  const actions = new Proxy({}, { get: (_, name) => (...args) => did.push([name, ...args]) });
  const header = buildHeader(actions);
  const plus = header.el.find(".tab").at(-1);
  State.view = "prompt";
  header.sync();
  assert.equal(plus.title, "New chat");
  plus.fire("click");
  assert.deepEqual(did.at(-1), ["newChat"]);
  State.view = "overview";
  header.sync();
  assert.equal(plus.title, "Drop");
  plus.fire("click");
  assert.deepEqual(did.at(-1), ["setView", "upload"]);
});
