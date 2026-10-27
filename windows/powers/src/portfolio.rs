//! The portfolio's digital twin: what it is worth at B3's last close, its classes against the user's own targets, the
//! road to those targets without selling (no tax, no fee), where this month's contribution and dividends go, its
//! passive income and the "magic number" of each fund, scenarios (the Selic, the IPCA, the dollar, the stock market),
//! and the shadow portfolio — the same money always kept on the targets — to measure what straying from them cost.
//!
//! Math on the user's own numbers and targets, never advice: what to buy and when stays with the user.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::model::{AssetClass, Market, Portfolio, Position, Snapshot};
use crate::sources::bcb::Macro;
use crate::text::{brl, grouped, pct};

/// One holding, valued.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holding {
    pub ticker: String,
    pub class: AssetClass,
    pub subtype: String,
    pub quantity: f64,
    pub average_price: f64,
    /// The price: the session's (Google Finance) while it is newer than B3's last close, else that close.
    pub price: Option<f64>,
    /// The price is the session's.
    #[serde(default)]
    pub live: bool,
    pub cost: f64,
    pub value: f64,
    /// Percent of the whole portfolio.
    pub weight: f64,
    pub gain: f64,
    pub gain_pct: Option<f64>,
    /// The value is the cost (no quote and no value told).
    pub unpriced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassView {
    pub class: AssetClass,
    pub label: String,
    pub value: f64,
    pub cost: f64,
    /// Percent of the portfolio.
    pub weight: f64,
    /// The user's target, percent.
    pub target: Option<f64>,
    /// R$ to add (positive) or that is over (negative) to be on target today.
    pub gap: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Valuation {
    pub holdings: Vec<Holding>,
    pub classes: Vec<ClassView>,
    pub value: f64,
    pub cost: f64,
    pub gain: f64,
    pub gain_pct: Option<f64>,
    pub cash: f64,
    /// The day of the quotes.
    pub priced_on: Option<Date>,
    /// When the newest of the session's prices used was (unix seconds), when some are.
    #[serde(default)]
    pub live_at: Option<i64>,
}

/// The user's targets as fractions adding up to 1 (whatever they add up to), or empty.
pub fn targets(p: &Portfolio) -> BTreeMap<AssetClass, f64> {
    let sum: f64 = p.targets.values().filter(|v| **v > 0.0).sum();
    if sum <= 0.0 {
        return BTreeMap::new();
    }
    p.targets.iter().filter(|(_, v)| **v > 0.0).map(|(c, v)| (*c, v / sum)).collect()
}

fn value_of(position: &Position, market: &Market) -> (f64, Option<f64>, bool) {
    if position.class.quoted() {
        if let Some(price) = market.price(&position.ticker) {
            return (position.quantity * price, Some(price), false);
        }
        if let Some(v) = position.current_value {
            return (v, None, false);
        }
        return (position.cost(), None, true);
    }
    match position.current_value {
        Some(v) => (v, None, false),
        None => (position.cost(), None, true),
    }
}

pub fn value(p: &Portfolio, market: &Market) -> Valuation {
    let mut holdings: Vec<Holding> = p
        .positions
        .iter()
        .map(|pos| {
            let (value, price, unpriced) = value_of(pos, market);
            let cost = pos.cost();
            Holding {
                ticker: pos.ticker.clone(),
                class: pos.class,
                subtype: pos.subtype.clone(),
                quantity: pos.quantity,
                average_price: pos.average_price,
                price,
                live: pos.class.quoted() && market.live_now(&pos.ticker).is_some(),
                cost,
                value,
                weight: 0.0,
                gain: value - cost,
                gain_pct: (cost > 0.0).then(|| (value / cost - 1.0) * 100.0),
                unpriced,
            }
        })
        .collect();
    let total: f64 = holdings.iter().map(|h| h.value).sum();
    for h in &mut holdings {
        h.weight = if total > 0.0 { h.value / total * 100.0 } else { 0.0 };
    }
    let t = targets(p);
    let mut by_class: BTreeMap<AssetClass, (f64, f64)> = BTreeMap::new();
    for h in &holdings {
        let e = by_class.entry(h.class).or_default();
        e.0 += h.value;
        e.1 += h.cost;
    }
    for c in t.keys() {
        by_class.entry(*c).or_default();
    }
    let classes = by_class
        .into_iter()
        .map(|(class, (value, cost))| ClassView {
            class,
            label: class.label().to_string(),
            value,
            cost,
            weight: if total > 0.0 { value / total * 100.0 } else { 0.0 },
            target: t.get(&class).map(|f| f * 100.0),
            gap: if t.is_empty() { 0.0 } else { t.get(&class).map(|f| f * total - value).unwrap_or(-value) },
        })
        .collect();
    let cost: f64 = holdings.iter().map(|h| h.cost).sum();
    Valuation {
        holdings,
        classes,
        value: total,
        cost,
        gain: total - cost,
        gain_pct: (cost > 0.0).then(|| (total / cost - 1.0) * 100.0),
        cash: p.cash,
        priced_on: market.quotes_day,
        live_at: p.positions.iter().filter(|pos| pos.class.quoted()).filter_map(|pos| market.live_now(&pos.ticker).map(|l| l.at)).max(),
    }
}

/// What it takes to reach the targets by adding money only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Road {
    /// R$ to add, all of it to the classes under target.
    pub to_add: f64,
    /// The class that sets it (the one most over its target).
    pub over: Option<AssetClass>,
    /// Classes held with no target (only selling would bring them to zero).
    pub untargeted: Vec<AssetClass>,
    /// Months at the user's usual monthly contribution.
    pub months: Option<f64>,
}

