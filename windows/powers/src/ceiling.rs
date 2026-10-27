//! Ceiling prices ("preço teto"): the most an asset is worth paying by a few classic methods, worked out from what
//! Coucou reads (B3's close, the CVM's statements and fund reports, FundosNet's dividends) and the user's own choices;
//! and an opportunity when the close is below one of them. Rules of thumb on stated assumptions, never advice.
//!
//! - Bazin: a year of dividends a share over the yield the user wants (Décio Bazin's 6 % for shares; for funds, by
//!   default, the CDI after the 15 % income tax, what the safe alternative pays against a fund's tax-free dividend).
//!   Projected: a fund's dividends at the pace of its last three months (a paper fund's moved by the Focus survey);
//!   a share's earnings grown by their last year's growth, paid out as last year.
//! - Graham: √(22.5 × earnings a share × book value a share).
//! - Peter Lynch: earnings a share × (growth + dividend yield): a fair P/E equal to both, growth capped at 25 %.
//! - P/VP: a fund's VP per share times the most P/VP the user pays (1 by default).
//! - The user's own ceiling, when they set one.

use serde::Serialize;

use crate::date::Date;
use crate::model::{AssetClass, Ceilings, CompanyData, FundData, Market, Store};
use crate::sources::bcb::Macro;
use crate::text::{brl, grouped, pct};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ceiling {
    /// "bazin", "bazin_projected", "graham", "lynch", "pvp" or "manual".
    pub method: &'static str,
    pub value: f64,
    /// What it was worked out from, in English for the brain.
    pub basis: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Assessment {
    pub ticker: String,
    pub class: Option<AssetClass>,
    pub price: Option<f64>,
    pub ceilings: Vec<Ceiling>,
    /// The methods that could not be worked out, and why.
    pub missing: Vec<String>,
    /// The methods that make an opportunity for this asset.
    pub watched: Vec<&'static str>,
}

/// One asset below one of its ceilings.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opportunity {
    pub ticker: String,
    pub method: &'static str,
    pub ceiling: f64,
    pub price: f64,
    /// How far below the ceiling the close is, percent.
    pub below: f64,
}

/// The method's name, in English.
pub fn label(method: &str) -> &'static str {
    match method {
        "bazin" => "Bazin",
        "bazin_projected" => "projected Bazin",
        "graham" => "Graham",
        "lynch" => "Peter Lynch",
        "pvp" => "P/VP",
        _ => "the user's own ceiling",
    }
}

pub const METHODS: &[&str] = &["bazin", "bazin_projected", "graham", "lynch", "pvp"];

/// The yield a fund must pay, percent a year, and where it comes from.
pub fn fund_yield(prefs: &Ceilings, economy: &Macro) -> (f64, String) {
    if let Some(y) = prefs.fund_yield.filter(|y| *y > 0.0) {
        return (y, "the yield the user asked for".into());
    }
    match economy.cdi.filter(|c| *c > 0.0) {
        Some(cdi) => (cdi * 0.85, format!("the CDI ({}) after the 15 % income tax", pct(cdi, 2))),
        None => (10.0, "10 % (the CDI is not known yet)".into()),
    }
}

fn watched(class: AssetClass, prefs: &Ceilings) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = if prefs.methods.is_empty() {
        if class == AssetClass::Fii {
            vec!["bazin_projected"]
        } else {
            vec!["bazin_projected", "graham"]
        }
    } else {
        METHODS.iter().copied().filter(|m| prefs.methods.iter().any(|p| p == m)).collect()
    };
    out.push("manual");
    out
}

