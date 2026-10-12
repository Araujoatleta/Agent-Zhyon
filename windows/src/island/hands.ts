// Mochi's hands on the island (src-tauri/src/agent): what he does shows up in the chat as he does it, and an action
// that waits for a click opens the island on the chat, as a permission request does, and keeps it open until it is
// answered — here, or on a paired phone. The chat view draws it all (views/chat.ts).

import { Bridge, onEvent } from "../core/bridge";
import type { HandsRequest, HandsStep } from "../core/hands";
import { Sound } from "../core/sound";
import { State, type ChatMessage } from "../core/state";
import type { Island } from "./island";

/** Chat message ids of the step rows: far above the chat's own, which count up from 1. */
let nextRowId = 1_000_000;
/** Which row each step of the current turn is, by its index in the turn. */
const rows = new Map<number, number>();
let rowsRun: number | null = null;

/** A step of the island's turn: a new row, or a new state of a row already there. */
export function applyStep(step: HandsStep): void {
  if (rowsRun !== State.handsRun) {
    rows.clear();
    rowsRun = State.handsRun;
  }
  const id = rows.get(step.index);
  const row = id == null ? undefined : State.chatHistory.find((m) => m.id === id);
  if (row) {
    row.action = step;
  } else {
    const message: ChatMessage = { id: nextRowId++, role: "action", content: "", action: step };
    State.chatHistory.push(message);
    rows.set(step.index, message.id);
  }
  // Mochi's face: waiting for a click, at work, or thinking about what comes next.
  State.stateOverride = step.status === "waiting" ? "approval" : step.status === "running" ? "working" : "thinking";
  State.notify();
}

/** An action waits for a click: the chat comes up with its card, unless a Claude Code card is on screen. */
function show(island: Island, request: HandsRequest) {
  State.handsRequest = request;
  Sound.play("approval");
  // A permission request on screen keeps its place; Mochi's card is there when the chat comes back
  // (State.defaultView).
  if (State.pendingApproval) {
    State.notify();
    return;
  }
  if (State.mode === "expanded" && State.view === "prompt") {
    island.setPinned(true);
  } else {
    State.isPinned = true;
    island.alert("prompt");
  }
  State.notify();
}

/** Returns what undoes it (the tests use it; the app registers once and keeps it). */
export function registerHands(island: Island): () => void {
  const off: Array<Promise<() => void>> = [];
  off.push(
    onEvent<{ run: number; origin: string; running: boolean }>("agent-run", (e) => {
      if (e.running && e.origin === "island") State.handsRun = e.run;
      if (!e.running && State.handsRun === e.run) State.handsRun = null;
      State.notify();
    }),
  );
  off.push(
    onEvent<{ run: number; origin: string; step: HandsStep }>("agent-step", (e) => {
      // The phone's turns stay in the phone's chat; only their actions waiting for a click come here.
      if (e.origin !== "island" || e.run !== State.handsRun) return;
      applyStep(e.step);
    }),
  );
  off.push(onEvent<HandsRequest>("agent-approval", (request) => show(island, request)));
  off.push(
    onEvent<{ id: string; allowed: boolean }>("agent-approval-resolved", (e) => {
      if (State.handsRequest?.id !== e.id) return;
      State.handsRequest = null;
      if (!State.pendingApproval) {
        State.isPinned = false;
        island.dropPin();
      }
      State.notify();
    }),
  );
  // An action may already be waiting (the page was reloaded while Mochi worked).
  void Bridge.agentStatus().then((status) => {
    if (status?.waiting) show(island, status.waiting);
  });
  return () => {
    for (const p of off) void p.then((undo) => undo());
  };
}