pub fn road(v: &Valuation, p: &Portfolio) -> Option<Road> {
    let t = targets(p);
    if t.is_empty() || v.value <= 0.0 {
        return None;
    }
    let mut needed = v.value;
    let mut over = None;
    for c in &v.classes {
        if let Some(f) = t.get(&c.class) {
            let total = c.value / f;
            if total > needed + 0.005 {
                needed = total;
                over = Some(c.class);
            }
        }
    }
    let untargeted = v.classes.iter().filter(|c| c.value > 0.0 && !t.contains_key(&c.class)).map(|c| c.class).collect();
    let to_add = (needed - v.value).max(0.0);
    let months = p.monthly_contribution.filter(|m| *m > 0.0).map(|m| to_add / m);
    Some(Road { to_add, over, untargeted, months })
}

/// Where one amount goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slice {
    pub class: AssetClass,
    pub amount: f64,
    /// The user's holdings of that class, the least weighted first, with how many shares the amount buys of each
    /// at the last close.
    pub options: Vec<(String, f64, Option<f64>)>,
}

/// Where `amount` goes to bring the classes nearer their targets, without selling: first to fill what is under its
/// target once the amount is added, then by the targets.
pub fn allocate(v: &Valuation, p: &Portfolio, amount: f64) -> Vec<Slice> {
    let t = targets(p);
    if t.is_empty() || amount <= 0.0 {
        return Vec::new();
    }
    let total_after = v.value + amount;
    let held = |c: AssetClass| v.classes.iter().find(|x| x.class == c).map(|x| x.value).unwrap_or(0.0);
    let deficits: BTreeMap<AssetClass, f64> = t.iter().map(|(c, f)| (*c, (f * total_after - held(*c)).max(0.0))).collect();
    let sum: f64 = deficits.values().sum();
    let mut slices: BTreeMap<AssetClass, f64> = BTreeMap::new();
    if sum >= amount && sum > 0.0 {
        for (c, d) in &deficits {
            slices.insert(*c, amount * d / sum);
        }
    } else {
        for (c, d) in &deficits {
            slices.insert(*c, *d);
        }
        let rest = amount - sum;
        for (c, f) in &t {
            *slices.entry(*c).or_default() += rest * f;
        }
    }
    slices
        .into_iter()
        .filter(|(_, a)| *a >= 0.01)
        .map(|(class, amount)| {
            let mut mine: Vec<&Holding> = v.holdings.iter().filter(|h| h.class == class).collect();
            mine.sort_by(|a, b| a.value.partial_cmp(&b.value).unwrap_or(std::cmp::Ordering::Equal));
            let options = mine.iter().map(|h| (h.ticker.clone(), h.weight, h.price.map(|pr| (amount / pr).floor()))).collect();
            Slice { class, amount, options }
        })
        .collect()
}

