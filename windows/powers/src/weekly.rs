//! The week in three minutes: a script the iPhone reads aloud (and saves as audio to share), made by this code from
//! the user's own records — the portfolio against a week ago (money added taken out), the week's best and worst moves,
//! the dividends that came and those ahead, what the reader read, the theses, the committee's meetings, and a lesson
//! of the glossary in the user's own assets. No brain needed: it is ready even when Hermes is not.

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::learn::{self, GLOSSARY};
use crate::library::{DocKind, Library};
use crate::model::{AssetClass, Market, Snapshot, Store};
use crate::text::{brl, pct};
use crate::{fii, portfolio, thesis};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Weekly {
    /// The Monday of the week it tells.
    pub week: Date,
    pub title: String,
    pub text: String,
    /// About how long it takes read aloud.
    pub seconds: u32,
}

/// The Monday of `day`'s week (`weekday` counts from Monday, 0).
pub fn monday(day: Date) -> Date {
    day.add_days(-(day.weekday() as i64))
}

fn signed_pct(v: f64) -> String {
    format!("{}{}", if v >= 0.0 { "mais " } else { "menos " }, pct(v.abs(), 1))
}

fn signed_pct_en(v: f64) -> String {
    format!("{}{}", if v >= 0.0 { "up " } else { "down " }, pct(v.abs(), 1))
}

/// The snapshot nearest to a week before the latest one (between 5 and 10 days before).
fn week_before<'a>(snaps: &'a [Snapshot], latest: &Snapshot) -> Option<&'a Snapshot> {
    snaps.iter().filter(|s| (5..=10).contains(&s.date.until(latest.date))).min_by_key(|s| (s.date.until(latest.date) - 7).abs())
}

/// The kind of a document, in Portuguese.
fn kind_pt(k: DocKind) -> &'static str {
    match k {
        DocKind::MaterialFact => "fato relevante",
        DocKind::Report => "relatório gerencial",
        DocKind::Release => "release de resultados",
        DocKind::Assembly => "assembleia",
        DocKind::Offering => "oferta de cotas",
        DocKind::Notice => "comunicado",
        DocKind::Other => "documento",
    }
}

