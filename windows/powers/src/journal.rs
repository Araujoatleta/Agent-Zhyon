//! Decisions taken with their reasons, and looked at again: every decision goes into a journal (what, at what price,
//! why, what the user expects, how sure they are), comes back after three, six and twelve months for the user's own
//! verdict, and over time gives a calibration score (are the "90 % sure" decisions right nine times out of ten?) and
//! their return against the CDI. Before a decision, an investment committee: one voice for it, one against, and a
//! risk auditor's pre-mortem ("a year from now this was a mistake: why?"), all on the same facts, then a synthesis.
//! The committee never buys or sells: the decision is the user's.

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::model::{AssetClass, Decision, Review};
use crate::portfolio::Valuation;
use crate::text::{brl, pct};

/// When a decision comes back: three, six and twelve months after it.
pub const REVIEW_DAYS: [i64; 3] = [90, 180, 365];

pub fn new_id(date: Date, ticker: &str, n: usize) -> String {
    format!("d-{}{:02}{:02}-{}-{}", date.y, date.m, date.d, ticker.to_lowercase(), n + 1)
}

/// The decisions with a review due: one of their three dates reached with fewer reviews than dates passed.
pub fn due(decisions: &[Decision], today: Date) -> Vec<&Decision> {
    decisions
        .iter()
        .filter(|d| d.status == "taken")
        .filter(|d| {
            let reached = REVIEW_DAYS.iter().filter(|days| d.date.add_days(**days) <= today).count();
            reached > d.reviews.len()
        })
        .collect()
}

/// The next date a decision comes back, if any is left.
pub fn next_review(d: &Decision) -> Option<Date> {
    REVIEW_DAYS.get(d.reviews.len()).map(|days| d.date.add_days(*days))
}

pub fn review(d: &mut Decision, today: Date, verdict: &str, note: &str, price: Option<f64>) {
    let verdict = match crate::text::plain(verdict).as_str() {
        v if v.starts_with("cert") || v.starts_with("right") || v.starts_with("acert") || v == "sim" => "right",
        v if v.starts_with("err") || v.starts_with("wrong") || v == "nao" => "wrong",
        _ => "mixed",
    };
    d.reviews.push(Review { date: today, price, verdict: verdict.into(), note: note.trim().to_string() });
}

/// How the user's confidence matches their results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Calibration {
    /// (confidence band, decisions judged, share judged right in percent).
    pub bands: Vec<(String, usize, f64)>,
    pub judged: usize,
    pub right: usize,
    /// Average return of the decisions with a price then and now, percent, and of the CDI over the same days.
    pub average_return: Option<f64>,
    pub average_cdi: Option<f64>,
}

pub fn calibration(decisions: &[Decision], price_now: impl Fn(&str) -> Option<f64>, cdi: Option<f64>, today: Date) -> Calibration {
    let mut bands: Vec<(String, usize, usize)> = vec![("até 50%".into(), 0, 0), ("50–75%".into(), 0, 0), ("75–90%".into(), 0, 0), ("90%+".into(), 0, 0)];
    let mut judged = 0;
    let mut right = 0;
    let mut returns = Vec::new();
    let mut cdis = Vec::new();
    for d in decisions.iter().filter(|d| d.status == "taken") {
        if let Some(last) = d.reviews.last() {
            judged += 1;
            let ok = last.verdict == "right";
            if ok {
                right += 1;
            }
            let c = d.confidence.unwrap_or(50);
            let i = match c {
                0..=50 => 0,
                51..=75 => 1,
                76..=89 => 2,
                _ => 3,
            };
            bands[i].1 += 1;
            if ok {
                bands[i].2 += 1;
            }
        }
        if let (Some(then), Some(now)) = (d.price, price_now(&d.ticker)) {
            if then > 0.0 {
                let sign = if d.action == "sell" || d.action == "avoid" { -1.0 } else { 1.0 };
                returns.push(sign * (now / then - 1.0) * 100.0);
                if let Some(rate) = cdi {
                    let days = d.date.until(today).max(0) as f64;
                    cdis.push(((1.0 + rate / 100.0).powf(days / 365.0) - 1.0) * 100.0);
                }
            }
        }
    }
    let avg = |v: &[f64]| (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64);
    Calibration {
        bands: bands.into_iter().filter(|b| b.1 > 0).map(|(name, n, ok)| (name, n, ok as f64 / n as f64 * 100.0)).collect(),
        judged,
        right,
        average_return: avg(&returns),
        average_cdi: avg(&cdis),
    }
}

