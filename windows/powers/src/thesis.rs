//! The living thesis: the user's reasons to hold an asset, written once in their own words, checked again whenever
//! something new comes in — the CVM's monthly report or statements, a price, or a document the reader read. A fund's
//! thesis reads the fund's own figures (P/VP, yield, dividend, VP) and what its manager's report states (vacancy,
//! default, LTV, occupancy); a company's, its statements (company.rs). A broken condition, or a document the brain
//! judged a risk to the thesis, is told once, and calls the committee (`to_meet`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::library::{DocKind, Library};
use crate::model::{Condition, Market, Store, Thesis};
use crate::text::{clip, fold, grouped, pct, plain};
use crate::{company, fii};

/// A fund's metrics a thesis may name, with what they mean (for the brain and the app).
pub const FUND_METRICS: &[(&str, &str)] = &[
    ("p_vp", "price over the VP per share"),
    ("dy", "yield over twelve months at the price, percent"),
    ("dividend", "last dividend a share, R$"),
    ("vp_change", "VP per share change over twelve months, percent"),
    ("vacancy", "vacancy the manager's report states, percent"),
    ("default", "default the manager's report states, percent"),
    ("ltv", "LTV the manager's report states, percent"),
    ("occupancy", "occupancy the manager's report states, percent"),
];

/// A fund's metrics now, each with where it comes from: the CVM's report and FundosNet (fii.rs), and the latest
/// manager's report read (library.rs).
pub fn fund_metrics(market: &Market, lib: Option<&Library>, ticker: &str, today: Date) -> BTreeMap<String, (f64, String)> {
    let mut out = BTreeMap::new();
    if let Some(fund) = market.funds.get(ticker) {
        let r = fii::report(fund, market.price(ticker), &market.economy, today);
        let cvm = format!("CVM {}", r.vp_date.map(|d| d.month_key()).unwrap_or_default());
        for (k, v, src) in [
            ("p_vp", r.p_vp, cvm.clone()),
            ("dy", r.yield_12m, "FundosNet".to_string()),
            ("dividend", r.last_dividend, "FundosNet".to_string()),
            ("vp_change", r.vp_change_12m, cvm.clone()),
        ] {
            if let Some(v) = v {
                out.insert(k.to_string(), (v, src));
            }
        }
    }
    if let Some(lib) = lib {
        let latest = lib.docs.iter().filter(|d| d.ticker == ticker && d.kind == DocKind::Report && d.is_read()).max_by_key(|d| d.delivered);
        if let Some(doc) = latest {
            let src = |page: u32| format!("{} p.{page}", doc.title);
            for f in &doc.figures {
                let key = match f.key.as_str() {
                    "vacancy_physical" | "vacancy_financial" => "vacancy",
                    "default" => "default",
                    "ltv" => "ltv",
                    "occupancy" => "occupancy",
                    _ => continue,
                };
                // The physical vacancy first, as reports lead with it.
                out.entry(key.to_string()).or_insert((f.value, src(f.page)));
            }
        }
    }
    out
}

/// The number written in a sentence, as Brazil writes it ("1,05", "9,5%"), from the sentence itself (not its plain
/// form, which splits "1,05" in two).
fn number_in(sentence: &str) -> Option<f64> {
    let chars: Vec<char> = sentence.chars().map(fold).collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let mut end = i;
            while end < chars.len() && (chars[end].is_ascii_digit() || ((chars[end] == ',' || chars[end] == '.') && end + 1 < chars.len() && chars[end + 1].is_ascii_digit())) {
                end += 1;
            }
            let s: String = chars[i..end].iter().collect();
            return crate::text::num(&s);
        }
        i += 1;
    }
    None
}

/// A sentence of a fund's thesis as a condition: "P/VP abaixo de 1", "DY acima de 9%", "vacância até 10%",
/// "inadimplência abaixo de 3%", "LTV abaixo de 30%", "dividendo acima de 0,08".
pub fn parse_fund_condition(sentence: &str) -> Option<Condition> {
    let p = plain(sentence);
    let has = |w: &str| p.contains(w);
    let metric = if has("p vp") || has("pvp") || has("preco sobre valor patrimonial") {
        "p_vp"
    } else if has("dy") || has("dividend yield") || has("yield") || has("rendimento anual") || has("rentabilidade") {
        "dy"
    } else if has("vacancia") {
        "vacancy"
    } else if has("inadimpl") {
        "default"
    } else if has("ltv") || has("alavancagem") {
        "ltv"
    } else if has("ocupacao") {
        "occupancy"
    } else if has("valor patrimonial") || has("vp") {
        "vp_change"
    } else if has("dividendo") || has("rendimento") || has("provento") || has("distribui") {
        "dividend"
    } else {
        return None;
    };
    let number = number_in(sentence);
    let op = if has("abaixo") || has("menor") || has("menos de") || has("ate ") || has("no maximo") || has("inferior") {
        "<"
    } else if has("acima") || has("maior") || has("mais de") || has("pelo menos") || has("no minimo") || has("superior") {
        ">"
    } else if number.is_some() {
        ">="
    } else {
        return None;
    };
    Some(Condition { metric: metric.into(), op: op.into(), value: number?, label: sentence.trim().to_string() })
}

