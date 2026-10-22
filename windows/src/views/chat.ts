// Chat view — DOM port of PromptView / ChatBubble / TypingDotsView /
// ModelPickerView from IslandViewContent.swift.
//
// Answers are rendered as Markdown (markdown.ts). A local model streams its
// answer: Rust sends `chat-delta` events with the text visible so far. The
// model name above the text field opens the picker: provider chips, then the
// models of the chosen provider, asked for only once it is picked.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import { renderMarkdown } from "./markdown";
import { Bridge, onEvent, type ChatContext, type ChatSummary, type ModelInfo } from "../core/bridge";
import { canSwitch, newChat, openChat, forgetChat, refreshShelf, resetChat } from "../core/chats";
import {
  changePreview, requestTitle, requestWarning, stepDetail, stepVerb, type HandsRequest, type HandsStep,
} from "../core/hands";
import {
  activeModel, pickModel, providerDef, visibleProviders, withModel, type ProviderDef,
} from "../core/providers";
import { Sound } from "../core/sound";
import { State, type ChatMessage } from "../core/state";
import { speak, stopSpeaking } from "../core/voice";
import type { ViewHost } from "./views";
import { N_, dayMonth, t, tl, tn } from "../i18n/i18n";

const STRINGS = {
  conversations: N_("Conversations"),
  newChat: N_("New chat"),
  resetChat: N_("Reset this conversation"),
  tapAgain: N_("Tap again to erase it"),
  erase: N_("Erase"),
  noChats: N_("No kept conversations yet."),
  yesterday: N_("Yesterday"),
  placeholderFirst: N_("Ask me anything…"),
  placeholderNext: N_("Continue…"),
  send: N_("Send"),
  switchModel: N_("Switch provider or model"),
  noModel: N_("Choose a model"),
  loading: N_("Loading models…"),
  noKey: N_("No API key — add it in Settings."),
  openSettings: N_("Open Settings"),
  stop: N_("Stop"),
  handsOn: N_("Mochi's hands are on: he can act on this computer. Click to chat without them."),
  handsOff: N_("Mochi's hands are off in this chat. Click to let him act on this computer."),
  fromPhone: N_("Asked from your phone"),
  inFolder: N_("in {folder}"),
  allow: N_("Allow"),
  deny: N_("Deny"),
};

/** A click this soon after a card shows is not taken: it was meant for what was there before. */
const CLICK_GUARD_MS = 600;

let nextId = 1;

/** A step of Mochi's hands: what he did, on what, and how it went. A click shows what a command printed. */
function actionRow(step: HandsStep): HTMLElement {
  const mark =
    step.status === "ok" ? svg(ICONS.check, 10, { stroke: 2.6 })
    : step.status === "error" ? svg(ICONS.xmark, 9)
    : step.status === "denied" ? svg(ICONS.xmark, 9)
    : h("i", { class: "hands-spin" });
  const detail = stepDetail(step);
  const line = h(
    "div",
    { class: `hands-step ${step.status}`, title: step.summary },
    h("span", { class: "hands-mark" }, mark),
    h("span", { class: "hands-verb", text: stepVerb(step.tool) }),
    h("span", { class: "hands-what", text: step.summary }),
    detail ? h("span", { class: "hands-detail", text: detail }) : null,
  );
  const row = h("div", { class: "hands-row" }, line);
  if (step.output) {
    const output = h("pre", { class: "hands-output", text: step.output });
    output.hidden = true;
    line.classList.add("openable");
    line.addEventListener("click", () => {
      output.hidden = !output.hidden;
    });
    row.append(output);
  }
  return h("div", { class: "chat-row" }, row);
}