/// What a proposal does to the portfolio's classes: (class, weight before, weight after, target).
pub fn sizing(v: &Valuation, class: AssetClass, amount: f64) -> Option<(f64, f64, Option<f64>)> {
    if v.value + amount <= 0.0 {
        return None;
    }
    let c = v.classes.iter().find(|c| c.class == class);
    let held = c.map(|c| c.value).unwrap_or(0.0);
    let before = if v.value > 0.0 { held / v.value * 100.0 } else { 0.0 };
    let after = (held + amount) / (v.value + amount) * 100.0;
    Some((before, after, c.and_then(|c| c.target)))
}

/// The voices of the committee, each asked on the same facts.
pub const ROLES: [(&str, &str); 3] = [
    (
        "advocate",
        "You argue FOR the proposal, as a sharp analyst who believes in it: the three strongest reasons, each tied to a fact given. No hype; if the facts are thin, say what would make the case.",
    ),
    (
        "critic",
        "You argue AGAINST the proposal, as a sceptical analyst: the three strongest reasons not to, each tied to a fact given (the price against the VP or the history, the dividend's health, the concentration against the user's own targets, the rates). No doom; be precise.",
    ),
    (
        "risk",
        "You are the risk auditor and run a pre-mortem: it is a year from now and this decision turned out to be a mistake. Tell the two or three most likely stories of how, what early signal would have warned, and the one question the user should answer before deciding.",
    ),
];

pub const SYNTHESIS: &str = "You chair the committee. From the three opinions below, write the synthesis for the user: the decision in one line (as a question for them, never an order), the strongest argument on each side, what would change the picture, and how this fits their own targets. Short, plain, in the user's language. Nothing is bought or sold by the committee.";

/// The committee's procedure, for a brain that runs it itself in one answer.
pub fn protocol() -> String {
    let mut out = String::from("Run an investment committee on the proposal, with the facts below and nothing invented:");
    for (name, role) in ROLES {
        out.push_str(&format!("\n[{name}] {role}"));
    }
    out.push_str(&format!("\n[synthesis] {SYNTHESIS}"));
    out.push_str("\nThen offer to record the decision in the journal (wealth_decision) once the user decides, with their reasons and how sure they are.");
    out
}

/// The facts a committee works on.
pub fn brief(proposal: &str, ticker: &str, facts: &[String], sizing: Option<(f64, f64, Option<f64>)>, class: AssetClass) -> String {
    let mut out = format!("Proposal: {proposal}\nAsset: {ticker} ({})", class.label());
    if let Some((before, after, target)) = sizing {
        out.push_str(&format!(
            "\nIts class would go from {} to {} of the portfolio{}.",
            pct(before, 1),
            pct(after, 1),
            target.map(|t| format!(" (the user's target: {})", pct(t, 0))).unwrap_or_default()
        ));
    }
    for f in facts {
        out.push_str(&format!("\n{f}"));
    }
    out
}

