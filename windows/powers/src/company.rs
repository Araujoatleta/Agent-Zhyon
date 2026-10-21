//! A listed company read from its own filings (the CVM's DFP and ITR): its quarters (the ITR's three-month figures,
//! or the year so far less the quarter before; the fourth from the year's statements), its last twelve months, the
//! ratios an investor looks at (margins, EBITDA, free cash flow, net debt over EBITDA, ROE, payout, growth), the
//! accounting red flags (profit up while cash from operations falls, receivables outgrowing sales, paying out more
//! than it earns…), and the user's living thesis: their own sentences as measurable conditions, checked at every
//! filing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::model::Condition;
use crate::sources::cvm_cia::Figures;
use crate::text::{brl, pct, plain};

/// One quarter's flows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quarter {
    pub end: Date,
    pub revenue: Option<f64>,
    pub gross_profit: Option<f64>,
    pub ebit: Option<f64>,
    pub financial_result: Option<f64>,
    pub net_income: Option<f64>,
    pub depreciation: Option<f64>,
    pub operating_cash: Option<f64>,
    pub capex: Option<f64>,
    pub dividends_paid: Option<f64>,
}

type Flow = fn(&Figures) -> Option<f64>;

const FLOWS: [(&str, Flow); 9] = [
    ("revenue", |f| f.revenue),
    ("gross_profit", |f| f.gross_profit),
    ("ebit", |f| f.ebit),
    ("financial_result", |f| f.financial_result),
    ("net_income", |f| f.net_income),
    ("depreciation", |f| f.depreciation),
    ("operating_cash", |f| f.operating_cash),
    ("capex", |f| f.capex),
    ("dividends_paid", |f| f.dividends_paid),
];

fn set(q: &mut Quarter, field: &str, v: Option<f64>) {
    match field {
        "revenue" => q.revenue = v,
        "gross_profit" => q.gross_profit = v,
        "ebit" => q.ebit = v,
        "financial_result" => q.financial_result = v,
        "net_income" => q.net_income = v,
        "depreciation" => q.depreciation = v,
        "operating_cash" => q.operating_cash = v,
        "capex" => q.capex = v,
        _ => q.dividends_paid = v,
    }
}

fn quarter_end(d: Date) -> bool {
    matches!((d.m, d.d), (3, 31) | (6, 30) | (9, 30) | (12, 31))
}

/// The quarters the figures allow, oldest first.
pub fn quarters(figures: &[Figures]) -> Vec<Quarter> {
    let flows: BTreeMap<(Date, Date), &Figures> = figures.iter().filter_map(|f| f.start.map(|s| ((s, f.end), f))).collect();
    let ends: Vec<Date> = {
        let mut e: Vec<Date> = flows.keys().map(|(_, e)| *e).filter(|e| quarter_end(*e)).collect();
        e.sort();
        e.dedup();
        e
    };
    let mut out = Vec::new();
    for end in ends {
        let quarter_start = end.add_months(-3).add_days(1).first_of_month();
        let year_start = Date { y: end.y, m: 1, d: 1 };
        let own = flows.get(&(quarter_start, end));
        let ytd = flows.get(&(year_start, end));
        let prev_end = Date { y: end.y, m: end.m.saturating_sub(3), d: 1 }.last_of_month();
        let prev_ytd = if end.m > 3 { flows.get(&(year_start, prev_end)) } else { None };
        let mut q = Quarter { end, ..Quarter::default() };
        for (field, get) in FLOWS {
            let value = own.and_then(|f| get(f)).or_else(|| match (ytd.and_then(|f| get(f)), end.m) {
                (Some(y), 3) => Some(y),
                (Some(y), _) => prev_ytd.and_then(|p| get(p)).map(|p| y - p),
                (None, _) => None,
            });
            set(&mut q, field, value);
        }
        if q.revenue.is_some() || q.net_income.is_some() || q.operating_cash.is_some() {
            out.push(q);
        }
    }
    out
}

/// The latest balance sheet, and the one a year before it.
fn balances(figures: &[Figures]) -> (Option<&Figures>, Option<&Figures>) {
    let mut list: Vec<&Figures> = figures.iter().filter(|f| f.start.is_none()).collect();
    list.sort_by_key(|f| f.end);
    let latest = list.last().copied();
    let year_before = latest.and_then(|l| list.iter().rev().find(|f| f.end <= l.end.add_months(-12)).copied());
    (latest, year_before)
}