/// A fund's income as Zheus estimates it from its announced payouts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Income {
    pub ticker: String,
    /// The last dividend per share (R$).
    pub last: Option<f64>,
    /// The average of the last twelve months' dividends per share, a month.
    pub average: Option<f64>,
    /// R$ a month for the user's shares, at the last dividend.
    pub monthly: f64,
    /// Shares whose monthly dividend buys one more share (price / dividend).
    pub magic_number: Option<f64>,
    /// Yearly yield at the last close (twelve months of dividends over the price), percent.
    pub yield_12m: Option<f64>,
}

pub fn income(v: &Valuation, market: &Market, today: Date) -> Vec<Income> {
    let year_ago = today.add_months(-12);
    v.holdings
        .iter()
        .filter(|h| h.class == AssetClass::Fii)
        .map(|h| {
            let payouts: Vec<&crate::sources::fnet::Payout> = market
                .funds
                .get(&h.ticker)
                .map(|f| f.payouts.iter().filter(|p| !p.amortization).collect())
                .unwrap_or_default();
            let last = payouts.iter().max_by_key(|p| p.record).map(|p| p.value);
            let recent: Vec<f64> = payouts.iter().filter(|p| p.record > year_ago && p.record <= today).map(|p| p.value).collect();
            let twelve: f64 = recent.iter().sum();
            let average = (!recent.is_empty()).then(|| twelve / 12.0_f64.min(recent.len() as f64).max(1.0));
            let price = h.price;
            Income {
                ticker: h.ticker.clone(),
                last,
                average,
                monthly: last.map(|d| d * h.quantity).unwrap_or(0.0),
                magic_number: match (price, last) {
                    (Some(p), Some(d)) if d > 0.0 => Some((p / d).ceil()),
                    _ => None,
                },
                yield_12m: match price {
                    Some(p) if p > 0.0 && !recent.is_empty() => Some(twelve / p * 100.0 * (12.0 / recent.len().max(1) as f64).min(12.0)),
                    _ => None,
                },
            }
        })
        .collect()
}

/// The capital, at the funds' own yield today, that pays `goal` a month; and what is missing.
pub fn income_goal(incomes: &[Income], v: &Valuation, goal: f64) -> Option<(f64, f64)> {
    let monthly: f64 = incomes.iter().map(|i| i.monthly).sum();
    let fii_value: f64 = v.holdings.iter().filter(|h| h.class == AssetClass::Fii).map(|h| h.value).sum();
    if monthly <= 0.0 || fii_value <= 0.0 {
        return None;
    }
    let monthly_yield = monthly / fii_value;
    let capital = goal / monthly_yield;
    Some((capital, (capital - fii_value).max(0.0)))
}

// ── Scenarios ─────────────────────────────────────────────────────────────────

/// A change of the economy over the next twelve months.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Scenario {
    /// Percentage points (−2: the Selic falls two points).
    pub selic: f64,
    pub ipca: f64,
    /// Percent the dollar moves.
    pub dollar: f64,
    /// Percent the stock market moves (besides what the rates do).
    pub stocks: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioLine {
    pub class: AssetClass,
    pub label: String,
    pub now: f64,
    pub after: f64,
    /// Change of the yearly income, R$ (fixed income and funds).
    pub income_change: f64,
    pub why: String,
}

