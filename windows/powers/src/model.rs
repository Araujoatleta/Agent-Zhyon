//! What Zheus keeps for the user (the Store: their portfolio and targets, trades and payouts, decisions and theses,
//! studies, cards, English, training, content) and what it read from the official sources (the Market: quotes, funds'
//! months and documents, dividends, companies' figures, the economy). Plain data, saved as JSON by the app.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::sources::{b3::Quote, bcb::Macro, cvm_cia::Figures, cvm_fii::FiiMonth, fnet::{Document, Payout}};

// ── The portfolio ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    Acao,
    Fii,
    Etf,
    Bdr,
    RendaFixa,
    Tesouro,
    Cripto,
    Outro,
}

impl AssetClass {
    pub const ALL: [AssetClass; 8] =
        [AssetClass::Acao, AssetClass::Fii, AssetClass::Etf, AssetClass::Bdr, AssetClass::RendaFixa, AssetClass::Tesouro, AssetClass::Cripto, AssetClass::Outro];

    /// "acao", "fii"…: the id the tools and the files use.
    pub fn id(self) -> &'static str {
        match self {
            AssetClass::Acao => "acao",
            AssetClass::Fii => "fii",
            AssetClass::Etf => "etf",
            AssetClass::Bdr => "bdr",
            AssetClass::RendaFixa => "renda_fixa",
            AssetClass::Tesouro => "tesouro",
            AssetClass::Cripto => "cripto",
            AssetClass::Outro => "outro",
        }
    }

    /// The name a Brazilian investor uses.
    pub fn label(self) -> &'static str {
        match self {
            AssetClass::Acao => "Ações",
            AssetClass::Fii => "FIIs",
            AssetClass::Etf => "ETFs",
            AssetClass::Bdr => "BDRs",
            AssetClass::RendaFixa => "Renda fixa",
            AssetClass::Tesouro => "Tesouro Direto",
            AssetClass::Cripto => "Cripto",
            AssetClass::Outro => "Outros",
        }
    }

    /// From any way of naming it: "Ações", "acoes", "FIIs", "fundos imobiliários", "Tesouro Direto", "CDB"…
    pub fn parse(s: &str) -> Option<AssetClass> {
        let p = crate::text::plain(s);
        let p = p.trim();
        Some(match p {
            _ if p.starts_with("aca") || p.starts_with("aco") || p == "stock" || p == "stocks" || p.contains("acoes") => AssetClass::Acao,
            _ if p.starts_with("fii") || p.contains("imobiliari") || p == "reit" => AssetClass::Fii,
            _ if p.starts_with("etf") || p.contains("fundo de indice") || p.contains("fundos de indice") => AssetClass::Etf,
            _ if p.starts_with("bdr") => AssetClass::Bdr,
            _ if p.contains("tesouro") || p.contains("titulo publico") => AssetClass::Tesouro,
            _ if p.contains("renda fixa") || p.starts_with("rf") || p.contains("cdb") || p.contains("lci") || p.contains("lca") || p.contains("debenture") || p.contains("cri") || p.contains("cra") || p.contains("poupanca") => AssetClass::RendaFixa,
            _ if p.contains("cripto") || p.contains("bitcoin") || p.contains("crypto") => AssetClass::Cripto,
            _ if p.starts_with("outr") || p == "other" => AssetClass::Outro,
            _ => return None,
        })
    }

    /// Whether B3 quotes it (a ticker), rather than an amount the user tells.
    pub fn quoted(self) -> bool {
        matches!(self, AssetClass::Acao | AssetClass::Fii | AssetClass::Etf | AssetClass::Bdr)
    }
}

/// One holding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    /// "GGRC11"; for fixed income, a name ("CDB Banco X 110% CDI").
    pub ticker: String,
    pub class: AssetClass,
    pub quantity: f64,
    /// R$ per share (for fixed income: the amount invested, with a quantity of 1).
    pub average_price: f64,
    /// FII: "tijolo", "papel", "hibrido", "fof"; fixed income: "pos" (CDI), "pre", "ipca"; ETF: "usd" when it follows
    /// a foreign index without hedge.
    #[serde(default)]
    pub subtype: String,
    /// What the user says it is worth now, for what B3 does not quote (fixed income, Tesouro, crypto).
    #[serde(default)]
    pub current_value: Option<f64>,
    #[serde(default)]
    pub note: String,
}