/// Twelve months of flows ending at a quarter.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Twelve {
    pub end: Date,
    pub revenue: Option<f64>,
    pub gross_profit: Option<f64>,
    pub ebit: Option<f64>,
    pub ebitda: Option<f64>,
    pub financial_result: Option<f64>,
    pub net_income: Option<f64>,
    pub operating_cash: Option<f64>,
    pub capex: Option<f64>,
    pub free_cash_flow: Option<f64>,
    pub dividends_paid: Option<f64>,
}

fn sum4(qs: &[&Quarter], get: impl Fn(&Quarter) -> Option<f64>) -> Option<f64> {
    if qs.len() < 4 {
        return None;
    }
    qs.iter().map(|q| get(q)).sum::<Option<f64>>()
}

/// The twelve months ending at `end`, from four consecutive quarters.
pub fn twelve(quarters: &[Quarter], end: Date) -> Option<Twelve> {
    let qs: Vec<&Quarter> = quarters.iter().filter(|q| q.end <= end && q.end > end.add_months(-12)).collect();
    if qs.len() < 4 {
        return None;
    }
    let ebit = sum4(&qs, |q| q.ebit);
    let dep = sum4(&qs, |q| q.depreciation);
    let ocf = sum4(&qs, |q| q.operating_cash);
    let capex = sum4(&qs, |q| q.capex.or(Some(0.0)));
    Some(Twelve {
        end,
        revenue: sum4(&qs, |q| q.revenue),
        gross_profit: sum4(&qs, |q| q.gross_profit),
        ebit,
        ebitda: match (ebit, dep) {
            (Some(e), Some(d)) => Some(e + d),
            _ => None,
        },
        financial_result: sum4(&qs, |q| q.financial_result),
        net_income: sum4(&qs, |q| q.net_income),
        operating_cash: ocf,
        capex,
        free_cash_flow: match (ocf, capex) {
            (Some(o), Some(c)) => Some(o - c),
            _ => None,
        },
        dividends_paid: sum4(&qs, |q| q.dividends_paid.or(Some(0.0))),
    })
}

/// The ratios, now and a year before.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub end: Option<Date>,
    pub now: BTreeMap<String, f64>,
    pub year_before: BTreeMap<String, f64>,
}

fn ratio(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) if b.abs() > 0.0 => Some(a / b * 100.0),
        _ => None,
    }
}

fn metrics_of(t: &Twelve, balance: Option<&Figures>) -> BTreeMap<String, f64> {
    let mut m = BTreeMap::new();
    let mut put = |k: &str, v: Option<f64>| {
        if let Some(v) = v.filter(|v| v.is_finite()) {
            m.insert(k.to_string(), v);
        }
    };
    put("revenue", t.revenue);
    put("ebit", t.ebit);
    put("ebitda", t.ebitda);
    put("net_income", t.net_income);
    put("operating_cash", t.operating_cash);
    put("capex", t.capex);
    put("fcf", t.free_cash_flow);
    put("dividends_paid", t.dividends_paid);
    put("gross_margin", ratio(t.gross_profit, t.revenue));
    put("ebit_margin", ratio(t.ebit, t.revenue));
    put("ebitda_margin", ratio(t.ebitda, t.revenue));
    put("net_margin", ratio(t.net_income, t.revenue));
    put("fcf_margin", ratio(t.free_cash_flow, t.revenue));
    put("payout", ratio(t.dividends_paid, t.net_income.filter(|n| *n > 0.0)));
    put("interest_burden", ratio(t.financial_result.map(|f| -f), t.ebit.filter(|e| *e > 0.0)));
    if let Some(b) = balance {
        let net_debt = b.debt.map(|d| d - b.cash.unwrap_or(0.0) - b.investments.unwrap_or(0.0));
        put("net_debt", net_debt);
        put("net_debt_with_leases", net_debt.map(|n| n + b.leases.unwrap_or(0.0)));
        put("net_debt_ebitda", match (net_debt, t.ebitda) {
            (Some(n), Some(e)) if e > 0.0 => Some(n / e),
            _ => None,
        });
        put("roe", ratio(t.net_income, b.equity.filter(|e| *e > 0.0)));
        put("equity", b.equity);
        put("receivables", b.receivables);
        put("cash", b.cash.map(|c| c + b.investments.unwrap_or(0.0)));
    }
    m
}