/// Rough rules of thumb, one per kind of asset, to compare scenarios side by side (not a forecast):
/// brick funds −4 % a point of Selic (their cap rates follow the rates), paper funds' income follows the CDI and the
/// IPCA, shares −3 % a point plus the market's move, a foreign-index ETF follows the dollar, post-fixed income earns
/// the new rate, fixed and IPCA-linked bonds −5 % a point (about five years of duration).
pub fn scenario(v: &Valuation, economy: &Macro, s: &Scenario) -> Vec<ScenarioLine> {
    let selic_now = economy.selic.unwrap_or(15.0).max(0.5);
    let ipca_now = economy.ipca_12m.unwrap_or(4.5).max(0.5);
    let mut lines: BTreeMap<AssetClass, ScenarioLine> = BTreeMap::new();
    for h in &v.holdings {
        let (factor, income, why) = match (h.class, h.subtype.as_str()) {
            (AssetClass::Fii, "papel") => {
                let yearly = h.value * 0.12;
                let change = yearly * (0.5 * s.selic / selic_now + 0.5 * s.ipca / ipca_now);
                (1.0 - 0.01 * s.selic.max(0.0), change, "paper funds: income follows the CDI and the IPCA")
            }
            (AssetClass::Fii, _) => (1.0 - 0.04 * s.selic, h.value * 0.10 * s.ipca / 100.0, "brick funds: −4 % a point of Selic, rents follow inflation"),
            (AssetClass::Acao, _) => (1.0 - 0.03 * s.selic + s.stocks / 100.0, 0.0, "shares: −3 % a point of Selic, plus the market"),
            (AssetClass::Etf | AssetClass::Bdr, "usd") | (AssetClass::Bdr, _) => (1.0 + s.dollar / 100.0 + s.stocks / 100.0, 0.0, "foreign assets: the dollar and the market abroad"),
            (AssetClass::Etf, _) => (1.0 - 0.03 * s.selic + s.stocks / 100.0, 0.0, "an ETF of the market"),
            (AssetClass::RendaFixa | AssetClass::Tesouro, "ipca") => (1.0 - 0.05 * s.selic, h.value * s.ipca / 100.0, "IPCA-linked bonds: −5 % a point, plus the inflation"),
            (AssetClass::RendaFixa | AssetClass::Tesouro, "pre") => (1.0 - 0.05 * s.selic, 0.0, "fixed-rate bonds: −5 % a point"),
            (AssetClass::RendaFixa | AssetClass::Tesouro, _) => (1.0, h.value * s.selic / 100.0, "post-fixed: earns the new rate"),
            (AssetClass::Cripto | AssetClass::Outro, _) => (1.0 + s.dollar / 100.0, 0.0, "follows the dollar, roughly"),
        };
        let line = lines.entry(h.class).or_insert_with(|| ScenarioLine {
            class: h.class,
            label: h.class.label().to_string(),
            now: 0.0,
            after: 0.0,
            income_change: 0.0,
            why: why.to_string(),
        });
        line.now += h.value;
        line.after += h.value * factor.max(0.0);
        line.income_change += income;
    }
    lines.into_values().collect()
}

// ── Snapshots and the shadow portfolio ───────────────────────────────────────

pub fn snapshot(v: &Valuation, incomes: &[Income], today: Date) -> Snapshot {
    let mut by_class = BTreeMap::new();
    let mut cost_by_class = BTreeMap::new();
    for c in &v.classes {
        if c.value > 0.0 || c.cost > 0.0 {
            by_class.insert(c.class, c.value);
            cost_by_class.insert(c.class, c.cost);
        }
    }
    Snapshot { date: today, value: v.value, cost: v.cost, by_class, cost_by_class, income: incomes.iter().map(|i| i.monthly).sum() }
}

/// Adds today's snapshot, replacing one of the same day, and thins out the old ones (one a week past 60 days).
pub fn keep_snapshot(list: &mut Vec<Snapshot>, s: Snapshot) {
    list.retain(|x| x.date != s.date);
    list.push(s);
    list.sort_by_key(|x| x.date);
    let Some(last) = list.last().map(|x| x.date) else { return };
    let mut kept: Vec<Snapshot> = Vec::new();
    for x in list.drain(..) {
        let old = x.date.until(last) > 60;
        if old {
            if let Some(prev) = kept.last() {
                if prev.date.until(x.date) < 7 {
                    continue;
                }
            }
        }
        kept.push(x);
    }
    *list = kept;
}