/// The user's thesis on `ticker` from their words: each sentence (split at ";", new lines and " e ") a condition when
/// it is measurable; the words kept whole. A fund's sentences read as a fund's, a company's as a company's.
pub fn write(store: &mut Store, market: &Market, ticker: &str, text: &str, today: Date) -> Thesis {
    let fund = market.funds.contains_key(ticker) || store.portfolio.position(ticker).is_some_and(|p| p.class == crate::model::AssetClass::Fii);
    let conditions: Vec<Condition> = text
        .split([';', '\n'])
        .flat_map(|s| s.split(" e "))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| if fund { parse_fund_condition(s) } else { company::parse_condition(s) })
        .collect();
    let thesis = Thesis { ticker: ticker.to_string(), conditions, written: today, text: clip(text.trim(), 1_000) };
    store.theses.retain(|t| t.ticker != ticker);
    store.theses.push(thesis.clone());
    thesis
}

/// One condition of a thesis now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub label: String,
    pub metric: String,
    pub op: String,
    pub value: f64,
    pub now: Option<f64>,
    /// None: not enough data to tell yet.
    pub holds: Option<bool>,
    pub source: String,
}

/// A thesis now: each condition, how many broke, and the documents the brain judged a risk to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub ticker: String,
    pub text: String,
    pub written: Date,
    pub checks: Vec<Check>,
    pub broken: usize,
    /// Read documents of the last 60 days whose digest says they may hurt the thesis: (key, headline).
    pub risks: Vec<(String, String)>,
}

impl State {
    pub fn ok(&self) -> bool {
        self.broken == 0 && self.risks.is_empty()
    }
}

fn holds(op: &str, now: f64, value: f64) -> Option<bool> {
    match op {
        ">" => Some(now > value),
        ">=" => Some(now >= value),
        "<" => Some(now < value),
        "<=" => Some(now <= value),
        _ => None,
    }
}

/// Every thesis of the user now.
pub fn states(store: &Store, market: &Market, lib: Option<&Library>, today: Date) -> Vec<State> {
    store
        .theses
        .iter()
        .map(|th| {
            let checks: Vec<Check> = if let Some(c) = market.companies.get(&th.ticker) {
                let m = company::metrics(&c.figures);
                company::check(&m, &th.conditions)
                    .into_iter()
                    .map(|c| Check { label: if c.condition.label.is_empty() { c.condition.metric.clone() } else { c.condition.label.clone() }, metric: c.condition.metric, op: c.condition.op, value: c.condition.value, now: c.now, holds: c.holds, source: "CVM DFP/ITR".into() })
                    .collect()
            } else {
                let m = fund_metrics(market, lib, &th.ticker, today);
                th.conditions
                    .iter()
                    .map(|c| {
                        let now = m.get(&c.metric);
                        Check {
                            label: if c.label.is_empty() { c.metric.clone() } else { c.label.clone() },
                            metric: c.metric.clone(),
                            op: c.op.clone(),
                            value: c.value,
                            now: now.map(|x| x.0),
                            holds: now.and_then(|x| holds(&c.op, x.0, c.value)),
                            source: now.map(|x| x.1.clone()).unwrap_or_default(),
                        }
                    })
                    .collect()
            };
            let risks = lib
                .map(|l| {
                    l.docs
                        .iter()
                        .filter(|d| d.ticker == th.ticker && d.delivered.is_some_and(|x| x.until(today) <= 60))
                        .filter(|d| d.digest.as_ref().is_some_and(|g| g.thesis_risk))
                        .map(|d| (d.key.clone(), d.headline()))
                        .collect()
                })
                .unwrap_or_default();
            State { ticker: th.ticker.clone(), text: th.text.clone(), written: th.written, broken: checks.iter().filter(|c| c.holds == Some(false)).count(), checks, risks }
        })
        .collect()
}