pub fn metrics(figures: &[Figures]) -> Metrics {
    let qs = quarters(figures);
    let Some(last) = qs.last().map(|q| q.end) else { return Metrics::default() };
    let (balance, balance_before) = balances(figures);
    let mut out = Metrics { end: Some(last), ..Metrics::default() };
    if let Some(t) = twelve(&qs, last) {
        out.now = metrics_of(&t, balance);
    }
    if let Some(t) = twelve(&qs, last.add_months(-12).last_of_month()) {
        out.year_before = metrics_of(&t, balance_before);
    }
    for (k, label) in [("revenue", "revenue_growth"), ("net_income", "net_income_growth"), ("ebitda", "ebitda_growth"), ("dividends_paid", "dividends_growth")] {
        if let (Some(now), Some(then)) = (out.now.get(k), out.year_before.get(k)) {
            if then.abs() > 0.0 {
                let g = (now / then - 1.0) * 100.0 * then.signum();
                out.now.insert(label.to_string(), g);
            }
        }
    }
    out
}

/// What the user's sentences can be measured against, with what each means.
pub const METRICS: &[(&str, &str)] = &[
    ("revenue_growth", "net revenue, last twelve months vs the twelve before, %"),
    ("net_income_growth", "net income growth, twelve months, %"),
    ("ebitda_growth", "EBITDA growth, twelve months, %"),
    ("dividends_growth", "dividends paid growth, twelve months, %"),
    ("gross_margin", "gross margin, %"),
    ("ebit_margin", "operating (EBIT) margin, %"),
    ("ebitda_margin", "EBITDA margin, %"),
    ("net_margin", "net margin, %"),
    ("fcf_margin", "free cash flow over revenue, %"),
    ("roe", "net income over equity, %"),
    ("payout", "dividends and interest on equity paid over net income, %"),
    ("net_debt_ebitda", "net debt over EBITDA, times"),
    ("interest_burden", "net financial expense over EBIT, %"),
    ("net_debt", "net debt (loans less cash), R$"),
    ("fcf", "free cash flow, twelve months, R$"),
    ("revenue", "net revenue, twelve months, R$"),
    ("ebitda", "EBITDA, twelve months, R$"),
    ("net_income", "net income, twelve months, R$"),
    ("dividends_paid", "dividends and interest on equity paid, twelve months, R$"),
];

/// A sentence of the user's thesis as a condition, when it is plain enough: "margem EBITDA acima de 45%",
/// "dívida caindo", "ROE maior que 15", "payout abaixo de 100%". The brain normally sends conditions already.
pub fn parse_condition(sentence: &str) -> Option<Condition> {
    let p = plain(sentence);
    let metric = if p.contains("divida liquida ebitda") || p.contains("alavancagem") {
        "net_debt_ebitda"
    } else if p.contains("margem ebitda") {
        "ebitda_margin"
    } else if p.contains("margem liquida") {
        "net_margin"
    } else if p.contains("margem bruta") {
        "gross_margin"
    } else if p.contains("margem ebit") || p.contains("margem operacional") {
        "ebit_margin"
    } else if p.contains("roe") || p.contains("retorno sobre") {
        "roe"
    } else if p.contains("payout") {
        "payout"
    } else if p.contains("fluxo de caixa livre") || p.contains("fcf") {
        "fcf"
    } else if p.contains("divida") {
        "net_debt"
    } else if p.contains("receita") || p.contains("faturamento") {
        if p.contains("cresc") || p.contains("subindo") || p.contains("aument") { "revenue_growth" } else { "revenue" }
    } else if p.contains("lucro") {
        if p.contains("cresc") || p.contains("subindo") || p.contains("aument") { "net_income_growth" } else { "net_income" }
    } else if p.contains("dividendo") || p.contains("proventos") {
        "dividends_paid"
    } else if p.contains("ebitda") {
        "ebitda"
    } else {
        return None;
    };
    let number = p.split(' ').find_map(|w| crate::text::num(w).filter(|_| w.chars().any(|c| c.is_ascii_digit())));
    let op = if p.contains("caindo") || p.contains("diminu") || p.contains("reduz") || p.contains("menor que o ano") {
        "falling"
    } else if p.contains("subindo") || p.contains("crescendo") || p.contains("aumentando") || (p.contains("cresc") && number.is_none()) {
        "rising"
    } else if p.contains("abaixo") || p.contains("menor") || p.contains("menos de") || p.contains("ate ") || p.contains("no maximo") {
        "<"
    } else if p.contains("acima") || p.contains("maior") || p.contains("mais de") || p.contains("pelo menos") || p.contains("no minimo") {
        ">"
    } else if number.is_some() {
        ">="
    } else {
        return None;
    };
    if matches!(op, "<" | ">" | ">=") && number.is_none() {
        return None;
    }
    Some(Condition { metric: metric.into(), op: op.into(), value: number.unwrap_or(0.0), label: sentence.trim().to_string() })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub condition: Condition,
    /// None: not enough data to tell.
    pub holds: Option<bool>,
    pub now: Option<f64>,
    pub before: Option<f64>,
}