/** The few lines of a file change, as the island's diff card draws them. */
function previewBox(r: HandsRequest): HTMLElement | null {
  const preview = changePreview(r);
  if (!preview) return null;
  const box = h(
    "div",
    { class: "hands-diff" },
    h(
      "div",
      { class: "hands-diff-counts" },
      preview.added > 0 ? h("span", { class: "plus", text: `+${preview.added}` }) : null,
      preview.removed > 0 ? h("span", { class: "minus", text: `−${preview.removed}` }) : null,
    ),
  );
  for (const line of preview.lines) {
    const sym = line.kind === "added" ? "+" : line.kind === "removed" ? "−" : line.kind === "gap" ? "⋯" : " ";
    box.append(
      h(
        "div",
        { class: `diff-line ${line.kind === "gap" ? "context" : line.kind}` },
        h("span", { class: "sym", text: sym }),
        h("span", { class: "txt", text: line.text }),
      ),
    );
  }
  if (preview.more > 0) box.append(h("div", { class: "hands-note", text: tn("{count} more line", "{count} more lines", preview.more) }));
  return box;
}

/** The card of an action waiting for a click: what, where, the change itself when it is a file, Deny and Allow. */
function requestCard(r: HandsRequest): HTMLElement {
  const shownAt = performance.now();
  const decide = (allow: boolean) => () => {
    if (performance.now() - shownAt < CLICK_GUARD_MS) return;
    Sound.play(allow ? "approve" : "pop");
    void Bridge.agentDecide(r.id, allow);
  };
  const warning = requestWarning(r);
  return h(
    "div",
    { class: "chat-row" },
    h(
      "div",
      { class: "hands-card" },
      h("div", { class: "hands-card-title", text: requestTitle(r) }),
      r.origin === "phone" ? h("div", { class: "hands-note", text: t(STRINGS.fromPhone) }) : null,
      h("div", { class: "code hands-code", text: r.summary, title: r.summary }),
      r.cwd ? h("div", { class: "hands-note", text: t(STRINGS.inFolder, { folder: r.cwd }) }) : null,
      previewBox(r),
      warning ? h("div", { class: "hands-warn", text: warning }) : null,
      h(
        "div",
        { class: "actions" },
        h("button", { class: "btn secondary", onclick: decide(false) }, h("span", { text: tl(STRINGS.deny) })),
        h("button", { class: "btn primary", onclick: decide(true) }, h("span", { text: tl(STRINGS.allow) })),
      ),
    ),
  );
}

function bubble(message: ChatMessage): HTMLElement {
  if (message.role === "action" && message.action) return actionRow(message.action);
  if (message.role === "user") {
    return h(
      "div",
      { class: "chat-row user" },
      h("div", { class: "bubble", text: message.content }),
    );
  }
  const reply = h("div", { class: "reply" });
  renderMarkdown(reply, message.content);
  return h("div", { class: "chat-row" }, reply);
}

function typingDots(): HTMLElement {
  return h(
    "div",
    { class: "chat-row" },
    h("div", { class: "typing" }, h("i"), h("i"), h("i")),
  );
}

/** The coloured chip showing what the question is about (a dropped file). */
function contextChip(label: string): HTMLElement {
  const chip = h("div", { class: "chip" }, h("i", { class: "chip-dot" }), h("span", { text: label }));
  requestAnimationFrame(() => chip.classList.add("settled"));
  return chip;
}

function saveSettings() {
  void Bridge.saveSettings(State.settings);
}

// ── Model picker ──────────────────────────────────────────────────────────────

interface Picker {
  el: HTMLElement;
  open(): void;
  close(): void;
  readonly isOpen: boolean;
}