pub fn for_model(decisions: &[Decision], today: Date, cal: &Calibration) -> String {
    if decisions.is_empty() {
        return "The decision journal is empty. Record a decision with wealth_decision (action record) when the user takes one: the asset, buy/sell/hold/avoid, the price, the reasons, what they expect and how sure they are.".into();
    }
    let mut out = format!("Decision journal ({} decisions):", decisions.len());
    for d in decisions.iter().rev().take(15) {
        out.push_str(&format!(
            "\n- {} {} {} {}{}{} — {}{}{}",
            d.id,
            d.date.br(),
            d.action,
            d.ticker,
            d.price.map(|p| format!(" at {}", brl(p))).unwrap_or_default(),
            d.confidence.map(|c| format!(", {}% sure", c)).unwrap_or_default(),
            crate::text::clip(&d.reasons, 140),
            if d.status != "taken" { format!(" [{}]", d.status) } else { String::new() },
            match (d.reviews.last(), next_review(d)) {
                (Some(r), _) => format!(" — last review {}: {}", r.date.br(), r.verdict),
                (None, Some(next)) => format!(" — review on {}", next.br()),
                _ => String::new(),
            }
        ));
    }
    let waiting = due(decisions, today);
    if !waiting.is_empty() {
        out.push_str(&format!(
            "\nReviews due now: {}. Ask the user, for each, whether it turned out right, wrong or mixed, and why.",
            waiting.iter().map(|d| format!("{} ({} {})", d.id, d.action, d.ticker)).collect::<Vec<_>>().join(", ")
        ));
    }
    if cal.judged > 0 {
        out.push_str(&format!("\nCalibration: {} of {} judged decisions right.", cal.right, cal.judged));
        for (band, n, ok) in &cal.bands {
            out.push_str(&format!(" {band} sure: {} right of {n}.", pct(*ok, 0)));
        }
    }
    if let Some(r) = cal.average_return {
        out.push_str(&format!(
            "\nAverage result since each decision: {}{}.",
            pct(r, 1),
            cal.average_cdi.map(|c| format!(" (the CDI over the same days: {})", pct(c, 1))).unwrap_or_default()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::{fixtures, value};

    fn d(s: &str) -> Date {
        Date::parse(s).unwrap()
    }

    fn decision(date: &str, ticker: &str, action: &str, price: f64, confidence: u8) -> Decision {
        Decision {
            id: new_id(d(date), ticker, 0),
            date: d(date),
            ticker: ticker.into(),
            action: action.into(),
            quantity: Some(10.0),
            price: Some(price),
            reasons: "VP descontado e dividendo estável".into(),
            expectation: "dividendo mantido".into(),
            confidence: Some(confidence),
            committee: String::new(),
            reviews: vec![],
            status: "taken".into(),
        }
    }

    #[test]
    fn decisions_come_back_and_build_a_calibration() {
        let today = d("2026-10-21");
        let mut list = vec![decision("2026-01-10", "GARE11", "buy", 8.0, 90), decision("2026-06-01", "TIMS3", "buy", 20.0, 60), decision("2026-10-01", "BTCI11", "buy", 9.0, 80)];
        let waiting: Vec<&str> = due(&list, today).iter().map(|d| d.ticker.as_str()).collect();
        assert_eq!(waiting, vec!["GARE11", "TIMS3"], "after 90 days; GARE11 is past 180 too");
        review(&mut list[0], today, "acertei", "o dividendo veio", Some(8.4));
        assert_eq!(list[0].reviews[0].verdict, "right");
        assert_eq!(due(&list, today).len(), 2, "GARE11 has a second date passed");
        review(&mut list[0], today, "certo", "", Some(8.4));
        review(&mut list[1], today, "errei", "a margem caiu", Some(19.2));
        assert_eq!(due(&list, today).len(), 0);
        assert_eq!(next_review(&list[0]), Some(d("2026-01-10").add_days(365)));
        let prices = |t: &str| match t {
            "GARE11" => Some(8.4),
            "TIMS3" => Some(19.2),
            _ => Some(9.0),
        };
        let cal = calibration(&list, prices, Some(15.0), today);
        assert_eq!((cal.judged, cal.right), (2, 1));
        assert_eq!(cal.bands.len(), 2);
        assert!((cal.average_return.unwrap() - (5.0 - 4.0 + 0.0) / 3.0).abs() < 1e-9);
        assert!(cal.average_cdi.unwrap() > 0.0);
        let text = for_model(&list, today, &cal);
        assert!(text.contains("Calibration: 1 of 2") && text.contains("review on"), "{text}");
        assert!(for_model(&[], today, &cal).contains("empty"));
    }

    #[test]
    fn the_committee_works_on_the_facts_and_the_targets() {
        let v = value(&fixtures::portfolio(), &fixtures::market());
        let s = sizing(&v, AssetClass::Fii, 420.0).unwrap();
        assert!(s.1 > s.0, "buying more FIIs raises their share");
        assert_eq!(s.2, Some(25.0));
        let text = brief("comprar 50 cotas de GARE11 a R$ 8,40", "GARE11", &["GARE11: P/VP 0,95; dividend health: healthy.".into()], Some(s), AssetClass::Fii);
        assert!(text.contains("the user's target: 25%") && text.contains("P/VP"));
        let p = protocol();
        assert!(p.contains("[advocate]") && p.contains("[critic]") && p.contains("pre-mortem") && p.contains("[synthesis]"));
    }
}