pub fn check(m: &Metrics, conditions: &[Condition]) -> Vec<Check> {
    conditions
        .iter()
        .map(|c| {
            let now = m.now.get(&c.metric).copied();
            let before = m.year_before.get(&c.metric).copied();
            let holds = match c.op.as_str() {
                ">" => now.map(|v| v > c.value),
                ">=" => now.map(|v| v >= c.value),
                "<" => now.map(|v| v < c.value),
                "<=" => now.map(|v| v <= c.value),
                // A growth rising is a growth above zero (it is already a change against a year before).
                "rising" if c.metric.ends_with("_growth") => now.map(|v| v > 0.0),
                "falling" if c.metric.ends_with("_growth") => now.map(|v| v < 0.0),
                "rising" => now.zip(before).map(|(a, b)| a > b),
                "falling" => now.zip(before).map(|(a, b)| a < b),
                _ => None,
            };
            Check { condition: c.clone(), holds, now, before }
        })
        .collect()
}

/// The accounting red flags of the last twelve months.
pub fn red_flags(m: &Metrics) -> Vec<String> {
    let g = |k: &str| m.now.get(k).copied();
    let b = |k: &str| m.year_before.get(k).copied();
    let mut out = Vec::new();
    if let (Some(ni), Some(ni0), Some(cfo), Some(cfo0)) = (g("net_income"), b("net_income"), g("operating_cash"), b("operating_cash")) {
        if ni > ni0 * 1.05 && cfo < cfo0 * 0.95 {
            out.push("Profit rose while the cash from operations fell: check the quality of the earnings (receivables, one-offs).".into());
        }
    }
    if let (Some(r), Some(r0), Some(growth)) = (g("receivables"), b("receivables"), g("revenue_growth")) {
        if r0 > 0.0 {
            let rg = (r / r0 - 1.0) * 100.0;
            if rg - growth > 15.0 {
                out.push(format!("Receivables grew {} while revenue grew {}: sales may be on longer credit.", pct(rg, 0), pct(growth, 0)));
            }
        }
    }
    if let Some(x) = g("net_debt_ebitda").filter(|x| *x > 3.0) {
        out.push(format!("Net debt is {:.1} times its EBITDA: heavily indebted.", x));
    }
    if let Some(x) = g("payout").filter(|x| *x > 100.0) {
        out.push(format!("It paid out {} of its profit: more than it earned.", pct(x, 0)));
    }
    if let Some(x) = g("fcf").filter(|x| *x < 0.0) {
        out.push(format!("Free cash flow was negative ({}) over twelve months.", brl(x)));
    }
    if let (Some(a), Some(a0)) = (g("ebitda_margin"), b("ebitda_margin")) {
        if a < a0 - 3.0 {
            out.push(format!("The EBITDA margin fell from {} to {}.", pct(a0, 1), pct(a, 1)));
        }
    }
    if let Some(x) = g("interest_burden").filter(|x| *x > 50.0) {
        out.push(format!("Financial expenses take {} of its operating profit.", pct(x, 0)));
    }
    out
}

