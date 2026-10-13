// Settings → Hermes Agent: connect the Hermes Agent running on this computer (its API server and key), and let it answer
// the chat — here and on the paired phone — with its own tools and memory. Rust owns the address and keeps the key in
// the keystore, bound to that address (src-tauri/src/hermes.rs); the page only asks. See docs/HERMES.md.

import { Bridge, type HomeMcpStatus } from "../core/bridge";
import type { Settings } from "../core/state";
import { h } from "../views/dom";
import { t } from "../i18n/i18n";
import { renderDiff, statusDot } from "./parts";

/** Where `hermes gateway` serves its API by default. */
const USUAL = "http://127.0.0.1:8642";
/** Hermes keeps its files in %LOCALAPPDATA%\hermes on Windows, in ~/.hermes elsewhere. */
const WINDOWS = typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");
const hermesFile = (name: string) => (WINDOWS ? `%LOCALAPPDATA%\\hermes\\${name}` : `~/.hermes/${name}`);
const ACCENT = "#F2C14E";

type Toggle = (on: boolean, onChange: (v: boolean) => void) => HTMLElement;

export interface HermesHost {
  /** The settings as they are now. */
  settings: () => Settings;
  /** Makes Hermes the chat's provider. */
  useInChat: () => Promise<void>;
  toggle: Toggle;
}

/** What goes in Hermes' config.yaml so it starts coucou-hook as an MCP server (single quotes: no escapes in YAML). */
export function mcpSnippet(command: string): string {
  const quoted = `'${command.replace(/'/g, "''")}'`;
  return ["mcp_servers:", "  coucou:", `    command: ${quoted}`, '    args: ["--mcp"]'].join("\n");
}

/** Mochi's home for Hermes: share the house over MCP (agent/mcp.rs), and how Hermes is told about it. */
function houseBlock(makeToggle: Toggle): HTMLElement {
  const details = h("div", { style: "display:flex;flex-direction:column;gap:6px" });
  const note = h("div", {});
  const show = (status: HomeMcpStatus | null) => {
    if (!status) return;
    switchEl.classList.toggle("on", status.on);
    switchEl.setAttribute("aria-pressed", String(status.on));
    details.replaceChildren();
    if (!status.on) return;
    if (!status.homeOn) details.append(h("div", { class: "notice warn", text: t("Mochi's home is off: turn it on in Settings → Mochi's home, and tick the devices Mochi may use.") }));
    const snippet = mcpSnippet(status.command);
    const copy = h("button", { text: t("Copy") });
    copy.addEventListener("click", () => {
      void navigator.clipboard?.writeText(snippet).then(() => {
        copy.textContent = t("Copied");
        window.setTimeout(() => (copy.textContent = t("Copy")), 1500);
      });
    });
    details.append(
      h("div", { class: "row" }, h("span", { class: "hint", text: t("Add this to Hermes' config.yaml ({path}), then restart hermes gateway:", { path: hermesFile("config.yaml") }) }), copy),
      renderDiff(snippet),
    );
  };
  const switchEl = makeToggle(false, (next) => {
    note.replaceChildren();
    Bridge.homeMcpSet(next).then(show, (err) => note.replaceChildren(h("div", { class: "notice err", text: errorText(err) })));
  });
  void Bridge.homeMcpStatus().then(show);
  return h(
    "div",
    { style: "display:flex;flex-direction:column;gap:6px" },
    h("div", { class: "row" }, h("label", { text: t("Mochi's home for Hermes") }), switchEl),
    h("div", { class: "hint", text: t("Let Hermes (and other agents on this computer that speak MCP) use the devices Mochi may use, with the same rules: only the devices you ticked, a click for what asks first, every action in the record. Off by default.") }),
    details,
    note,
  );
}

const errorText = (err: unknown) => String(err).replace(/^Error:\s*/, "");

/** The section, and what redraws it when the settings change elsewhere (Connect, the island's picker). */
export function hermesSection(host: HermesHost): { el: HTMLElement; redraw: () => void } {
  const body = h("div", { style: "display:flex;flex-direction:column;gap:6px" });
  const label = () => h("label", {}, h("i", { class: "dot", style: `background:${ACCENT};margin-right:8px` }), h("span", { text: t("Hermes Agent") }));

  const connectedView = (s: Settings) => {
    const inUse = s.chatProvider === "hermes";
    const use = h("button", { class: inUse ? "" : "primary", text: inUse ? t("In use") : t("Use in chat") }) as HTMLButtonElement;
    use.disabled = inUse;
    use.addEventListener("click", () => void host.useInChat().then(redraw));
    const disconnect = h("button", { class: "danger", text: t("Disconnect") }) as HTMLButtonElement;
    const note = h("div", {});
    disconnect.addEventListener("click", async () => {
      disconnect.disabled = true;
      try {
        await Bridge.hermesDisconnect();
      } catch (err) {
        note.replaceChildren(h("div", { class: "notice err", text: errorText(err) }));
        disconnect.disabled = false;
      }
    });
    const name = s.chatModels.hermes || "hermes-agent";
    body.append(
      h("div", { class: "row" }, label(), h("span", { class: "path", text: s.hermesUrl }), statusDot(true), use, disconnect),
      h("div", { class: "hint", text: t("Hermes answers as {name}. Choose it above the chat box, or for the phone in Settings → Phone.", { name }) }),
      note,
    );
  };

  const formView = () => {
    const address = h("input", { type: "text", placeholder: USUAL, style: "flex:1 1 auto;min-width:0", spellcheck: "false", autocomplete: "off" }) as HTMLInputElement;
    const key = h("input", { type: "password", placeholder: "API_SERVER_KEY", style: "flex:1 1 auto;min-width:0", spellcheck: "false", autocomplete: "off" }) as HTMLInputElement;
    const connect = h("button", { class: "primary", text: t("Connect") }) as HTMLButtonElement;
    const status = h("div", {});
    connect.addEventListener("click", async () => {
      connect.disabled = true;
      status.replaceChildren(h("div", { class: "hint", text: t("Connecting…") }));
      try {
        // Rust stores the address and tells both windows (settings-changed), which redraws this.
        await Bridge.hermesConnect(address.value, key.value);
        key.value = "";
      } catch (err) {
        status.replaceChildren(h("div", { class: "notice err", text: errorText(err) }));
        connect.disabled = false;
      }
    });
    body.append(
      h("div", { class: "row" }, label(), address, connect),
      h("div", { class: "row" }, h("label", { text: t("API key") }), key),
      status,
      h("div", { class: "hint", text: t("In Hermes' .env file ({path}): API_SERVER_ENABLED=true and API_SERVER_KEY= a long secret of your choice, then run hermes gateway. Type the same secret above. The key stays in this computer's keystore and only goes to that address.", { path: hermesFile(".env") }) }),
    );
  };

  function redraw() {
    body.replaceChildren();
    const s = host.settings();
    if (s.hermesUrl) connectedView(s);
    else formView();
  }

  redraw();
  const el = h(
    "section",
    {},
    h("h2", {}, h("span", { text: t("Hermes Agent") })),
    h("div", { class: "hint", text: t("Let Hermes Agent answer the chat, here and on your phone, with its own tools and memory: your calendar, Spotify, your home, scheduled tasks. Hermes runs on this computer; Coucou shows each thing it does and asks you before what it wants approved.") }),
    body,
    houseBlock(host.toggle),
  );
  return { el, redraw };
}
