// Zheus One in the island (src-tauri/src/zheus_one): the house as glass tiles — the vacuum, the air conditioning,
// the lights, the TV — with a click for each; the missions ("Prepare my arrival" with the minutes of the way home,
// "Our situation" on the TV); the Autopilot's suggestions; and a plan's Foresight — its steps and their times, what
// clashes and the ways around it — waiting for Approve, then running step by step. Nothing here acts without a click.

import { Bridge } from "../core/bridge";
import { State } from "../core/state";
import { Sound } from "../core/sound";
import {
  ETAS, clock, presenceLine, stepStates, tiles, when, type Plan, type Suggestion, type Tile, type World, type ZheusStatus,
} from "../core/zheus";
import { language, t, tl } from "../i18n/i18n";
import { h, svg } from "./dom";
import { ICONS } from "./icons";
import type { ViewActions, ViewHost } from "./views";

const TILE_ICONS: Record<Tile["kind"], string> = {
  vacuum: ICONS.vacuum,
  climate: ICONS.snowflake,
  light: ICONS.bulb,
  tv: ICONS.tv,
};

const errorText = (err: unknown) => String(err).replace(/^Error:\s*/, "");

export function buildZheus(actions: ViewActions): ViewHost {
  const top = h("div", { class: "zheus-top" });
  const main = h("div", { class: "zheus-main" });
  const foot = h("div", { class: "zheus-foot" });
  const body = h("div", { class: "zheus-body" }, top, main, foot);
  const card = h("div", { class: "card wash zheus-card" }, body);
  card.style.setProperty("--wash", "rgba(56,189,248,0.42)");
  const el = h("div", { class: "view" }, card);

  let world: World | null = null;
  let loading = false;
  let note = "";
  let wasOn = false;
  let drawn = "";
  let refreshTimer = 0;
  /** The running plan's outcomes when last seen: a new one means the house changed. */
  let lastRun = "";

  async function refresh(fresh = false) {
    if (loading) return;
    loading = true;
    try {
      world = await Bridge.zheusWorld(fresh);
      note = "";
    } catch (err) {
      note = errorText(err);
    } finally {
      loading = false;
      draw(true);
    }
  }

  /** The house changed (an action, a plan's step): read it again in a moment, once. */
  function refreshSoon(ms = 900) {
    window.clearTimeout(refreshTimer);
    refreshTimer = window.setTimeout(() => void refresh(true), ms);
  }

  async function run(work: () => Promise<unknown>) {
    actions.blip();
    try {
      await work();
      note = "";
    } catch (err) {
      note = errorText(err);
      Sound.play("error");
    }
    draw(true);
  }

  // ── The top: where the user is, and the missions ──
  function drawTop(status: ZheusStatus) {
    const etas = h("div", { class: "zheus-etas" });
    for (const m of ETAS) {
      etas.append(
        h("button", {
          class: "zheus-chip",
          text: `${m} min`,
          title: t("Prepare my arrival in {minutes} min", { minutes: m }),
          onclick: () => void run(() => Bridge.zheusPlan(m)),
        }),
      );
    }
    const busy = !!(status.pending || status.running);
    top.replaceChildren(
      h("div", { class: "zheus-presence" }, svg(ICONS.sparkles, 12), h("span", { text: world ? presenceLine(world) : t("The house") })),
      h(
        "div",
        { class: "zheus-missions" },
        busy ? null : h("span", { class: "zheus-label", text: t("Prepare my arrival") }),
        busy ? null : etas,
        h("button", {
          class: "zheus-chip tv",
          title: t("Our situation, full screen on the TV"),
          onclick: () => void run(() => Bridge.zheusTv(!status.tv)),
        }, svg(ICONS.tv, 12, { stroke: 1.7 }), h("span", { text: status.tv ? t("Close the TV") : t("Situation") })),
      ),
    );
  }

  // ── The tiles ──
  function tileEl(tile: Tile): HTMLElement {
    const buttons = tile.actions.map((a) =>
      h("button", {
        class: a.label === "−" || a.label === "+" ? "zheus-mini round" : "zheus-mini",
        text: a.label,
        onclick: () =>
          void run(async () => {
            for (const id of a.targets) {
              const out = await Bridge.zheusDevice(id, a.action, a.value ?? null);
              if (!out.ok) throw new Error(out.detail);
            }
            refreshSoon();
          }),
      }),
    );
    return h(
      "div",
      { class: `zheus-tile ${tile.kind}${tile.on ? " on" : ""}` },
      h("div", { class: "zheus-tile-icon" }, svg(TILE_ICONS[tile.kind], 15, { stroke: 1.7 })),
      h("div", { class: "zheus-tile-title", text: tile.title, title: tile.title }),
      h("div", { class: "zheus-tile-line", text: tile.line, title: tile.line }),
      h("div", { class: "zheus-tile-actions" }, ...buttons),
    );
  }

  function drawTiles() {
    const list = world ? tiles(world, language()) : [];
    if (!world) {
      main.replaceChildren(h("div", { class: "zheus-empty", text: loading ? t("Looking at the house…") : note || t("Looking at the house…") }));
      return;
    }
    if (!list.length) {
      main.replaceChildren(
        h("div", { class: "zheus-empty", text: t("No device is shared with Zheus yet. Tick the ones it may use in Settings → Mochi's home.") }),
      );
      return;
    }
    main.replaceChildren(h("div", { class: "zheus-tiles" }, ...list.map(tileEl)));
  }

  // ── A plan: waiting for Approve, or running ──
  function drawPlan(plan: Plan, running: boolean, status: ZheusStatus) {
    const states = running ? stepStates(status.running) : [];
    const rows = plan.steps.map((s, i) => {
      const state = states[i] ?? "waiting";
      const mark = !running ? h("i", { class: "zheus-dot" }) : state === "done" ? svg(ICONS.check, 11, { stroke: 2.2 }) : state === "failed" ? svg(ICONS.xmark, 10) : h("i", { class: "zheus-dot" });
      const outcome = running ? status.running?.done[i] : null;
      return h(
        "div",
        { class: `zheus-step ${state}`, title: s.why || s.label },
        h("span", { class: "zheus-when", text: when(plan, s) }),
        h("span", { class: "zheus-mark" }, mark),
        h("span", { class: "zheus-what", text: s.label }),
        outcome && !outcome.ok && outcome.detail ? h("span", { class: "zheus-why", text: outcome.detail }) : s.asks ? h("span", { class: "zheus-why", text: t("asks first") }) : null,
      );
    });
    const conflicts = plan.conflicts.map((c) => h("div", { class: "zheus-conflict" }, svg(ICONS.bang, 10), h("span", { text: c })));
    const choices = !running && plan.choices.length > 1
      ? h(
          "div",
          { class: "zheus-choices" },
          ...plan.choices.map((c) =>
            h("button", {
              class: c.id === plan.chosen ? "zheus-choice on" : "zheus-choice",
              text: c.label,
              title: c.label,
              onclick: () => void run(() => Bridge.zheusChoose(c.id)),
            }),
          ),
        )
      : null;
    const buttons = running
      ? [h("button", { class: "btn secondary", onclick: () => void run(() => Bridge.zheusCancel()) }, h("span", { text: tl("Stop") }))]
      : [
          h("button", { class: "btn secondary", onclick: () => void run(() => Bridge.zheusCancel()) }, h("span", { text: tl("Cancel") })),
          h("button", { class: "btn primary", onclick: () => void run(() => Bridge.zheusApprove(plan.id)) }, h("span", { text: tl("Approve") })),
        ];
    main.replaceChildren(
      h(
        "div",
        { class: `zheus-plan${running ? " running" : ""}` },
        h(
          "div",
          { class: "zheus-plan-head" },
          h("span", { class: "zheus-plan-title", text: running ? t("Preparing your arrival") : t("Your arrival: the plan") }),
          h("span", { class: "zheus-plan-sub", text: t("around {time} · in about {minutes} min", { time: clock(plan.arriveAt), minutes: plan.eta }) }),
        ),
        h("div", { class: "zheus-steps" }, ...rows),
        ...conflicts,
        choices,
        h("div", { class: "zheus-plan-actions" }, ...buttons),
      ),
    );
  }

  // ── The foot: the Autopilot's suggestions, else what ties the house together ──
  function suggestionEl(s: Suggestion): HTMLElement {
    return h(
      "div",
      { class: "zheus-suggestion" },
      svg(ICONS.sparkles, 10),
      h("span", { class: "zheus-suggestion-text", text: s.text, title: s.text }),
      h("button", { class: "zheus-mini", text: t("No"), onclick: () => void run(() => Bridge.zheusSuggestion(s.key, false)) }),
      h("button", { class: "zheus-mini yes", text: t("Yes"), onclick: () => void run(async () => { await Bridge.zheusSuggestion(s.key, true); refreshSoon(); }) }),
    );
  }

  function drawFoot(status: ZheusStatus) {
    if (note) {
      foot.replaceChildren(h("div", { class: "zheus-note", text: note }));
    } else if (status.pending || status.running) {
      // The plan takes the room.
      foot.replaceChildren();
    } else if (status.suggestions.length) {
      foot.replaceChildren(suggestionEl(status.suggestions[status.suggestions.length - 1]));
    } else if (world?.facts.length) {
      foot.replaceChildren(h("div", { class: "zheus-facts", text: world.facts.slice(0, 2).join(" ") }));
    } else {
      foot.replaceChildren();
    }
  }

  function drawOff(status: ZheusStatus | null) {
    top.replaceChildren(h("div", { class: "zheus-presence" }, svg(ICONS.sparkles, 12), h("span", { text: "Zheus One" })));
    const text = !status?.enabled
      ? t("Zheus One prepares your house and your day as one mission: say \"I'm on my way home\" and it shows you the plan. Turn it on in Settings → Zheus One.")
      : t("Turn Mochi's home on first, in Settings → Mochi's home: Zheus One works with the devices you share there.");
    main.replaceChildren(
      h(
        "div",
        { class: "zheus-empty" },
        h("span", { text }),
        h("button", { class: "btn secondary", onclick: () => actions.openSettingsWindow() }, h("span", { text: tl("Open Settings") })),
      ),
    );
    foot.replaceChildren();
  }

  function draw(force = false) {
    const status = State.zheus;
    const key = JSON.stringify([status, world?.now, world?.facts, world?.devices.map((d) => [d.id, d.state]), note, loading]);
    if (!force && key === drawn) return;
    drawn = key;
    if (!status?.enabled || !status.homeOn) {
      drawOff(status);
      return;
    }
    drawTop(status);
    if (status.pending) drawPlan(status.pending, false, status);
    else if (status.running) drawPlan(status.running.plan, true, status);
    else drawTiles();
    drawFoot(status);
  }

  return {
    el,
    sync() {
      const on = State.view === "zheus" && State.mode === "expanded";
      if (on && !wasOn) void refresh(true);
      // A plan's step was carried out: the house changed, read it again.
      const run = JSON.stringify(State.zheus?.running?.done ?? null);
      if (on && run !== lastRun && lastRun !== "") refreshSoon(1200);
      lastRun = run;
      wasOn = on;
      if (on) draw();
    },
  };
}
