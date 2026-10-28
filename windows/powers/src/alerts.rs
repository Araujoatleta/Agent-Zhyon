//! What AXYON tells the moment it happens, during B3's session (from the session's prices, read every few minutes):
//! an asset that goes below the user's own ceiling or back above it, a strong day for an asset held or watched (3 %,
//! then 6 %, either way), and a strong day for the whole portfolio (2 %, then 4 %). Each once a day. The
//! opportunities below a ceiling by the user's margin are told by ceiling::to_tell, once a week.

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::model::{Market, Store};

/// A strong day for an asset, percent either way: told from the first step, and again from the second.
pub const ASSET_STEPS: [f64; 2] = [3.0, 6.0];
/// For the whole portfolio.
pub const PORTFOLIO_STEPS: [f64; 2] = [2.0, 4.0];

/// What was told today, so it is told once.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Told {
    pub day: Option<Date>,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// The price went below the user's own ceiling (it was above it at the last close).
    IntoZone,
    /// It went back above the user's own ceiling.
    OutOfZone,
    Up,
    Down,
    PortfolioUp,
    PortfolioDown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Moment {
    pub kind: Kind,
    /// The asset; empty for the portfolio.
    pub ticker: String,
    /// The price now (the portfolio: its value now).
    pub now: f64,
    /// The day's change, percent.
    pub day: f64,
    /// The user's ceiling, when the moment is about it.
    pub ceiling: Option<f64>,
    /// The day's change in money of what the user holds of it (of the whole portfolio for the portfolio).
    pub money: Option<f64>,
    /// The step passed (3 or 6, 2 or 4).
    pub step: Option<f64>,
}

/// The moments of the session not told yet today; the record of what was told is the store's.
pub fn to_tell(store: &mut Store, market: &Market, today: Date) -> Vec<Moment> {
    if store.told_today.day != Some(today) {
        store.told_today = Told { day: Some(today), keys: Vec::new() };
    }
    let found = moments(store, market, today);
    let mut out = Vec::new();
    for (keys, m) in found {
        // The first key is the moment's own; the others (the smaller steps) count as told with it.
        if keys.first().is_none_or(|k| store.told_today.keys.contains(k)) {
            continue;
        }
        for k in keys {
            if !store.told_today.keys.contains(&k) {
                store.told_today.keys.push(k);
            }
        }
        out.push(m);
    }
    out
}

/// The largest of `steps` the change passes, and the keys it is told under (its own first, then the smaller ones).
fn step_passed(steps: &[f64], change: f64, what: &str) -> Option<(f64, Vec<String>)> {
    let step = *steps.iter().rev().find(|s| change.abs() >= **s)?;
    let side = if change > 0.0 { "up" } else { "down" };
    Some((step, steps.iter().rev().filter(|s| **s <= step).map(|s| format!("{side}:{what}:{s}")).collect()))
}

