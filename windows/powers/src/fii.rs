//! Real-estate funds as Zheus watches them. Their dividends' health: the dividend's trend against the net asset value
//! per share (a fund paying more than it earns sees its VP/cota sink), the month's return on its assets, cuts, the
//! price against the VP (P/VP), and, for paper funds, where the Focus survey says the rates are going. And the
//! guardian of their events: record and payment dates, assemblies, offerings and preemptive rights, material facts,
//! amortizations — each with what it asks of the user and by when.

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::model::FundData;
use crate::sources::bcb::Macro;
use crate::sources::cvm_fii::{subtype, FiiMonth};
use crate::sources::fnet::{Payout, Topic};
use crate::text::{brl, clip, pct};

/// How healthy a fund's dividend looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    Good,
    Watch,
    Risk,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundReport {
    pub ticker: String,
    pub name: String,
    pub cnpj: String,
    /// "papel", "tijolo", "hibrido", "fof", or "".
    pub kind: String,
    pub segment: String,
    pub price: Option<f64>,
    pub vp_share: Option<f64>,
    pub vp_date: Option<Date>,
    pub p_vp: Option<f64>,
    /// VP/cota change, percent, over six and twelve months.
    pub vp_change_6m: Option<f64>,
    pub vp_change_12m: Option<f64>,
    /// The month's dividend yield the fund reported, percent, and its twelve-month average.
    pub dy_month: Option<f64>,
    pub dy_avg_12m: Option<f64>,
    /// The month's return on its assets (the VP's own move), summed over six months, percent.
    pub asset_return_6m: Option<f64>,
    pub last_dividend: Option<f64>,
    pub avg_dividend_3m: Option<f64>,
    pub avg_dividend_12m: Option<f64>,
    /// Yearly yield at the price: twelve months of dividends over it, percent (fewer months known: at their pace).
    pub yield_12m: Option<f64>,
    /// How many of the last twelve months the dividends known cover.
    #[serde(default)]
    pub yield_months: u32,
    pub holders: Option<f64>,
    pub holders_change_12m: Option<f64>,
    /// Cash and liquid assets over net assets, percent.
    pub liquidity: Option<f64>,
    pub health: Health,
    /// Why, in short sentences (English, for the brain to tell in the user's language).
    pub signals: Vec<String>,
    /// Paper funds: a rough dividend six months ahead if the Focus survey is right (half CDI, half IPCA).
    pub projection_6m: Option<f64>,
}

fn months_back<'a>(months: &'a [FiiMonth], from: &FiiMonth, n: i32) -> Option<&'a FiiMonth> {
    let target = from.date.add_months(-n);
    months.iter().filter(|m| m.date <= target && m.vp_share.is_some()).max_by_key(|m| m.date)
}

fn change(now: Option<f64>, then: Option<f64>) -> Option<f64> {
    match (now, then) {
        (Some(a), Some(b)) if b > 0.0 => Some((a / b - 1.0) * 100.0),
        _ => None,
    }
}

