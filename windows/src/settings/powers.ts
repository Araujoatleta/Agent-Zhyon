// Settings → Zheus' superpowers, in the same glass as Zheus One: the switch; whether Hermes has the tools; the
// portfolio (pasted from Investidor10 or a broker, or B3's file imported) with its targets per class, the monthly
// contribution, the passive income goal and the cash; the public exam, English and training profiles; the month's
// content kit; and the official data's last reading. Rust owns all of it (src-tauri/src/powers): the page only asks.
// Nothing here buys, sells, sends or posts anything.

import { Bridge, onEvent } from "../core/bridge";
import { CLASS_IDS, amountOf, classLabel, liveLine, money, refreshLine, sheetLine, targetsFrom, targetsTotal, wayLine, type PowersView } from "../core/powers";
import { t } from "../i18n/i18n";
import { h, svg } from "../views/dom";
import { ICONS } from "../views/icons";
import { field, glassCard, notice, number } from "./zheus";
import { mcpSnippet } from "./hermes";

type Toggle = (on: boolean, onChange: (v: boolean) => void) => HTMLElement;

const errorText = (err: unknown) => String(err).replace(/^Error:\s*/, "");

/** An amount field in reais: "R$" inside, written and read the Brazilian way ("20.924,80"). */
function amount(value: number | null): { el: HTMLElement; input: HTMLInputElement } {
  const input = h("input", { type: "text", inputmode: "decimal", placeholder: "0" }) as HTMLInputElement;
  input.value = value && value > 0 ? money(value, Number.isInteger(value) ? 0 : 2).replace(/^R\$\s*/, "") : "";
  return { el: h("label", { class: "zheus-input money" }, h("span", { class: "zheus-unit", text: "R$" }), input), input };
}

/** A number with a unit too long to sit inside the field ("h a week"): the unit beside it. */
function measure(value: number, min: number, max: number, unit: string): { el: HTMLElement; input: HTMLInputElement } {
  const n = number(value, min, max, unit);
  n.el.classList.add("unit-after");
  return n;
}

function select(options: [string, string][], value: string): HTMLSelectElement {
  const el = h("select", {}) as HTMLSelectElement;
  for (const [id, label] of options) el.append(h("option", { value: id, text: label }));
  el.value = options.some(([id]) => id === value) ? value : options[0][0];
  return el;
}

/** A card's Save button with its note: runs `save`, then says how it went. */
function saver(save: () => Promise<unknown>): { el: HTMLElement } {
  const said = h("span", { class: "zheus-saved" });
  const button = h("button", { class: "primary", text: t("Save") }) as HTMLButtonElement;
  button.addEventListener("click", () =>
    void (async () => {
      button.disabled = true;
      said.replaceChildren();
      try {
        await save();
        said.append(notice(t("Saved."), true));
      } catch (err) {
        said.append(notice(errorText(err), false));
      } finally {
        button.disabled = false;
      }
    })(),
  );
  return { el: h("div", { class: "zheus-actions" }, said, button) };
}