/// The real portfolio against its shadow — the same money, kept on the targets at every step — since the first
/// snapshot: each class earns what the user's own holdings of it earned between two snapshots (money added is taken
/// out by the change of cost), and a class the user does not hold earns the CDI (fixed income) or nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shadow {
    pub since: Date,
    /// Percent.
    pub real: f64,
    pub shadow: f64,
    /// Shadow minus real, in points: positive means the targets would have done better.
    pub difference: f64,
}

pub fn shadow(snapshots: &[Snapshot], p: &Portfolio, cdi: Option<f64>) -> Option<Shadow> {
    let t = targets(p);
    if t.is_empty() || snapshots.len() < 2 {
        return None;
    }
    let mut real = 1.0;
    let mut shade = 1.0;
    for pair in snapshots.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if a.value <= 0.0 {
            continue;
        }
        let days = a.date.until(b.date).max(0) as f64;
        let cdi_period = cdi.map(|r| (1.0 + r / 100.0).powf(days / 365.0) - 1.0).unwrap_or(0.0);
        let class_return = |c: AssetClass| -> f64 {
            let v0 = a.by_class.get(&c).copied().unwrap_or(0.0);
            let v1 = b.by_class.get(&c).copied().unwrap_or(0.0);
            let flow = b.cost_by_class.get(&c).copied().unwrap_or(0.0) - a.cost_by_class.get(&c).copied().unwrap_or(0.0);
            if v0 > 0.0 {
                (v1 - v0 - flow) / v0
            } else if matches!(c, AssetClass::RendaFixa | AssetClass::Tesouro) {
                cdi_period
            } else {
                0.0
            }
        };
        let r: f64 = a.by_class.iter().map(|(c, v)| v / a.value * class_return(*c)).sum();
        let s: f64 = t.iter().map(|(c, f)| f * class_return(*c)).sum();
        real *= 1.0 + r;
        shade *= 1.0 + s;
    }
    let real = (real - 1.0) * 100.0;
    let shade = (shade - 1.0) * 100.0;
    Some(Shadow { since: snapshots[0].date, real, shadow: shade, difference: shade - real })
}

/// "15:16": a time in Brasília (UTC−3).
pub fn brasilia_clock(at: i64) -> String {
    let secs = (at - 3 * 3600).rem_euclid(86_400);
    format!("{:02}:{:02}", secs / 3600, secs % 3600 / 60)
}

