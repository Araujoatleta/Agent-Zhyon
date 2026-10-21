// Zheus' superpowers (src-tauri/src/powers) as the island, Settings and the TV mode show them: the words and bars of
// src/core/powers.ts, the island's view (src/views/powers.ts), the section of Settings (src/settings/powers.ts) and the
// content kit's PNG copies (src/island/powers.ts), on a fake DOM and through the real bridge — a question written into
// the chat for the user to send, the portfolio pasted and imported, the forms saved through the brain's own tools,
// and nothing sent before a click.

import { test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { emit, internals, sent } from "./tauri.mjs";

installFakeDom();
const {
  amountOf, classBars, classLabel, incomeProgress, money, percent, pngName, pricedLine, quickQuestions, refreshLine, targetsFrom,
  targetsTotal,
} = await import("../src/core/powers.ts");
const { State } = await import("../src/core/state.ts");
const { buildPowers } = await import("../src/views/powers.ts");
const { powersSection } = await import("../src/settings/powers.ts");
const { savePngs } = await import("../src/island/powers.ts");

/** What the mocked Rust side answers, by command. */
let answers = {};
const plainInvoke = internals.invoke;
internals.invoke = async (cmd, args) => {
  const result = await plainInvoke(cmd, args);
  if (cmd in answers) {
    const a = typeof answers[cmd] === "function" ? answers[cmd](args) : answers[cmd];
    if (a instanceof Error) throw a;
    return a;
  }
  return result;
};
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
const all = (root) => [root, ...root.walk()];
const byText = (root, tag, text) => all(root).find((e) => e.tagName === tag && e.textContent === text);
const byClass = (root, cls) => all(root).filter((e) => (e.className ?? "").split(" ").includes(cls));

const summary = (fields = {}) => ({
  hasPortfolio: true,
  value: 6523.7,
  gainPct: 1.82,
  pricedOn: "2026-10-20",
  classes: [
    { id: "acao", label: "Ações", weight: 8.76, target: 25 },
    { id: "fii", label: "FIIs", weight: 80.77, target: 25 },
    { id: "etf", label: "ETFs", weight: 10.48, target: 25 },
    { id: "renda_fixa", label: "Renda fixa", weight: 0, target: 25 },
  ],
  income: 26.7,
  incomeGoal: 1000,
  studyStreak: 0,
  englishStreak: 0,
  nextTraining: null,
  selic: 13.75,
  ...fields,
});

const profile = (fields = {}) => ({
  positions: 6,
  source: "paste",
  updated: "2026-10-21",
  targets: { acao: 25, fii: 25, etf: 25, renda_fixa: 25 },
  incomeGoal: 1000,
  monthlyContribution: 500,
  cash: 0,
  trades: 0,
  payouts: 0,
  decisions: 0,
  cards: 0,
  exam: "",
  examDate: null,
  hoursPerWeek: 10,
  salary: null,
  currentIncome: null,
  subjects: [],
  englishLevel: "B1",
  englishMinutes: 15,
  englishMode: false,
  gymDays: 4,
  gymSplit: "upper_lower",
  gymGoal: "hipertrofia",
  privateContent: true,
  contentFolder: "C:\\Users\\Ana\\Documents\\Coucou\\Zheus",
  ...fields,
});

const view = (fields = {}) => ({
  on: true,
  summary: summary(),
  today: [
    { area: "study", text: "Study: Economia (50 min)", urgent: false },
    { area: "money", text: "GGRC11 · data-com · today · R$ 13,20", urgent: true },
  ],
  alerts: [{ area: "thesis", text: "TIMS3: 1 condition(s) of your thesis broke", urgent: false }],
  refresh: { running: false, at: "07:30", problems: [] },
  profile: profile(),
  hermes: "ready",
  command: "C:\\Users\\Ana\\AppData\\Local\\Coucou\\bin\\coucou-hook.exe",
  ...fields,
});

// ── The words and the bars ───────────────────────────────────────────────────

test("money and percents are written the Brazilian way, whatever the interface's language", () => {
  assert.equal(money(6523.7), "R$ 6.523,70");
  assert.equal(money(1234567.891), "R$ 1.234.567,89");
  assert.equal(money(-12.5), "−R$ 12,50");
  assert.equal(money(1000, 0), "R$ 1.000");
  assert.equal(percent(80.77), "80,8%");
  assert.equal(percent(1.82, 1, true), "+1,8%");
  assert.equal(percent(-3.6, 1, true), "−3,6%");
  assert.equal(pricedLine(summary()), "+1,8% on cost · B3's close of 20/10");
  assert.equal(classLabel("fii"), "Real-estate funds");
  assert.equal(classLabel("renda_fixa"), "Fixed income");
  assert.equal(classLabel("whatever"), "Other");
});

test("the classes are bars against the user's targets, the widest first", () => {
  const bars = classBars(summary().classes);
  assert.deepEqual(bars.map((b) => b.id), ["fii", "etf", "acao", "renda_fixa"]);
  const fii = bars[0];
  assert.equal(fii.side, "over");
  assert.equal(fii.width, 100, "the widest fills the track");
  assert.ok(Math.abs(fii.mark - (25 / 80.77) * 100) < 1e-9, "its target's mark");
  assert.equal(bars[3].side, "under");
  assert.equal(bars[3].width, 0);
  assert.equal(classBars([{ id: "fii", label: "FIIs", weight: 24, target: 25 }])[0].side, "on", "within two points");
  assert.equal(classBars([{ id: "fii", label: "FIIs", weight: 24, target: null }])[0].mark, null);
  assert.ok(Math.abs(incomeProgress(summary()) - 2.67) < 1e-9);
  assert.equal(incomeProgress(summary({ incomeGoal: null })), null);
  assert.equal(incomeProgress(summary({ income: 2000 })), 100, "never past the goal");
});

test("the forms' numbers are read the way the user types them", () => {
  assert.deepEqual(targetsFrom({ acao: "25", fii: "25,5", etf: "", renda_fixa: "0", cripto: "x" }), { acao: 25, fii: 25.5 });
  assert.equal(targetsTotal({ acao: 25, fii: 25, etf: 25, renda_fixa: 25 }), 100);
  assert.equal(amountOf("1.234,56"), 1234.56);
  assert.equal(amountOf("R$ 500"), 500);
  assert.equal(amountOf("500.5"), 500.5);
  assert.equal(amountOf("1.000"), 1000, "a dot before three digits groups thousands");
  assert.equal(amountOf("20.924,80"), 20924.8);
  assert.equal(amountOf("1.234.567"), 1234567);
  assert.equal(amountOf(""), null);
  assert.equal(amountOf("-3"), null);
  assert.equal(pngName("1-patrimonio.svg"), "1-patrimonio");
  assert.equal(refreshLine({ running: true, at: null, problems: [] }), "Reading B3, the CVM and the Banco Central…");
  assert.equal(refreshLine({ running: false, at: "07:30", problems: ["FundosNet: no answer in time"] }), "Read at 07:30, 1 source(s) did not answer");
  assert.equal(refreshLine({ running: false, at: null, problems: [] }), "Official data not read yet today");
  assert.deepEqual(quickQuestions(summary({ hasPortfolio: false })), ["Let me paste my portfolio", "What is my day?"]);
});

// ── The island ───────────────────────────────────────────────────────────────

const actions = () => {
  const calls = [];
  return { calls, blip: () => calls.push("blip"), openSettingsWindow: () => calls.push("settings"), setView: (v) => calls.push(`view:${v}`) };
};

test("the island shows the portfolio against its targets, the day with what not to miss first, and asks for nothing", () => {
  State.settings = { ...State.settings, powersEnabled: true };
  State.powers = view();
  State.view = "powers";
  State.mode = "expanded";
  const host = buildPowers(actions());
  host.sync();
  assert.ok(byText(host.el, "SPAN", "R$ 6.523,70"));
  assert.ok(byText(host.el, "SPAN", "+1,8% on cost · B3's close of 20/10"));
  const bars = byClass(host.el, "powers-bar");
  assert.equal(bars.length, 4);
  assert.ok(bars[0].className.includes("over"), "the funds are over their target");
  assert.ok(byText(host.el, "SPAN", "R$ 26,70/month"));
  const lines = byClass(host.el, "powers-line");
  assert.equal(lines[0].textContent, "GGRC11 · data-com · today · R$ 13,20", "the urgent first");
  assert.ok(lines[0].className.includes("urgent") && lines[0].className.includes("money"));
  assert.equal(lines.length, 3, "the alert and the day's two lines");
  assert.equal(sent("powers_refresh").length, 0, "nothing read before a click");
});

test("a question for Zheus opens the chat with it written, for the user to send", () => {
  State.powers = view();
  State.view = "powers";
  State.mode = "expanded";
  State.chatDraft = null;
  const a = actions();
  const host = buildPowers(a);
  host.sync();
  byText(host.el, "BUTTON", "Where should this month's contribution go?").fire("click");
  assert.equal(State.chatDraft, "Where should this month's contribution go?");
  assert.ok(a.calls.includes("view:prompt"));
  assert.equal(sent("chat_send").length, 0, "nothing is sent: the user sends it");
});

test("Refresh asks Rust to read again, and while it reads the button waits", async () => {
  answers = { powers_refresh: view({ refresh: { running: true, at: "07:30", problems: [] } }) };
  State.powers = view();
  State.view = "powers";
  State.mode = "expanded";
  const host = buildPowers(actions());
  host.sync();
  byText(host.el, "SPAN", "Refresh").parentElement.fire("click");
  await flush();
  assert.equal(sent("powers_refresh").length, 1);
  assert.ok(byText(host.el, "SPAN", "Reading…"));
});

test("off, the island says where to turn the superpowers on", () => {
  State.settings = { ...State.settings, powersEnabled: false };
  State.powers = view({ on: false });
  State.view = "powers";
  State.mode = "expanded";
  const a = actions();
  const host = buildPowers(a);
  host.sync();
  byText(host.el, "SPAN", "Open Settings").parentElement.fire("click");
  assert.deepEqual(a.calls.filter((c) => c === "settings"), ["settings"]);
  State.settings = { ...State.settings, powersEnabled: true };
});

// ── Settings ─────────────────────────────────────────────────────────────────

const toggle = (on, onChange) => {
  const el = document.createElement("button");
  el.className = on ? "switch on" : "switch";
  el.addEventListener("click", () => {
    const next = !el.classList.contains("on");
    el.classList.toggle("on", next);
    onChange(next);
  });
  return el;
};

test("the switch turns the superpowers on, and the forms come with what Rust keeps", async () => {
  answers = { powers_status: view({ on: false }), powers_set: view() };
  const section = powersSection(toggle);
  await flush();
  assert.equal(byClass(section, "zheus-box").length, 0, "off: no form");
  byClass(section, "switch")[0].fire("click");
  await flush();
  assert.deepEqual(sent("powers_set").at(-1), { on: true });
  assert.ok(byText(section, "H3", "Portfolio") && byText(section, "H3", "Public exam") && byText(section, "H3", "English") && byText(section, "H3", "Training"));
  assert.ok(byText(section, "SPAN", "6 positions · updated 21/10/2026 · R$ 6.523,70"));
  assert.ok(all(section).some((e) => e.className === "notice ok" && e.textContent.startsWith("Zheus (Hermes) has the superpowers")));
});

test("the portfolio's targets and amounts are saved through the brain's own tool", async () => {
  answers = { powers_status: view(), powers_save: view() };
  const section = powersSection(toggle);
  await flush();
  const box = all(section).find((e) => e.className === "zheus-box" && byText(e, "H3", "Portfolio"));
  const targetInputs = all(box).filter((e) => e.tagName === "INPUT" && e.getAttribute("type") === "number");
  assert.equal(targetInputs.length, 7, "one per class");
  assert.equal(targetInputs[0].value, "25");
  targetInputs[0].value = "30";
  targetInputs[1].value = "20";
  const money = all(box).filter((e) => e.tagName === "INPUT" && e.getAttribute("inputmode") === "decimal");
  assert.deepEqual(money.map((m) => m.value), ["500", "1.000", ""], "written the Brazilian way");
  money[0].value = "1.200,50";
  byText(box, "BUTTON", "Save").fire("click");
  await flush();
  const saved = sent("powers_save").at(-1);
  assert.equal(saved.tool, "wealth_update");
  assert.deepEqual(saved.input.targets, { acao: 30, fii: 20, etf: 25, renda_fixa: 25 });
  assert.equal(saved.input.monthly_contribution, 1200.5);
  assert.equal(saved.input.income_goal, 1000);
  assert.equal(saved.input.cash, 0);
  assert.ok(byText(box, "DIV", "Saved."));
});

test("a portfolio pasted or B3's file imported goes to Rust as it is, and a report while typing keeps the forms", async () => {
  answers = {
    powers_status: view(),
    powers_paste: { text: "Saved: 6 positions, 0 trades, 0 payouts and events.", view: view() },
    powers_import: { text: "Saved: 4 positions, 0 trades, 0 payouts and events.", view: view() },
  };
  const section = powersSection(toggle);
  await flush();
  const paste = all(section).find((e) => e.tagName === "TEXTAREA");
  paste.value = "GGRC11 150 R$ 9,20\nGARE11 180 R$ 8,55";
  byText(section, "BUTTON", "Save the portfolio").fire("click");
  await flush();
  assert.deepEqual(sent("powers_paste").at(-1), { text: "GGRC11 150 R$ 9,20\nGARE11 180 R$ 8,55" });
  assert.equal(paste.value, "", "emptied once saved");
  const file = all(section).find((e) => e.tagName === "INPUT" && e.getAttribute("type") === "file");
  file.files = [{ name: "posicao-2026-10-20.xlsx", arrayBuffer: async () => new Uint8Array([80, 75, 3, 4]).buffer }];
  file.fire("change");
  await flush();
  await flush();
  assert.deepEqual(sent("powers_import").at(-1), { name: "posicao-2026-10-20.xlsx", bytes: [80, 75, 3, 4] });
  // A report from Rust (the background reading) while the user types: the field keeps what they typed.
  const exam = all(section).find((e) => e.tagName === "INPUT" && e.getAttribute("placeholder") === "Analista do BACEN");
  exam.value = "Auditor da Receita";
  emit("powers-changed", view({ refresh: { running: true, at: "07:30", problems: [] } }));
  assert.equal(exam.value, "Auditor da Receita");
  assert.ok(byText(section, "SPAN", "Reading B3, the CVM and the Banco Central…"), "the status follows");
});

test("studies, English, training and the content's privacy are saved with their own tools", async () => {
  answers = { powers_status: view(), powers_save: view() };
  const section = powersSection(toggle);
  await flush();
  const card = (title) => all(section).find((e) => e.className === "zheus-box" && byText(e, "H3", title));
  const english = card("English");
  all(english).find((e) => e.tagName === "INPUT" && e.getAttribute("type") === "checkbox").checked = true;
  byText(english, "BUTTON", "Save").fire("click");
  await flush();
  assert.deepEqual(sent("powers_save").at(-1), { tool: "life_english", input: { action: "set", level: "B1", minutes_per_day: 15, mode: true } });
  const gym = card("Training");
  byText(gym, "BUTTON", "Save").fire("click");
  await flush();
  assert.deepEqual(sent("powers_save").at(-1), { tool: "life_gym", input: { action: "set", days_per_week: 4, split: "upper_lower", goal: "hipertrofia" } });
  const study = card("Public exam");
  all(study).find((e) => e.getAttribute?.("placeholder") === "Analista do BACEN").value = "Analista do BACEN";
  byText(study, "BUTTON", "Save").fire("click");
  await flush();
  const saved = sent("powers_save").at(-1);
  assert.equal(saved.tool, "life_study");
  assert.equal(saved.input.action, "set");
  assert.equal(saved.input.exam, "Analista do BACEN");
  const content = card("Monthly content");
  const priv = all(content).find((e) => e.tagName === "INPUT" && e.getAttribute("type") === "checkbox");
  priv.checked = false;
  priv.fire("change");
  await flush();
  assert.deepEqual(sent("powers_save").at(-1), { tool: "content_private", input: false });
});

test("a Hermes Coucou does not run gets the lines to add, with coucou-hook's own path", async () => {
  answers = { powers_status: view({ hermes: "" }) };
  const section = powersSection(toggle);
  await flush();
  const snippet = all(section).find((e) => e.tagName === "PRE");
  assert.ok(snippet.textContent.includes("coucou-hook.exe") && snippet.textContent.includes("mcp_servers:"), snippet.textContent);
});

// ── The content kit's PNG copies ─────────────────────────────────────────────

test("each chart of a kit is drawn and saved beside its SVG; one that cannot be drawn keeps its SVG", async () => {
  const kit = { folder: "C:\\Users\\Ana\\Documents\\Coucou\\Zheus\\2026-10", files: [{ name: "1-patrimonio.svg", svg: "<svg/>" }, { name: "3-alocacao.svg", svg: "bad" }] };
  const before = sent("powers_save_png").length;
  const saved = await savePngs(kit, async (svg) => {
    if (svg === "bad") throw new Error("no");
    return new Uint8Array([137, 80, 78, 71]);
  });
  assert.equal(saved, 1);
  const calls = sent("powers_save_png").slice(before);
  assert.deepEqual(calls, [{ folder: kit.folder, name: "1-patrimonio", bytes: [137, 80, 78, 71] }]);
});

test("the chat takes the question written for it into its field, and sends nothing by itself", async () => {
  const { buildPrompt } = await import("../src/views/chat.ts");
  const before = sent("chat_send").length;
  const host = buildPrompt(() => {});
  State.chatDraft = "What is my day?";
  State.view = "overview";
  host.sync();
  const input = all(host.el).find((e) => e.tagName === "INPUT" && (e.className ?? "").includes("chat-input"));
  assert.notEqual(input.value, "What is my day?", "only once the chat is on screen");
  State.view = "prompt";
  host.sync();
  assert.equal(input.value, "What is my day?");
  assert.equal(State.chatDraft, null, "taken once");
  assert.equal(sent("chat_send").length, before);
});
