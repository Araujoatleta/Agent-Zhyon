// Settings → Hermes Agent: connect the Hermes Agent running on this computer (its API server and key), and let it answer
// the chat — here and on the paired phone — with its own tools and memory. Rust owns the address and keeps the key in
// the keystore, bound to that address (src-tauri/src/hermes.rs); the page only asks. See docs/HERMES.md.

import { Bridge, type HermesBrain, type HermesGateway, type HermesProgress, type HermesSetupStatus, type HomeMcpStatus } from "../core/bridge";
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
  /** Keeps the name the user gives their assistant. */
  setName: (name: string) => void;
  toggle: Toggle;
}

/** What Coucou will do when the user clicks Install, step by step, before anything happens. */
export function zheusPlan(status: Pick<HermesSetupStatus, "home" | "commit" | "address">, brain: HermesBrain, house: boolean): string[] {
  const key = brain.id === "ollama" || brain.id === "lmstudio"
    ? t("Point Hermes at {brain} on this computer, the server Coucou already uses.", { brain: brain.name })
    : t("Give Hermes your {brain} key that Coucou already keeps: only in memory, when Coucou starts it, never in a file.", { brain: brain.name });
  return [
    t("Download Hermes Agent's official installer from GitHub (version {commit}) and check it (SHA-256) before running it.", { commit: status.commit }),
    t("Install Hermes in {path} with everything it needs (its Python and its own Git): about 1 GB, 5 to 15 minutes. Nothing is asked along the way.", { path: status.home }),
    key,
    house
      ? t("Start Hermes on this computer only ({address}) with a private key, whenever Coucou runs, and connect the chat, the phone and Mochi's home to it.", { address: status.address })
      : t("Start Hermes on this computer only ({address}) with a private key, whenever Coucou runs, and connect the chat and the phone to it.", { address: status.address }),
  ];
}

/** The gateway's state in words. */
export function gatewayText(gateway: HermesGateway): string {
  switch (gateway.state) {
    case "running":
      return t("Hermes is running with Coucou, on this computer only.");
    case "starting":
      return t("Starting Hermes… (the first start can take a minute or two)");
    case "elsewhere":
      return t("Another Hermes already runs on this computer: connect to it below, with the key of its .env.");
    case "stopped":
      return t("Hermes stopped: {reason}", { reason: gateway.reason });
    default:
      return t("Hermes is not running.");
  }
}

