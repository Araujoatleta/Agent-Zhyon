// Zheus' superpowers as the island, Settings and the TV mode show them (src-tauri/src/powers, coucou-powers): the
// portfolio and its classes against the user's targets, the passive income against its goal, the alerts, the day's
// lines (already in the user's language), the official data's last reading, the user's sheet, what AXYON tells the
// moment it happens, and the content kit's charts turned into PNG files. Pure functions here; the views draw them.

import { t } from "../i18n/i18n";
import type { BotEmoteName } from "./layout";
import type { SoundName } from "./sound";

export interface ClassLine {
  /** "acao", "fii", "etf", "bdr", "renda_fixa", "tesouro", "cripto", "outro". */
  id: string;
  label: string;
  /** Percent of the portfolio. */
  weight: number;
  target: number | null;
}

export interface PowersSummary {
  hasPortfolio: boolean;
  value: number;
  gainPct: number | null;
  pricedOn: string | null;
  /** When the newest of the session's prices (Google Finance) was, unix seconds, when the value uses some. */
  liveAt: number | null;
  classes: ClassLine[];
  /** R$ a month. */
  income: number;
  incomeGoal: number | null;
  studyStreak: number;
  englishStreak: number;
  nextTraining: string | null;
  selic: number | null;
}

/** A day's item or an alert, in the user's language. */
export interface PowersLine {
  area: string;
  text: string;
  urgent: boolean;
}

export interface RefreshState {
  running: boolean;
  at: string | null;
  problems: string[];
}

/** The last reading of the session's prices. */
export interface LiveState {
  at: string | null;
  /** The assets Google Finance gave no price for. */
  missed: string[];
}

/** The user's Google Sheet, as Settings keeps it. */
export interface SheetLink {
  id: string;
  tab: string;
  account: string;
}

/** The last round with the user's sheet. */
export interface SheetState {
  running: boolean;
  /** When it was last read, and when AXYON last wrote its tab ("15:20"). */
  at: string | null;
  written: string | null;
  tab: string;
  /** AXYON's own tab (read and written), rather than one of the user's (only read). */
  ours: boolean;
  positions: number;
  /** Assets watched with the user's ceiling, not held. */
  watched: number;
  problem: string | null;
  skipped: string[];
}

/** One thing AXYON tells about the portfolio the moment it happens (src-tauri/src/powers, tell). */
export interface PowersAlert {
  kind: "opportunity" | "into_zone" | "out_of_zone" | "up" | "down" | "portfolio_up" | "portfolio_down";
  /** The asset; empty for the whole portfolio. */
  ticker: string;
  /** Its headline and what follows, in the user's language. */
  title: string;
  text: string;
  tone: "good" | "bad" | "info";
  /** When the price was, in Brasília ("15:16"), when it is the session's. */
  at: string | null;
}

/** An alert as the island keeps it: when it came (ms). */
export interface ShownAlert extends PowersAlert {
  came: number;
}

/** How long an alert stays in the ★ tab. */
export const ALERT_LASTS = 30 * 60_000;

export interface PowersProfile {
  positions: number;
  source: string;
  updated: string | null;
  targets: Record<string, number>;
  incomeGoal: number | null;
  monthlyContribution: number | null;
  cash: number;
  trades: number;
  payouts: number;
  decisions: number;
  cards: number;
  exam: string;
  examDate: string | null;
  hoursPerWeek: number;
  salary: number | null;
  currentIncome: number | null;
  subjects: string[];
  englishLevel: string;
  englishMinutes: number;
  englishMode: boolean;
  gymDays: number;
  gymSplit: string;
  gymGoal: string;
  privateContent: boolean;
  contentFolder: string;
}

export interface PowersView {
  on: boolean;
  summary: PowersSummary;
  today: PowersLine[];
  alerts: PowersLine[];
  refresh: RefreshState;
  profile: PowersProfile;
  /** "ready": Hermes, as Coucou runs it, has the tools; "missing": Coucou is adding them; "": not Coucou's Hermes. */
  hermes: "" | "ready" | "missing";
  /** coucou-hook, for another Hermes' config. */
  command: string;
  live: LiveState;
  sheetLink: SheetLink;
  sheet: SheetState;
  /** gog is on this computer. */
  gog: boolean;
}

export interface KitFile {
  name: string;
  svg: string;
}

export interface Kit {
  folder: string;
  files: KitFile[];
}

/** The classes a target may be set for, in the order Settings lists them. */
export const CLASS_IDS = ["acao", "fii", "etf", "bdr", "renda_fixa", "tesouro", "cripto"] as const;

