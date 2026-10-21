// Zheus One's TV mode (src-tauri/src/zheus_one: open_tv): "show our situation" turns the settings window into a full
// screen of glass — on the TV or projector plugged into this computer when there is one — with the house, the plan
// under way, the decisions waiting for the user, and the assistant's overview. Esc, or Close, gives the window back.
// It reads the house every half minute while it is on screen, and nothing at all once it is closed.

import { Bridge, onEvent } from "../core/bridge";
import {
  clock, presenceLine, stepStates, tiles, when, type Suggestion, type Tile, type World, type ZheusStatus,
} from "../core/zheus";
import { degreesText } from "../core/home";
import { classBars, incomeProgress, money, percent, pricedLine, type PowersView } from "../core/powers";
import { language, t } from "../i18n/i18n";
import { h, svg } from "../views/dom";
import { ICONS } from "../views/icons";
import { renderMarkdown } from "../views/markdown";

const TILE_ICONS: Record<Tile["kind"], string> = {
  vacuum: ICONS.vacuum,
  climate: ICONS.snowflake,
  light: ICONS.bulb,
  tv: ICONS.tv,
};

/** Wires the TV mode into the settings window: shown on Rust's word, closed by Esc or its button. */
export function wireTvMode(root: HTMLElement) {
  const screen = h("div", { class: "zheus-tv", hidden: true });
  document.body.append(screen);
  let status: ZheusStatus | null = null;
  let world: World | null = null;
  let powers: PowersView | null = null;
  let timer = 0;

  const close = () => void Bridge.zheusTv(false);

  const read = async () => {
    status = (await Bridge.zheusStatus()) ?? status;
    world = await Bridge.zheusWorld(true).catch(() => world);
    powers = (await Bridge.powersStatus()) ?? powers;
    draw();
  };

  const show = (on: boolean) => {
    screen.hidden = !on;
    root.hidden = on;
    window.clearInterval(timer);
    if (on) {
      void read();
      timer = window.setInterval(() => void read(), 30_000);
    }
  };

  const tileEl = (tile: Tile) =>
    h(
      "div",
      { class: `zheus-glass zheus-tv-tile ${tile.kind}${tile.on ? " on" : ""}` },
      h("div", { class: "zheus-tv-icon" }, svg(TILE_ICONS[tile.kind], 26, { stroke: 1.6 })),
      h("div", { class: "zheus-tv-tile-title", text: tile.title }),
      h("div", { class: "zheus-tv-tile-line", text: tile.line }),
    );

  const decision = (s: Suggestion) =>
    h(
      "div",
      { class: "zheus-tv-decision" },
      h("span", { text: s.text }),
      h(
        "div",
        { class: "zheus-tv-buttons" },
        h("button", { text: t("No"), onclick: () => void Bridge.zheusSuggestion(s.key, false).then(read) }),
        h("button", { class: "primary", text: t("Yes"), onclick: () => void Bridge.zheusSuggestion(s.key, true).then(read) }),
      ),
    );

  const draw = () => {
    if (screen.hidden) return;
    const lang = language();
    const now = world ? clock(world.now) : "";
    const plan = status?.pending ?? status?.running?.plan ?? null;
    const states = stepStates(status?.running ?? null);
    const rooms = (world?.rooms ?? []).filter((r) => r.temperature != null || r.humidity != null);

    const house = h(
      "div",
      { class: "zheus-tv-col" },
      h("h2", { text: t("The house") }),
      h("div", { class: "zheus-tv-tiles" }, ...(world ? tiles(world, lang) : []).map(tileEl)),
      rooms.length
        ? h(
            "div",
            { class: "zheus-glass zheus-tv-rooms" },
            ...rooms.map((r) =>
              h(
                "div",
                { class: "zheus-tv-room" },
                h("span", { text: r.name }),
                h("span", { text: [r.temperature != null ? degreesText(r.temperature, lang) : "", r.humidity != null ? `${r.humidity}%` : ""].filter(Boolean).join(" · ") }),
              ),
            ),
          )
        : null,
      world?.facts.length ? h("div", { class: "zheus-tv-facts" }, ...world.facts.map((f) => h("div", { text: f }))) : null,
    );

    const mission = h(
      "div",
      { class: "zheus-tv-col" },
      h("h2", { text: plan ? (status?.running ? t("Preparing your arrival") : t("Your arrival: the plan")) : t("Decisions waiting") }),
      plan
        ? h(
            "div",
            { class: "zheus-glass zheus-tv-plan" },
            h("div", { class: "zheus-tv-sub", text: t("around {time} · in about {minutes} min", { time: clock(plan.arriveAt), minutes: plan.eta }) }),
            ...plan.steps.map((s, i) =>
              h(
                "div",
                { class: `zheus-tv-step ${states[i] ?? "waiting"}` },
                h("span", { class: "zheus-when", text: when(plan, s) }),
                h("span", { text: s.label }),
              ),
            ),
            ...plan.conflicts.map((c) => h("div", { class: "zheus-conflict", text: `⚠ ${c}` })),
          )
        : null,
      ...(status?.suggestions ?? []).map(decision),
      !plan && !status?.suggestions.length ? h("div", { class: "zheus-tv-quiet", text: t("Nothing is waiting for you.") }) : null,
    );

    const brief = h("div", { class: "zheus-glass zheus-tv-brief" });
    if (status?.briefing?.text) renderMarkdown(brief, status.briefing.text);
    else brief.append(h("div", { class: "zheus-tv-quiet", text: t("Zheus is preparing the overview…") }));

    // Zheus' superpowers, while they are on: the portfolio against its targets, and the day.
    const money_ = powers?.on && powers.summary.hasPortfolio ? powersPanel(powers) : null;

    screen.replaceChildren(
      h(
        "div",
        { class: "zheus-tv-head" },
        h("div", { class: "zheus-tv-title" }, svg(ICONS.sparkles, 22), h("span", { text: t("Our situation") })),
        h("div", { class: "zheus-tv-presence", text: world ? presenceLine(world) : "" }),
        h("div", { class: "zheus-tv-clock", text: now }),
        h("button", { class: "zheus-tv-close", text: t("Close"), onclick: close }),
      ),
      h("div", { class: "zheus-tv-grid" }, house, mission, h("div", { class: "zheus-tv-col" }, h("h2", { text: t("Zheus' overview") }), brief), money_),
    );
  };

  const powersPanel = (v: PowersView) => {
    const s = v.summary;
    const progress = incomeProgress(s);
    const lines = [...v.alerts, ...v.today].sort((a, b) => Number(b.urgent) - Number(a.urgent)).slice(0, 6);
    return h(
      "div",
      { class: "zheus-glass zheus-tv-powers" },
      h(
        "div",
        {},
        h("h3", { text: t("Your money") }),
        h("div", { class: "powers-value", text: money(s.value) }),
        h("div", { class: "zheus-tv-sub", text: pricedLine(s) }),
        h(
          "div",
          { class: "powers-bars" },
          ...classBars(s.classes).slice(0, 6).map((b) =>
            h(
              "div",
              { class: `powers-bar ${b.side}` },
              h("span", { class: "powers-bar-label", text: b.label }),
              h(
                "span",
                { class: "powers-bar-track" },
                h("i", { class: "powers-bar-fill", style: `width:${b.width.toFixed(1)}%` }),
                b.mark == null ? null : h("i", { class: "powers-bar-mark", style: `left:${b.mark.toFixed(1)}%` }),
              ),
              h("span", { class: "powers-bar-value", text: percent(b.weight, 0) }),
            ),
          ),
        ),
        h(
          "div",
          { class: "powers-income" },
          h("span", { class: "powers-income-label", text: t("Passive income") }),
          h("span", { class: "powers-income-value", text: t("{amount}/month", { amount: money(s.income) }) }),
          progress == null ? null : h("span", { class: "powers-income-track" }, h("i", { style: `width:${progress.toFixed(1)}%` })),
        ),
      ),
      h(
        "div",
        {},
        h("h3", { text: t("Your day") }),
        ...(lines.length
          ? lines.map((l) => h("div", { class: `powers-line ${l.area}${l.urgent ? " urgent" : ""}` }, h("i", { class: "powers-dot" }), h("span", { text: l.text })))
          : [h("div", { class: "zheus-tv-quiet", text: t("Nothing to watch today.") })]),
      ),
    );
  };

  void onEvent<boolean>("zheus-tv", show);
  void onEvent<ZheusStatus>("zheus-changed", (s) => {
    status = s;
    draw();
  });
  void onEvent<PowersView>("powers-changed", (v) => {
    powers = v;
    draw();
  });
  document.addEventListener("keydown", (e) => {
    if (!screen.hidden && e.key === "Escape") close();
  });
  // Opened before this page listened (the first time the window is shown): ask.
  void Bridge.zheusStatus().then((s) => {
    if (s?.tv) show(true);
  });
}