/// A thesis' state in words, for the brain.
pub fn for_model(states: &[State]) -> String {
    if states.is_empty() {
        return "The user wrote no thesis yet. A thesis is their reasons to hold an asset, in their words (\"P/VP abaixo de 1; DY acima de 9%; vacância até 10%\"): AXYON checks it whenever something new comes in.".into();
    }
    let mut out = String::from("The user's theses, checked now:");
    for s in states {
        out.push_str(&format!("\n{} (written {}): {}", s.ticker, s.written.br(), if s.ok() { "holding" } else { "AT RISK" }));
        for c in &s.checks {
            let now = c.now.map(|v| if matches!(c.metric.as_str(), "p_vp" | "dividend") { grouped(v, 2) } else { pct(v, 1) }).unwrap_or_else(|| "no data yet".into());
            let state = match c.holds {
                Some(true) => "holds",
                Some(false) => "BROKEN",
                None => "unknown",
            };
            out.push_str(&format!("\n  - {}: {state} (now {now}{})", c.label, if c.source.is_empty() { String::new() } else { format!(", {}", c.source) }));
        }
        for (_, h) in &s.risks {
            out.push_str(&format!("\n  - a document may hurt it: {h}"));
        }
    }
    out
}

/// Something worth a notification: a document of an asset followed (a material fact, an assembly, an offering, or
/// one the brain judged a risk to the thesis), or a thesis gone at risk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    /// "document" or "thesis".
    pub kind: String,
    pub ticker: String,
    /// The document's kind ("material_fact"…), or the conditions broken.
    pub what: String,
    /// Its headline, or the user's own words for what broke.
    pub text: String,
    pub key: String,
}

/// The most keys of what was told, kept.
const TOLD_KEPT: usize = 400;

/// What to tell now, each once (`store.told_documents` keeps the keys).
pub fn to_tell(store: &mut Store, market: &Market, lib: &Library, today: Date) -> Vec<Notice> {
    let mut out = Vec::new();
    for d in lib.docs.iter().filter(|d| d.is_read() && d.delivered.is_some_and(|x| x.until(today) <= 10)) {
        let worth = d.kind.urgent() || d.digest.as_ref().is_some_and(|g| g.thesis_risk);
        if worth && !store.told_documents.contains(&d.key) {
            out.push(Notice { kind: "document".into(), ticker: d.ticker.clone(), what: d.kind.id().into(), text: d.headline(), key: d.key.clone() });
        }
    }
    for s in states(store, market, Some(lib), today) {
        let mut broken: Vec<&Check> = s.checks.iter().filter(|c| c.holds == Some(false)).collect();
        if broken.is_empty() {
            continue;
        }
        broken.sort_by(|a, b| a.metric.cmp(&b.metric));
        let key = format!("thesis:{}:{}", s.ticker, broken.iter().map(|c| c.metric.as_str()).collect::<Vec<_>>().join(","));
        if !store.told_documents.contains(&key) {
            out.push(Notice { kind: "thesis".into(), ticker: s.ticker.clone(), what: broken.iter().map(|c| c.metric.clone()).collect::<Vec<_>>().join(","), text: broken.iter().map(|c| c.label.clone()).collect::<Vec<_>>().join("; "), key });
        }
    }
    for n in &out {
        store.told_documents.push(n.key.clone());
    }
    let extra = store.told_documents.len().saturating_sub(TOLD_KEPT);
    store.told_documents.drain(..extra);
    out
}

/// A meeting the committee should hold by itself: a new urgent document of an asset followed since its last verdict,
/// or a thesis at risk with no verdict in a week.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    pub ticker: String,
    /// "document" or "thesis".
    pub reason: String,
    pub document: String,
    /// What it is about, for the proposal: "Fato relevante de GGRC11: …".
    pub about: String,
}