impl Position {
    pub fn cost(&self) -> f64 {
        self.quantity * self.average_price
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Portfolio {
    pub positions: Vec<Position>,
    /// The share of each class the user wants, in percent (adds up to 100 when set).
    #[serde(default)]
    pub targets: BTreeMap<AssetClass, f64>,
    /// The passive income the user aims for, R$ a month.
    #[serde(default)]
    pub income_goal: Option<f64>,
    /// What the user usually invests a month, R$.
    #[serde(default)]
    pub monthly_contribution: Option<f64>,
    /// Cash waiting at the broker, R$.
    #[serde(default)]
    pub cash: f64,
    #[serde(default)]
    pub updated: Option<Date>,
    /// Where it came from last: "chat", "B3", "Investidor10", "text", "file".
    #[serde(default)]
    pub source: String,
    /// The share of the portfolio the user wants in each asset, percent (their sheet's "% Ideal" or "Meta" column, or
    /// set in the app).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub asset_targets: BTreeMap<String, f64>,
}

impl Portfolio {
    pub fn position(&self, ticker: &str) -> Option<&Position> {
        self.positions.iter().find(|p| p.ticker.eq_ignore_ascii_case(ticker))
    }

    pub fn tickers(&self, class: Option<AssetClass>) -> Vec<String> {
        self.positions.iter().filter(|p| p.class.quoted() && class.is_none_or(|c| p.class == c)).map(|p| p.ticker.clone()).collect()
    }
}

// ── Trades and payouts (from B3's statements) ────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeKind {
    #[default]
    Buy,
    Sell,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub date: Date,
    pub ticker: String,
    pub kind: TradeKind,
    pub quantity: f64,
    pub price: f64,
    #[serde(default)]
    pub fees: f64,
    #[serde(default)]
    pub class: Option<AssetClass>,
    /// "14:30", when the user said the time.
    #[serde(default)]
    pub time: String,
    /// "chat" for a trade the user told Zheus, "" for one from a file.
    #[serde(default)]
    pub source: String,
    /// The order the user told their trades in (1, 2…; 0 for a file's): what "undo the last one" undoes.
    #[serde(default)]
    pub seq: u64,
    /// For a trade told in the chat: the position's average price after it (a sale's cost basis).
    #[serde(default)]
    pub average: Option<f64>,
}

/// Something that happened to a holding other than a trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A FII's dividend (rendimento).
    Income,
    Dividend,
    /// Juros sobre capital próprio (taxed at source).
    InterestOnEquity,
    /// Capital returned: lowers the cost.
    Amortization,
    /// Bonus shares.
    Bonus,
    /// Split (quantity multiplied by `value`).
    Split,
    /// Reverse split (quantity divided by `value`).
    Grouping,
    /// Shares received in a subscription (bought at `value` each).
    Subscription,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub date: Date,
    pub ticker: String,
    pub kind: EventKind,
    /// Shares involved (bonus shares received, shares held for a payout).
    #[serde(default)]
    pub quantity: f64,
    /// R$ received in total for payouts; the factor for splits; R$ a share for subscriptions and bonus shares' cost.
    #[serde(default)]
    pub value: f64,
}

// ── Decisions and theses ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub date: Date,
    pub price: Option<f64>,
    /// "right", "wrong", "mixed" (as the user judged it).
    pub verdict: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub id: String,
    pub date: Date,
    pub ticker: String,
    /// "buy", "sell", "hold", "avoid".
    pub action: String,
    #[serde(default)]
    pub quantity: Option<f64>,
    #[serde(default)]
    pub price: Option<f64>,
    pub reasons: String,
    /// What the user expects to happen, in their words.
    #[serde(default)]
    pub expectation: String,
    /// 0–100.
    #[serde(default)]
    pub confidence: Option<u8>,
    /// What the committee said, when one was held.
    #[serde(default)]
    pub committee: String,
    #[serde(default)]
    pub reviews: Vec<Review>,
    /// "proposed" (waiting for the user), "taken", "dropped".
    #[serde(default = "taken")]
    pub status: String,
}

fn taken() -> String {
    "taken".into()
}

/// One measurable line of a thesis: `metric` `op` `value` ("ebitda_margin > 45").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    pub metric: String,
    /// ">", ">=", "<", "<=", "rising", "falling".
    pub op: String,
    #[serde(default)]
    pub value: f64,
    /// The user's own words for it.
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thesis {
    pub ticker: String,
    pub conditions: Vec<Condition>,
    pub written: Date,
    #[serde(default)]
    pub text: String,
}

