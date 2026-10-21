// Zheus' superpowers in the island (src-tauri/src/powers): the portfolio's value and its classes against the user's
// targets, the passive income against its goal, the alerts and the day's lines, the official data's last reading,
// and questions for Zheus a click away (they open the chat with the question written, for the user to send). Nothing
// here buys, sells or sends anything.

import { Bridge } from "../core/bridge";
import { classBars, incomeProgress, money, percent, pricedLine, quickQuestions, refreshLine, type PowersView } from "../core/powers";
import { Sound } from "../core/sound";
import { State } from "../core/state";
import { t, tl } from "../i18n/i18n";
import { h, svg } from "./dom";
import { ICONS } from "./icons";
import type { ViewActions, ViewHost } from "./views";

/** How many classes and lines of the day fit. */
const MAX_BARS = 5;
const MAX_LINES = 5;

const errorText = (err: unknown) => String(err).replace(/^Error:\s*/, "");

export function buildPowers(actions: ViewActions): ViewHost {
  const top = h("div", { class: "zheus-top powers-top" });
  const main = h("div", { class: "powers-main" });
  const foot = h("div", { class: "zheus-foot powers-foot" });
  const body = h("div", { class: "zheus-body" }, top, main, foot);
  const card = h("div", { class: "card wash zheus-card powers-card" }, body);
  card.style.setProperty("--wash", "rgba(52,211,153,0.36)");
  const el = h("div", { class: "view" }, card);

  let note = "";
  let drawn = "";

  async function refresh() {
    actions.blip();
    try {
      State.powers = await Bridge.powersRefresh();
      note = "";
    } catch (err) {
      note = errorText(err);
      Sound.play("error");
    }
    draw(true);
  }

  /** The chat, with the question written for the user to send. */
  function ask(question: string) {
    actions.blip();
    State.chatDraft = question;
    actions.setView("prompt");
  }

  function drawOff() {
    top.replaceChildren(h("div", { class: "zheus-presence" }, svg(ICONS.star, 12), h("span", { text: t("Zheus' superpowers") })));
    main.replaceChildren(
      h(
        "div",
        { class: "zheus-empty" },
        h("span", { text: t("Zheus follows your portfolio against B3's and the CVM's data, and your studies, English and training. Turn the superpowers on in Settings → Zheus' superpowers.") }),
        h("button", { class: "btn secondary", onclick: () => actions.openSettingsWindow() }, h("span", { text: tl("Open Settings") })),
      ),
    );
    foot.replaceChildren();
  }

  function drawTop(v: PowersView) {
    const s = v.summary;
    const value = s.hasPortfolio
      ? h("span", { class: "powers-value", text: money(s.value) })
      : h("span", { class: "powers-value dim", text: t("No portfolio yet") });
    top.replaceChildren(
      h("div", { class: "zheus-presence" }, svg(ICONS.star, 12), value),
      h("span", { class: "powers-priced", text: s.hasPortfolio ? pricedLine(s) : "" }),
      h(
        "button",
        {
          class: `zheus-chip${v.refresh.running ? " busy" : ""}`,
          title: refreshLine(v.refresh),
          disabled: v.refresh.running,
          onclick: () => void refresh(),
        },
        svg(ICONS.arrowClockwise, 11, { stroke: 1.8 }),
        h("span", { text: v.refresh.running ? t("Reading…") : t("Refresh") }),
      ),
    );
  }

  function drawPortfolio(v: PowersView): HTMLElement {
    const s = v.summary;
    if (!s.hasPortfolio) {
      return h(
        "div",
        { class: "powers-col powers-empty" },
        h("div", { class: "powers-col-title", text: t("Portfolio") }),
        h("div", { class: "powers-hint", text: t("Paste your portfolio in the chat (from Investidor10 or your broker), or import B3's file in Settings.") }),
      );
    }
    const bars = classBars(s.classes).slice(0, MAX_BARS).map((b) =>
      h(
        "div",
        { class: `powers-bar ${b.side}`, title: b.target == null ? percent(b.weight) : t("{weight} of the portfolio, target {target}", { weight: percent(b.weight), target: percent(b.target, 0) }) },
        h("span", { class: "powers-bar-label", text: b.label }),
        h(
          "span",
          { class: "powers-bar-track" },
          h("i", { class: "powers-bar-fill", style: `width:${b.width.toFixed(1)}%` }),
          b.mark == null ? null : h("i", { class: "powers-bar-mark", style: `left:${b.mark.toFixed(1)}%` }),
        ),
        h("span", { class: "powers-bar-value", text: percent(b.weight, 0) }),
      ),
    );
    const progress = incomeProgress(s);
    const income = h(
      "div",
      { class: "powers-income" },
      h("span", { class: "powers-income-label", text: t("Passive income") }),
      h("span", { class: "powers-income-value", text: t("{amount}/month", { amount: money(s.income) }) }),
      progress == null ? null : h("span", { class: "powers-income-track" }, h("i", { style: `width:${progress.toFixed(1)}%` })),
      progress == null ? null : h("span", { class: "powers-income-goal", text: t("Goal: {amount}/month", { amount: money(s.incomeGoal ?? 0, 0) }) }),
    );
    return h("div", { class: "powers-col" }, h("div", { class: "powers-bars" }, ...bars), income);
  }

  function drawDay(v: PowersView): HTMLElement {
    const lines = [...v.alerts, ...v.today].sort((a, b) => Number(b.urgent) - Number(a.urgent)).slice(0, MAX_LINES);
    const list = lines.length
      ? lines.map((l) => h("div", { class: `powers-line ${l.area}${l.urgent ? " urgent" : ""}`, title: l.text }, h("i", { class: "powers-dot" }), h("span", { text: l.text })))
      : [h("div", { class: "powers-hint", text: t("Nothing to watch today.") })];
    return h("div", { class: "powers-col" }, h("div", { class: "powers-col-title", text: t("Today") }), ...list);
  }

  function drawFoot(v: PowersView) {
    if (note) {
      foot.replaceChildren(h("div", { class: "zheus-note", text: note }));
      return;
    }
    foot.replaceChildren(
      h(
        "div",
        { class: "powers-asks" },
        ...quickQuestions(v.summary).map((q) => h("button", { class: "zheus-chip powers-ask", text: q, onclick: () => ask(q) })),
      ),
    );
  }

  function draw(force = false) {
    const v = State.powers;
    const key = JSON.stringify([v, note, State.settings.powersEnabled]);
    if (!force && key === drawn) return;
    drawn = key;
    if (!v?.on || State.settings.powersEnabled !== true) {
      drawOff();
      return;
    }
    drawTop(v);
    main.replaceChildren(drawPortfolio(v), drawDay(v));
    drawFoot(v);
  }

  return {
    el,
    sync() {
      if (State.view === "powers" && State.mode === "expanded") draw();
    },
  };
}