pub fn assess_fund(ticker: &str, fund: Option<&FundData>, price: Option<f64>, economy: &Macro, prefs: &Ceilings, today: Date) -> Assessment {
    let mut a = Assessment { ticker: ticker.into(), class: Some(AssetClass::Fii), price, watched: watched(AssetClass::Fii, prefs), ..Assessment::default() };
    let Some(fund) = fund else {
        a.missing.push("the fund's dividends and reports are not read yet".into());
        return a;
    };
    let r = crate::fii::report(fund, price, economy, today);
    let (y, why) = fund_yield(prefs, economy);
    match r.dividends_year.filter(|d| *d > 0.0) {
        Some(year) => a.ceilings.push(Ceiling {
            method: "bazin",
            value: year / (y / 100.0),
            basis: format!("{} of dividends a share in a year{} over {} ({why})", brl(year), if r.yield_months < 12 { format!(" (at the pace of the {} months known)", r.yield_months) } else { String::new() }, pct(y, 1)),
        }),
        None => a.missing.push("Bazin: fewer than three months of dividends known".into()),
    }
    match r.avg_dividend_3m.or(r.last_dividend).filter(|d| *d > 0.0) {
        Some(month) => {
            let mut year = month * 12.0;
            let mut how = format!("the last three months' pace, {} a month", brl(month));
            if r.kind == "papel" {
                if let (Some(later), Some(last)) = (r.projection_6m, r.last_dividend.filter(|l| *l > 0.0)) {
                    year *= later / last;
                    how.push_str(&format!(", moved by the Focus survey's rates ({} in six months)", brl(later)));
                }
            }
            a.ceilings.push(Ceiling { method: "bazin_projected", value: year / (y / 100.0), basis: format!("{how}: {} a year over {}", brl(year), pct(y, 1)) });
        }
        None => a.missing.push("projected Bazin: no dividend known yet".into()),
    }
    match r.vp_share.filter(|v| *v > 0.0) {
        Some(vp) => a.ceilings.push(Ceiling { method: "pvp", value: vp * prefs.max_pvp, basis: format!("VP/cota {} times a P/VP of {}", brl(vp), grouped(prefs.max_pvp, 2)) }),
        None => a.missing.push("P/VP: the fund's VP per share is not read yet".into()),
    }
    a
}

/// A share's figures a share: earnings, book value and dividends over the last twelve months (or the year so far at
/// a year's pace), and the earnings' growth on the year before, percent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PerShare {
    pub eps: Option<f64>,
    pub bvps: Option<f64>,
    pub dps: Option<f64>,
    pub growth: Option<f64>,
    pub shares: f64,
    pub end: Option<Date>,
}

/// The share count is checked against the price: a book value a share that puts the P/VP beyond 0,02–100 means a
/// count in thousands (some companies file it so), or a wrong one.
fn shares_checked(shares: f64, equity: Option<f64>, price: Option<f64>) -> Option<f64> {
    let plausible = |s: f64| match (equity.filter(|e| *e > 0.0), price.filter(|p| *p > 0.0)) {
        (Some(e), Some(p)) => (0.02..=100.0).contains(&(p / (e / s))),
        _ => true,
    };
    [shares, shares * 1000.0].into_iter().find(|s| *s > 0.0 && plausible(*s))
}

pub fn per_share(c: &CompanyData, price: Option<f64>) -> Result<PerShare, String> {
    let shares = c.shares.filter(|s| *s > 0.0).ok_or("the CVM's capital table gave no share count")?;
    let m = crate::company::metrics(&c.figures);
    let get = |k: &str| m.now.get(k).copied();
    let shares = shares_checked(shares, get("equity"), price).ok_or("the share count the CVM gives does not fit the price")?;
    let eps = get("net_income").map(|n| n / shares);
    let growth = match (get("net_income"), m.year_before.get("net_income").copied()) {
        (Some(now), Some(before)) if now > 0.0 && before > 0.0 => Some((now / before - 1.0) * 100.0),
        _ => None,
    };
    Ok(PerShare { eps, bvps: get("equity").map(|e| e / shares), dps: get("dividends_paid").map(|d| d / shares), growth, shares, end: m.end })
}