export function powersSection(makeToggle: Toggle): HTMLElement {
  let view: PowersView | null = null;
  /** The forms are drawn once per switch-on: the reports that come while the user types must not wipe them. */
  let formsFor = false;
  const note = h("div", {});
  const status = h("div", { class: "powers-status" });
  const forms = h("div", { class: "zheus-panel-body" });

  const onSwitch = makeToggle(false, (next) =>
    void (async () => {
      note.replaceChildren();
      try {
        view = await Bridge.powersSet(next);
        draw();
      } catch (err) {
        note.append(notice(errorText(err), false));
      }
    })(),
  );
  onSwitch.setAttribute("aria-label", t("Zheus' superpowers"));

  // ── What changes on its own: Hermes, the portfolio's line, the official data ──
  function drawStatus() {
    const v = view;
    if (!v?.on) {
      status.replaceChildren();
      return;
    }
    const p = v.profile;
    const hermes =
      v.hermes === "ready"
        ? notice(t("Zheus (Hermes) has the superpowers: just ask him in the chat, on the island or the phone."), true)
        : v.hermes === "missing"
          ? h("div", { class: "notice warn", text: t("Giving the superpowers to Hermes… (it starts again in a moment)") })
          : h(
              "div",
              { class: "powers-own-hermes" },
              h("p", { class: "hint", text: t("Mochi's own chat has them already. For a Hermes that Coucou does not run, add Coucou to its config.yaml, then restart hermes gateway:") }),
              h("pre", { class: "powers-snippet", text: mcpSnippet(v.command) }),
            );
    const portfolio = p.positions
      ? t("{count} positions · updated {date}", { count: p.positions, date: p.updated ? p.updated.split("-").reverse().join("/") : "—" })
      : t("No portfolio yet: paste it below, import B3's file, or paste it in the chat.");
    const value = v.summary.hasPortfolio ? ` · ${money(v.summary.value)}` : "";
    const problems = v.refresh.problems.length
      ? h("details", { class: "powers-problems" }, h("summary", { text: t("What did not answer") }), ...v.refresh.problems.map((x) => h("div", { class: "hint", text: x })))
      : null;
    status.replaceChildren(
      hermes,
      h(
        "div",
        { class: "powers-lines" },
        h("div", { class: "powers-line-row" }, svg(ICONS.star, 12), h("span", { text: portfolio + value })),
        ...(v.sheetLink.id ? [h("div", { class: "powers-line-row" }, svg(ICONS.doc, 12, { stroke: 1.8 }), h("span", { text: sheetLine(v) }))] : []),
        h("div", { class: "powers-line-row" }, svg(ICONS.arrowClockwise, 12, { stroke: 1.8 }), h("span", { text: liveLine(v) })),
        h("div", { class: "powers-line-row" }, svg(ICONS.arrowClockwise, 12, { stroke: 1.8 }), h("span", { text: refreshLine(v.refresh) })),
      ),
      ...(problems ? [problems] : []),
    );
    drawSheetStatus();
  }

  // ── Your Google Sheet ──
  const sheetStatus = h("div", { class: "powers-sheet-status" });
  function drawSheetStatus() {
    const v = view;
    if (!v?.sheetLink.id) {
      sheetStatus.replaceChildren();
      return;
    }
    const s = v.sheet;
    const rows = s.skipped.length
      ? [h("details", { class: "powers-problems" }, h("summary", { text: t("Rows left out ({count})", { count: s.skipped.length }) }), ...s.skipped.map((x) => h("div", { class: "hint", text: x })))]
      : [];
    sheetStatus.replaceChildren(s.problem && !s.running ? notice(s.problem, false) : h("p", { class: "hint", text: sheetLine(v) }), ...rows);
  }

  function sheetCard(v: PowersView): HTMLElement {
    const link = h("input", { type: "text", placeholder: "https://docs.google.com/spreadsheets/d/…" }) as HTMLInputElement;
    link.value = v.sheetLink.id;
    const tab = h("input", { type: "text", placeholder: t("AXYON (its own tab)") }) as HTMLInputElement;
    tab.value = v.sheetLink.tab;
    const account = h("input", { type: "text", placeholder: t("gog's default") }) as HTMLInputElement;
    account.value = v.sheetLink.account;
    const said = h("span", { class: "zheus-saved" });
    const save = h("button", { class: "primary", text: t("Save and read") }) as HTMLButtonElement;
    save.addEventListener("click", () =>
      void (async () => {
        save.disabled = true;
        said.replaceChildren();
        try {
          view = await Bridge.powersSheet(link.value, tab.value, account.value);
          link.value = view.sheetLink.id;
          drawStatus();
        } catch (err) {
          said.append(notice(errorText(err), false));
        } finally {
          save.disabled = false;
        }
      })(),
    );
    return glassCard(
      svg(ICONS.doc, 15, { stroke: 1.7 }),
      t("Your Google Sheet"),
      h("p", { class: "hint", text: t("AXYON keeps your portfolio in its own AXYON tab of your Google Sheet, made there the first time, through gog with the sign-in your assistant's gog skill uses. You both write it: your rows (Ativo, Classe, Quantidade, Preço médio, Meu teto; a ticker with your ceiling and no quantity is watched) are read back every 10 minutes and with Refresh, and what you ask AXYON in the chat (a buy, a sale, a ceiling) is written there at once. Beside them AXYON writes the price now, the result, its own ceilings and what to watch. Name another tab to have it only read. Your other tabs are never written. Leave the link empty and save to stop.") }),
      v.gog
        ? h("p", { class: "hint", text: wayLine(v) })
        : notice(t("AXYON found no way to your Google Sheet on this computer: ask AXYON Core to set up its google-workspace skill (it asks for your Google sign-in once), or install gog (github.com/openclaw/gogcli) and sign in with gog auth add."), false),
      h("div", { class: "zheus-grid" }, field(t("Sheet link or ID"), [link]), field(t("Tab"), [tab]), field(t("gog account"), [account])),
      sheetStatus,
      h("div", { class: "zheus-actions" }, said, save),
    );
  }

  // ── The forms ──
  function portfolioCard(v: PowersView): HTMLElement {
    const p = v.profile;
    const paste = h("textarea", { class: "powers-paste", rows: "5", placeholder: t("Paste your portfolio here: the table from Investidor10 or your broker, or lines like \"150 GGRC11 a 9,20\".") }) as HTMLTextAreaElement;
    const pasteNote = h("div", {});
    const pasteButton = h("button", { text: t("Save the portfolio") }) as HTMLButtonElement;
    pasteButton.addEventListener("click", () =>
      void (async () => {
        pasteNote.replaceChildren();
        pasteButton.disabled = true;
        try {
          const done = await Bridge.powersPaste(paste.value);
          view = done.view;
          paste.value = "";
          pasteNote.append(notice(done.text, true));
          drawStatus();
        } catch (err) {
          pasteNote.append(notice(errorText(err), false));
        } finally {
          pasteButton.disabled = false;
        }
      })(),
    );
    const file = h("input", { type: "file", accept: ".xlsx,.csv,.txt", class: "powers-file" }) as HTMLInputElement;
    file.addEventListener("change", () =>
      void (async () => {
        const picked = file.files?.[0];
        if (!picked) return;
        pasteNote.replaceChildren();
        try {
          const bytes = Array.from(new Uint8Array(await picked.arrayBuffer()));
          const done = await Bridge.powersImport(picked.name, bytes);
          view = done.view;
          pasteNote.append(notice(done.text, true));
          drawStatus();
        } catch (err) {
          pasteNote.append(notice(errorText(err), false));
        } finally {
          file.value = "";
        }
      })(),
    );

    const targets = Object.fromEntries(CLASS_IDS.map((id) => [id, number(p.targets[id] ?? 0, 0, 100, "%")]));
    for (const id of CLASS_IDS) if (!p.targets[id]) targets[id].input.value = "";
    const total = h("span", { class: "zheus-field-hint" });
    const sum = () => {
      const all = targetsFrom(Object.fromEntries(CLASS_IDS.map((id) => [id, targets[id].input.value])));
      const n = targetsTotal(all);
      total.textContent = n ? t("{total}% in all", { total: Math.round(n) }) : t("No target: Zheus only says where the money is.");
      total.classList.toggle("off-target", n > 0 && Math.abs(n - 100) > 0.5);
    };
    for (const id of CLASS_IDS) targets[id].input.addEventListener("input", sum);
    sum();
    const contribution = amount(p.monthlyContribution);
    const goal = amount(p.incomeGoal);
    const cash = amount(p.cash);
    const save = saver(async () => {
      const chosen = targetsFrom(Object.fromEntries(CLASS_IDS.map((id) => [id, targets[id].input.value])));
      view = await Bridge.powersSave("wealth_update", {
        targets: chosen,
        monthly_contribution: amountOf(contribution.input.value) ?? 0,
        income_goal: amountOf(goal.input.value) ?? 0,
        cash: amountOf(cash.input.value) ?? 0,
        source: "settings",
      });
      drawStatus();
    });
    return glassCard(
      svg(ICONS.star, 15),
      t("Portfolio"),
      h("p", { class: "hint", text: t("Zheus values it at B3's last close, follows your funds' notices and the companies' statements, and plans the road to your targets without selling. Your figures stay on this computer.") }),
      paste,
      h("div", { class: "row powers-import" }, h("label", { class: "powers-file-label" }, svg(ICONS.doc, 13, { stroke: 1.7 }), h("span", { text: t("Import B3's file") }), file), pasteButton),
      h("p", { class: "hint", text: t("B3's Área do Investidor → Extratos: Posição (what you hold), Movimentação (dividends, events) and Negociação (trades, for the taxes). XLSX or CSV.") }),
      pasteNote,
      h("div", { class: "zheus-grid three" }, ...CLASS_IDS.map((id) => field(classLabel(id), [targets[id].el]))),
      total,
      h(
        "div",
        { class: "zheus-grid three" },
        field(t("Monthly contribution"), [contribution.el]),
        field(t("Passive income goal (a month)"), [goal.el]),
        field(t("Cash at the broker"), [cash.el]),
      ),
      save.el,
    );
  }

  function studyCard(v: PowersView): HTMLElement {
    const p = v.profile;
    const exam = h("input", { type: "text", placeholder: "Analista do BACEN" }) as HTMLInputElement;
    exam.value = p.exam;
    const date = h("input", { type: "date", class: "zheus-time" }) as HTMLInputElement;
    date.value = p.examDate ?? "";
    const hours = measure(p.hoursPerWeek || 10, 1, 80, t("h a week"));
    const salary = amount(p.salary);
    const income = amount(p.currentIncome);
    const save = saver(async () => {
      view = await Bridge.powersSave("life_study", {
        action: "set",
        exam: exam.value.trim() || undefined,
        date: date.value || undefined,
        hours_per_week: Number(hours.input.value) || undefined,
        salary: amountOf(salary.input.value) ?? undefined,
        current_income: amountOf(income.input.value) ?? undefined,
      });
    });
    return glassCard(
      svg(ICONS.doc, 15, { stroke: 1.7 }),
      t("Public exam"),
      h("p", { class: "hint", text: t("A week split by the subjects' weights and your weak spots, reviews at 1, 7 and 30 days, and what one hour of study is worth. Tell Zheus your subjects in the chat (\"Economia peso 30, nível 2\").") }),
      h(
        "div",
        { class: "zheus-grid" },
        field(t("Exam"), [exam]),
        field(t("Date"), [date]),
        field(t("Study time"), [hours.el]),
        field(t("The post's salary"), [salary.el]),
        field(t("Your income now"), [income.el]),
        field(t("Subjects"), [h("span", { class: "zheus-until", text: p.subjects.length ? p.subjects.join(", ") : t("none yet") })]),
      ),
      save.el,
    );
  }

  function englishCard(v: PowersView): HTMLElement {
    const p = v.profile;
    const level = select(["A1", "A2", "B1", "B2", "C1", "C2"].map((l) => [l, l]), p.englishLevel || "B1");
    const minutes = measure(p.englishMinutes || 15, 5, 180, t("min a day"));
    const mode = h("input", { type: "checkbox" }) as HTMLInputElement;
    mode.checked = p.englishMode;
    const save = saver(async () => {
      view = await Bridge.powersSave("life_english", { action: "set", level: level.value, minutes_per_day: Number(minutes.input.value) || 15, mode: mode.checked });
    });
    return glassCard(
      svg(ICONS.bubble, 15),
      t("English"),
      h("p", { class: "hint", text: t("Lessons from the companies of your portfolio: their results to read in English, five words a day, a speaking prompt. With English mode on, Zheus answers in English and corrects your sentences.") }),
      h(
        "div",
        { class: "zheus-grid three" },
        field(t("Level"), [level]),
        field(t("Practice"), [minutes.el]),
        field(t("English mode"), [h("label", { class: "zheus-check" }, mode, h("span", { text: t("Answers in English") }))]),
      ),
      save.el,
    );
  }

  function gymCard(v: PowersView): HTMLElement {
    const p = v.profile;
    const days = measure(p.gymDays || 4, 1, 7, t("days a week"));
    const split = select(
      [
        ["upper_lower", t("Upper / lower")],
        ["full_body", t("Full body")],
        ["ppl", t("Push / pull / legs")],
      ],
      p.gymSplit,
    );
    const goal = select(
      [
        ["hipertrofia", t("Muscle")],
        ["força", t("Strength")],
        ["emagrecimento", t("Weight loss")],
        ["saúde", t("Health")],
      ],
      p.gymGoal,
    );
    const save = saver(async () => {
      view = await Bridge.powersSave("life_gym", { action: "set", days_per_week: Number(days.input.value) || 4, split: split.value, goal: goal.value });
    });
    return glassCard(
      svg(ICONS.timer, 15, { stroke: 1.7 }),
      t("Training"),
      h("p", { class: "hint", text: t("Today's session with the load to try (double progression), and how ready you are from your sleep, soreness, energy and study load. Tell Zheus how it went (\"supino 3x10 com 60 kg\").") }),
      h("div", { class: "zheus-grid three" }, field(t("Frequency"), [days.el]), field(t("Split"), [split]), field(t("Goal"), [goal])),
      save.el,
    );
  }

  function contentCard(v: PowersView): HTMLElement {
    const p = v.profile;
    const priv = h("input", { type: "checkbox" }) as HTMLInputElement;
    priv.checked = p.privateContent;
    priv.addEventListener("change", () => void Bridge.powersSave("content_private", priv.checked).then((x) => (view = x)));
    const kitNote = h("div", {});
    const make = h("button", { text: t("Make this month's kit") }) as HTMLButtonElement;
    make.addEventListener("click", () =>
      void (async () => {
        kitNote.replaceChildren();
        make.disabled = true;
        try {
          const folder = await Bridge.powersKit(null);
          const open = h("button", { text: t("Open the folder"), onclick: () => void Bridge.powersOpenFolder(null) });
          kitNote.append(h("div", { class: "row" }, notice(t("Saved in {folder}", { folder }), true), open));
        } catch (err) {
          kitNote.append(notice(errorText(err), false));
        } finally {
          make.disabled = false;
        }
      })(),
    );
    return glassCard(
      svg(ICONS.copy, 15, { stroke: 1.7 }),
      t("Monthly content"),
      h("p", { class: "hint", text: t("Square charts of your month (the portfolio's growth, the income, the split, your studies and training) as PNG images in Documents → Coucou → Zheus, each with a caption for you to adapt. Coucou never posts anything.") }),
      h("label", { class: "zheus-check" }, priv, h("span", { text: t("Private: percentages only, no amounts in reais") })),
      h("div", { class: "row" }, make),
      kitNote,
    );
  }

  function dataCard(): HTMLElement {
    const read = h("button", { text: t("Read now") }) as HTMLButtonElement;
    const said = h("div", {});
    read.addEventListener("click", () =>
      void (async () => {
        said.replaceChildren();
        try {
          view = await Bridge.powersRefresh();
          drawStatus();
        } catch (err) {
          said.append(notice(errorText(err), false));
        }
      })(),
    );
    return glassCard(
      svg(ICONS.arrowClockwise, 15, { stroke: 1.8 }),
      t("Official data"),
      h("p", { class: "hint", text: t("B3's daily quotes, the CVM's reports on funds and companies, FundosNet's notices and the Banco Central's rates and Focus survey: free public data, no account, and nothing of yours is sent. Read once a day while the superpowers are on.") }),
      h("div", { class: "row" }, read),
      said,
    );
  }

  function draw() {
    const v = view;
    const on = v?.on === true;
    onSwitch.classList.toggle("on", on);
    onSwitch.setAttribute("aria-pressed", String(on));
    drawStatus();
    if (on && v && !formsFor) {
      forms.replaceChildren(sheetCard(v), portfolioCard(v), studyCard(v), englishCard(v), gymCard(v), contentCard(v), dataCard());
      drawSheetStatus();
      formsFor = true;
    } else if (!on) {
      forms.replaceChildren();
      formsFor = false;
    }
    forms.classList.toggle("off", !on);
  }

  void Bridge.powersStatus().then((v) => {
    view = v ?? null;
    draw();
  });
  void onEvent<PowersView>("powers-changed", (v) => {
    view = v;
    draw();
  });

  return h(
    "section",
    { id: "powers", class: "zheus-panel powers-panel" },
    h(
      "div",
      { class: "zheus-hero" },
      h("span", { class: "zheus-hero-icon" }, svg(ICONS.star, 22)),
      h(
        "div",
        { class: "zheus-hero-text" },
        h("h2", {}, h("span", { text: t("Zheus' superpowers") })),
        h("p", { class: "hint", text: t("Your portfolio read against B3's, the CVM's and the Banco Central's data — your funds' dividends and notices, your companies against your thesis, the road to your targets, taxes, a committee before each decision — and your studies, English and training. Zheus only reads and keeps your records: he never buys, sells, pays or posts. Off by default.") }),
      ),
      onSwitch,
    ),
    note,
    status,
    forms,
  );
}
