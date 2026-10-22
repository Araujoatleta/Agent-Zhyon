// The island's kept conversations (src-tauri/src/chat_history.rs): the one the chat goes on with after Coucou or the
// computer restarts, "New chat" (+ in the chat, Ctrl+K), going back to a kept one, and erasing one. Rust keeps them,
// sealed, on this computer; Hermes goes on in the same session, so it remembers the conversation too.

import { Bridge, type ChatLine, type ChatShelf } from "./bridge";
import { State, type ChatMessage } from "./state";

/** Lines put back on the island count down from -1: the chat's own count up from 1. */
let restoredId = 0;

function messages(lines: ChatLine[]): ChatMessage[] {
  return lines.map((line) => ({ id: --restoredId, role: line.role, content: line.content }));
}

/** The shelf as Rust now has it; with `words`, its conversation's words take the chat's place. */
function take(shelf: ChatShelf | null, words: boolean) {
  if (!shelf) return;
  State.chatShelf = shelf;
  if (words) State.chatHistory = messages(shelf.messages);
  State.notify();
}

/** An empty chat, with no file or window riding along. */
function emptyChat() {
  State.chatHistory = [];
  State.droppedFile = null;
  State.promptContext = null;
}

/** At start: the conversation the island was in, back on it (unless one has begun meanwhile). */
export async function resumeChat(): Promise<void> {
  const shelf = await Bridge.chatShelf();
  take(shelf, State.chatHistory.length === 0);
}

/** The list as it is now (the panel opening: a turn may have added a conversation since). */
export async function refreshShelf(): Promise<void> {
  take(await Bridge.chatShelf(), false);
}

/** Whether the chat may change conversation now: not while an answer is on its way, it would land in the other one. */
export function canSwitch(): boolean {
  return !State.chatSending;
}

/** "New chat": an empty chat; the conversation before stays in the list. False when it can't now. */
export async function newChat(): Promise<boolean> {
  if (!canSwitch()) return false;
  emptyChat();
  State.notify();
  take(await Bridge.chatReset(), false);
  return true;
}

/** Back to a kept conversation; throws, with the reason, when it is no longer kept. */
export async function openChat(id: string): Promise<void> {
  if (!canSwitch()) return;
  const shelf = await Bridge.chatOpen(id);
  State.droppedFile = null;
  State.promptContext = null;
  take(shelf, true);
}

/** Erases a kept conversation for good; erasing the one on the island empties the chat. */
export async function forgetChat(id: string): Promise<void> {
  if (!canSwitch()) return;
  const onIsland = State.chatShelf?.current === id;
  const shelf = await Bridge.chatForget(id);
  if (onIsland) emptyChat();
  take(shelf, false);
}

/** "Reset this conversation": erased, and the chat starts over. One not kept yet (no answer so far) just empties. */
export async function resetChat(): Promise<void> {
  if (!canSwitch()) return;
  // Asked afresh: the conversation got its place in the list with its first answer.
  const shelf = await Bridge.chatShelf();
  take(shelf, false);
  const current = shelf?.current;
  if (current) await forgetChat(current);
  else await newChat();
}