/// The portfolio on one day, for its history, the shadow portfolio and the content kit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub date: Date,
    pub value: f64,
    pub cost: f64,
    /// Value by class.
    pub by_class: BTreeMap<AssetClass, f64>,
    /// Cost by class (what was put in): its change between two days is the money added or taken out.
    #[serde(default)]
    pub cost_by_class: BTreeMap<AssetClass, f64>,
    /// Estimated passive income a month, R$.
    #[serde(default)]
    pub income: f64,
    /// Each quoted asset's price that day, for the week's moves (weekly.rs).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub prices: BTreeMap<String, f64>,
}

// ── The committee's verdicts, learning, the training portfolio ──────────────

/// A point a mind made: what it says, the fact or page it rests on ("F3", "D1 p.4"), what kind of statement it is
/// (fato, calculo, hipotese, estimativa, ausente), and what this code found when it checked it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MindPoint {
    pub text: String,
    pub source: String,
    pub kind: String,
    /// The source exists and holds the point's figures (None: not checked).
    pub verified: Option<bool>,
    /// What the check found, when something.
    pub check: String,
}

/// A point as kept before the committee checked its points: `["text", "F3"]`.
#[derive(Deserialize)]
#[serde(untagged)]
enum PointKept {
    Pair(String, String),
    Full(MindPoint),
}

fn points_kept<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<MindPoint>, D::Error> {
    let kept = Vec::<PointKept>::deserialize(d)?;
    Ok(kept
        .into_iter()
        .map(|p| match p {
            PointKept::Pair(text, source) => MindPoint { text, source, ..MindPoint::default() },
            PointKept::Full(p) => p,
        })
        .collect())
}

/// What one of the committee's minds said (committee.rs).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MindVote {
    /// "valuation", "legal", "forensic", "risk", "macro".
    pub id: String,
    /// "for", "against", "neutral".
    pub vote: String,
    /// 0 to 100, as the evidence allows.
    pub confidence: u8,
    /// A veto (legal and risk only), and its reason among the objective ones (committee.rs, VETO_REASONS).
    pub veto: bool,
    pub veto_reason: String,
    /// Why it voted so.
    pub why: String,
    /// Its points, each with the fact or the page it rests on.
    #[serde(deserialize_with = "points_kept")]
    pub points: Vec<MindPoint>,
    /// The data it lacked that could change its view.
    pub missing: Vec<String>,
    /// Revised after the audit.
    pub revised: bool,
    /// Its vote before the audit or this code changed it, when one did.
    pub was: String,
}

/// One of the committee's two opinions: on the asset (positivo, neutro, negativo) or on the portfolio (aumentar,
/// manter, reduzir, evitar_novo_aporte).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Opinion {
    pub view: String,
    pub why: String,
    pub confidence: Option<u8>,
    pub sources: Vec<String>,
}

/// What the audit found: by the auditor (the brain, checking the minds against each other and the sources) or by this
/// code (sources that do not exist, figures not in their source, a vote on absent data, a veto without its criterion,
/// sources that disagree). Severity: critico, material, observacao.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AuditItem {
    pub severity: String,
    pub minds: Vec<String>,
    pub text: String,
    pub sources: Vec<String>,
    /// "auditor" or "codigo".
    pub by: String,
}

/// A document the committee read, as D1, D2…
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DocRef {
    pub id: String,
    pub key: String,
    pub title: String,
    pub kind: String,
    pub delivered: Option<Date>,
    pub url: String,
    pub pages: usize,
}

/// The committee's verdict on a proposal, kept for the app and the journal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Verdict {
    pub id: String,
    pub ticker: String,
    pub proposal: String,
    pub date: Option<Date>,
    pub minds: Vec<MindVote>,
    /// The chair's question to the user (never an order).
    pub question: String,
    /// The strongest argument of the side that lost.
    pub dissent: String,
    /// What would change the picture.
    pub would_change: String,
    /// How it fits the user's own targets.
    pub fit: String,
    /// Why the committee met: "user" (asked), "document" (a new document of the asset), "thesis" (a condition broke).
    pub reason: String,
    /// The document that called it, when one did.
    pub document: String,
    /// 2: with the audit, the two opinions and the facts.
    pub version: u8,
    /// Opinion A, the asset's quality; opinion B, its fit in the user's portfolio.
    pub asset: Opinion,
    pub portfolio_fit: Opinion,
    pub audit: Vec<AuditItem>,
    /// What the user should check before deciding.
    pub to_verify: Vec<String>,
    /// The facts and documents the committee had, as it cited them.
    pub facts: Vec<crate::evidence::Fact>,
    pub documents: Vec<DocRef>,
}

