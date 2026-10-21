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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeKind {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

/// What Zheus read from the official sources, kept between runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Market {
    pub quotes: BTreeMap<String, Quote>,
    pub quotes_day: Option<Date>,
    pub funds: BTreeMap<String, FundData>,
    /// Ticker roots to fund CNPJs (from the CVM's reports).
    pub fund_cnpjs: BTreeMap<String, String>,
    pub funds_read: Option<Date>,
    pub companies: BTreeMap<String, CompanyData>,
    /// Tickers to company CNPJ and name (from the CVM's register).
    pub company_cnpjs: BTreeMap<String, (String, String)>,
    pub economy: Macro,
    pub economy_read: Option<Date>,
}

impl Market {
    /// The latest close of a ticker, if B3 published it.
    pub fn price(&self, ticker: &str) -> Option<f64> {
        self.quotes.get(&ticker.to_uppercase()).map(|q| q.close).filter(|p| *p > 0.0)
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
    fn an_empty_store_reads_from_an_old_or_partial_file() {
        let s: Store = serde_json::from_str(r#"{"portfolio":{"positions":[{"ticker":"GGRC11","class":"fii","quantity":150,"averagePrice":9.2}]}}"#).unwrap();
        assert_eq!(s.portfolio.positions[0].cost(), 150.0 * 9.2);
        assert!(s.cards.is_empty());
        assert!(s.content.private);
        let round: Store = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(round, s);
    }
}
