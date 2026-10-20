// Settings → Hermes Agent: connect the Hermes Agent running on this computer (its API server and key), and let it answer
// the chat — here and on the paired phone — with its own tools and memory. Rust owns the address and keeps the key in
// the keystore, bound to that address (src-tauri/src/hermes.rs); the page only asks. Above the form, Zheus: Hermes
// installed and run by Coucou, with its brain (a key, a local server or the user's ChatGPT subscription), the
// assistant's name and tone, and skill packs (src-tauri/src/hermes_setup.rs). See docs/HERMES.md.

import { Bridge, type HermesBrain, type HermesDiagnosis, type HermesGateway, type HermesHealth, type HermesLogin, type HermesProgress, type HermesSetupStatus, type HomeMcpStatus } from "../core/bridge";
import type { Settings } from "../core/state";
import { h } from "../views/dom";
import { N_, labels, t } from "../i18n/i18n";
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

/** The assistant's tones (src-tauri/src/hermes_setup.rs PERSONAS), as Settings names them. */
const PERSONAS = labels({
  friendly: N_("Friendly"),
  direct: N_("Direct"),
  professional: N_("Professional"),
  playful: N_("Playful"),
  butler: N_("Butler (formal, witty)"),
  custom: N_("Custom"),
});
/** The skill packs Hermes installs (Mochi's home is the checkbox and the switch of its own). */
const PACKS = labels({
  search: N_("Free web search (DuckDuckGo)"),
  news: N_("News and feeds (RSS)"),
});
const OWN_TONE = "custom";
/** The model picker's last choice: a name typed in. */
const OTHER_MODEL = "__other";
/** How quick Hermes is (src-tauri/src/hermes_setup.rs SPEEDS): "" is Hermes' own. */
const SPEEDS = labels({
  fast: N_("Fast"),
  "": N_("Usual"),
  deep: N_("Thorough"),
});
const MAX_TONE = 1500;
/** Looks like the settings window's text fields, for the tone of the user's own. */
const AREA_STYLE =
  "font:400 12.5px var(--font);background:rgba(255,255,255,0.07);border:1px solid var(--hairline);border-radius:8px;color:var(--ink);padding:6px 8px;resize:vertical;min-height:64px;width:100%;box-sizing:border-box";

/** A brain's name in the picker: ChatGPT says it runs on the user's subscription. */
const brainName = (brain: HermesBrain) => (brain.login ? t("ChatGPT (your subscription)") : brain.name);

/** The packs' names, for the plan. */
const packNames = (skills: string[]) => skills.filter((id) => id in PACKS).map((id) => PACKS[id as keyof typeof PACKS]).join(", ");

/** What Coucou does with the brain, in one line of the plan. */
function brainPlan(brain: HermesBrain): string {
  if (brain.login) {
    return t("Sign in with your ChatGPT account (Plus or Pro): Coucou opens OpenAI's page and shows a code to type there. No key: the sign-in stays in Hermes' own files.");
  }
  return brain.id === "ollama" || brain.id === "lmstudio"
    ? t("Point Hermes at {brain} on this computer, the server Coucou already uses.", { brain: brain.name })
    : t("Give Hermes your {brain} key that Coucou already keeps: only in memory, when Coucou starts it, never in a file.", { brain: brain.name });
}

/**
 * What Coucou will do when the user clicks Install, step by step, before anything happens: the download and the
 * installation (unless Hermes is there), the brain, SOUL.md, the skill packs ticked, and the gateway.
 */