/// The portfolio in words, for the brain.
pub fn for_model(v: &Valuation, p: &Portfolio, incomes: &[Income], economy: &Macro) -> String {
    let mut out = String::new();
    let day = match (v.live_at, v.priced_on) {
        (Some(at), close) => format!(
            " (prices from Google Finance during the session, the newest at {} in Brasília, some 15 minutes late{})",
            brasilia_clock(at),
            close.map(|d| format!("; B3's close of {} for the rest", d.br())).unwrap_or_default()
        ),
        (None, Some(d)) => format!(" (B3 close of {})", d.br()),
        (None, None) => String::new(),
    };
    out.push_str(&format!(
        "Portfolio worth {}{day}, cost {}, {} ({}).",
        brl(v.value),
        brl(v.cost),
        if v.gain >= 0.0 { format!("up {}", brl(v.gain)) } else { format!("down {}", brl(-v.gain)) },
        v.gain_pct.map(|g| pct(g, 2)).unwrap_or_else(|| "—".into())
    ));
    out.push_str("\nBy class (actual vs the user's target):");
    for c in &v.classes {
        let target = c.target.map(|t| format!(" vs target {}", pct(t, 0))).unwrap_or_else(|| " (no target)".into());
        out.push_str(&format!("\n- {}: {} = {}{}", c.label, brl(c.value), pct(c.weight, 1), target));
    }
    out.push_str("\nHoldings:");
    for h in &v.holdings {
        let price = h.price.map(|p| format!(", {} {}", if h.live { "now" } else { "last close" }, brl(p))).unwrap_or_else(|| if h.unpriced { ", no quote (valued at cost)".into() } else { String::new() });
        out.push_str(&format!(
            "\n- {} ({}{}): {} × avg {}{price} = {} ({}), {}",
            h.ticker,
            h.class.label(),
            if h.subtype.is_empty() { String::new() } else { format!(", {}", h.subtype) },
            grouped(h.quantity, if h.quantity.fract() == 0.0 { 0 } else { 2 }),
            brl(h.average_price),
            brl(h.value),
            pct(h.weight, 1),
            h.gain_pct.map(|g| format!("{}{}", if g >= 0.0 { "+" } else { "" }, pct(g, 1))).unwrap_or_default()
        ));
    }
    let monthly: f64 = incomes.iter().map(|i| i.monthly).sum();
    if monthly > 0.0 {
        out.push_str(&format!("\nEstimated passive income from the funds: {} a month at their last dividends.", brl(monthly)));
        if let Some(goal) = p.income_goal.filter(|g| *g > 0.0) {
            out.push_str(&format!(" Goal {} a month ({} of it).", brl(goal), pct(monthly / goal * 100.0, 0)));
            if let Some((capital, missing)) = income_goal(incomes, v, goal) {
                out.push_str(&format!(" At today's yield it takes about {} in funds: {} more.", brl(capital), brl(missing)));
            }
        }
    }
    if let Some(r) = road(v, p) {
        if r.to_add > 0.5 {
            out.push_str(&format!(
                "\nTo reach the targets by adding money only (no selling, no tax): about {} more{}{}.",
                brl(r.to_add),
                r.over.map(|c| format!(", set by {} being the most over its target", c.label())).unwrap_or_default(),
                r.months.map(|m| format!(" (~{} months at {} a month)", grouped(m.ceil(), 0), brl(p.monthly_contribution.unwrap_or(0.0)))).unwrap_or_default()
            ));
        }
    }
    if let (Some(selic), Some(cdi)) = (economy.selic, economy.cdi) {
        out.push_str(&format!("\nEconomy: Selic {}, CDI {}", pct(selic, 2), pct(cdi, 2)));
        if let Some(i) = economy.ipca_12m {
            out.push_str(&format!(", IPCA 12 months {}", pct(i, 2)));
        }
        out.push('.');
    }
    out
}

#[cfg(test)]
pub mod fixtures {
    use super::*;
    use crate::model::{FundData, Portfolio, Position};
    use crate::sources::b3::Quote;
    use crate::sources::fnet::Payout;

    pub fn position(t: &str, class: AssetClass, q: f64, pm: f64, sub: &str) -> Position {
        Position { ticker: t.into(), class, quantity: q, average_price: pm, subtype: sub.into(), current_value: None, note: String::new() }
    }

    /// A portfolio like many users hold: mostly real-estate funds, a share, an ETF and a little fixed income (made-up
    /// figures).
    pub fn portfolio() -> Portfolio {
        let mut p = Portfolio {
            positions: vec![
                position("TIMS3", AssetClass::Acao, 30.0, 18.60, ""),
                position("GGRC11", AssetClass::Fii, 150.0, 9.20, "tijolo"),
                position("GARE11", AssetClass::Fii, 180.0, 8.55, "tijolo"),
                position("BTCI11", AssetClass::Fii, 140.0, 8.90, "papel"),
                position("PORD11", AssetClass::Fii, 120.0, 8.45, "papel"),
                position("SPXB11", AssetClass::Etf, 40.0, 16.75, ""),
                Position { current_value: Some(49.8), ..position("Tesouro Selic 2031", AssetClass::RendaFixa, 1.0, 49.0, "pos") },
            ],
            monthly_contribution: Some(500.0),
            income_goal: Some(1000.0),
            ..Portfolio::default()
        };
        for c in [AssetClass::Acao, AssetClass::Fii, AssetClass::Etf, AssetClass::RendaFixa] {
            p.targets.insert(c, 25.0);
        }
        p
    }

    fn quote(t: &str, close: f64) -> Quote {
        Quote {
            ticker: t.into(),
            date: Date::new(2026, 10, 20).unwrap(),
            open: close,
            high: close,
            low: close,
            average: close,
            close,
            trades: 1,
            volume: 1.0,
            bdi: String::new(),
            kind: String::new(),
            name: String::new(),
            isin: String::new(),
            market: 10,
        }
    }