fn average(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

/// The dividends per share of the last `n` record dates (newest first in, any order out).
fn last_dividends(payouts: &[Payout], today: Date, n: usize) -> Vec<f64> {
    let mut list: Vec<&Payout> = payouts.iter().filter(|p| !p.amortization && p.record <= today).collect();
    list.sort_by_key(|p| std::cmp::Reverse(p.record));
    // A notice published again (or corrected) for the same record date counts once.
    list.dedup_by_key(|p| p.record);
    list.into_iter().take(n).map(|p| p.value).collect()
}

/// The dividends of the twelve months to `today`, one per record date, and how many months they cover.
fn year_of_dividends(payouts: &[Payout], today: Date) -> (f64, usize) {
    let year_ago = today.add_months(-12);
    let mut list: Vec<&Payout> = payouts.iter().filter(|p| !p.amortization && p.record > year_ago && p.record <= today).collect();
    list.sort_by_key(|p| p.record);
    list.dedup_by_key(|p| p.record);
    let mut months: Vec<(i32, u32)> = list.iter().map(|p| (p.record.y, p.record.m)).collect();
    months.dedup();
    (list.iter().map(|p| p.value).sum(), months.len())
}

pub fn report(fund: &FundData, price: Option<f64>, economy: &Macro, today: Date) -> FundReport {
    let months: Vec<FiiMonth> = {
        let mut m = fund.months.clone();
        m.sort_by_key(|x| x.date);
        m
    };
    let latest = months.iter().rev().find(|m| m.vp_share.is_some());
    let info = months.last();
    let vp = latest.and_then(|m| m.vp_share);
    let vp_6 = latest.and_then(|l| months_back(&months, l, 6)).and_then(|m| m.vp_share);
    let vp_12 = latest.and_then(|l| months_back(&months, l, 12)).and_then(|m| m.vp_share);
    let recent: Vec<&FiiMonth> = months.iter().rev().take(12).collect();
    let dy_avg = average(&recent.iter().filter_map(|m| m.dividend_yield).collect::<Vec<_>>());
    let asset_6: Vec<f64> = months.iter().rev().take(6).filter_map(|m| m.return_assets).collect();
    let asset_return_6m = (!asset_6.is_empty()).then(|| asset_6.iter().sum());
    let holders_12 = latest.and_then(|l| months_back(&months, l, 12)).and_then(|m| m.holders);
    let last = last_dividends(&fund.payouts, today, 1).first().copied();
    let d3 = average(&last_dividends(&fund.payouts, today, 3));
    let d12_list = last_dividends(&fund.payouts, today, 12);
    let d12 = average(&d12_list);
    // Fewer than twelve months known (FundosNet gives the latest documents first): the months known, at a year's pace.
    let (twelve, covered) = year_of_dividends(&fund.payouts, today);
    let yearly = match covered {
        12.. => Some(twelve),
        3..=11 => Some(twelve / covered as f64 * 12.0),
        _ => None,
    };
    let kind = info.map(|m| subtype(m).to_string()).unwrap_or_default();
    let mut r = FundReport {
        ticker: fund.ticker.clone(),
        // A fund the CVM's reports do not list (read under the CNPJ the user gave): its name from FundosNet.
        name: info.map(|m| m.name.clone()).filter(|n| !n.is_empty()).or_else(|| fund.documents.iter().map(|d| d.fund.clone()).find(|n| !n.is_empty())).unwrap_or_default(),
        cnpj: fund.cnpj.clone(),
        kind: kind.clone(),
        segment: info.map(|m| m.segment.clone()).unwrap_or_default(),
        price,
        vp_share: vp,
        vp_date: latest.map(|m| m.date),
        p_vp: match (price, vp) {
            (Some(p), Some(v)) if v > 0.0 => Some(p / v),
            _ => None,
        },
        vp_change_6m: change(vp, vp_6),
        vp_change_12m: change(vp, vp_12),
        dy_month: latest.and_then(|m| m.dividend_yield),
        dy_avg_12m: dy_avg,
        asset_return_6m,
        last_dividend: last,
        avg_dividend_3m: d3,
        avg_dividend_12m: d12,
        yield_12m: match (price, yearly) {
            (Some(p), Some(y)) if p > 0.0 && y > 0.0 => Some(y / p * 100.0),
            _ => None,
        },
        yield_months: covered.min(12) as u32,
        holders: latest.and_then(|m| m.holders),
        holders_change_12m: change(latest.and_then(|m| m.holders), holders_12),
        liquidity: latest.and_then(|m| match (m.cash, m.net_assets) {
            (Some(c), Some(n)) if n > 0.0 => Some(c / n * 100.0),
            _ => None,
        }),
        health: Health::Unknown,
        signals: Vec::new(),
        projection_6m: None,
    };
    judge(&mut r, d12_list.len());
    if kind == "papel" {
        r.projection_6m = project(last, economy, today);
        if let (Some(now), Some(later)) = (last, r.projection_6m) {
            if later < now * 0.95 {
                r.signals.push(format!(
                    "Paper fund: if the Focus survey is right about the rates, its dividend could drift to about {} a share in six months (from {}).",
                    brl(later),
                    brl(now)
                ));
            }
        }
    }
    r
}

/// The verdict and its reasons.
fn judge(r: &mut FundReport, dividends_known: usize) {
    let mut risk = 0;
    let mut watch = 0;
    if let (Some(v12), Some(d)) = (r.vp_change_12m, r.avg_dividend_12m) {
        if v12 < -5.0 && d > 0.0 {
            risk += 1;
            r.signals.push(format!(
                "Its net asset value per share fell {} in twelve months while it kept paying: it may be paying out more than it earns, or its assets lost value.",
                pct(-v12, 1)
            ));
        } else if v12 < -2.0 {
            watch += 1;
            r.signals.push(format!("Its VP/cota fell {} in twelve months.", pct(-v12, 1)));
        }
    }
    if let Some(a) = r.asset_return_6m {
        if a < -3.0 {
            watch += 1;
            r.signals.push(format!("The return on its assets added up to {} over six months.", pct(a, 1)));
        }
    }
    if let (Some(d3), Some(d12)) = (r.avg_dividend_3m, r.avg_dividend_12m) {
        if dividends_known >= 6 && d3 < d12 * 0.85 {
            risk += 1;
            r.signals.push(format!("The last three dividends ({} on average) are {} below its twelve-month average ({}).", brl(d3), pct((1.0 - d3 / d12) * 100.0, 0), brl(d12)));
        } else if dividends_known >= 6 && d3 > d12 * 1.15 {
            watch += 1;
            r.signals.push(format!("The last dividends ({} on average) are well above the year's ({}): check whether something one-off (a sale, a fine) explains it.", brl(d3), brl(d12)));
        }
    }
    if let Some(pvp) = r.p_vp {
        if pvp < 0.85 {
            watch += 1;
            r.signals.push(format!("It trades at {:.2} times its VP/cota: the market is doubting something (a tenant, its credit) or it is cheap.", pvp));
        } else if pvp > 1.15 {
            watch += 1;
            r.signals.push(format!("It trades at {:.2} times its VP/cota: a premium.", pvp));
        }
    }
    if let Some(h) = r.holders_change_12m {
        if h < -5.0 {
            watch += 1;
            r.signals.push(format!("Holders fell {} in twelve months.", pct(-h, 0)));
        }
    }
    r.health = if r.vp_share.is_none() && r.last_dividend.is_none() {
        Health::Unknown
    } else if risk > 0 {
        Health::Risk
    } else if watch > 1 {
        Health::Watch
    } else {
        if r.signals.is_empty() {
            r.signals.push("Nothing unusual: its VP/cota and its dividends are steady.".into());
        }
        if watch == 1 {
            Health::Watch
        } else {
            Health::Good
        }
    };
}

/// A paper fund's dividend in six months if the CDI and the IPCA move the way the Focus survey expects: half its
/// income follows the CDI (the Selic), half the IPCA (a rough mix; the manager's report has the real one).
pub fn project(last: Option<f64>, economy: &Macro, today: Date) -> Option<f64> {
    let last = last?;
    let selic_now = economy.selic?;
    let selic_then = economy.selic_at(today.y).or(economy.selic_at(today.y + 1))?;
    let selic_next = economy.selic_at(today.y + 1).unwrap_or(selic_then);
    // Six months ahead: halfway between this year's end and the next one's, weighted by where in the year we are.
    let w = (today.m as f64 + 6.0 - 12.0).clamp(0.0, 12.0) / 12.0;
    let selic_6m = selic_then * (1.0 - w) + selic_next * w;
    let ipca_now = economy.ipca_12m.unwrap_or(4.5).max(0.5);
    let ipca_6m = economy.ipca_at(today.y + 1).unwrap_or(ipca_now);
    let factor = 0.5 * (selic_6m / selic_now.max(0.5)) + 0.5 * (ipca_6m / ipca_now);
    Some(last * factor)
}

// ── The guardian of events ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Importance {
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundEvent {
    pub ticker: String,
    pub date: Date,
    /// "record", "payment", "amortization", "assembly", "offering", "material_fact", "notice", "report".
    pub kind: String,
    pub title: String,
    /// What it asks of the user, if anything (English, for the brain).
    pub action: String,
    pub deadline: Option<Date>,
    pub url: Option<String>,
    pub importance: Importance,
    /// R$ for the user's shares, for payouts.
    pub amount: Option<f64>,
}

/// The events of a fund the user holds `quantity` of: payouts ahead (record and payment dates), and the documents of
/// the last `days` days that ask something of them.
pub fn events(fund: &FundData, quantity: f64, today: Date, days: i64) -> Vec<FundEvent> {
    let mut out = Vec::new();
    for p in &fund.payouts {
        let amount = Some(p.value * quantity).filter(|a| *a > 0.0);
        let what = if p.amortization { "amortization" } else { "dividend" };
        if p.record >= today && p.record.until(today) > -45 {
            out.push(FundEvent {
                ticker: fund.ticker.clone(),
                date: p.record,
                kind: "record".into(),
                title: format!("Record date of a {what} of {} a share", brl(p.value)),
                action: format!("Only shares held at the end of {} receive it.", p.record.br()),
                deadline: Some(p.record),
                url: Some(crate::sources::fnet::view_url(p.document)),
                importance: Importance::Normal,
                amount,
            });
        }
        if let Some(pay) = p.payment.filter(|d| *d >= today && d.until(today) > -45) {
            out.push(FundEvent {
                ticker: fund.ticker.clone(),
                date: pay,
                kind: if p.amortization { "amortization".into() } else { "payment".into() },
                title: format!("Payment of a {what} of {} a share", brl(p.value)),
                action: if p.amortization {
                    "An amortization returns capital: it lowers the average price (and the cost declared to the tax office), it is not income.".into()
                } else {
                    String::new()
                },
                deadline: None,
                url: Some(crate::sources::fnet::view_url(p.document)),
                importance: if p.amortization { Importance::High } else { Importance::Low },
                amount,
            });
        }
    }
    for d in &fund.documents {
        let Some(delivered) = d.delivered else { continue };
        if delivered.until(today) > days || delivered > today {
            continue;
        }
        let title = clip(&[d.category.as_str(), d.kind.as_str(), d.species.as_str(), d.info.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" — "), 160);
        let reference = d.reference_date();
        let (kind, action, deadline, importance) = match d.topic {
            Topic::Assembly => (
                "assembly",
                match reference.filter(|r| *r >= today) {
                    Some(r) => format!("An assembly on {}: read the proposal and vote before it (through the broker or the administrator's site).", r.br()),
                    None => "An assembly's documents: what was proposed or decided may change the fund.".to_string(),
                },
                reference.filter(|r| *r >= today),
                Importance::High,
            ),
            Topic::Offering => (
                "offering",
                "An offering of new shares: if it gives holders preemptive rights, they have a deadline to subscribe or to sell the rights; a price below the VP/cota dilutes those who stay out.".to_string(),
                None,
                Importance::High,
            ),
            Topic::MaterialFact => ("material_fact", "A material fact: read what changed (a tenant, a sale, a default) and what it does to the dividend.".to_string(), None, Importance::High),
            Topic::Payout if !d.structured => ("notice", "A payout notice.".to_string(), None, Importance::Low),
            Topic::Payout => continue,
            Topic::Report => ("report", "The manager's monthly report.".to_string(), None, Importance::Low),
            Topic::Notice => ("notice", String::new(), None, Importance::Normal),
            Topic::Filing | Topic::Other => continue,
        };
        out.push(FundEvent {
            ticker: fund.ticker.clone(),
            date: delivered,
            kind: kind.into(),
            title,
            action,
            deadline,
            url: Some(d.url()),
            importance,
            amount: None,
        });
    }
    out.sort_by(|a, b| a.importance.cmp(&b.importance).then(b.date.cmp(&a.date)));
    out
}

pub fn health_word(h: Health) -> &'static str {
    match h {
        Health::Good => "healthy",
        Health::Watch => "worth watching",
        Health::Risk => "at risk",
        Health::Unknown => "not enough data yet",
    }
}