/// Overrides the brain may pass for a company the CVM does not cover (earnings, book value and dividends a share,
/// growth percent).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Given {
    pub eps: Option<f64>,
    pub bvps: Option<f64>,
    pub dps: Option<f64>,
    pub growth: Option<f64>,
}

pub fn assess_stock(ticker: &str, company: Option<&CompanyData>, price: Option<f64>, prefs: &Ceilings, given: &Given) -> Assessment {
    let mut a = Assessment { ticker: ticker.into(), class: Some(AssetClass::Acao), price, watched: watched(AssetClass::Acao, prefs), ..Assessment::default() };
    let mut s = match company.map(|c| per_share(c, price)) {
        Some(Ok(s)) => s,
        Some(Err(e)) => {
            a.missing.push(format!("the CVM's figures a share: {e}"));
            PerShare::default()
        }
        None => {
            a.missing.push("the company's statements are not read yet".into());
            PerShare::default()
        }
    };
    s.eps = given.eps.or(s.eps);
    s.bvps = given.bvps.or(s.bvps);
    s.dps = given.dps.or(s.dps);
    s.growth = given.growth.or(s.growth);
    let y = prefs.stock_yield.max(0.1);
    match s.dps.filter(|d| *d > 0.0) {
        Some(dps) => a.ceilings.push(Ceiling { method: "bazin", value: dps / (y / 100.0), basis: format!("{} of dividends a share over twelve months, over {}", brl(dps), pct(y, 1)) }),
        None => a.missing.push("Bazin: no dividend a share known".into()),
    }
    match (s.eps.filter(|e| *e > 0.0), s.dps.filter(|d| *d > 0.0)) {
        (Some(eps), Some(dps)) => {
            let payout = (dps / eps).clamp(0.0, 1.0);
            let g = s.growth.unwrap_or(0.0).clamp(-20.0, 20.0);
            let next = eps * (1.0 + g / 100.0);
            a.ceilings.push(Ceiling {
                method: "bazin_projected",
                value: next * payout / (y / 100.0),
                basis: format!("earnings a share {} grown {} to {}, paid out at {}, over {}", brl(eps), pct(g, 1), brl(next), pct(payout * 100.0, 0), pct(y, 1)),
            });
        }
        _ => a.missing.push("projected Bazin: it needs positive earnings and dividends a share".into()),
    }
    match (s.eps.filter(|e| *e > 0.0), s.bvps.filter(|b| *b > 0.0)) {
        (Some(eps), Some(bvps)) => a.ceilings.push(Ceiling { method: "graham", value: (22.5 * eps * bvps).sqrt(), basis: format!("√(22,5 × earnings a share {} × book value a share {})", brl(eps), brl(bvps)) }),
        _ => a.missing.push("Graham: it needs positive earnings and book value a share".into()),
    }
    match (s.eps.filter(|e| *e > 0.0), s.growth, price.filter(|p| *p > 0.0)) {
        (Some(eps), Some(g), Some(p)) => {
            let g = g.clamp(0.0, 25.0);
            let dy = s.dps.unwrap_or(0.0) / p * 100.0;
            if g + dy > 0.0 {
                a.ceilings.push(Ceiling { method: "lynch", value: eps * (g + dy), basis: format!("earnings a share {} × (growth {} + dividend yield {}), a fair P/E of {}", brl(eps), grouped(g, 1), grouped(dy, 1), grouped(g + dy, 1)) });
            } else {
                a.missing.push("Peter Lynch: no growth and no dividend".into());
            }
        }
        _ => a.missing.push("Peter Lynch: it needs positive earnings, their growth and the price".into()),
    }
    a
}

/// The class Coucou keeps for a ticker: the user's position, else B3's own kind of share.
fn class_of(store: &Store, market: &Market, ticker: &str) -> Option<AssetClass> {
    store.portfolio.position(ticker).map(|p| p.class).or_else(|| match market.quotes.get(ticker).map(crate::sources::b3::kind_of) {
        Some("fii") => Some(AssetClass::Fii),
        Some("acao") => Some(AssetClass::Acao),
        Some("etf") => Some(AssetClass::Etf),
        Some("bdr") => Some(AssetClass::Bdr),
        _ => None,
    })
}