/** A class's name in the user's language. */
export function classLabel(id: string): string {
  switch (id) {
    case "acao":
      return t("Shares");
    case "fii":
      return t("Real-estate funds");
    case "etf":
      return t("ETFs");
    case "bdr":
      return t("BDRs");
    case "renda_fixa":
      return t("Fixed income");
    case "tesouro":
      return t("Tesouro Direto");
    case "cripto":
      return t("Crypto");
    default:
      return t("Other");
  }
}

/** "R$ 7.905,72" (reais, the Brazilian way, whatever the interface's language). */
export function money(v: number, decimals = 2): string {
  const fixed = Math.abs(v).toFixed(decimals);
  const [whole, cents] = fixed.split(".");
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  return `${v < 0 ? "−" : ""}R$ ${grouped}${cents ? `,${cents}` : ""}`;
}

/** "12,5%" / "+3,2%". */
export function percent(v: number, decimals = 1, signed = false): string {
  const text = Math.abs(v).toFixed(decimals).replace(".", ",");
  const sign = v < 0 ? "−" : signed && v > 0 ? "+" : "";
  return `${sign}${text}%`;
}

/** One bar of the classes: its width now and the target's mark, both as a share of the widest (0 to 100). */
export interface ClassBar {
  id: string;
  label: string;
  weight: number;
  target: number | null;
  /** Width of the bar, percent of the track. */
  width: number;
  /** Where the target's mark sits, percent of the track; null without a target. */
  mark: number | null;
  /** "over", "under" or "on" its target (within 2 points), "" without one. */
  side: "over" | "under" | "on" | "";
}

export function classBars(classes: ClassLine[]): ClassBar[] {
  const widest = Math.max(1, ...classes.map((c) => Math.max(c.weight, c.target ?? 0)));
  return classes
    .slice()
    .sort((a, b) => b.weight - a.weight || (b.target ?? 0) - (a.target ?? 0))
    .map((c) => {
      const side: ClassBar["side"] =
        c.target == null ? "" : Math.abs(c.weight - c.target) <= 2 ? "on" : c.weight > c.target ? "over" : "under";
      return {
        id: c.id,
        label: classLabel(c.id),
        weight: c.weight,
        target: c.target,
        width: Math.min(100, (c.weight / widest) * 100),
        mark: c.target == null ? null : Math.min(100, (c.target / widest) * 100),
        side,
      };
    });
}

/** The passive income against its goal: 0 to 100, null without a goal. */
export function incomeProgress(s: PowersSummary): number | null {
  if (!s.incomeGoal || s.incomeGoal <= 0) return null;
  return Math.max(0, Math.min(100, (s.income / s.incomeGoal) * 100));
}

/** The line under the portfolio's value: the gain, and where the prices come from (the session's from Google
 *  Finance, else B3's last close). */
export function pricedLine(s: PowersSummary): string {
  const parts: string[] = [];
  if (s.gainPct != null) parts.push(t("{gain} on cost", { gain: percent(s.gainPct, 1, true) }));
  if (s.liveAt) parts.push(t("Google Finance at {time}", { time: clockOf(s.liveAt) }));
  else if (s.pricedOn) parts.push(t("B3's close of {date}", { date: brDate(s.pricedOn) }));
  return parts.join(" · ");
}

