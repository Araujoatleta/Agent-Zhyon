// Zheus One's reports → the island (src-tauri/src/zheus_one): its status kept in State, a plan opening the island on
// it, what Zheus says (the welcome home, the day's summary, what the Autopilot did) in the chat and aloud when the user
// asked for it, and a suggestion showing the island.

import { Bridge, onEvent } from "../core/bridge";
import type { IslandViewName } from "../core/layout";
import { Sound } from "../core/sound";
import { State } from "../core/state";
import type { Plan, Suggestion, ZheusStatus } from "../core/zheus";
import { speak } from "../core/voice";

export interface ZheusHost {
  alert(view: IslandViewName): void;
}

/** Ids for what Zheus says in the chat, apart from those the chat gives the user's own messages. */
let sayId = 1_000_000;

export function registerZheus(island: ZheusHost) {
  void Bridge.zheusStatus().then((s) => {
    if (s) {
      State.zheus = s;
      State.notify();
    }
  });
  void onEvent<ZheusStatus>("zheus-changed", (s) => {
    State.zheus = s;
    State.notify();
  });
  // A plan was made (from the chat, the phone, Hermes or the island): show it, waiting for a yes.
  void onEvent<Plan>("zheus-plan", () => {
    if (State.paused) return;
    Sound.play("pop");
    island.alert("zheus");
  });
  void onEvent<string>("zheus-say", (text) => {
    if (!text) return;
    State.chatHistory.push({ id: sayId++, role: "assistant", content: text });
    Sound.play("finish");
    if (!State.paused) island.alert("prompt");
    if (State.settings.readAloud === true && State.settings.soundEnabled) void speak(text);
    State.notify();
  });
  void onEvent<Suggestion>("zheus-suggestion", () => {
    if (State.paused) return;
    Sound.play("blip");
    island.alert("zheus");
  });
}
