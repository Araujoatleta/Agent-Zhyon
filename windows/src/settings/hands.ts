// Settings → Mochi's hands: let the chat act on this computer, the folder Mochi works in, and how much he may do
// without asking. Off until the user turns it on; Rust owns all three (src-tauri/src/agent): the page only asks.

import { Bridge } from "../core/bridge";
import type { HandsStatusReport } from "../core/hands";
import { h } from "../views/dom";
import { t } from "../i18n/i18n";

type Toggle = (on: boolean, onChange: (v: boolean) => void) => HTMLElement;

const LEVELS: { id: HandsStatusReport["level"]; label: () => string }[] = [
  { id: "ask", label: () => t("Ask before every change") },
  { id: "edits", label: () => t("Edit files in the folder without asking") },
  { id: "read", label: () => t("Read only: look, never change") },
];

export function handsSection(makeToggle: Toggle): HTMLElement {
  const note = h("div", { class: "hint" });
  const shellNote = h("div", { class: "hint" });
  const folder = h("input", { type: "text", style: "flex:1 1 auto;min-width:0", spellcheck: "false", autocomplete: "off" }) as HTMLInputElement;
  const useFolder = h("button", { text: t("Use this folder") });
  const level = h("select", {}) as HTMLSelectElement;
  for (const l of LEVELS) level.append(h("option", { value: l.id, text: l.label() }));
  let current: HandsStatusReport | null = null;

  const show = (status: HandsStatusReport | null) => {
    if (!status) return;
    current = status;
    switchEl.classList.toggle("on", status.enabled);
    switchEl.setAttribute("aria-pressed", String(status.enabled));
    if (document.activeElement !== folder) folder.value = status.folder;
    folder.placeholder = status.suggested;
    level.value = status.level;
    shellNote.textContent = t("Commands run in {shell}.", { shell: status.shell });
  };

  /** Sends the three together; Rust checks the folder and says why it refuses. */
  const apply = async (enabled: boolean) => {
    note.textContent = "";
    // Turning it on with an empty field takes the folder offered in it.
    const path = folder.value.trim() || (enabled ? current?.suggested ?? "" : "");
    try {
      show(await Bridge.agentSettingsSet(enabled, path, level.value));
    } catch (err) {
      note.textContent = String(err).replace(/^Error:\s*/, "");
      show(current);
    }
  };

  const switchEl = makeToggle(false, (next) => void apply(next));
  useFolder.addEventListener("click", () => void apply(current?.enabled ?? false));
  level.addEventListener("change", () => void apply(current?.enabled ?? false));
  void Bridge.agentStatus().then(show);

  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: t("Mochi's hands") })),
    h("div", { class: "hint", text: t("Let Mochi act on this computer from the chat: look through your files, change them, run commands, and read or open web pages — with whichever AI the chat uses. Off by default.") }),
    h("div", { class: "hint", text: t("Every change, command and web request waits for your click (Allow or Deny), on the island or on your paired phone. Only looking inside the folder below needs no click.") }),
    h("div", { class: "row" }, h("label", { text: t("Mochi's hands") }), switchEl),
    h("div", { class: "row" }, h("label", { text: t("Working folder") }), folder, useFolder),
    h("div", { class: "row" }, h("label", { text: t("Without asking") }), level),
    shellNote,
    h("div", { class: "hint", text: t("What Mochi reads is sent to the AI provider you chat with, as anything you type is. Files that look like keys or passwords always wait for a click.") }),
    note,
  );
}