/// The week's script, in Portuguese (`pt`) or English.
pub fn script(store: &Store, market: &Market, lib: Option<&Library>, today: Date, pt: bool) -> Weekly {
    let week = monday(today);
    let mut parts: Vec<String> = Vec::new();
    let tr = |a: &str, b: &str| if pt { a.to_string() } else { b.to_string() };
    parts.push(if pt { format!("Seu resumo da semana, {}.", today.br()) } else { format!("Your week, {}.", today.br()) });

    // The portfolio against a week ago.
    let v = portfolio::value(&store.portfolio, market);
    if v.value > 0.0 {
        let mut line = if pt { format!("Sua carteira vale {}", brl(v.value)) } else { format!("Your portfolio is worth {}", brl(v.value)) };
        if let Some(latest) = store.snapshots.iter().max_by_key(|s| s.date) {
            if let Some(before) = week_before(&store.snapshots, latest) {
                if before.value > 0.0 {
                    let change = (latest.value - before.value - (latest.cost - before.cost)) / before.value * 100.0;
                    line.push_str(&if pt { format!(", {} na semana, sem contar os aportes", signed_pct(change)) } else { format!(", {} over the week, money added aside", signed_pct_en(change)) });
                }
                // The week's best and worst moves of what is held.
                let mut moves: Vec<(String, f64)> = latest
                    .prices
                    .iter()
                    .filter_map(|(t, now)| before.prices.get(t).filter(|b| **b > 0.0).map(|b| (t.clone(), (now / b - 1.0) * 100.0)))
                    .collect();
                moves.sort_by(|a, b| b.1.total_cmp(&a.1));
                line.push('.');
                parts.push(line);
                if let (Some(best), Some(worst)) = (moves.first(), moves.last()) {
                    if moves.len() >= 2 && (best.1 - worst.1).abs() > 0.05 {
                        parts.push(if pt { format!("O destaque foi {}, {}; o pior, {}, {}.", best.0, signed_pct(best.1), worst.0, signed_pct(worst.1)) } else { format!("The best was {}, {}; the worst, {}, {}.", best.0, signed_pct_en(best.1), worst.0, signed_pct_en(worst.1)) });
                    }
                }
            } else {
                line.push('.');
                parts.push(line);
            }
        } else {
            line.push('.');
            parts.push(line);
        }
    } else {
        parts.push(tr("Ainda não há carteira no AXYON: conte no chat o que você tem, ou importe da B3.", "AXYON has no portfolio yet: tell it in the chat what you hold, or import B3's file."));
    }

    // The dividends of the week and those ahead.
    let mut came = 0.0;
    let mut came_list = Vec::new();
    let mut ahead = Vec::new();
    for p in store.portfolio.positions.iter().filter(|p| p.class == AssetClass::Fii && p.quantity > 0.0) {
        let Some(fund) = market.funds.get(&p.ticker) else { continue };
        let since = crate::invest::held_since(store, &p.ticker);
        for pay in fund.payouts.iter().filter(|x| !x.amortization && since.is_none_or(|s| x.record >= s)) {
            if let Some(paid) = pay.payment {
                if paid <= today && today.until(paid) > -7 {
                    let amount = pay.value * p.quantity;
                    came += amount;
                    came_list.push(format!("{} {}", p.ticker, brl(amount)));
                }
            }
        }
        for e in fii::events(fund, p.quantity, today, 0).into_iter().filter(|e| e.date >= today && e.date.until(today) > -14) {
            if e.kind == "payment" {
                if let Some(a) = e.amount {
                    ahead.push((e.date, format!("{} {} {}", p.ticker, brl(a), if pt { format!("em {}", e.date.short()) } else { format!("on {}", e.date.short()) })));
                }
            }
        }
    }
    if came > 0.0 {
        parts.push(if pt { format!("Caíram {} em proventos: {}.", brl(came), came_list.join(", ")) } else { format!("{} of dividends came in: {}.", brl(came), came_list.join(", ")) });
    }
    ahead.sort_by_key(|x| x.0);
    if !ahead.is_empty() {
        let list: Vec<String> = ahead.into_iter().take(4).map(|x| x.1).collect();
        parts.push(if pt { format!("Nos próximos dias: {}.", list.join("; ")) } else { format!("In the next days: {}.", list.join("; ")) });
    }

    // What the reader read this week.
    if let Some(lib) = lib {
        let read: Vec<_> = lib.docs.iter().filter(|d| d.is_read() && d.read.is_some_and(|r| r >= week.add_days(-7))).collect();
        if !read.is_empty() {
            let mut line = if pt { format!("O AXYON leu {} documento{} dos seus ativos.", read.len(), if read.len() == 1 { "" } else { "s" }) } else { format!("AXYON read {} document{} of your assets.", read.len(), if read.len() == 1 { "" } else { "s" }) };
            let mut top: Vec<_> = read.clone();
            top.sort_by(|a, b| a.kind.cmp(&b.kind));
            for d in top.iter().take(3) {
                line.push_str(&format!(" {}, {}: {}.", d.ticker, if pt { kind_pt(d.kind) } else { d.kind.label() }, d.headline().trim_end_matches('.')));
            }
            parts.push(line);
        }
    }

    // The theses.
    let states = thesis::states(store, market, lib, today);
    if !states.is_empty() {
        let at_risk: Vec<&thesis::State> = states.iter().filter(|s| !s.ok()).collect();
        if at_risk.is_empty() {
            parts.push(tr("Suas teses estão de pé.", "Your theses hold."));
        } else {
            for s in at_risk.iter().take(3) {
                let broken: Vec<String> = s.checks.iter().filter(|c| c.holds == Some(false)).map(|c| c.label.clone()).collect();
                let why = if broken.is_empty() { s.risks.first().map(|r| r.1.clone()).unwrap_or_default() } else { broken.join("; ") };
                parts.push(if pt { format!("Atenção à tese de {}: {}.", s.ticker, why.trim_end_matches('.')) } else { format!("Mind your thesis on {}: {}.", s.ticker, why.trim_end_matches('.')) });
            }
        }
    }

    // The committee's meetings this week.
    for verdict in store.verdicts.iter().filter(|x| x.date.is_some_and(|d| d >= week.add_days(-7))).rev().take(2) {
        let (f, a, _) = verdict.score();
        let vetoes = verdict.vetoed_by();
        let veto = if vetoes.is_empty() {
            String::new()
        } else if pt {
            format!(", com veto do {}", vetoes.iter().map(|v| crate::committee::mind(v).map(|m| m.name_pt).unwrap_or(v)).collect::<Vec<_>>().join(" e do "))
        } else {
            format!(", vetoed by {}", vetoes.join(" and "))
        };
        parts.push(if pt { format!("O comitê se reuniu sobre {}: {f} a favor, {a} contra{veto}.", verdict.ticker) } else { format!("The committee met on {}: {f} for, {a} against{veto}.", verdict.ticker) });
    }

    // The week's lesson, in the user's own assets when it can be.
    let usable: Vec<&learn::Term> = GLOSSARY.iter().filter(|t| t.metric.is_none_or(|m| learn::mine(store, market, lib, m, today).is_some())).collect();
    if !usable.is_empty() {
        let t = usable[(week.days() / 7).rem_euclid(usable.len() as i64) as usize];
        let mine = t.metric.and_then(|m| learn::mine(store, market, lib, m, today));
        parts.push(if pt {
            format!("Lição da semana: {}. {}{}", t.term, t.simple, mine.map(|m| format!(" Na sua carteira: {m}.")).unwrap_or_default())
        } else {
            format!("This week's lesson: {}. {}{}", t.term, t.simple, mine.map(|m| format!(" In your portfolio: {m}.")).unwrap_or_default())
        });
    }
    parts.push(tr("Isto não é recomendação de investimento: é o seu resumo, para decidir com calma. Até a próxima semana.", "This is not investment advice: it is your summary, to decide calmly. Until next week."));
    let text = parts.join("\n\n");
    let words = text.split_whitespace().count() as u32;
    Weekly { week, title: if pt { format!("Resumo da semana · {}", week.br()) } else { format!("Your week · {}", week.br()) }, text, seconds: words * 60 / 150 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::fixtures;
    use std::collections::BTreeMap;

    #[test]
    fn the_week_is_told_from_the_records_in_plain_words() {
        let today = Date::new(2026, 10, 23).unwrap();
        assert_eq!(monday(today), Date::new(2026, 10, 19).unwrap());
        assert_eq!(monday(Date::new(2026, 10, 19).unwrap()), Date::new(2026, 10, 19).unwrap());
        let market = fixtures::market();
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let mk = |d: Date, value: f64, gare: f64| Snapshot { date: d, value, cost: 5000.0, by_class: BTreeMap::new(), cost_by_class: BTreeMap::new(), income: 0.0, prices: [("GARE11".to_string(), gare), ("GGRC11".to_string(), 9.0)].into() };
        store.snapshots = vec![mk(today.add_days(-7), 5000.0, 8.0), mk(today, 5100.0, 8.4)];
        thesis::write(&mut store, &market, "GARE11", "DY acima de 50%", today);
        let w = script(&store, &market, None, today, true);
        assert!(w.text.starts_with("Seu resumo da semana, 23/10/2026."), "{}", w.text);
        assert!(w.text.contains("mais 2,0% na semana, sem contar os aportes"), "{}", w.text);
        assert!(w.text.contains("O destaque foi GARE11, mais 5,0%; o pior, GGRC11, mais 0,0%."), "{}", w.text);
        assert!(w.text.contains("Atenção à tese de GARE11: DY acima de 50%."), "{}", w.text);
        assert!(w.text.contains("Lição da semana:") && w.text.contains("não é recomendação"));
        assert!(w.seconds > 20 && w.seconds < 300, "{}", w.seconds);
        let en = script(&store, &market, None, today, false);
        assert!(en.text.starts_with("Your week") && en.text.contains("up 2,0% over the week"), "{}", en.text);
        let empty = script(&Store::default(), &market, None, today, true);
        assert!(empty.text.contains("Ainda não há carteira"));
    }
}
