// Zheus' superpowers' reports → the island (src-tauri/src/powers): the view kept in State, and the content kit's
// charts turned into PNG files beside their SVGs (the island's page draws them; Rust saves what it is given, only
// into the kit's own folder).

import { Bridge, onEvent } from "../core/bridge";
import { pngName, svgToPng, type Kit, type PowersView } from "../core/powers";
import { State } from "../core/state";

export function registerPowers() {
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