impl Verdict {
    /// (for, against, neutral).
    pub fn score(&self) -> (usize, usize, usize) {
        let count = |v: &str| self.minds.iter().filter(|m| m.vote == v).count();
        (count("for"), count("against"), count("neutral"))
    }

    pub fn vetoed_by(&self) -> Vec<String> {
        self.minds.iter().filter(|m| m.veto).map(|m| m.id.clone()).collect()
    }
}

/// The dividends received as the user's sheet records them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RecordedDividends {
    /// Where: "Sua planilha — aba Meu Patrimônio".
    pub source: String,
    pub read: Option<Date>,
    /// Month ("2026-10") to the total received that month, R$.
    pub months: BTreeMap<String, f64>,
    /// Month to ticker to R$, when the sheet lists them asset by asset.
    pub by_asset: BTreeMap<String, BTreeMap<String, f64>>,
}

/// A practice portfolio with pretend money, at the real prices (learn.rs).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Paper {
    pub started: Option<Date>,
    /// The pretend money it began with, R$.
    pub start: f64,
    pub cash: f64,
    pub positions: Vec<Position>,
    pub trades: Vec<Trade>,
}

/// What the user learns with their own portfolio (learn.rs).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Learn {
    /// Explanations in plain words, with the terms defined (the committee, the reader, the weekly summary).
    pub apprentice: bool,
    pub paper: Paper,
    /// The week of the last weekly summary made (its Monday), so one a week is told.
    pub weekly_told: Option<Date>,
}