/// The moments now, each with the keys it is told under (what was told today in the store's record).
fn moments(store: &Store, market: &Market, today: Date) -> Vec<(Vec<String>, Moment)> {
    let p = &store.portfolio;
    let mut tickers: Vec<String> = p.tickers(None);
    for t in store.ceilings.manual.keys() {
        if !tickers.contains(t) {
            tickers.push(t.clone());
        }
    }
    let mut out = Vec::new();
    let (mut moved, mut live_held) = (0.0, false);
    for t in &tickers {
        let Some(live) = market.live_now(t).filter(|l| l.day() == today) else { continue };
        let Some(day) = market.day_change(t) else { continue };
        let before = live.price / (1.0 + day / 100.0);
        let money = p.position(t).filter(|x| x.class.quoted()).map(|x| x.quantity * (live.price - before));
        if let Some(m) = money {
            moved += m;
            live_held = true;
        }
        if let Some(c) = store.ceilings.manual.get(t).copied().filter(|c| *c > 0.0) {
            // Under it at the last close, or since today.
            let was_under = before <= c || store.told_today.keys.contains(&format!("into:{t}"));
            let kind = if before > c && live.price <= c {
                Some(Kind::IntoZone)
            } else if was_under && live.price > c {
                Some(Kind::OutOfZone)
            } else {
                None
            };
            if let Some(kind) = kind {
                let key = format!("{}:{t}", if kind == Kind::IntoZone { "into" } else { "out" });
                out.push((vec![key], Moment { kind, ticker: t.clone(), now: live.price, day, ceiling: Some(c), money, step: None }));
            }
        }
        if let Some((step, keys)) = step_passed(&ASSET_STEPS, day, t) {
            let kind = if day > 0.0 { Kind::Up } else { Kind::Down };
            out.push((keys, Moment { kind, ticker: t.clone(), now: live.price, day, ceiling: None, money, step: Some(step) }));
        }
    }
    if live_held {
        let value = crate::portfolio::value(p, market).value;
        let before = value - moved;
        if before > 0.0 {
            let day = moved / before * 100.0;
            if let Some((step, keys)) = step_passed(&PORTFOLIO_STEPS, day, "portfolio") {
                let kind = if day > 0.0 { Kind::PortfolioUp } else { Kind::PortfolioDown };
                out.push((keys, Moment { kind, ticker: String::new(), now: value, day, ceiling: None, money: Some(moved), step: Some(step) }));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AssetClass, LivePrice, Position};

    fn held(t: &str, class: AssetClass, q: f64) -> Position {
        Position { ticker: t.into(), class, quantity: q, average_price: 10.0, subtype: String::new(), current_value: None, note: String::new() }
    }

    fn quote(t: &str, close: f64) -> crate::sources::b3::Quote {
        crate::sources::b3::Quote {
            ticker: t.into(),
            date: Date::new(2026, 10, 26).unwrap(),
            open: close,
            high: close,
            low: close,
            average: close,
            close,
            trades: 1,
            volume: 1.0,
            bdi: "02".into(),
            kind: "ON".into(),
            name: t.into(),
            isin: String::new(),
            market: 10,
        }
    }

    #[test]
    fn crossings_strong_days_and_the_portfolios_day_are_told_once() {
        let today = Date::new(2026, 10, 27).unwrap();
        // 27/10 15:16 in Brasília.
        let at = 1_793_125_002;
        let mut store = Store::default();
        store.portfolio.positions = vec![held("GGRC11", AssetClass::Fii, 150.0), held("TIMS3", AssetClass::Acao, 30.0)];
        let mut rdb = held("RDB Nubank", AssetClass::RendaFixa, 1.0);
        rdb.current_value = Some(5000.0);
        store.portfolio.positions.push(rdb);
        store.ceilings.manual.insert("GGRC11".into(), 8.80);
        store.ceilings.manual.insert("KNRI11".into(), 150.0);
        let mut market = Market::default();
        for (t, c) in [("GGRC11", 8.85), ("TIMS3", 18.1), ("KNRI11", 152.0)] {
            market.quotes.insert(t.into(), quote(t, c));
        }
        let live = |t: &str, price: f64, at: i64| (t.to_string(), LivePrice { price, at, previous: None });
        market.keep_live([live("GGRC11", 8.69, at), live("TIMS3", 18.78, at), live("KNRI11", 151.0, at)]);
        let told = to_tell(&mut store, &market, today);
        let kinds: Vec<(Kind, &str)> = told.iter().map(|m| (m.kind, m.ticker.as_str())).collect();
        assert_eq!(kinds, [(Kind::IntoZone, "GGRC11"), (Kind::Up, "TIMS3")], "8,85 → 8,69 under the ceiling of 8,80; TIMS3 +3,8 %");
        let tims = &told[1];
        assert_eq!(tims.step, Some(3.0));
        assert!((tims.money.unwrap() - 30.0 * (18.78 - 18.1)).abs() < 1e-6, "what the user's 30 shares made today");
        assert!(to_tell(&mut store, &market, today).is_empty(), "told once");
        // TIMS3 goes on to +6 %, GGRC11 back above the ceiling: each told; the portfolio stays under 2 %.
        market.keep_live([live("TIMS3", 19.25, at + 600), live("GGRC11", 8.84, at + 600)]);
        let told = to_tell(&mut store, &market, today);
        assert_eq!(told.iter().map(|m| (m.kind, m.step)).collect::<Vec<_>>(), [(Kind::OutOfZone, None), (Kind::Up, Some(6.0))]);
        // Back to +4 %: the 3 % step was told with the 6 %.
        market.keep_live([live("TIMS3", 18.85, at + 1200)]);
        assert!(to_tell(&mut store, &market, today).is_empty());
        // A strong day for the whole portfolio.
        let mut store = Store::default();
        store.portfolio.positions = vec![held("TIMS3", AssetClass::Acao, 100.0)];
        let mut market = Market::default();
        market.quotes.insert("TIMS3".into(), quote("TIMS3", 18.1));
        market.keep_live([("TIMS3".to_string(), LivePrice { price: 17.5, at, previous: Some(18.1) })]);
        let told = to_tell(&mut store, &market, today);
        assert_eq!(told.iter().map(|m| m.kind).collect::<Vec<_>>(), [Kind::Down, Kind::PortfolioDown]);
        let whole = &told[1];
        assert_eq!(whole.step, Some(2.0));
        assert!((whole.day - (17.5 / 18.1 - 1.0) * 100.0).abs() < 1e-6 && (whole.money.unwrap() + 60.0).abs() < 1e-6);
        // Another day: told again.
        assert_eq!(to_tell(&mut store, &market, today.add_days(1)).len(), 0, "the price is of the 27th's session: nothing on the 28th");
        assert_eq!(store.told_today.day, Some(today.add_days(1)));
    }
}
