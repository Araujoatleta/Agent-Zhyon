// Mochi's answers read aloud on this computer (src/core/voice.ts): the text a voice reads, the voice picked (one of
// this computer only), and the chat reading an answer when the user asked for it — on a fake speechSynthesis.

import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { internals } from "./tauri.mjs";

installFakeDom();

/** A speechSynthesis that records what it was asked to read. */
const spoken = [];
let voices = [];
let cancels = 0;
globalThis.speechSynthesis = {
  getVoices: () => voices,
  speak: (u) => spoken.push(u),
  cancel: () => cancels++,
  addEventListener() {},
  removeEventListener() {},
};
globalThis.SpeechSynthesisUtterance = class {
  constructor(text) {
    this.text = text;
  }
};

const { pickVoice, speak, speechLanguage, spokenText } = await import("../src/core/voice.ts");
const { buildPrompt } = await import("../src/views/chat.ts");
const { DEFAULT_SETTINGS, State } = await import("../src/core/state.ts");

const MARIA = { name: "Microsoft Maria - Portuguese (Brazil)", lang: "pt-BR", localService: true };
const HELIA = { name: "Microsoft Helia - Portuguese (Portugal)", lang: "pt-PT", localService: true };
const ZIRA = { name: "Microsoft Zira - English (United States)", lang: "en-US", localService: true };
const ONLINE = { name: "Microsoft Francisca Online (Natural)", lang: "pt-BR", localService: false };

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  spoken.length = 0;
  cancels = 0;
  voices = [ZIRA, MARIA];
});

test("an answer is read as sentences: no marks, no code, no link addresses", () => {
  assert.equal(
    spokenText("**Pronto!** A TV da sala está `ligada`.\n\n- YouTube aberto\n```\nnpm test\n```\nVeja [o guia](https://example.com)."),
    "Pronto! A TV da sala está ligada. YouTube aberto. Veja o guia.",
  );
  assert.equal(spokenText("# Feito\n> nada mais"), "Feito. nada mais.");
  assert.equal(
    spokenText(`A TV está ligada. O YouTube está aberto na tela da sala agora mesmo. ${"x".repeat(200)}`, 80),
    "A TV está ligada. O YouTube está aberto na tela da sala agora mesmo.",
  );
  assert.equal(spokenText("y".repeat(30), 10), "yyyyyyyyyy…");
  assert.equal(spokenText("```\ncode\n```"), "");
});

test("the voice is one of this computer, in the app's language first", () => {
  assert.equal(speechLanguage("pt-BR"), "pt-BR");
  assert.equal(speechLanguage("zh-Hans"), "zh-CN");
  assert.equal(speechLanguage("en"), "en-US");
  assert.equal(pickVoice([HELIA, MARIA, ZIRA], "pt-BR"), MARIA);
  assert.equal(pickVoice([HELIA, ZIRA], "pt-BR"), HELIA, "the same language, from elsewhere");
  assert.equal(pickVoice([ONLINE, ZIRA], "pt-BR"), null, "an online voice would send the text away");
  assert.equal(pickVoice([{ name: "x", lang: "pt_BR" }], "pt-BR")?.name, "x");
  assert.equal(pickVoice([], "en-US"), null);
});

test("speak reads with the voice picked, and stays silent without one", async () => {
  assert.equal(await speak("**A TV** está ligada", "pt-BR"), true);
  assert.equal(spoken.length, 1);
  assert.equal(spoken[0].text, "A TV está ligada.");
  assert.equal(spoken[0].voice, MARIA);
  assert.equal(spoken[0].lang, "pt-BR");
  voices = [ZIRA];
  assert.equal(await speak("Olá", "ru-RU"), false);
  assert.equal(await speak("```\nonly code\n```", "en-US"), false);
  assert.equal(spoken.length, 1);
});

// ── The chat ──────────────────────────────────────────────────────────────────

let reply = "";
const plainInvoke = internals.invoke;
internals.invoke = async (cmd, args) => {
  const result = await plainInvoke(cmd, args);
  if (cmd === "chat_send") return { text: reply };
  return result;
};

async function ask(settings, question = "liga a TV") {
  State.settings = { ...DEFAULT_SETTINGS, chatModels: {}, ...settings };
  State.chatHistory = [];
  State.stateOverride = null;
  State.view = "prompt";
  State.droppedFile = null;
  const view = buildPrompt(() => {});
  view.sync();
  view.el.querySelector(".chat-input").value = question;
  view.el.querySelector(".send-btn").fire("click");
  await flush();
  await flush();
  await flush();
}

test("the chat reads the answer aloud only when asked, and never when the island is muted", async () => {
  reply = "**Pronto!** A TV da sala está ligada e o YouTube está aberto.";
  await ask({ readAloud: false });
  assert.equal(spoken.length, 0, "off by default");
  await ask({ readAloud: true, soundEnabled: false });
  assert.equal(spoken.length, 0, "muted");
  await ask({ readAloud: true, soundEnabled: true });
  assert.equal(spoken.length, 1);
  assert.equal(spoken[0].text, "Pronto! A TV da sala está ligada e o YouTube está aberto.");
  assert.ok(cancels > 0, "a new question stops what was being read");
});