/** Provider chips, then the chosen provider's models — ModelPickerView. */
function buildPicker(onChange: () => void): Picker {
  const chips = h("div", { class: "picker-chips" });
  const list = h("div", { class: "picker-list" });
  const el = h("div", { class: "picker" }, chips, h("div", { class: "picker-rule" }), list);

  /** Models already asked for, by provider; a model server is asked again each time. */
  const cache = new Map<string, ModelInfo[]>();
  let isOpen = false;
  let request = 0;

  function drawChips() {
    clear(chips);
    for (const p of visibleProviders(State.settings)) {
      const on = p.id === State.settings.chatProvider;
      const chip = h(
        "button",
        { class: on ? "picker-chip on" : "picker-chip", style: `--accent:${p.accent}` },
        h("i", { class: "picker-dot" }),
        h("span", { text: t(p.name) }),
      );
      chip.addEventListener("click", () => {
        if (p.id === State.settings.chatProvider) return;
        State.settings = { ...State.settings, chatProvider: p.id };
        saveSettings();
        Sound.play("pop");
        drawChips();
        void loadModels();
        onChange();
      });
      chips.append(chip);
    }
  }

  function status(text: string, withSettings = false) {
    clear(list);
    const line = h("div", { class: "picker-status", text });
    if (withSettings) {
      line.append(
        h("button", {
          class: "picker-link",
          text: tl(STRINGS.openSettings),
          onclick: () => void Bridge.openSettingsWindow(),
        }),
      );
    }
    list.append(line);
  }

  function drawModels(p: ProviderDef, models: ModelInfo[]) {
    clear(list);
    const current = activeModel(State.settings);
    for (const m of models) {
      const on = m.id === current;
      const row = h(
        "button",
        { class: on ? "picker-model on" : "picker-model", style: `--accent:${p.accent}`, title: m.id },
        h("span", { class: "picker-model-name", text: m.label }),
        on ? svg(ICONS.check, 11, { stroke: 2.2 }) : null,
      );
      row.addEventListener("click", () => {
        State.settings = withModel(State.settings, p.id, m.id);
        saveSettings();
        Sound.play("blip");
        close();
      });
      list.append(row);
    }
    list.querySelector(".picker-model.on")?.scrollIntoView({ block: "nearest" });
  }

  async function loadModels() {
    const p = providerDef(State.settings.chatProvider);
    const ticket = ++request;
    const cached = p.urlField ? undefined : cache.get(p.id);
    if (cached) {
      drawModels(p, cached);
      return;
    }
    // Nothing is asked of a provider that has no key yet.
    if (p.key && !(await Bridge.secretPresent(p.key))) {
      if (ticket === request) status(t(STRINGS.noKey), true);
      return;
    }
    if (ticket !== request) return;
    status(t(STRINGS.loading));
    try {
      const models = await Bridge.chatModels(p.id);
      if (ticket !== request) return;
      if (!p.urlField) cache.set(p.id, models);
      const keep = pickModel(p, models.map((m) => m.id), activeModel(State.settings));
      if (keep && keep !== activeModel(State.settings)) {
        State.settings = withModel(State.settings, p.id, keep);
        saveSettings();
        onChange();
      }
      drawModels(p, models);
    } catch (err) {
      if (ticket === request) status(String(err).replace(/^Error:\s*/, ""), Boolean(p.key));
    }
  }

  function open() {
    isOpen = true;
    el.classList.add("on");
    drawChips();
    onChange();
    void loadModels();
  }

  function close() {
    isOpen = false;
    request++;
    el.classList.remove("on");
    onChange();
  }

  return {
    el,
    open,
    close,
    get isOpen() {
      return isOpen;
    },
  };
}

// ── Kept conversations (core/chats.ts) ────────────────────────────────────────

interface ShelfPanel {
  el: HTMLElement;
  open(): void;
  close(): void;
  readonly isOpen: boolean;
}

/** When a conversation was last written in: the time today, "Yesterday", else the day. */
function when(updated: number, now = new Date()): string {
  const d = new Date(updated * 1000);
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((day(now) - day(d)) / 86_400_000);
  if (days === 0) return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  if (days === 1) return t(STRINGS.yesterday);
  return dayMonth(d.getMonth(), d.getDate());
}

/**
 * The conversations the chat keeps (it takes the log's place, like the model picker): a new one, resetting the one
 * on screen, and the list to go back to one or erase it. Erasing asks for a second click.
 */