// ── Studies, cards, English, training, content ───────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subject {
    pub name: String,
    /// How much it counts in the exam (questions or points).
    pub weight: f64,
    /// How well the user knows it, 0 (not at all) to 5 (mastered).
    #[serde(default)]
    pub level: u8,
    #[serde(default)]
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyProfile {
    /// "Analista do BACEN", "Polícia Federal - Agente"…
    pub exam: String,
    #[serde(default)]
    pub board: String,
    #[serde(default)]
    pub date: Option<Date>,
    /// Gross monthly salary of the post, R$.
    #[serde(default)]
    pub salary: Option<f64>,
    /// What the user earns now a month, R$ (for the value of passing).
    #[serde(default)]
    pub current_income: Option<f64>,
    /// Hours a week the user can study.
    #[serde(default)]
    pub hours_per_week: f64,
    #[serde(default)]
    pub subjects: Vec<Subject>,
    /// The user's own estimate of their chance of passing, percent (for the value of an hour).
    #[serde(default)]
    pub chance: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyLog {
    pub date: Date,
    pub subject: String,
    pub minutes: u32,
    #[serde(default)]
    pub questions: u32,
    #[serde(default)]
    pub correct: u32,
}

/// A flashcard, scheduled by SM-2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: String,
    /// "concurso", "english", or a subject.
    pub deck: String,
    pub front: String,
    pub back: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub ease: f64,
    /// Days until the next review.
    pub interval: u32,
    pub reps: u32,
    #[serde(default)]
    pub lapses: u32,
    pub due: Date,
    pub created: Date,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnglishProfile {
    /// "A2", "B1", "B2", "C1".
    pub level: String,
    pub minutes_per_day: u32,
    /// Zheus answers in English and corrects the user's English in the chat.
    #[serde(default)]
    pub mode: bool,
    #[serde(default)]
    pub streak_days: u32,
    #[serde(default)]
    pub last_day: Option<Date>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PracticeLog {
    pub date: Date,
    pub minutes: u32,
    /// "reading", "speaking", "writing", "listening", "cards".
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GymProfile {
    pub days_per_week: u8,
    /// "full_body", "upper_lower", "abc", "abcde", "ppl".
    pub split: String,
    /// "hipertrofia", "forca", "saude", "emagrecimento".
    pub goal: String,
    #[serde(default)]
    pub session_minutes: u32,
}

impl Default for GymProfile {
    fn default() -> Self {
        GymProfile { days_per_week: 4, split: "upper_lower".into(), goal: "hipertrofia".into(), session_minutes: 60 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetLog {
    pub reps: u32,
    pub kg: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExerciseLog {
    pub name: String,
    pub sets: Vec<SetLog>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workout {
    pub date: Date,
    /// "A", "Upper", "Full"…
    #[serde(default)]
    pub session: String,
    pub exercises: Vec<ExerciseLog>,
    #[serde(default)]
    pub minutes: u32,
    /// Perceived effort, 1–10.
    #[serde(default)]
    pub effort: Option<u8>,
    #[serde(default)]
    pub note: String,
}

/// How the user slept and feels, as they told it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wellness {
    pub date: Date,
    #[serde(default)]
    pub sleep_hours: Option<f64>,
    /// 0 none … 3 very sore.
    #[serde(default)]
    pub soreness: Option<u8>,
    /// 1 drained … 5 great.
    #[serde(default)]
    pub energy: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentPrefs {
    /// Percentages instead of reais in what is made to be posted.
    pub private: bool,
    #[serde(default)]
    pub platforms: Vec<String>,
}

impl Default for ContentPrefs {
    fn default() -> Self {
        ContentPrefs { private: true, platforms: vec!["instagram".into()] }
    }
}

/// Everything Zheus keeps for the user.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Store {
    pub version: u32,
    pub portfolio: Portfolio,
    pub trades: Vec<Trade>,
    pub events: Vec<Event>,
    pub decisions: Vec<Decision>,
    pub theses: Vec<Thesis>,
    pub snapshots: Vec<Snapshot>,
    pub study: StudyProfile,
    pub study_logs: Vec<StudyLog>,
    pub cards: Vec<Card>,
    pub english: EnglishProfile,
    pub practice: Vec<PracticeLog>,
    pub gym: GymProfile,
    pub workouts: Vec<Workout>,
    pub wellness: Vec<Wellness>,
    pub content: ContentPrefs,
    /// Ticker roots to the CNPJ of their fund, as the user (or Zheus' brain) gave it: for the funds the CVM's reports
    /// list without the ISIN of their shares. It wins over what Coucou learned.
    pub fund_cnpjs: BTreeMap<String, String>,
    /// How the user wants ceiling prices worked out, their own ceilings, and the opportunities already told.
    pub ceilings: Ceilings,
    /// The user's spreadsheet, when the portfolio comes from it (sheet.rs).
    pub sheet: crate::sheet::SheetLink,
    /// AXYON's work in its tab of the spreadsheet (workspace.rs).
    pub sheet_sync: crate::workspace::Sync,
    /// The moments told today during the session (alerts.rs).
    pub told_today: crate::alerts::Told,
    /// The committee's last verdicts, newest last (committee.rs).
    pub verdicts: Vec<Verdict>,
    /// The dividends the user's own sheet records month by month ("Proventos pagos"): the record, when there is one.
    pub recorded_dividends: RecordedDividends,
    pub learn: Learn,
    /// The documents and broken theses already told, by key (library.rs, thesis.rs).
    pub told_documents: Vec<String>,
}

/// The user's choices for ceiling prices (ceiling.rs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Ceilings {
    /// The dividend yield a share must pay, percent a year: Décio Bazin's own 6 %.
    pub stock_yield: f64,
    /// The yield a fund must pay, percent a year. None: the CDI after the 15 % income tax, the fund's tax-free
    /// dividend against what the safe alternative pays.
    pub fund_yield: Option<f64>,
    /// The most P/VP the user pays for a fund (1: its VP per share).
    pub max_pvp: f64,
    /// How far below a ceiling the close must be to be told, percent.
    pub margin: f64,
    /// The methods that make an opportunity ("bazin", "bazin_projected", "graham", "lynch", "pvp"); empty: the
    /// projected Bazin for funds, and the projected Bazin and Graham for shares.
    pub methods: Vec<String>,
    /// Ceilings the user set themselves, by ticker: those assets are watched too, held or not.
    pub manual: BTreeMap<String, f64>,
    /// When each opportunity was last told, and how far below its ceiling it was then (percent).
    pub told: BTreeMap<String, (Date, f64)>,
}

impl Default for Ceilings {
    fn default() -> Self {
        Ceilings { stock_yield: 6.0, fund_yield: None, max_pvp: 1.0, margin: 0.0, methods: Vec::new(), manual: BTreeMap::new(), told: BTreeMap::new() }
    }
}

/// A fund as Zheus read it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundData {
    pub ticker: String,
    pub cnpj: String,
    /// Its months, oldest first.
    #[serde(default)]
    pub months: Vec<FiiMonth>,
    /// Newest first.
    #[serde(default)]
    pub documents: Vec<Document>,
    /// Oldest first.
    #[serde(default)]
    pub payouts: Vec<Payout>,
    #[serde(default)]
    pub checked: Option<Date>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyData {
    pub ticker: String,
    pub cnpj: String,
    pub name: String,
    #[serde(default)]
    pub figures: Vec<Figures>,
    #[serde(default)]
    pub checked: Option<Date>,
    /// The CVM's files that could not be read at the last reading (the others were).
    #[serde(default)]
    pub problems: Vec<String>,
    /// Its shares outstanding (issued, less those in treasury), from the CVM's "composição do capital", and when.
    #[serde(default)]
    pub shares: Option<f64>,
    #[serde(default)]
    pub shares_date: Option<Date>,
    /// The reader that read it (refresh::COMPANY_READER): one read by an older one is read again once.
    #[serde(default)]
    pub reader: u32,
    /// Its documents the CVM lists (IPE: material facts, notices, results releases, assemblies), newest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<crate::library::Listed>,
    /// When they were last listed.
    #[serde(default)]
    pub documents_read: Option<Date>,
}

/// What Zheus read from the official sources, kept between runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Market {
    pub quotes: BTreeMap<String, Quote>,
    pub quotes_day: Option<Date>,
    pub funds: BTreeMap<String, FundData>,
    /// Ticker roots to fund CNPJs (from the ISINs in the CVM's reports).
    pub fund_cnpjs: BTreeMap<String, String>,
    pub funds_read: Option<Date>,
    pub companies: BTreeMap<String, CompanyData>,
    /// Tickers to company CNPJ and name (from the CVM's register).
    pub company_cnpjs: BTreeMap<String, (String, String)>,
    pub economy: Macro,
    pub economy_read: Option<Date>,
    /// The prices read during the session (Google Finance), by ticker. Each is the price while it is newer than B3's
    /// last close of the ticker; once B3 publishes that day's close, the close is the price again.
    pub live: BTreeMap<String, LivePrice>,
}

/// A price read during the session.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePrice {
    pub price: f64,
    /// When it was, unix seconds.
    pub at: i64,
    /// The previous session's close, as Google gave it with the price.
    #[serde(default)]
    pub previous: Option<f64>,
}

impl LivePrice {
    /// The session it belongs to: its day in Brasília (UTC−3, B3's own time).
    pub fn day(&self) -> Date {
        Date::from_days((self.at - 3 * 3600).div_euclid(86_400))
    }
}

impl Market {
    /// The price of a ticker: the session's while it is newer than B3's last close, else that close.
    pub fn price(&self, ticker: &str) -> Option<f64> {
        self.live_now(ticker).map(|l| l.price).or_else(|| self.close(ticker))
    }

    /// The latest close of a ticker, if B3 published it.
    pub fn close(&self, ticker: &str) -> Option<f64> {
        self.quotes.get(&ticker.to_uppercase()).map(|q| q.close).filter(|p| *p > 0.0)
    }

    /// The session's price of a ticker, while it is newer than B3's last close of it.
    pub fn live_now(&self, ticker: &str) -> Option<&LivePrice> {
        let t = ticker.to_uppercase();
        let live = self.live.get(&t).filter(|l| l.price > 0.0)?;
        match self.quotes.get(&t) {
            Some(q) if q.close > 0.0 && q.date >= live.day() => None,
            _ => Some(live),
        }
    }

    /// The day's change of a ticker during the session, percent: its price now against the previous session's close
    /// (Google's, else B3's last).
    pub fn day_change(&self, ticker: &str) -> Option<f64> {
        let live = self.live_now(ticker)?;
        let before = live.previous.filter(|p| *p > 0.0).or_else(|| self.close(ticker))?;
        Some((live.price / before - 1.0) * 100.0)
    }

    /// Keeps `read` (each ticker's price, when it was and the previous close) as the session's prices, and lets go of
    /// those B3's closes made old.
    pub fn keep_live(&mut self, read: impl IntoIterator<Item = (String, LivePrice)>) {
        for (ticker, live) in read {
            if live.price > 0.0 && live.price.is_finite() {
                let t = ticker.to_uppercase();
                if self.live.get(&t).is_none_or(|l| l.at <= live.at) {
                    self.live.insert(t, live);
                }
            }
        }
        let quotes = &self.quotes;
        self.live.retain(|t, l| quotes.get(t).is_none_or(|q| q.date < l.day()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_are_named_every_way() {
        assert_eq!(AssetClass::parse("Ações"), Some(AssetClass::Acao));
        assert_eq!(AssetClass::parse("FIIs"), Some(AssetClass::Fii));
        assert_eq!(AssetClass::parse("Fundos Imobiliários"), Some(AssetClass::Fii));
        assert_eq!(AssetClass::parse("ETFs"), Some(AssetClass::Etf));
        assert_eq!(AssetClass::parse("Tesouro Direto"), Some(AssetClass::Tesouro));
        assert_eq!(AssetClass::parse("Renda Fixa"), Some(AssetClass::RendaFixa));
        assert_eq!(AssetClass::parse("CDB"), Some(AssetClass::RendaFixa));
        assert_eq!(AssetClass::parse("renda_fixa"), Some(AssetClass::RendaFixa));
        assert_eq!(AssetClass::parse("xyz"), None);
        assert_eq!(serde_json::to_string(&AssetClass::RendaFixa).unwrap(), "\"renda_fixa\"");
    }

    #[test]
    fn the_sessions_price_wins_until_b3_publishes_that_days_close() {
        let mut m = Market::default();
        let quote = |day: u32, close: f64| Quote {
            ticker: "GGRC11".into(),
            date: Date::new(2026, 10, day).unwrap(),
            open: close,
            high: close,
            low: close,
            average: close,
            close,
            trades: 1,
            volume: 1.0,
            bdi: "12".into(),
            kind: "CI".into(),
            name: "GGR COVEPI".into(),
            isin: String::new(),
            market: 10,
        };
        m.quotes.insert("GGRC11".into(), quote(26, 8.85));
        assert_eq!(m.price("ggrc11"), Some(8.85));
        // 27/10 15:16 in Brasília (18:16 UTC).
        let at = 1_793_125_002;
        let live = |t: &str, price: f64, at: i64| (t.to_string(), LivePrice { price, at, previous: None });
        m.keep_live([live("GGRC11", 8.69, at), live("TIMS3", 18.78, at), live("BAD1", 0.0, at)]);
        assert_eq!(m.live["GGRC11"].day(), Date::new(2026, 10, 27).unwrap());
        assert_eq!((m.price("GGRC11"), m.close("GGRC11")), (Some(8.69), Some(8.85)));
        assert_eq!(m.price("TIMS3"), Some(18.78), "no close known: the session's price");
        assert!((m.day_change("GGRC11").unwrap() - (8.69 / 8.85 - 1.0) * 100.0).abs() < 1e-9, "against B3's close");
        assert_eq!(m.day_change("TIMS3"), None, "no close to say it against");
        m.keep_live([("TIMS3".to_string(), LivePrice { price: 18.78, at, previous: Some(18.1) })]);
        assert!((m.day_change("TIMS3").unwrap() - 3.756_906).abs() < 1e-5, "against Google's previous close");
        assert!(!m.live.contains_key("BAD1"));
        m.keep_live([live("GGRC11", 8.60, at - 60)]);
        assert_eq!(m.price("GGRC11"), Some(8.69), "an older reading does not replace a newer one");
        // B3's file of the 27th: its close is the price again, and the session's goes.
        m.quotes.insert("GGRC11".into(), quote(27, 8.70));
        assert_eq!(m.price("GGRC11"), Some(8.70));
        m.keep_live([]);
        assert!(!m.live.contains_key("GGRC11") && m.live.contains_key("TIMS3"));
    }

    #[test]
    fn an_empty_store_reads_from_an_old_or_partial_file() {
        let s: Store = serde_json::from_str(r#"{"portfolio":{"positions":[{"ticker":"GGRC11","class":"fii","quantity":150,"averagePrice":9.2}]}}"#).unwrap();
        assert_eq!(s.portfolio.positions[0].cost(), 150.0 * 9.2);
        assert!(s.cards.is_empty());
        assert!(s.content.private);
        let round: Store = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(round, s);
    }
}