/// A fund's report in words, for the brain.
pub fn report_for_model(r: &FundReport) -> String {
    let mut out = format!("{} — {}", r.ticker, if r.name.is_empty() { "(name unknown)".to_string() } else { r.name.clone() });
    if !r.kind.is_empty() || !r.segment.is_empty() {
        out.push_str(&format!(" [{}{}]", r.kind, if r.segment.is_empty() { String::new() } else { format!(", {}", r.segment) }));
    }
    out.push_str(&format!("\nDividend health: {}.", health_word(r.health)));
    let mut facts = Vec::new();
    if let Some(p) = r.price {
        facts.push(format!("price {}", brl(p)));
    }
    if let (Some(v), Some(d)) = (r.vp_share, r.vp_date) {
        facts.push(format!("VP/cota {} ({})", brl(v), d.month_key()));
    }
    if let Some(x) = r.p_vp {
        facts.push(format!("P/VP {:.2}", x));
    }
    if let Some(x) = r.vp_change_12m {
        facts.push(format!("VP 12m {}{}", if x >= 0.0 { "+" } else { "" }, pct(x, 1)));
    }
    if let Some(x) = r.last_dividend {
        facts.push(format!("last dividend {} a share", brl(x)));
    }
    if let Some(x) = r.avg_dividend_12m {
        facts.push(format!("12m average {}", brl(x)));
    }
    if let Some(x) = r.yield_12m {
        if r.yield_months >= 12 {
            facts.push(format!("12m yield {}", pct(x, 1)));
        } else {
            facts.push(format!("yield {} a year at the pace of the last {} months known", pct(x, 1), r.yield_months));
        }
    }
    if let Some(x) = r.dy_month {
        facts.push(format!("DY reported for the month {}", pct(x, 2)));
    }
    if let Some(x) = r.holders {
        facts.push(format!("{} holders", crate::text::grouped(x, 0)));
    }
    if let Some(x) = r.liquidity {
        facts.push(format!("liquidity {} of net assets", pct(x, 1)));
    }
    if !facts.is_empty() {
        out.push_str(&format!("\n{}.", facts.join("; ")));
    }
    for s in &r.signals {
        out.push_str(&format!("\n- {s}"));
    }
    out
}