function buildShelf(onChange: () => void): ShelfPanel {
  const fresh = h("button", { class: "shelf-new" }, svg(ICONS.plus, 10, { stroke: 2 }), h("span", { text: tl(STRINGS.newChat) }));
  const resetLabel = h("span", { text: t(STRINGS.resetChat) });
  const reset = h("button", { class: "shelf-reset" }, resetLabel);
  const list = h("div", { class: "picker-list shelf-list" });
  const el = h("div", { class: "shelf" }, h("div", { class: "shelf-head" }, fresh, reset), h("div", { class: "picker-rule" }), list);

  let isOpen = false;
  /** The one a first click on its × (or on Reset: "current") asked to erase, until it is clicked again. */
  let arming: string | null = null;
  let armTimer: number | null = null;

  function arm(what: string | null) {
    arming = what;
    if (armTimer != null) window.clearTimeout(armTimer);
    armTimer = what == null ? null : window.setTimeout(() => arm(null), 4000);
    draw();
  }

  function row(c: ChatSummary): HTMLElement {
    const on = c.id === State.chatShelf?.current;
    const armed = arming === c.id;
    const erase = h(
      "button",
      { class: armed ? "shelf-erase armed" : "shelf-erase", title: tl(armed ? STRINGS.tapAgain : STRINGS.erase) },
      svg(ICONS.xmark, 8),
    );
    erase.addEventListener("click", (e) => {
      e.stopPropagation();
      if (!canSwitch()) return;
      if (arming !== c.id) {
        arm(c.id);
        return;
      }
      arm(null);
      Sound.play("pop");
      void forgetChat(c.id).then(draw);
    });
    const line = h(
      "div",
      { class: `picker-model shelf-chat${on ? " on" : ""}${armed ? " armed" : ""}`, title: c.title },
      h("span", { class: "picker-model-name", text: armed ? t(STRINGS.tapAgain) : c.title || "…" }),
      h("span", { class: "shelf-when", text: when(c.updated) }),
      erase,
    );
    line.addEventListener("click", () => {
      if (!canSwitch()) return;
      if (on) {
        close();
        return;
      }
      Sound.play("blip");
      openChat(c.id)
        .then(close)
        .catch((err) => {
          State.noteMessage = String(err).replace(/^Error:\s*/, "");
          State.view = "note";
          State.notify();
        });
    });
    return line;
  }

  function draw() {
    const chats = State.chatShelf?.chats ?? [];
    clear(list);
    if (chats.length === 0) list.append(h("div", { class: "picker-status", text: t(STRINGS.noChats) }));
    for (const c of chats) list.append(row(c));
    reset.classList.toggle("armed", arming === "current");
    resetLabel.textContent = t(arming === "current" ? STRINGS.tapAgain : STRINGS.resetChat);
    const empty = State.chatHistory.length === 0;
    reset.disabled = empty || !canSwitch();
    fresh.disabled = !canSwitch();
  }

  fresh.addEventListener("click", () => {
    Sound.play("blip");
    void newChat().then((done) => done && close());
  });
  reset.addEventListener("click", () => {
    if (!canSwitch()) return;
    if (arming !== "current") {
      arm("current");
      return;
    }
    arm(null);
    Sound.play("pop");
    void resetChat().then(close);
  });

  function open() {
    isOpen = true;
    arm(null);
    el.classList.add("on");
    onChange();
    void refreshShelf().then(() => isOpen && draw());
  }

  function close() {
    isOpen = false;
    arm(null);
    el.classList.remove("on");
    onChange();
  }

  return {
    el,
    open,
    close,
    get isOpen() {
      return isOpen;
    },
  };
}

// ── View ──────────────────────────────────────────────────────────────────────