export function zheusPlan(
  status: Pick<HermesSetupStatus, "home" | "commit" | "address"> & { installed?: boolean },
  brain: HermesBrain,
  house: boolean,
  skills: string[] = [],
  name = "Zheus",
): string[] {
  const install = status.installed
    ? []
    : [
        t("Download Hermes Agent's official installer from GitHub (version {commit}) and check it (SHA-256) before running it.", { commit: status.commit }),
        t("Install Hermes in {path} with everything it needs (its Python and its own Git): about 1 GB, 5 to 15 minutes. Nothing is asked along the way.", { path: status.home }),
      ];
  const packs = packNames(skills);
  return [
    ...install,
    brainPlan(brain),
    t("Write {name}'s name and tone to Hermes' SOUL.md (one that is already there is backed up first).", { name }),
    ...(packs ? [t("Install the skill packs you ticked with Hermes' own command (hermes skills install): {packs}.", { packs })] : []),
    house
      ? t("Start Hermes on this computer only ({address}) with a private key, whenever Coucou runs, and connect the chat, the phone and Mochi's home to it.", { address: status.address })
      : t("Start Hermes on this computer only ({address}) with a private key, whenever Coucou runs, and connect the chat and the phone to it.", { address: status.address }),
  ];
}

/** The ChatGPT sign-in in words. */
export function loginText(login: HermesLogin): string {
  switch (login.state) {
    case "signedIn":
      return t("Signed in with your ChatGPT account.");
    case "waiting":
      return t("Waiting for you on OpenAI's page…");
    case "failed":
      return t("The ChatGPT sign-in did not go through: {reason}", { reason: login.reason });
    case "signedOut":
      return t("Not signed in to ChatGPT: Hermes cannot answer until you sign in.");
    default:
      return t("Hermes uses your ChatGPT subscription (Plus or Pro): sign in with your account.");
  }
}

/** The API line of the diagnosis: is the gateway answering, and as Hermes. */
export function healthApiText(health: HermesHealth): string {
  switch (health.state) {
    case "down":
      return t("The API does not answer: the gateway is off, or nothing listens on the port yet.");
    case "foreign":
      return t("Another program answers on this port — it is not Hermes. Close it, or give Hermes another port.");
    default:
      return t("The API answers on this computer.");
  }
}

/** The authentication line of the diagnosis. */
export function healthAuthText(health: HermesHealth): string {
  switch (health.state) {
    case "ready":
      return t("The key is valid: Hermes answers as {model}.", { model: health.model });
    case "unauthorized":
      return t("The key is wrong or missing. Repair it below, or paste the API_SERVER_KEY of Hermes' .env.");
    default:
      return t("The key cannot be checked while the API does not answer.");
  }
}