/** Unix seconds as "15:16", on this computer's clock. */
export function clockOf(unix: number): string {
  const d = new Date(unix * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** "2026-10-20" as "20/10". */
export function brDate(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  return m ? `${m[3]}/${m[2]}` : iso;
}

/** The user's sheet's last round, for Settings. */
export function sheetLine(v: PowersView): string {
  const s = v.sheet;
  if (s.running) return t("Reading the sheet…");
  if (s.problem) return s.problem;
  if (!s.at) return t("The sheet is read in a moment.");
  if (!s.ours) return t("The {tab} tab, only read, at {time}: {count} positions", { tab: s.tab, time: s.at, count: s.positions });
  const read = t("AXYON's tab, read at {time}: {count} positions, {watched} watched", { time: s.at, count: s.positions, watched: s.watched });
  return s.written ? `${read} · ${t("written at {time}", { time: s.written })}` : read;
}

/** The sign before an alert, by its kind. */
export function alertIcon(a: PowersAlert): string {
  switch (a.kind) {
    case "opportunity":
    case "into_zone":
      return "🟢";
    case "out_of_zone":
      return "🔴";
    case "up":
    case "portfolio_up":
      return "📈";
    default:
      return "📉";
  }
}

/** The alerts that came together, as one message in the chat. */
export function alertsMessage(alerts: PowersAlert[]): string {
  return alerts.map((a) => `${alertIcon(a)} ${a.title} — ${a.text}${a.at ? ` (${a.at})` : ""}`).join("\n");
}

/** AXYON's face and sound for alerts that came together: those of the first, the most important. */
export function alertMood(a: PowersAlert): { emote: BotEmoteName; sound: SoundName } {
  if (a.kind === "opportunity" || a.kind === "into_zone") return { emote: "proud", sound: "proud" };
  if (a.tone === "good") return { emote: "happy", sound: "love" };
  if (a.tone === "bad") return { emote: "surprised", sound: "gulp" };
  return { emote: "wink", sound: "wink" };
}

/** The question an alert offers to ask AXYON (written in the chat for the user to send). */
export function alertQuestion(a: PowersAlert): string {
  if (!a.ticker) return t("Why did my portfolio move so much today?");
  if (a.kind === "opportunity" || a.kind === "into_zone") return t("Is it a good time to buy {ticker}?", { ticker: a.ticker });
  return t("What is happening with {ticker} today?", { ticker: a.ticker });
}

/** The alerts still shown at `now`, newest first, less those closed (by `alertKey`). */
export function shownAlerts(list: ShownAlert[], now: number, closed: ReadonlySet<string> = new Set()): ShownAlert[] {
  return list.filter((a) => now - a.came < ALERT_LASTS && !closed.has(alertKey(a)));
}

export function alertKey(a: ShownAlert): string {
  return `${a.came}:${a.kind}:${a.ticker}`;
}

/** The session's prices: where they come from, and when they were last read. */
export function liveLine(v: PowersView): string {
  const base = v.live.at ? t("Prices from Google Finance read at {time}", { time: v.live.at }) : t("Prices from Google Finance during B3's session");
  return `${base} · ${t("some 15 minutes late; B3's official close in the evening")}`;
}

/** The official data's state, for the island's foot and Settings. */
export function refreshLine(r: RefreshState): string {
  if (r.running) return t("Reading B3, the CVM and the Banco Central…");
  if (r.at) return r.problems.length ? t("Read at {time}, {count} source(s) did not answer", { time: r.at, count: r.problems.length }) : t("Read at {time}", { time: r.at });
  return t("Official data not read yet today");
}

/** The questions the island offers to ask Zheus, a click each. */
export function quickQuestions(s: PowersSummary): string[] {
  const out: string[] = [];
  if (s.hasPortfolio) {
    out.push(t("How are my real-estate funds?"));
    out.push(t("Where should this month's contribution go?"));
  } else {
    out.push(t("Let me paste my portfolio"));
  }
  out.push(t("What is my day?"));
  return out;
}

/** The targets typed in Settings, as the tool takes them (empty and zero left out). */
export function targetsFrom(fields: Record<string, string>): Record<string, number> {
  const out: Record<string, number> = {};
  for (const id of CLASS_IDS) {
    const raw = (fields[id] ?? "").trim().replace(",", ".");
    const v = Number(raw);
    if (raw !== "" && Number.isFinite(v) && v > 0) out[id] = Math.min(100, v);
  }
  return out;
}

/** The sum of the targets, to say whether they make 100 %. */
export function targetsTotal(targets: Record<string, number>): number {
  return Object.values(targets).reduce((a, b) => a + b, 0);
}

/** An amount typed the Brazilian way ("1.234,56", "1.000", "500") as a number; null when it is not one. A dot before
 *  groups of three digits separates thousands; otherwise ("500.5") it is the decimal point. */
export function amountOf(raw: string): number | null {
  const text = raw.trim().replace(/^R\$\s*/i, "");
  if (!text) return null;
  const normal = text.includes(",")
    ? text.replace(/\./g, "").replace(",", ".")
    : /^\d{1,3}(\.\d{3})+$/.test(text)
      ? text.replace(/\./g, "")
      : text;
  const v = Number(normal);
  return Number.isFinite(v) && v >= 0 ? v : null;
}

/** A kit chart's PNG file name: "1-patrimonio" → "1-patrimonio" (the extension is Rust's to add). */
export function pngName(name: string): string {
  return name.replace(/\.(svg|png)$/i, "").toLowerCase();
}

/** A chart of the content kit drawn into a 1080 × 1080 PNG (the browser renders the SVG). */
export async function svgToPng(svg: string, size = 1080): Promise<Uint8Array> {
  const blob = new Blob([svg], { type: "image/svg+xml" });
  const url = URL.createObjectURL(blob);
  try {
    const img = new Image();
    img.decoding = "async";
    const loaded = new Promise<void>((resolve, reject) => {
      img.onload = () => resolve();
      img.onerror = () => reject(new Error("the chart could not be drawn"));
    });
    img.src = url;
    await loaded;
    const canvas = document.createElement("canvas");
    canvas.width = size;
    canvas.height = size;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("no canvas");
    ctx.drawImage(img, 0, 0, size, size);
    const png = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
    if (!png) throw new Error("no PNG");
    return new Uint8Array(await png.arrayBuffer());
  } finally {
    URL.revokeObjectURL(url);
  }
}