export function buildPrompt(onHeightChange: () => void): ViewHost {
  const chipRow = h("div", { class: "chip-row" });
  const log = h("div", { class: "chat-log" });
  const input = h("input", {
    type: "text",
    class: "chat-input",
    placeholder: t(STRINGS.placeholderFirst),
    spellcheck: "false",
  }) as HTMLInputElement;
  const sendIcon = svg(ICONS.arrowUp, 11);
  const stopIcon = svg(ICONS.stop, 9);
  const send = h("button", { class: "send-btn", title: tl(STRINGS.send) }, sendIcon);
  const bar = h("div", { class: "chat-bar" }, input, send);

  const modelDot = h("i", { class: "model-dot" });
  const modelName = h("span", { class: "model-name" });
  const modelBtn = h(
    "button",
    { class: "model-btn", title: tl(STRINGS.switchModel) },
    modelDot,
    modelName,
    svg(ICONS.chevronUpDown, 9, { stroke: 2 }),
  );
  // Mochi's hands in this chat, when they are on in Settings: on (the default) or off for plain chat.
  const handsBtn = h("button", { class: "hands-btn" }, svg(ICONS.hand, 11, { stroke: 1.7 }));
  // The kept conversations: back to an old one, a new one, or this one reset (core/chats.ts).
  const shelfBtn = h("button", { class: "shelf-btn", title: tl(STRINGS.conversations) }, svg(ICONS.stack, 11));
  const modelRow = h("div", { class: "model-row" }, shelfBtn, handsBtn, modelBtn);

  const body = h("div", { class: "chat-body" });
  const picker = buildPicker(() => {
    body.classList.toggle("picking", picker.isOpen);
    drawModelButton();
  });
  const shelf = buildShelf(() => {
    State.chatShelfOpen = shelf.isOpen;
    body.classList.toggle("shelving", shelf.isOpen);
    shelfBtn.classList.toggle("on", shelf.isOpen);
    renderedKey = ""; // the log comes back with the conversation now on the island
    onHeightChange();
  });
  body.append(chipRow, log, picker.el, shelf.el, modelRow, bar);

  const el = h("div", { class: "view" }, h("div", { class: "card wash chat-card" }, body));
  (el.querySelector(".card") as HTMLElement).style.setProperty("--wash", "rgba(99,102,241,0.5)");

  let sending = false;
  /** The turn being written uses Mochi's hands: the send button stops it. */
  let withHands = false;
  let renderedKey = "";
  // A local model answers token by token: where its text so far is shown.
  let live: HTMLElement | null = null;

  /** Mochi's hands are on in Settings, and on in this chat. */
  const handsActive = () => State.settings.agentEnabled === true && State.settings.chatHands !== false;

  function drawModelButton() {
    const p = providerDef(State.settings.chatProvider);
    modelDot.style.background = p.accent;
    modelName.textContent = activeModel(State.settings) || t(STRINGS.noModel);
    modelBtn.classList.toggle("open", picker.isOpen);
    modelBtn.disabled = sending;
    // The hands toggle only exists while the hands are on in Settings, and not for Hermes, which has its own.
    handsBtn.hidden = State.settings.agentEnabled !== true || State.settings.chatProvider === "hermes";
    handsBtn.classList.toggle("on", handsActive());
    handsBtn.disabled = sending;
    shelfBtn.disabled = sending;
    handsBtn.title = t(handsActive() ? STRINGS.handsOn : STRINGS.handsOff);
    // While a turn with hands runs, the send button is its stop button.
    const stopping = sending && withHands;
    send.classList.toggle("stop", stopping);
    send.title = t(stopping ? STRINGS.stop : STRINGS.send);
    send.replaceChildren(stopping ? stopIcon : sendIcon);
  }

  handsBtn.addEventListener("click", () => {
    if (sending) return;
    State.settings = { ...State.settings, chatHands: !handsActive() };
    saveSettings();
    Sound.play("blip");
    drawModelButton();
  });

  modelBtn.addEventListener("click", () => {
    if (sending) return;
    if (shelf.isOpen) shelf.close();
    if (picker.isOpen) picker.close();
    else picker.open();
  });

  shelfBtn.addEventListener("click", () => {
    if (sending) return;
    if (picker.isOpen) picker.close();
    if (shelf.isOpen) shelf.close();
    else shelf.open();
  });

  void onEvent<string>("chat-delta", (text) => {
    if (!sending || !text) return; // nothing visible yet: the dots stay
    if (!live) {
      live = h("div", { class: "reply" });
      log.querySelector(".typing")?.parentElement?.remove();
      log.append(h("div", { class: "chat-row" }, live));
    }
    renderMarkdown(live, text);
    log.scrollTop = log.scrollHeight;
  });

  async function submit() {
    const query = input.value.trim();
    if (!query || sending) return;
    if (picker.isOpen) picker.close();
    stopSpeaking(); // a new question: the last answer stops being read
    input.value = "";
    if (shelf.isOpen) shelf.close();
    sending = true;
    State.chatSending = true;
    // A turn with Hermes works like one with hands: its steps show, and the send button stops it.
    withHands = handsActive() || State.settings.chatProvider === "hermes";
    drawModelButton();
    Sound.play("send");

    State.chatHistory.push({ id: nextId++, role: "user", content: query });
    State.stateOverride = "thinking";
    State.notify();
    onHeightChange();

    const file = State.droppedFile;
    const context: ChatContext | null =
      State.chatHistory.filter((m) => m.role === "user").length === 1 && file ? { kind: "file", name: file.name, path: file.path } : null;

    try {
      const reply = await Bridge.chatSend(query, context, withHands);
      State.chatHistory.push({ id: nextId++, role: "assistant", content: reply.text });
      State.stateOverride = null;
      Sound.play("finish");
      // Read aloud when the user asked for it (Settings → Mochi's home → Voice), unless the island is muted.
      if (State.settings.readAloud === true && State.settings.soundEnabled) void speak(reply.text);
    } catch (err) {
      State.stateOverride = null;
      State.noteMessage = String(err).replace(/^Error:\s*/, "");
      State.view = "note";
      Sound.play("error");
    } finally {
      sending = false;
      State.chatSending = false;
      withHands = false;
      live = null;
      renderedKey = ""; // the finished answer replaces the streamed one
      drawModelButton();
      State.notify();
      onHeightChange();
      input.focus();
    }
  }

  send.addEventListener("click", () => {
    if (sending && withHands) {
      void Bridge.agentCancel();
      return;
    }
    void submit();
  });
  input.addEventListener("keydown", (e) => {
    const key = (e as KeyboardEvent).key;
    if (key === "Enter") {
      e.preventDefault();
      void submit();
    } else if (key === "Escape" && picker.isOpen) {
      e.preventDefault();
      picker.close();
    }
    e.stopPropagation(); // Escape closes the island, not the chat
  });

  return {
    el,
    sync() {
      const file = State.droppedFile;
      const wantChip = file?.name ?? "";
      if (chipRow.dataset.label !== wantChip) {
        chipRow.dataset.label = wantChip;
        clear(chipRow);
        if (wantChip) chipRow.append(contextChip(wantChip));
      }

      // Redrawn when a message comes, a step of Mochi's hands changes, or an action starts or stops waiting.
      const thinking = sending && (State.stateOverride === "thinking" || State.stateOverride === "working");
      const steps = State.chatHistory.map((m) => (m.action ? `${m.action.index}${m.action.status}` : "")).join(",");
      // The first and last ids tell one conversation from another of the same length (core/chats.ts).
      const ends = `${State.chatHistory[0]?.id ?? ""}~${State.chatHistory.at(-1)?.id ?? ""}`;
      const key = `${State.chatHistory.length}|${ends}|${thinking}|${steps}|${State.handsRequest?.id ?? ""}`;
      if (key !== renderedKey && !live) {
        renderedKey = key;
        clear(log);
        for (const m of State.chatHistory) log.append(bubble(m));
        if (thinking && State.stateOverride === "thinking") log.append(typingDots());
        if (State.handsRequest) log.append(requestCard(State.handsRequest));
        log.scrollTop = log.scrollHeight;
      }

      // Leaving the chat folds the picker and the conversations away.
      if (State.view !== "prompt" && picker.isOpen) picker.close();
      if (State.view !== "prompt" && shelf.isOpen) shelf.close();
      drawModelButton();

      input.placeholder = t(State.chatHistory.length === 0 ? STRINGS.placeholderFirst : STRINGS.placeholderNext);
      input.disabled = sending;
      // A question written by another view (Zheus' superpowers): in the field, for the user to send or change.
      if (State.chatDraft != null && State.view === "prompt" && !sending) {
        input.value = State.chatDraft;
        State.chatDraft = null;
        input.focus();
      }
    },
    focus() {
      input.focus();
      input.select();
    },
  };
}