pub fn events_for_model(list: &[FundEvent]) -> String {
    if list.is_empty() {
        return "No event ahead and no document asking anything of the user lately.".into();
    }
    let mut out = String::new();
    for e in list {
        out.push_str(&format!(
            "\n- [{}] {} {} — {}{}{}{}",
            match e.importance {
                Importance::High => "important",
                Importance::Normal => "notice",
                Importance::Low => "info",
            },
            e.ticker,
            e.date.br(),
            e.title,
            e.amount.map(|a| format!(" (≈ {} for the user's shares)", brl(a))).unwrap_or_default(),
            if e.action.is_empty() { String::new() } else { format!(". {}", e.action) },
            e.url.as_ref().map(|u| format!(" Document: {u}")).unwrap_or_default()
        ));
    }
    out.trim_start().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::bcb::Expectation;
    use crate::sources::fnet::{Document, Payout};

    fn d(s: &str) -> Date {
        Date::parse(s).unwrap()
    }

    fn month(date: &str, vp: f64, dy: f64, assets: f64, holders: f64) -> FiiMonth {
        FiiMonth {
            cnpj: "1".into(),
            date: d(date),
            name: "GGR COVEPI RENDA FII".into(),
            segment: "Logística".into(),
            mandate: "Renda".into(),
            vp_share: Some(vp),
            net_assets: Some(vp * 1000.0),
            dividend_yield: Some(dy),
            return_assets: Some(assets),
            holders: Some(holders),
            cash: Some(vp * 50.0),
            ..FiiMonth::default()
        }
    }

    fn payout(record: &str, value: f64) -> Payout {
        Payout { record: d(record), payment: Some(d(record).add_days(7)), value, amortization: false, tax_free: Some(true), period: String::new(), document: 7 }
    }

    fn fund(vps: &[f64], dividends: &[f64]) -> FundData {
        let start = d("2025-10-01");
        let months = vps.iter().enumerate().map(|(i, vp)| month(&start.add_months(i as i32).to_string(), *vp, 0.9, -0.5, 12000.0)).collect();
        let today = d("2026-10-21");
        let payouts = dividends.iter().rev().enumerate().map(|(i, v)| payout(&today.add_months(-(i as i32)).add_days(-7).to_string(), *v)).collect();
        FundData { ticker: "GGRC11".into(), cnpj: "1".into(), months, payouts, ..FundData::default() }
    }

    #[test]
    fn six_months_of_notices_are_a_years_pace_and_a_repeated_one_counts_once() {
        // As FundosNet gave GGRC11's: six monthly notices of R$ 0,10, two of them published twice.
        let today = d("2026-10-21");
        let mut f = FundData { ticker: "GGRC11".into(), ..FundData::default() };
        for (i, rec) in ["2026-05-04", "2026-06-01", "2026-07-01", "2026-08-03", "2026-09-01", "2026-10-01"].iter().enumerate() {
            f.payouts.push(Payout { document: 10 + i as u64, ..payout(rec, 0.10) });
        }
        f.payouts.push(Payout { document: 99, ..payout("2026-08-03", 0.10) });
        f.payouts.push(Payout { document: 98, ..payout("2026-06-01", 0.10) });
        let r = report(&f, Some(9.25), &Macro::default(), today);
        assert_eq!(r.yield_months, 6);
        assert!((r.yield_12m.unwrap() - 1.2 / 9.25 * 100.0).abs() < 1e-6, "{:?}", r.yield_12m);
        assert!((r.avg_dividend_12m.unwrap() - 0.10).abs() < 1e-9);
        assert!(report_for_model(&r).contains("at the pace of the last 6 months known"));
    }

    #[test]
    fn a_steady_fund_is_healthy() {
        let f = fund(&[9.5; 13], &[0.088; 12]);
        let r = report(&f, Some(9.12), &Macro::default(), d("2026-10-21"));
        assert_eq!(r.health, Health::Good, "{:?}", r.signals);
        assert!((r.p_vp.unwrap() - 9.12 / 9.5).abs() < 1e-9);
        assert_eq!(r.vp_change_12m, Some(0.0));
        assert_eq!(r.kind, "tijolo");
        assert!((r.yield_12m.unwrap() - 0.088 * 12.0 / 9.12 * 100.0).abs() < 1e-6);
        assert!(report_for_model(&r).contains("Dividend health: healthy"));
    }

    #[test]
    fn a_sinking_vp_and_a_cut_are_risks() {
        let vps: Vec<f64> = (0..13).map(|i| 10.0 - 0.1 * i as f64).collect();
        let mut divs = vec![0.10; 9];
        divs.extend([0.07, 0.07, 0.07]);
        let f = fund(&vps, &divs);
        let r = report(&f, Some(7.4), &Macro::default(), d("2026-10-21"));
        assert_eq!(r.health, Health::Risk);
        assert!(r.signals.iter().any(|s| s.contains("fell") && s.contains("twelve months")), "{:?}", r.signals);
        assert!(r.signals.iter().any(|s| s.contains("below its twelve-month average")));
        assert!(r.signals.iter().any(|s| s.contains("times its VP/cota")));
    }

    #[test]
    fn a_paper_funds_dividend_follows_the_expected_rates() {
        let econ = Macro {
            selic: Some(15.0),
            ipca_12m: Some(5.0),
            selic_expected: vec![
                Expectation { indicator: "Selic".into(), date: d("2026-10-16"), year: 2026, median: 15.0 },
                Expectation { indicator: "Selic".into(), date: d("2026-10-16"), year: 2027, median: 12.0 },
            ],
            ipca_expected: vec![Expectation { indicator: "IPCA".into(), date: d("2026-10-16"), year: 2027, median: 4.0 }],
            ..Macro::default()
        };
        let p = project(Some(0.10), &econ, d("2026-10-21")).unwrap();
        // Six months on (April 2027): a third of the way to 12 %; the IPCA at 4 % instead of 5 %.
        let selic_6m = 15.0 * (1.0 - 4.0 / 12.0) + 12.0 * (4.0 / 12.0);
        assert!((p - 0.10 * (0.5 * selic_6m / 15.0 + 0.5 * 0.8)).abs() < 1e-9);
        assert!(project(Some(0.1), &Macro::default(), d("2026-10-21")).is_none());
        let mut f = fund(&[9.0; 13], &[0.10; 12]);
        for m in &mut f.months {
            m.segment = "Títulos e Val. Mob.".into();
        }
        let r = report(&f, Some(9.0), &econ, d("2026-10-21"));
        assert_eq!(r.kind, "papel");
        assert!(r.projection_6m.unwrap() < 0.1);
        assert!(r.signals.iter().any(|s| s.contains("Focus")));
    }

    #[test]
    fn the_guardian_lists_what_needs_the_user() {
        let today = d("2026-10-21");
        let mut f = fund(&[9.5; 13], &[0.088; 12]);
        f.payouts.push(Payout { record: d("2026-10-30"), payment: Some(d("2026-11-07")), value: 0.09, amortization: false, tax_free: Some(true), period: String::new(), document: 11 });
        f.payouts.push(Payout { record: d("2026-10-30"), payment: Some(d("2026-11-07")), value: 0.05, amortization: true, tax_free: None, period: String::new(), document: 12 });
        let doc = |id: u64, topic: Topic, category: &str, reference: &str, delivered: &str| Document {
            id,
            category: category.into(),
            kind: String::new(),
            species: String::new(),
            reference: reference.into(),
            delivered: Date::parse(delivered),
            info: String::new(),
            topic,
            structured: false,
            fund: String::new(),
            trading: String::new(),
        };
        f.documents = vec![
            doc(1, Topic::Assembly, "Assembleia", "10/11/2026", "2026-10-10"),
            doc(2, Topic::MaterialFact, "Fato Relevante", "05/10/2026", "2026-10-05"),
            doc(3, Topic::Filing, "Informe Mensal", "09/2026", "2026-10-15"),
            doc(4, Topic::Offering, "Oferta Pública", "", "2026-06-01"),
        ];
        let list = events(&f, 150.0, today, 30);
        let kinds: Vec<&str> = list.iter().map(|e| e.kind.as_str()).collect();
        assert!(kinds.contains(&"record") && kinds.contains(&"payment") && kinds.contains(&"amortization"));
        assert!(kinds.contains(&"assembly") && kinds.contains(&"material_fact"));
        assert!(!kinds.contains(&"offering"), "too old");
        assert!(!kinds.iter().any(|k| *k == "filing"));
        assert_eq!(list[0].importance, Importance::High, "the important ones first");
        let assembly = list.iter().find(|e| e.kind == "assembly").unwrap();
        assert_eq!(assembly.deadline, Some(d("2026-11-10")));
        let record = list.iter().find(|e| e.kind == "record" && e.amount == Some(0.09 * 150.0)).unwrap();
        assert_eq!(record.deadline, Some(d("2026-10-30")));
        let text = events_for_model(&list);
        assert!(text.contains("[important] GGRC11") && text.contains("exibirDocumento?id=1"));
        assert!(events_for_model(&[]).contains("No event"));
    }
}