pub fn assess(store: &Store, market: &Market, ticker: &str, given: &Given, today: Date) -> Assessment {
    let price = market.price(ticker);
    let class = class_of(store, market, ticker).or_else(|| crate::text::ticker(ticker).map(|t| crate::import::guess_class(&t).0));
    let mut a = match class {
        Some(AssetClass::Fii) => assess_fund(ticker, market.funds.get(ticker), price, &market.economy, &store.ceilings, today),
        Some(AssetClass::Acao) => assess_stock(ticker, market.companies.get(ticker), price, &store.ceilings, given),
        class => Assessment {
            ticker: ticker.into(),
            class,
            price,
            missing: vec!["ETFs, BDRs and fixed income follow an index or a rate: these methods do not price them".into()],
            watched: vec!["manual"],
            ..Assessment::default()
        },
    };
    if let Some(v) = store.ceilings.manual.get(ticker).filter(|v| **v > 0.0) {
        a.ceilings.push(Ceiling { method: "manual", value: *v, basis: "the ceiling the user set".into() });
    }
    a
}

/// How far below `ceiling` the close is, percent (negative: above it).
pub fn below(price: f64, ceiling: f64) -> f64 {
    (1.0 - price / ceiling) * 100.0
}

/// The assets watched: those held (shares and funds) and those the user gave a ceiling to.
pub fn watched_tickers(store: &Store) -> Vec<String> {
    let mut out: Vec<String> = store.portfolio.positions.iter().filter(|p| matches!(p.class, AssetClass::Acao | AssetClass::Fii)).map(|p| p.ticker.clone()).collect();
    for t in store.ceilings.manual.keys() {
        if !out.contains(t) {
            out.push(t.clone());
        }
    }
    out
}

