// Zheus' superpowers' reports → the island (src-tauri/src/powers): the view kept in State, what AXYON tells the moment
// it happens (the ★ tab opens on it, with AXYON's face and sound, in the chat, and aloud when the user asked for it),
// and the content kit's charts turned into PNG files beside their SVGs (the island's page draws them; Rust saves what
// it is given, only into the kit's own folder).

import { Bridge, onEvent } from "../core/bridge";
import type { BotEmoteName, IslandViewName } from "../core/layout";
import { alertMood, alertsMessage, pngName, svgToPng, type Kit, type PowersAlert, type PowersView } from "../core/powers";
import { Sound } from "../core/sound";
import { State } from "../core/state";
import { speak } from "../core/voice";

export interface PowersHost {
  alert(view: IslandViewName): void;
  emote(name: BotEmoteName): void;
}

/** The alerts the island keeps. */
const KEPT_ALERTS = 12;
/** Views the user is busy in (writing, answering an agent, dropping a file…): an alert does not take their place. */
const BUSY_VIEWS: ReadonlySet<IslandViewName> = new Set(["approval", "question", "prompt", "upload", "uploading", "choose", "settings", "wardrobe", "zheus"]);
/** Ids for the alerts in the chat, apart from the chat's own and Zheus One's. */
let alertId = 2_000_000;

export function registerPowers(island: PowersHost) {
  void Bridge.powersStatus().then((v) => {
    if (v) {
      State.powers = v;
      State.notify();
    }
  });
  void onEvent<PowersView>("powers-changed", (v) => {
    State.powers = v;
    State.notify();
  });
  void onEvent<Kit>("powers-kit", (kit) => void savePngs(kit));
  void onEvent<PowersAlert[]>("powers-alert", (alerts) => tellAlerts(island, alerts));
}

/** Alerts that came together: kept for the ★ tab, written in the chat, told with the first one's face and sound. The
 *  island opens on the ★ tab unless paused or busy with the user (then the chat and the sound say it). */
export function tellAlerts(island: PowersHost, alerts: PowersAlert[], now = Date.now()) {
  if (!alerts?.length) return;
  State.powersAlerts = [...alerts.map((a) => ({ ...a, came: now })), ...State.powersAlerts].slice(0, KEPT_ALERTS);
  State.chatHistory.push({ id: alertId++, role: "assistant", content: alertsMessage(alerts) });
  const mood = alertMood(alerts[0]);
  Sound.play(mood.sound);
  if (!State.paused) {
    const busy = State.mode === "expanded" && BUSY_VIEWS.has(State.view);
    if (!busy) island.alert("powers");
    island.emote(mood.emote);
  }
  if (State.settings.readAloud === true && State.settings.soundEnabled) void speak(`${alerts[0].title}. ${alerts[0].text}`);
  State.notify();
}

/** Each chart of a kit as a PNG, one after the other; one that cannot be drawn keeps its SVG only. */
export async function savePngs(kit: Kit, draw: (svg: string) => Promise<Uint8Array> = svgToPng): Promise<number> {
  let saved = 0;
  for (const f of kit.files) {
    try {
      const png = await draw(f.svg);
      await Bridge.powersSavePng(kit.folder, pngName(f.name), Array.from(png));
      saved++;
    } catch {
      // The SVG stays in the folder.
    }
  }
  return saved;
}