/// The meetings due, at most `most` (each costs a brain's answer).
pub fn to_meet(store: &Store, market: &Market, lib: &Library, today: Date, most: usize) -> Vec<Meeting> {
    let mut out: Vec<Meeting> = Vec::new();
    let last = |t: &str| store.verdicts.iter().rev().find(|v| v.ticker == t).and_then(|v| v.date);
    for d in lib.docs.iter().filter(|d| d.is_read() && d.delivered.is_some_and(|x| x.until(today) <= 30)) {
        let worth = d.kind.urgent() || d.digest.as_ref().is_some_and(|g| g.thesis_risk);
        let met = store.verdicts.iter().any(|v| v.document == d.key) || last(&d.ticker).is_some_and(|v| d.read.is_some_and(|r| r <= v) && d.delivered.is_some_and(|x| x <= v));
        if worth && !met && !out.iter().any(|m| m.ticker == d.ticker) {
            out.push(Meeting { ticker: d.ticker.clone(), reason: "document".into(), document: d.key.clone(), about: format!("{} ({}): {}", d.ticker, d.kind.label(), d.headline()) });
        }
    }
    for s in states(store, market, Some(lib), today) {
        if s.broken > 0 && !out.iter().any(|m| m.ticker == s.ticker) && last(&s.ticker).is_none_or(|v| v.until(today) >= 7) {
            let what: Vec<String> = s.checks.iter().filter(|c| c.holds == Some(false)).map(|c| c.label.clone()).collect();
            out.push(Meeting { ticker: s.ticker.clone(), reason: "thesis".into(), document: String::new(), about: format!("{}: the user's thesis broke ({})", s.ticker, what.join("; ")) });
        }
    }
    out.truncate(most);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::fixtures::report_pages;
    use crate::library::Pending;
    use crate::model::{FundData, Verdict};

    fn day(d: u32) -> Date {
        Date::new(2026, 10, d).unwrap()
    }

    #[test]
    fn a_funds_sentences_become_conditions() {
        let c = parse_fund_condition("P/VP abaixo de 1,05").unwrap();
        assert_eq!((c.metric.as_str(), c.op.as_str(), c.value), ("p_vp", "<", 1.05));
        let c = parse_fund_condition("DY acima de 9,5%").unwrap();
        assert_eq!((c.metric.as_str(), c.op.as_str(), c.value), ("dy", ">", 9.5));
        let c = parse_fund_condition("vacância até 10%").unwrap();
        assert_eq!((c.metric.as_str(), c.op.as_str(), c.value), ("vacancy", "<", 10.0));
        assert_eq!(parse_fund_condition("inadimplência menor que 3%").unwrap().metric, "default");
        assert_eq!(parse_fund_condition("dividendo pelo menos 0,08").unwrap().value, 0.08);
        assert!(parse_fund_condition("gosto da gestora").is_none());
        assert!(parse_fund_condition("vacância baixa").is_none(), "no number, nothing to check");
    }

    #[test]
    fn a_thesis_is_checked_against_the_report_and_its_break_told_once_and_met() {
        let mut store = Store::default();
        let mut market = Market::default();
        market.funds.insert("GGRC11".into(), FundData { ticker: "GGRC11".into(), ..FundData::default() });
        let th = write(&mut store, &market, "GGRC11", "vacância até 7%; inadimplência abaixo de 2% e gosto da gestora", day(1));
        assert_eq!(th.conditions.len(), 2, "the measurable ones; the words kept: {th:?}");
        assert!(th.text.contains("gosto da gestora"));

        let mut lib = Library::default();
        let pending = Pending {
            key: "fnet:904".into(),
            ticker: "GGRC11".into(),
            kind: DocKind::Report,
            title: "Relatório Gerencial 09/2026".into(),
            reference: "09/2026".into(),
            delivered: Some(day(10)),
            url: String::new(),
            download: String::new(),
        };
        lib.ingest(&pending, report_pages("7,5", "0,088"), day(28));
        let s = &states(&store, &market, Some(&lib), day(28))[0];
        assert_eq!(s.broken, 1, "vacancy 7,5% is over 7%");
        let vac = s.checks.iter().find(|c| c.metric == "vacancy").unwrap();
        assert_eq!((vac.now, vac.holds, vac.source.as_str()), (Some(7.5), Some(false), "Relatório Gerencial 09/2026 p.2"));
        assert_eq!(s.checks.iter().find(|c| c.metric == "default").unwrap().holds, Some(true));
        assert!(for_model(&[s.clone()]).contains("BROKEN (now 7,5%, Relatório Gerencial 09/2026 p.2)"));

        let told = to_tell(&mut store, &market, &lib, day(28));
        assert_eq!(told.len(), 1, "a report is not told on its own; the broken thesis is: {told:?}");
        assert_eq!((told[0].kind.as_str(), told[0].text.as_str()), ("thesis", "vacância até 7%"));
        assert!(to_tell(&mut store, &market, &lib, day(28)).is_empty(), "told once");

        let meet = to_meet(&store, &market, &lib, day(28), 3);
        assert_eq!(meet.len(), 1);
        assert_eq!(meet[0].reason, "thesis");
        store.verdicts.push(Verdict { ticker: "GGRC11".into(), date: Some(day(28)), reason: "thesis".into(), ..Verdict::default() });
        assert!(to_meet(&store, &market, &lib, day(30), 3).is_empty(), "met this week");

        // A material fact read later: told, and the committee meets on it.
        let fact = Pending { key: "fnet:905".into(), kind: DocKind::MaterialFact, title: "Fato Relevante".into(), delivered: Some(day(29)), ..pending };
        lib.ingest(&fact, vec!["Inadimplência do locatário X".into()], day(30));
        let told = to_tell(&mut store, &market, &lib, day(30));
        assert_eq!((told.len(), told[0].kind.as_str(), told[0].what.as_str()), (1, "document", "material_fact"));
        let meet = to_meet(&store, &market, &lib, day(30), 3);
        assert_eq!((meet[0].reason.as_str(), meet[0].document.as_str()), ("document", "fnet:905"));
    }
}