/** The installation's line: "Step 3 of 9: Create Python environment". */
export function progressText(progress: HermesProgress): string {
  if (progress.total > 0 && progress.done < progress.total) {
    return t("Step {n} of {total}: {step}", { n: progress.done + 1, total: progress.total, step: progress.step });
  }
  return progress.step;
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

/**
 * Zheus: the assistant's name, and Hermes installed, set up and run by Coucou (src-tauri/src/hermes_setup.rs). The
 * plan is on screen before the click; nothing is downloaded or installed before it.
 */
function zheusBlock(host: HermesHost): HTMLElement {
  const box = h("div", { style: "display:flex;flex-direction:column;gap:6px" });
  const name = h("input", { type: "text", placeholder: "Zheus", style: "flex:1 1 auto;min-width:0", maxlength: "40", spellcheck: "false", autocomplete: "off" }) as HTMLInputElement;
  name.value = host.settings().assistantName ?? "";
  name.addEventListener("change", () => host.setName(name.value.trim()));
  const state = h("div", { style: "display:flex;flex-direction:column;gap:6px" });
  let last: HermesSetupStatus | null = null;
  let chosen = "";
  let house = true;
  let configure = false;

  const brainPicker = (status: HermesSetupStatus) => {
    const select = h("select", {}) as HTMLSelectElement;
    const ready = status.brains.filter((b) => b.ready);
    for (const b of status.brains) {
      const option = h("option", { value: b.id, text: b.ready ? b.name : t("{name} (add its key in Settings first)", { name: b.name }) }) as HTMLOptionElement;
      option.disabled = !b.ready;
      select.append(option);
    }
    if (!chosen || !ready.some((b) => b.id === chosen)) chosen = status.brain && ready.some((b) => b.id === status.brain) ? status.brain : ready[0]?.id ?? "";
    select.value = chosen;
    select.addEventListener("change", () => {
      chosen = select.value;
      show(last);
    });
    return select;
  };

  const show = (status: HermesSetupStatus | null) => {
    if (!status) return;
    last = status;
    state.replaceChildren();
    const p = status.progress;
    if (p.running) {
      const stop = h("button", { text: t("Stop") });
      stop.addEventListener("click", () => void Bridge.hermesSetupCancel().then(show, () => {}));
      state.append(
        h("div", { class: "row" }, statusDot(true), h("span", { text: progressText(p) }), stop),
        ...(p.last ? [h("div", { class: "hint path", text: p.last })] : []),
      );
      return;
    }
    if (p.error) state.append(h("div", { class: "notice err", text: p.error }));
    if (status.managed) {
      const on = status.gateway.state !== "off";
      const sw = host.toggle(on, (next) => void Bridge.hermesGatewaySet(next).then(show, (err) => state.append(h("div", { class: "notice err", text: errorText(err) }))));
      const brain = status.brains.find((b) => b.id === status.brain);
      state.append(
        h("div", { class: "row" }, h("label", { text: t("Run Hermes with Coucou") }), sw),
        h("div", { class: "row" }, statusDot(status.gateway.state === "running"), h("span", { class: "hint", text: gatewayText(status.gateway) })),
        ...(brain ? [h("div", { class: "hint", text: t("Its brain: {brain}, with the key Coucou keeps.", { brain: brain.name }) })] : []),
      );
      return;
    }
    const ready = status.brains.filter((b) => b.ready);
    if (ready.length === 0) {
      state.append(h("div", { class: "notice warn", text: t("Add an AI key first (Anthropic, OpenAI, Google or OpenRouter, in the sections above) or connect Ollama or LM Studio: Hermes needs a brain.") }));
      return;
    }
    const picker = brainPicker(status);
    const brain = status.brains.find((b) => b.id === chosen) ?? ready[0];
    const houseBox = h("input", { type: "checkbox" }) as HTMLInputElement;
    houseBox.checked = house;
    houseBox.addEventListener("change", () => {
      house = houseBox.checked;
      show(last);
    });
    const install = h("button", { class: "primary", text: status.installed ? t("Let Coucou run Hermes") : t("Install Hermes") }) as HTMLButtonElement;
    install.addEventListener("click", () => {
      install.disabled = true;
      Bridge.hermesSetupStart(chosen, house, !status.installed || configure).then(show, (err) => {
        install.disabled = false;
        state.append(h("div", { class: "notice err", text: errorText(err) }));
      });
    });
    const rows: HTMLElement[] = [h("div", { class: "row" }, h("label", { text: t("Its brain") }), picker)];
    if (status.installed) {
      const configureBox = h("input", { type: "checkbox" }) as HTMLInputElement;
      configureBox.checked = configure;
      configureBox.addEventListener("change", () => (configure = configureBox.checked));
      rows.unshift(h("div", { class: "hint", text: t("Hermes is already on this computer ({path}): Coucou can run it, without changing its settings unless you tick it below.", { path: status.home }) }));
      rows.push(h("label", { class: "row" }, configureBox, h("span", { text: t("Also give it this brain (its model and provider change)") })));
    }
    rows.push(h("label", { class: "row" }, houseBox, h("span", { text: t("Let it use Mochi's home (the devices you ticked, with the same rules)") })));
    const plan = h("ol", { class: "hint", style: "margin:0;padding-left:18px;display:flex;flex-direction:column;gap:2px" }, ...zheusPlan(status, brain, house).map((line) => h("li", { text: line })));
    state.append(
      ...rows,
      ...(status.installed ? [] : [h("div", { class: "hint", text: t("What Coucou will do:") }), plan]),
      h("div", { class: "row" }, install),
      h("div", { class: "hint", text: t("Hermes Agent is free and open source (MIT), by Nous Research. Its own tools then ask you before anything risky, here or on your phone.") }),
    );
  };

  box.append(
    h("div", { class: "row" }, h("label", { text: t("Your assistant's name") }), name),
    h("div", { class: "hint", text: t("Hermes is told it answers to this name, here and on your phone: \"Zheus, what's on my calendar today?\"") }),
    state,
  );
  void Bridge.hermesSetupStatus().then(show);
  // The installation and the gateway move on by themselves: kept up to date while the section is shown.
  const watch = window.setInterval(() => {
    if (!box.isConnected) return window.clearInterval(watch);
    if (document.visibilityState !== "visible") return;
    if (last && !last.progress.running && last.gateway.state !== "starting" && !(last.managed && last.gateway.state === "off")) return;
    void Bridge.hermesSetupStatus().then(show);
  }, 2000);
  return box;
}

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
    zheusBlock(host),
    body,
    houseBlock(host.toggle),
  );
  return { el, redraw };
}