/// The assets below one of their watched ceilings by more than the user's margin: the furthest below first, one per
/// asset (its furthest below).
pub fn opportunities(store: &Store, market: &Market, today: Date) -> Vec<Opportunity> {
    let mut out = Vec::new();
    for t in watched_tickers(store) {
        let a = assess(store, market, &t, &Given::default(), today);
        let Some(price) = a.price else { continue };
        let best = a
            .ceilings
            .iter()
            .filter(|c| a.watched.contains(&c.method) && c.value > 0.0)
            .map(|c| (c, below(price, c.value)))
            .filter(|(_, b)| *b > store.ceilings.margin.max(0.0))
            .max_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((c, b)) = best {
            out.push(Opportunity { ticker: t, method: c.method, ceiling: c.value, price, below: b });
        }
    }
    out.sort_by(|a, b| b.below.partial_cmp(&a.below).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// The opportunities not told yet, or told over a week ago, or 5 points further below since: those to say now. The
/// user's record is updated. A record is kept for the week even when the close goes back above the ceiling, so a
/// price that goes up and down around it is told once a week, not every day; it goes after the week, or when the
/// asset is no longer watched.
pub fn to_tell(store: &mut Store, market: &Market, today: Date) -> Vec<Opportunity> {
    let now = opportunities(store, market, today);
    let watched = watched_tickers(store);
    store.ceilings.told.retain(|t, (day, _)| day.until(today) < 7 && watched.contains(t));
    let mut out = Vec::new();
    for o in now {
        let tell = match store.ceilings.told.get(&o.ticker) {
            Some((_, was)) => o.below >= was + 5.0,
            None => true,
        };
        if tell {
            store.ceilings.told.insert(o.ticker.clone(), (today, o.below));
            out.push(o);
        }
    }
    out
}

pub fn for_model(a: &Assessment, margin: f64) -> String {
    let kind = match a.class {
        Some(AssetClass::Fii) => "a real-estate fund",
        Some(AssetClass::Acao) => "a share",
        Some(AssetClass::Etf) => "an ETF",
        _ => "an asset",
    };
    let mut out = format!("{} ({kind}) — {}", a.ticker, a.price.map(|p| format!("price {}", brl(p))).unwrap_or_else(|| "no price yet".into()));
    for c in &a.ceilings {
        let against = match a.price.filter(|_| c.value > 0.0) {
            Some(p) => {
                let b = below(p, c.value);
                if b > 0.0 {
                    format!(" → the close is {} below it", pct(b, 1))
                } else {
                    format!(" → the close is {} above it", pct(-b, 1))
                }
            }
            None => String::new(),
        };
        out.push_str(&format!("\n- {}: ceiling {} ({}){against}{}", label(c.method), brl(c.value), c.basis, if a.watched.contains(&c.method) { " [watched]" } else { "" }));
    }
    for m in &a.missing {
        out.push_str(&format!("\n- Not worked out: {m}."));
    }
    if margin > 0.0 {
        out.push_str(&format!("\nAn opportunity is told when the close is more than {} below a watched ceiling.", pct(margin, 1)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Ceilings, Position};
    use crate::portfolio::fixtures;

    fn today() -> Date {
        Date::new(2026, 10, 21).unwrap()
    }

    #[test]
    fn a_fund_by_its_dividends_and_its_vp() {
        let market = fixtures::market();
        let economy = Macro { cdi: Some(13.65), ..Macro::default() };
        // GGRC11 in the fixture: R$ 0,088 a month for twelve months, closing at R$ 9,12.
        let a = assess_fund("GGRC11", market.funds.get("GGRC11"), Some(9.12), &economy, &Ceilings::default(), today());
        let bazin = a.ceilings.iter().find(|c| c.method == "bazin").unwrap();
        let y = 13.65 * 0.85;
        assert!((bazin.value - 0.088 * 12.0 / (y / 100.0)).abs() < 1e-6, "{}", bazin.value);
        assert!(bazin.basis.contains("after the 15 % income tax"), "{}", bazin.basis);
        let projected = a.ceilings.iter().find(|c| c.method == "bazin_projected").unwrap();
        assert!((projected.value - bazin.value).abs() < 1e-6, "steady dividends: the same");
        assert!(a.missing.iter().any(|m| m.starts_with("P/VP")), "no VP read in the fixture");
        // The user's own yield.
        let mine = Ceilings { fund_yield: Some(10.0), ..Ceilings::default() };
        let a = assess_fund("GGRC11", market.funds.get("GGRC11"), Some(9.12), &economy, &mine, today());
        assert!((a.ceilings[0].value - 0.088 * 12.0 / 0.10).abs() < 1e-6);
        assert_eq!(a.watched, vec!["bazin_projected", "manual"]);
    }

    #[test]
    fn a_share_by_bazin_graham_and_lynch() {
        let c = CompanyData { ticker: "TIMS3".into(), figures: crate::company::fixtures::figures(0.1), shares: Some(2_420_000_000.0), ..CompanyData::default() };
        let s = per_share(&c, Some(19.0)).unwrap();
        let (eps, bvps, dps) = (s.eps.unwrap(), s.bvps.unwrap(), s.dps.unwrap());
        assert!(eps > 0.0 && bvps > 0.0 && dps > 0.0, "{s:?}");
        let a = assess_stock("TIMS3", Some(&c), Some(19.0), &Ceilings::default(), &Given::default());
        let get = |m: &str| a.ceilings.iter().find(|c| c.method == m).map(|c| c.value);
        assert!((get("bazin").unwrap() - dps / 0.06).abs() < 1e-6);
        assert!((get("graham").unwrap() - (22.5 * eps * bvps).sqrt()).abs() < 1e-6);
        if let Some(g) = s.growth {
            let fair = eps * (g.clamp(0.0, 25.0) + dps / 19.0 * 100.0);
            assert!((get("lynch").unwrap() - fair).abs() < 1e-6);
        }
        // A count filed in thousands is read as such: the P/VP must make sense.
        let thousands = CompanyData { shares: Some(2_420_000.0), ..c.clone() };
        assert!((per_share(&thousands, Some(19.0)).unwrap().shares - 2_420_000_000.0).abs() < 1.0);
        // No share count: the brain's own figures are used.
        let none = CompanyData { shares: None, ..c };
        let a = assess_stock("TIMS3", Some(&none), Some(19.0), &Ceilings::default(), &Given { eps: Some(1.5), bvps: Some(10.0), dps: Some(1.2), growth: Some(8.0) });
        assert!((a.ceilings.iter().find(|c| c.method == "graham").unwrap().value - (22.5f64 * 1.5 * 10.0).sqrt()).abs() < 1e-9);
        assert!(a.missing.iter().any(|m| m.contains("share count")));
    }

    #[test]
    fn opportunities_are_told_once_then_again_a_week_later_or_further_below() {
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let mut market = fixtures::market();
        market.economy.cdi = Some(13.65);
        // GGRC11: R$ 1,056 a year over 11,6 % → about R$ 9,10, against a close of R$ 9,12: just above it.
        assert!(opportunities(&store, &market, today()).iter().all(|o| o.ticker != "GGRC11"));
        market.quotes.get_mut("GGRC11").unwrap().close = 8.0;
        let told = to_tell(&mut store, &market, today());
        let g = told.iter().find(|o| o.ticker == "GGRC11").expect("GGRC11 below its projected Bazin");
        assert_eq!(g.method, "bazin_projected");
        assert!(g.below > 10.0 && g.below < 14.0, "{}", g.below);
        assert!(to_tell(&mut store, &market, today()).iter().all(|o| o.ticker != "GGRC11"), "told once");
        assert!(to_tell(&mut store, &market, today().add_days(7)).iter().any(|o| o.ticker == "GGRC11"), "again a week later");
        market.quotes.get_mut("GGRC11").unwrap().close = 7.2;
        assert!(to_tell(&mut store, &market, today().add_days(8)).iter().any(|o| o.ticker == "GGRC11"), "5 points further below");
        // Back above it and below again within the week: not told again; after the week, it is.
        market.quotes.get_mut("GGRC11").unwrap().close = 9.5;
        assert!(to_tell(&mut store, &market, today().add_days(9)).is_empty());
        market.quotes.get_mut("GGRC11").unwrap().close = 7.2;
        assert!(to_tell(&mut store, &market, today().add_days(10)).is_empty(), "up and down within the week: told once");
        assert!(to_tell(&mut store, &market, today().add_days(15)).iter().any(|o| o.ticker == "GGRC11"), "a week after");
        // Sold: no longer watched, its record goes.
        store.portfolio.positions.retain(|p| p.ticker != "GGRC11");
        to_tell(&mut store, &market, today().add_days(16));
        assert!(!store.ceilings.told.contains_key("GGRC11"));
        // A ceiling the user set, for an asset they do not hold.
        store.ceilings.manual.insert("VALE3".into(), 70.0);
        market.quotes.insert("VALE3".into(), crate::sources::b3::Quote { ticker: "VALE3".into(), close: 60.0, ..market.quotes["TIMS3"].clone() });
        let o = opportunities(&store, &market, today());
        let v = o.iter().find(|o| o.ticker == "VALE3").unwrap();
        assert_eq!(v.method, "manual");
        assert!((v.below - (1.0 - 60.0 / 70.0) * 100.0).abs() < 1e-9);
        // An ETF has no ceiling by these methods.
        store.portfolio.positions.push(Position { ticker: "BOVA11".into(), class: AssetClass::Etf, quantity: 1.0, average_price: 100.0, subtype: String::new(), current_value: None, note: String::new() });
        let a = assess(&store, &market, "BOVA11", &Given::default(), today());
        assert!(a.ceilings.is_empty() && a.missing[0].contains("ETFs"));
    }
}