fn show(metric: &str, v: f64) -> String {
    if matches!(metric, "net_debt" | "fcf" | "revenue" | "ebitda" | "net_income" | "dividends_paid" | "operating_cash" | "capex" | "ebit" | "equity" | "cash" | "receivables" | "net_debt_with_leases") {
        let (n, unit) = if v.abs() >= 1e9 { (v / 1e9, " bi") } else if v.abs() >= 1e6 { (v / 1e6, " mi") } else { (v, "") };
        format!("R$ {}{unit}", crate::text::grouped(n, if unit.is_empty() { 0 } else { 2 }))
    } else if metric == "net_debt_ebitda" {
        format!("{v:.2}x")
    } else {
        pct(v, 1)
    }
}

/// The company in words, for the brain.
pub fn for_model(ticker: &str, name: &str, m: &Metrics, checks: &[Check], flags: &[String]) -> String {
    let mut out = format!("{ticker} — {name}");
    match m.end {
        Some(end) => out.push_str(&format!(" (CVM filings, twelve months to {})", end.br())),
        None => {
            out.push_str(": no quarterly figures from the CVM yet.");
            return out;
        }
    }
    let keys = ["revenue", "revenue_growth", "ebitda", "ebitda_margin", "net_income", "net_margin", "net_income_growth", "fcf", "net_debt", "net_debt_ebitda", "roe", "payout", "dividends_paid"];
    let parts: Vec<String> = keys
        .iter()
        .filter_map(|k| {
            m.now.get(*k).map(|v| {
                let before = m.year_before.get(*k).map(|b| format!(" (a year before {})", show(k, *b))).unwrap_or_default();
                format!("{k}: {}{before}", show(k, *v))
            })
        })
        .collect();
    out.push_str(&format!("\n{}", parts.join("; ")));
    if !checks.is_empty() {
        out.push_str("\nThe user's thesis:");
        for c in checks {
            let mark = match c.holds {
                Some(true) => "holds",
                Some(false) => "BROKEN",
                None => "cannot tell yet",
            };
            let label = if c.condition.label.is_empty() { format!("{} {} {}", c.condition.metric, c.condition.op, c.condition.value) } else { c.condition.label.clone() };
            out.push_str(&format!("\n- {label}: {mark}{}", c.now.map(|v| format!(" (now {})", show(&c.condition.metric, v))).unwrap_or_default()));
        }
    }
    if flags.is_empty() {
        out.push_str("\nNo accounting red flag in the last twelve months.");
    } else {
        out.push_str("\nRed flags:");
        for f in flags {
            out.push_str(&format!("\n- {f}"));
        }
    }
    out
}

#[cfg(test)]
pub mod fixtures {
    use super::*;