/** The copyable report: plain lines, in the app's language, with no secret in them. */
export function diagnosisReport(d: HermesDiagnosis): string {
  const yes = t("yes");
  const no = t("no");
  return [
    `Coucou · Hermes — ${d.address}`,
    `${t("Installed")}: ${d.installed ? yes : no}`,
    `${t("Run by Coucou")}: ${d.managed ? yes : no}`,
    `${t("API")}: ${healthApiText(d.health)}`,
    `${t("Authentication")}: ${healthAuthText(d.health)}`,
    `${t("A Hermes service of its own")}: ${d.ownService ? yes : no}`,
  ].join("\n");
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

/** The code to type on OpenAI's page, large, with what to do with it. */
function codePanel(login: Extract<HermesLogin, { state: "waiting" }>, cancel: () => void): HTMLElement {
  const copy = h("button", { text: t("Copy code") });
  copy.addEventListener("click", () => {
    void navigator.clipboard?.writeText(login.code).then(() => {
      copy.textContent = t("Copied");
      window.setTimeout(() => (copy.textContent = t("Copy code")), 1500);
    });
  });
  const open = h("button", { class: "primary", text: t("Open the page") });
  open.addEventListener("click", () => void Bridge.openUrl(login.url));
  const stop = h("button", { text: t("Cancel") });
  stop.addEventListener("click", cancel);
  return h(
    "div",
    { class: "notice", style: "display:flex;flex-direction:column;gap:8px;border:1px solid var(--hairline)" },
    h("div", { text: t("On OpenAI's page, signed in with your ChatGPT Plus or Pro account, type this code:") }),
    h("div", { class: "path", style: "font:600 26px ui-monospace,Consolas,monospace;letter-spacing:3px;user-select:all", text: login.code }),
    h("div", { class: "row" }, copy, open, stop),
    h("div", { class: "hint", text: t("The code works for 15 minutes. Hermes keeps nothing until you finish on OpenAI's page.") }),
  );
}

const PLAN_STYLE = "margin:0;padding-left:18px;display:flex;flex-direction:column;gap:2px";

/**
 * Zheus: the assistant's name and tone, and Hermes installed, set up and run by Coucou (src-tauri/src/hermes_setup.rs)
 * with its brain (an AI key Coucou keeps, a server on this computer, or the user's ChatGPT subscription) and its skill
 * packs. The plan is on screen before the click; nothing is downloaded, installed or written before it.
 */
function zheusBlock(host: HermesHost): HTMLElement {
  const box = h("div", { style: "display:flex;flex-direction:column;gap:6px" });
  const name = h("input", { type: "text", placeholder: "Zheus", style: "flex:1 1 auto;min-width:0", maxlength: "40", spellcheck: "false", autocomplete: "off" }) as HTMLInputElement;
  const tone = h("select", {}) as HTMLSelectElement;
  for (const id of Object.keys(PERSONAS) as (keyof typeof PERSONAS)[]) tone.append(h("option", { value: id, text: PERSONAS[id] }));
  const own = h("textarea", { rows: "3", maxlength: String(MAX_TONE), spellcheck: "true", style: AREA_STYLE, placeholder: t("In your own words: how it speaks, how it calls you, what it keeps in mind.") }) as HTMLTextAreaElement;
  const ownBox = h("div", {});
  const saved = h("div", {});
  const save = h("button", { text: t("Save") }) as HTMLButtonElement;
  const state = h("div", { style: "display:flex;flex-direction:column;gap:6px" });
  let last: HermesSetupStatus | null = null;
  let chosen = "";
  let house = true;
  let configure = false;
  /** The name or the tone changed and was not saved yet: what Rust says does not overwrite it. */
  let edited = false;
  /** "Sign in with ChatGPT" was clicked and the code is not there yet. */
  let signingIn = false;

  const fail = (err: unknown) => state.append(h("div", { class: "notice err", text: errorText(err) }));
  const showOwn = () => ownBox.replaceChildren(...(tone.value === OWN_TONE ? [own] : []));
  const fill = (assistant: { name: string; persona: string; personaText: string }) => {
    // Rust says "Zheus" when no name was given: the empty field already shows it.
    if (!(assistant.name === "Zheus" && name.value.trim() === "")) name.value = assistant.name;
    tone.value = assistant.persona in PERSONAS ? assistant.persona : "friendly";
    own.value = assistant.personaText;
    showOwn();
  };
  const initial = host.settings();
  fill({ name: initial.assistantName ?? "", persona: initial.assistantPersona ?? "", personaText: initial.assistantPersonaText ?? "" });
  const touched = () => {
    edited = true;
    saved.replaceChildren();
  };
  name.addEventListener("input", touched);
  own.addEventListener("input", touched);
  tone.addEventListener("change", () => {
    touched();
    showOwn();
  });
  save.addEventListener("click", () => {
    save.disabled = true;
    Bridge.hermesAssistantSet(name.value.trim(), tone.value, own.value).then(
      (status) => {
        edited = false;
        save.disabled = false;
        const managed = status?.managed ?? false;
        saved.replaceChildren(h("div", { class: "notice ok", text: managed ? t("Saved: Hermes uses it from your next message.") : t("Saved: Hermes gets it when Coucou sets it up.") }));
        show(status);
      },
      (err) => {
        save.disabled = false;
        saved.replaceChildren(h("div", { class: "notice err", text: errorText(err) }));
      },
    );
  });

  const brainPicker = (status: HermesSetupStatus) => {
    const select = h("select", {}) as HTMLSelectElement;
    const ready = status.brains.filter((b) => b.ready);
    for (const b of status.brains) {
      const option = h("option", { value: b.id, text: b.ready ? brainName(b) : t("{name} (add its key in Settings first)", { name: b.name }) }) as HTMLOptionElement;
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

  /** The ChatGPT sign-in: where it is, and the button that moves it on. */
  const chatgptLine = (status: HermesSetupStatus): HTMLElement[] => {
    const login = status.login;
    if (login.state === "waiting") return [codePanel(login, () => void Bridge.hermesLoginCancel().then(show, fail))];
    if (login.state === "signedIn") {
      const out = h("button", { text: t("Sign out") }) as HTMLButtonElement;
      out.addEventListener("click", () => {
        out.disabled = true;
        Bridge.hermesLogout().then(show, (err) => {
          out.disabled = false;
          fail(err);
        });
      });
      return [h("div", { class: "row" }, statusDot(true), h("span", { class: "hint", text: loginText(login) }), out)];
    }
    const start = h("button", { class: "primary", text: signingIn ? t("Asking OpenAI for a sign-in code…") : t("Sign in with ChatGPT") }) as HTMLButtonElement;
    start.disabled = signingIn;
    start.addEventListener("click", () => {
      signingIn = true;
      start.disabled = true;
      start.textContent = t("Asking OpenAI for a sign-in code…");
      Bridge.hermesLoginStart().then(show, (err) => {
        signingIn = false;
        start.disabled = false;
        start.textContent = t("Sign in with ChatGPT");
        fail(err);
      });
    });
    return [
      h("div", { class: "row" }, statusDot(false), h("span", { class: "hint", text: loginText(login) })),
      h("div", { class: "row" }, start),
      h("div", { class: "hint", text: t("Coucou opens OpenAI's page and shows a code to type there. The sign-in stays in Hermes' own files; Coucou never sees your password.") }),
    ];
  };

  /** The skill packs (Mochi's home has its own switch): ticked ones are installed with Hermes. */
  const skillsBlock = (status: HermesSetupStatus): HTMLElement[] => {
    const boxes: [string, HTMLInputElement][] = [];
    const rows = status.skillPacks
      .filter((id): id is keyof typeof PACKS => id in PACKS)
      .map((id) => {
        const tick = h("input", { type: "checkbox" }) as HTMLInputElement;
        tick.checked = status.skills.includes(id);
        boxes.push([id, tick]);
        tick.addEventListener("change", () => {
          Bridge.hermesSkillsSet(boxes.filter(([, b]) => b.checked).map(([pack]) => pack)).then(show, fail);
        });
        return h("label", { class: "row" }, tick, h("span", { text: PACKS[id] }));
      });
    return [
      h("div", { class: "hint", text: status.managed ? t("Skill packs: one you tick is installed in Hermes at once (hermes skills install); unticking one removes only what Coucou installed.") : t("Skill packs to install with Hermes:") }),
      ...rows,
      h("div", { class: "hint", text: t("Already included with Hermes: Google Workspace, Notion, maps, Obsidian, e-mail triage and reminders.") }),
    ];
  };

  /** The subscription's models (ChatGPT's, best first), or one typed: Hermes is given it and starts again. */
  const modelBlock = (status: HermesSetupStatus): HTMLElement[] => {
    if (status.models.length === 0) return [];
    const select = h("select", {}) as HTMLSelectElement;
    for (const model of status.models) select.append(h("option", { value: model, text: model }));
    select.append(h("option", { value: OTHER_MODEL, text: t("Another model…") }));
    const typed = h("input", { type: "text", placeholder: "gpt-…", maxlength: "100", spellcheck: "false", autocomplete: "off", style: "flex:1 1 auto;min-width:0" }) as HTMLInputElement;
    const known = status.models.includes(status.model);
    select.value = known ? status.model : OTHER_MODEL;
    typed.value = known ? "" : status.model;
    const typedRow = h("div", { class: "row" }, typed);
    const apply = h("button", { text: t("Use this model") }) as HTMLButtonElement;
    const picked = () => (select.value === OTHER_MODEL ? typed.value.trim() : select.value);
    const refresh = () => {
      typedRow.style.display = select.value === OTHER_MODEL ? "" : "none";
      apply.disabled = !picked() || picked() === status.model;
    };
    select.addEventListener("change", refresh);
    typed.addEventListener("input", refresh);
    apply.addEventListener("click", () => {
      apply.disabled = true;
      Bridge.hermesModelSet(picked()).then(show, (err) => {
        apply.disabled = false;
        fail(err);
      });
    });
    refresh();
    return [
      h("div", { class: "row" }, h("label", { text: t("Its model") }), select, apply),
      typedRow,
      h("div", { class: "hint", text: t("The models of your ChatGPT subscription, best first: which ones your plan reaches is OpenAI's call. Hermes starts again with the one you pick.") }),
    ];
  };

  /** How long Hermes thinks before each step: given with each message, no restart. */
  const speedBlock = (status: HermesSetupStatus): HTMLElement[] => {
    const select = h("select", {}) as HTMLSelectElement;
    for (const [id, name] of Object.entries(SPEEDS)) select.append(h("option", { value: id, text: name }));
    select.value = status.speed in SPEEDS ? status.speed : "";
    select.addEventListener("change", () => {
      select.disabled = true;
      Bridge.hermesSpeedSet(select.value).then(show, (err) => {
        select.disabled = false;
        fail(err);
      });
    });
    return [
      h("div", { class: "row" }, h("label", { text: t("Its speed") }), select),
      h("div", { class: "hint", text: t("Fast: Hermes thinks less before each step, so it answers and acts on the screen sooner (opening a site, playing a video, clicking). Thorough: it thinks longer before each one. From the next message on.") }),
    ];
  };

  /** Hermes may use this computer: its computer_use tool, through cua-driver (installed by Hermes' own command). */
  const computerBlock = (status: HermesSetupStatus): HTMLElement[] => [
    h("div", { class: "row" }, h("label", { text: t("Let it use this computer (screen, mouse, keyboard)") }), host.toggle(status.computerUse, (next) => void Bridge.hermesComputerUseSet(next).then(show, fail))),
    h("div", { class: "hint", text: t("Hermes then sees the screen and clicks, scrolls and types in your apps and websites, in the background, without taking your mouse: \"open YouTube and play the trailer\". Turning it on installs cua-driver, Hermes' own computer-use driver, and Hermes starts again. What it does on the screen waits for your OK (Allow, here or on your phone).") }),
  ];

  /** Another brain for the Hermes Coucou runs: its settings change and it starts again. */
  const changeBrain = (status: HermesSetupStatus): HTMLElement[] => {
    const picker = brainPicker(status);
    const target = status.brains.find((b) => b.id === chosen);
    const apply = h("button", { text: t("Give it this brain") }) as HTMLButtonElement;
    apply.disabled = !target || chosen === status.brain;
    apply.addEventListener("click", () => {
      apply.disabled = true;
      Bridge.hermesSetupStart(chosen, status.house, true).then(show, (err) => {
        apply.disabled = false;
        fail(err);
      });
    });
    const rows: HTMLElement[] = [h("div", { class: "row" }, h("label", { text: t("Change its brain") }), picker, apply)];
    if (target && chosen !== status.brain) {
      rows.push(h("ol", { class: "hint", style: PLAN_STYLE }, h("li", { text: brainPlan(target) }), h("li", { text: t("Start Hermes again with it, on this computer only.") })));
    }
    return rows;
  };

  /** The diagnosis panel: one button checks the integration live and shows each part, with Repair and Copy. */
  const diagnoseBlock = (): HTMLElement => {
    const panel = h("div", { style: "display:flex;flex-direction:column;gap:6px;margin-top:6px" });
    const result = h("div", { style: "display:flex;flex-direction:column;gap:6px" });
    const check = h("button", { text: t("Check the integration") }) as HTMLButtonElement;
    const line = (ok: boolean, label: string) => h("div", { class: "row" }, statusDot(ok), h("span", { class: "hint", text: label }));
    const draw = (d: HermesDiagnosis) => {
      check.disabled = false;
      const ready = d.health.state === "ready";
      const rows: HTMLElement[] = [
        line(d.installed, d.installed ? t("Hermes is installed on this computer.") : t("Hermes is not installed on this computer.")),
        line(d.health.state !== "down", healthApiText(d.health)),
        line(ready, healthAuthText(d.health)),
      ];
      if (d.ownService) {
        rows.push(h("div", { class: "notice warn", text: t("A start-up entry of Hermes' own was found: it may run the gateway beside Coucou's and fight for the port. Let only Coucou run it below.") }));
      }
      const actions: HTMLElement[] = [];
      if (d.managed && d.installed) {
        const repair = h("button", { text: t("Repair the key") }) as HTMLButtonElement;
        repair.addEventListener("click", () => {
          repair.disabled = true;
          Bridge.hermesRepairKey().then(draw, (err) => {
            repair.disabled = false;
            result.append(h("div", { class: "notice err", text: errorText(err) }));
          });
        });
        actions.push(repair);
      }
      if (d.ownService) {
        const detach = h("button", { text: t("Let only Coucou run Hermes") }) as HTMLButtonElement;
        detach.addEventListener("click", () => {
          detach.disabled = true;
          Bridge.hermesDetachService().then(draw, (err) => {
            detach.disabled = false;
            result.append(h("div", { class: "notice err", text: errorText(err) }));
          });
        });
        actions.push(detach);
      }
      const copy = h("button", { text: t("Copy the report") }) as HTMLButtonElement;
      copy.addEventListener("click", () => void navigator.clipboard?.writeText(diagnosisReport(d)));
      actions.push(copy);
      result.replaceChildren(...rows, h("div", { class: "row" }, ...actions));
    };
    check.addEventListener("click", () => {
      check.disabled = true;
      result.replaceChildren(h("div", { class: "hint", text: t("Checking…") }));
      Bridge.hermesDiagnose().then(draw, (err) => {
        check.disabled = false;
        result.replaceChildren(h("div", { class: "notice err", text: errorText(err) }));
      });
    });
    panel.append(h("div", { class: "row" }, h("label", { text: t("Diagnosis") }), check), result);
    return panel;
  };

  const show = (status: HermesSetupStatus | null) => {
    if (!status) return;
    last = status;
    if (status.login.state !== "unknown") signingIn = false;
    if (!edited && status.assistant) fill(status.assistant);
    state.replaceChildren();
    const p = status.progress;
    if (p.running) {
      const stop = h("button", { text: t("Stop") });
      stop.addEventListener("click", () => void Bridge.hermesSetupCancel().then(show, () => {}));
      state.append(
        h("div", { class: "row" }, statusDot(true), h("span", { text: progressText(p) }), stop),
        ...(p.last ? [h("div", { class: "hint path", text: p.last })] : []),
        ...(status.login.state === "waiting" ? [codePanel(status.login, () => void Bridge.hermesLoginCancel().then(show, fail))] : []),
      );
      return;
    }
    if (p.error) state.append(h("div", { class: "notice err", text: p.error }));
    if (status.managed) {
      const on = status.gateway.state !== "off";
      const sw = host.toggle(on, (next) => void Bridge.hermesGatewaySet(next).then(show, fail));
      const brain = status.brains.find((b) => b.id === status.brain);
      state.append(
        h("div", { class: "row" }, h("label", { text: t("Run Hermes with Coucou") }), sw),
        h("div", { class: "row" }, statusDot(status.gateway.state === "running"), h("span", { class: "hint", text: gatewayText(status.gateway) })),
        ...(brain ? [h("div", { class: "hint", text: brain.login ? t("Its brain: ChatGPT, with your subscription.") : t("Its brain: {brain}, with the key Coucou keeps.", { brain: brain.name }) })] : []),
        ...(brain?.login ? chatgptLine(status) : []),
        ...modelBlock(status),
        ...speedBlock(status),
        ...changeBrain(status),
        ...computerBlock(status),
        ...skillsBlock(status),
        diagnoseBlock(),
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
        fail(err);
      });
    });
    const rows: HTMLElement[] = [h("div", { class: "row" }, h("label", { text: t("Its brain") }), picker)];
    if (brain.login) {
      rows.push(h("div", { class: "hint", text: t("Needs ChatGPT Plus or Pro. During the setup Coucou opens OpenAI's page and shows a code: sign in there with your ChatGPT account. No key needed.") }));
    }
    if (status.installed) {
      const configureBox = h("input", { type: "checkbox" }) as HTMLInputElement;
      configureBox.checked = configure;
      configureBox.addEventListener("change", () => {
        configure = configureBox.checked;
        show(last);
      });
      rows.unshift(h("div", { class: "hint", text: t("Hermes is already on this computer ({path}): Coucou can run it, without changing its settings unless you tick it below.", { path: status.home }) }));
      rows.push(h("label", { class: "row" }, configureBox, h("span", { text: t("Also give it this brain (its model and provider change)") })));
    }
    rows.push(h("label", { class: "row" }, houseBox, h("span", { text: t("Let it use Mochi's home (the devices you ticked, with the same rules)") })));
    // Nothing is changed in a Hermes that is already there unless asked: no plan then, and no skill packs, which only a
    // setup that changes Hermes installs (once Coucou runs it, a pack ticked is installed at once).
    const changes = !status.installed || configure;
    if (changes) rows.push(...skillsBlock(status));
    const plan = h("ol", { class: "hint", style: PLAN_STYLE }, ...zheusPlan(status, brain, house, status.skills, status.assistant?.name).map((line) => h("li", { text: line })));
    state.append(
      ...rows,
      ...(changes ? [h("div", { class: "hint", text: t("What Coucou will do:") }), plan] : []),
      h("div", { class: "row" }, install),
      h("div", { class: "hint", text: t("Hermes Agent is free and open source (MIT), by Nous Research. Its own tools then ask you before anything risky, here or on your phone.") }),
    );
  };

  box.append(
    h("div", { class: "row" }, h("label", { text: t("Your assistant's name") }), name),
    h("div", { class: "hint", text: t("Hermes is told it answers to this name, here and on your phone: \"Zheus, what's on my calendar today?\"") }),
    h("div", { class: "row" }, h("label", { text: t("Its tone") }), tone),
    ownBox,
    h("div", { class: "row" }, h("span", { class: "hint", text: t("Coucou writes the name and tone to Hermes' SOUL.md when it runs Hermes, and tells Hermes with each message.") }), save),
    saved,
    state,
  );
  void Bridge.hermesSetupStatus().then(show);
  // The installation, the sign-in and the gateway move on by themselves: kept up to date while the section is shown.
  const watch = window.setInterval(() => {
    if (!box.isConnected) return window.clearInterval(watch);
    if (document.visibilityState !== "visible") return;
    const moving = !last || last.progress.running || last.gateway.state === "starting" || (last.managed && last.gateway.state === "off") || last.login.state === "waiting" || signingIn;
    if (!moving) return;
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
  // id: the installer's page brings Settings here (settings-focus, src/settings/main.ts).
  const el = h(
    "section",
    { id: "hermes" },
    h("h2", {}, h("span", { text: t("Hermes Agent") })),
    h("div", { class: "hint", text: t("Let Hermes Agent answer the chat, here and on your phone, with its own tools and memory: your calendar, Spotify, your home, scheduled tasks. Hermes runs on this computer; Coucou shows each thing it does and asks you before what it wants approved.") }),
    zheusBlock(host),
    body,
    houseBlock(host.toggle),
  );
  return { el, redraw };
}