    pub fn market() -> Market {
        let mut m = Market { quotes_day: Date::new(2026, 10, 20), ..Market::default() };
        for (t, c) in [("TIMS3", 19.20), ("GGRC11", 9.12), ("GARE11", 8.40), ("BTCI11", 9.05), ("PORD11", 8.60), ("SPXB11", 17.12)] {
            m.quotes.insert(t.into(), quote(t, c));
        }
        let payout = |d: &str, v: f64| Payout {
            record: Date::parse(d).unwrap(),
            payment: None,
            value: v,
            amortization: false,
            tax_free: Some(true),
            period: String::new(),
            document: 1,
        };
        for (t, values) in [("GGRC11", 0.088), ("GARE11", 0.075), ("BTCI11", 0.10), ("PORD11", 0.11)] {
            let mut f = FundData { ticker: t.into(), ..FundData::default() };
            for i in 0..12 {
                f.payouts.push(payout(&Date::new(2026, 10, 14).unwrap().add_months(-i).to_string(), values));
            }
            m.funds.insert(t.into(), f);
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn today() -> Date {
        Date::new(2026, 10, 21).unwrap()
    }

    #[test]
    fn the_portfolio_is_valued_at_the_last_close_and_against_its_targets() {
        let (p, m) = (portfolio(), market());
        let v = value(&p, &m);
        let fii = v.classes.iter().find(|c| c.class == AssetClass::Fii).unwrap();
        let expected_fii = 150.0 * 9.12 + 180.0 * 8.40 + 140.0 * 9.05 + 120.0 * 8.60;
        assert!((fii.value - expected_fii).abs() < 1e-6);
        assert!(fii.weight > 75.0, "FIIs are most of it");
        assert_eq!(fii.target, Some(25.0));
        assert!(fii.gap < 0.0, "over its target");
        let tesouro = v.holdings.iter().find(|h| h.class == AssetClass::RendaFixa).unwrap();
        assert_eq!(tesouro.value, 49.8, "a value the user told");
        assert!(!tesouro.unpriced);
        assert!((v.value - (expected_fii + 30.0 * 19.20 + 40.0 * 17.12 + 49.8)).abs() < 1e-6);
        let text = for_model(&v, &p, &[], &Macro::default());
        assert!(text.contains("FIIs: R$") && text.contains("vs target 25%"));
    }

    #[test]
    fn the_road_to_the_targets_without_selling() {
        let (p, m) = (portfolio(), market());
        let v = value(&p, &m);
        let r = road(&v, &p).unwrap();
        let fii = v.classes.iter().find(|c| c.class == AssetClass::Fii).unwrap().value;
        assert!((r.to_add - (fii / 0.25 - v.value)).abs() < 1e-6, "the FIIs set it: their value over 25 %");
        assert_eq!(r.over, Some(AssetClass::Fii));
        assert!((r.months.unwrap() - r.to_add / 500.0).abs() < 1e-9);
        // 500 this month: all of it to the classes under target, nothing to the FIIs.
        let slices = allocate(&v, &p, 500.0);
        let total: f64 = slices.iter().map(|s| s.amount).sum();
        assert!((total - 500.0).abs() < 1e-6);
        assert!(slices.iter().all(|s| s.class != AssetClass::Fii));
        let rf = slices.iter().find(|s| s.class == AssetClass::RendaFixa).unwrap();
        let acao = slices.iter().find(|s| s.class == AssetClass::Acao).unwrap();
        assert!(rf.amount > acao.amount, "the furthest under its target gets the most");
        let shares = acao.options[0].2.unwrap();
        assert_eq!(shares, (acao.amount / 19.20).floor());
        // A huge amount fills every gap, then follows the targets.
        let big = allocate(&v, &p, 100_000.0);
        assert!(big.iter().any(|s| s.class == AssetClass::Fii));
        assert!(allocate(&v, &Portfolio::default(), 100.0).is_empty());
    }

    #[test]
    fn passive_income_magic_numbers_and_the_goal() {
        let (p, m) = (portfolio(), market());
        let v = value(&p, &m);
        let incomes = income(&v, &m, today());
        let ggrc = incomes.iter().find(|i| i.ticker == "GGRC11").unwrap();
        assert_eq!(ggrc.last, Some(0.088));
        assert!((ggrc.monthly - 150.0 * 0.088).abs() < 1e-9);
        assert_eq!(ggrc.magic_number, Some((9.12f64 / 0.088).ceil()));
        assert!((ggrc.yield_12m.unwrap() - 0.088 * 12.0 / 9.12 * 100.0).abs() < 1e-6);
        let (capital, missing) = income_goal(&incomes, &v, 1000.0).unwrap();
        assert!(capital > missing && missing > 0.0);
        let text = for_model(&v, &p, &incomes, &Macro::default());
        assert!(text.contains("passive income"), "{text}");
    }

    #[test]
    fn scenarios_move_each_kind_its_own_way() {
        let (p, m) = (portfolio(), market());
        let v = value(&p, &m);
        let econ = Macro { selic: Some(15.0), ipca_12m: Some(5.0), ..Macro::default() };
        let lines = scenario(&v, &econ, &Scenario { selic: -2.0, ..Scenario::default() });
        let fii = lines.iter().find(|l| l.class == AssetClass::Fii).unwrap();
        assert!(fii.after > fii.now, "lower rates lift brick funds");
        assert!(fii.income_change < 0.0, "and paper funds earn less");
        let rf = lines.iter().find(|l| l.class == AssetClass::RendaFixa).unwrap();
        assert_eq!(rf.after, rf.now, "post-fixed keeps its value");
        assert!(rf.income_change < 0.0);
        let up = scenario(&v, &econ, &Scenario { dollar: 10.0, ..Scenario::default() });
        assert!(up.iter().all(|l| l.class != AssetClass::Etf || (l.after - l.now).abs() < 1e-9), "an ETF without 'usd' is the Brazilian market");
    }

    #[test]
    fn snapshots_and_the_shadow_portfolio() {
        let p = portfolio();
        let mut snaps = Vec::new();
        let mk = |d: Date, fii: f64, acao: f64, fii_cost: f64| Snapshot {
            date: d,
            value: fii + acao,
            cost: fii_cost + 100.0,
            by_class: [(AssetClass::Fii, fii), (AssetClass::Acao, acao)].into(),
            cost_by_class: [(AssetClass::Fii, fii_cost), (AssetClass::Acao, 100.0)].into(),
            income: 0.0,
        };
        keep_snapshot(&mut snaps, mk(today().add_days(-30), 900.0, 100.0, 900.0));
        // The FIIs gained 10 % (plus 100 added), the shares lost 20 %.
        keep_snapshot(&mut snaps, mk(today(), 1090.0, 80.0, 1000.0));
        keep_snapshot(&mut snaps, mk(today(), 1090.0, 80.0, 1000.0));
        assert_eq!(snaps.len(), 2, "one a day");
        let s = shadow(&snaps, &p, Some(15.0)).unwrap();
        // Real: 90 % in FIIs (+10 %) and 10 % in shares (−20 %) = +7 %.
        assert!((s.real - 7.0).abs() < 1e-6, "{s:?}");
        // Targets: a quarter each of FIIs (+10 %), shares (−20 %), ETFs (0) and fixed income (the CDI).
        let cdi = ((1.15f64).powf(30.0 / 365.0) - 1.0) * 100.0;
        assert!((s.shadow - (0.25 * 10.0 - 0.25 * 20.0 + 0.25 * cdi)).abs() < 1e-6);
        assert!(s.difference < 0.0, "the user's tilt to FIIs did better here");
        // Old snapshots thin out to one a week.
        let mut many: Vec<Snapshot> = (0..200).map(|i| mk(today().add_days(-200 + i), 1.0, 1.0, 1.0)).collect();
        let last = many.pop().unwrap();
        keep_snapshot(&mut many, last);
        assert!(many.len() < 100 && many.len() > 60);
    }
}