    /// Two years of quarters of a TIM-like company: the ITR's own three-month figures for Q1–Q3 (with the year so
    /// far), the DFC as the year so far, the year's statements for Q4, and the balance sheets.
    pub fn figures(growth: f64) -> Vec<Figures> {
        let mut out = Vec::new();
        for (y, scale) in [(2025, 1.0), (2026, 1.0 + growth)] {
            let mut ytd_rev = 0.0;
            let mut ytd_cfo = 0.0;
            for q in 1..=4u32 {
                let end = Date { y, m: q * 3, d: 1 }.last_of_month();
                let start = Date { y, m: q * 3 - 2, d: 1 };
                let rev = 6.0e9 * scale;
                ytd_rev += rev;
                ytd_cfo += 3.0e9 * scale;
                if q < 4 {
                    out.push(Figures {
                        start: Some(start),
                        end,
                        revenue: Some(rev),
                        gross_profit: Some(rev * 0.5),
                        ebit: Some(rev * 0.25),
                        net_income: Some(rev * 0.15),
                        financial_result: Some(-rev * 0.03),
                        ..Figures::default()
                    });
                }
                out.push(Figures {
                    start: Some(Date { y, m: 1, d: 1 }),
                    end,
                    revenue: Some(ytd_rev),
                    gross_profit: Some(ytd_rev * 0.5),
                    ebit: Some(ytd_rev * 0.25),
                    net_income: Some(ytd_rev * 0.15),
                    financial_result: Some(-ytd_rev * 0.03),
                    depreciation: Some(ytd_rev * 0.22),
                    operating_cash: Some(ytd_cfo),
                    capex: Some(ytd_rev * 0.18),
                    dividends_paid: Some(ytd_rev * 0.1),
                    ..Figures::default()
                });
                out.push(Figures {
                    start: None,
                    end,
                    cash: Some(4.0e9),
                    debt: Some(6.0e9 - (y - 2025) as f64 * 1.0e9),
                    leases: Some(12.0e9),
                    equity: Some(25.0e9),
                    receivables: Some(3.0e9 * scale),
                    ..Figures::default()
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::figures;
    use super::*;

    #[test]
    fn quarters_come_from_three_month_figures_or_the_year_so_far() {
        let qs = quarters(&figures(0.10));
        assert_eq!(qs.len(), 8);
        let q4 = qs.iter().find(|q| q.end == Date::new(2026, 12, 31).unwrap()).unwrap();
        assert!((q4.revenue.unwrap() - 6.6e9).abs() < 1.0, "the year less nine months");
        let q2 = qs.iter().find(|q| q.end == Date::new(2026, 6, 30).unwrap()).unwrap();
        assert!((q2.operating_cash.unwrap() - 3.3e9).abs() < 1.0, "the cash flow is only the year so far");
        assert!((q2.depreciation.unwrap() - 6.6e9 * 0.22).abs() < 1.0);
    }

    #[test]
    fn ratios_red_flags_and_the_living_thesis() {
        let m = metrics(&figures(0.10));
        assert_eq!(m.end, Date::new(2026, 12, 31));
        let ebitda_margin = m.now["ebitda_margin"];
        assert!((ebitda_margin - 47.0).abs() < 1e-6, "EBIT 25 % + D&A 22 %");
        assert!((m.now["revenue_growth"] - 10.0).abs() < 1e-6);
        assert!((m.now["net_debt"] - 1.0e9).abs() < 1.0, "5 bi of loans less 4 bi of cash");
        assert!(m.now["net_debt"] < m.year_before["net_debt"]);
        assert!((m.now["payout"] - 100.0 / 1.5).abs() < 1e-6);
        assert!((m.now["roe"] - 26.4e9 * 0.15 / 25.0e9 * 100.0).abs() < 1e-6);
        let conditions: Vec<Condition> = ["margem EBITDA acima de 45%", "dívida caindo", "ROE maior que 20", "payout abaixo de 50%", "receita crescendo"]
            .iter()
            .map(|s| parse_condition(s).unwrap())
            .collect();
        assert_eq!(conditions[0], Condition { metric: "ebitda_margin".into(), op: ">".into(), value: 45.0, label: "margem EBITDA acima de 45%".into() });
        assert_eq!(conditions[1].op, "falling");
        assert_eq!(conditions[4].metric, "revenue_growth");
        let checks = check(&m, &conditions);
        let holds: Vec<Option<bool>> = checks.iter().map(|c| c.holds).collect();
        assert_eq!(holds, vec![Some(true), Some(true), Some(false), Some(false), Some(true)]);
        assert!(parse_condition("gosto da empresa").is_none());
        let flags = red_flags(&m);
        assert!(flags.is_empty(), "{flags:?}");
        let text = for_model("TIMS3", "TIM S.A.", &m, &checks, &flags);
        assert!(text.contains("ebitda_margin: 47,0%") && text.contains("BROKEN") && text.contains("No accounting red flag"), "{text}");
    }

    #[test]
    fn red_flags_show_up() {
        let mut m = Metrics::default();
        m.now.insert("net_income".into(), 120.0);
        m.year_before.insert("net_income".into(), 100.0);
        m.now.insert("operating_cash".into(), 80.0);
        m.year_before.insert("operating_cash".into(), 100.0);
        m.now.insert("net_debt_ebitda".into(), 3.5);
        m.now.insert("payout".into(), 140.0);
        m.now.insert("fcf".into(), -10.0);
        m.now.insert("ebitda_margin".into(), 30.0);
        m.year_before.insert("ebitda_margin".into(), 36.0);
        assert_eq!(red_flags(&m).len(), 5);
        assert!(for_model("X", "Y", &Metrics::default(), &[], &[]).contains("no quarterly figures"));
    }
}
