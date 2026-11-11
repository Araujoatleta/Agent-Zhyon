//! The radar: every real-estate fund traded on B3, not only those the user holds, read from the CVM's monthly reports
//! (two years of every fund's months, one ZIP a year) and B3's quotes of the day, and scored by this code alone. The
//! phone does the work: a few hundred funds, tens of thousands of months, joined by ISIN, measured and ranked in a
//! moment, with what each figure rests on.
//!
//! What is measured, per fund: the price over its net asset value a share (P/VP), a year of the dividend yields the
//! fund reported (the CVM's "dividend yield do mês", on the net asset value) and that year on today's price, the
//! change of the net asset value a share and of the holders in twelve months, what traded that day, and how many of
//! the last twelve months paid. The score (0 to 100) weighs the discount, the income, the stability of the net asset
//! value, the holders, the liquidity and the regularity; a yield well above the market's on a falling net asset value is marked a
//! possible trap, a very deep discount something to look into. A ranking to start from, never a recommendation.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::date::Date;
use crate::model::{AssetClass, Market, Store};
use crate::sources::b3::{self, Quote};
use crate::sources::cvm_fii::{self, FiiMonth};
use crate::text::{brl, grouped, pct};

/// Under this traded in the day (R$), a fund is not ranked: its price says little.
pub const MIN_VOLUME: f64 = 20_000.0;
/// Fewer months of reports than this, and a fund is not ranked.
pub const MIN_MONTHS: usize = 6;
/// The most funds kept.
pub const MOST: usize = 600;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RadarFund {
    pub ticker: String,
    pub cnpj: String,
    pub name: String,
    /// tijolo, papel, fof, hibrido or "".
    pub kind: String,
    pub segment: String,
    pub price: f64,
    pub vp: f64,
    pub pvp: f64,
    /// A year of the monthly yields the fund reported, on its net asset value, percent.
    pub dy12_vp: f64,
    /// That year on today's price (estimated: the reported yield over the P/VP), percent.
    pub dy12: f64,
    /// The net asset value a share against twelve months before, percent.
    pub vp_change: Option<f64>,
    pub holders: Option<f64>,
    pub holders_change: Option<f64>,
    pub net_assets: Option<f64>,
    /// R$ traded on the quotes' day.
    pub volume: f64,
    /// Of the last twelve months, how many paid.
    pub paid_months: u32,
    pub months: u32,
    /// The fund's last reported month.
    pub reported: Option<Date>,
    pub score: f64,
    /// trap (a high yield on a falling net asset value), deep (a discount past 35 %), shrinking (holders leaving).
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Radar {
    /// When it was read, and the quotes' day.
    pub read: Option<Date>,
    pub quotes_day: Option<Date>,
    /// Best score first.
    pub funds: Vec<RadarFund>,
    /// How much was gone through: funds in the CVM's files, their months, the quotes.
    pub scanned_funds: u32,
    pub scanned_months: u32,
    pub scanned_quotes: u32,
    /// How long the measuring took, in milliseconds (the reading aside).
    pub took_ms: u32,
}

fn ramp(value: f64, zero: f64, one: f64) -> f64 {
    if (one - zero).abs() < f64::EPSILON {
        return 0.0;
    }
    ((value - zero) / (one - zero)).clamp(0.0, 1.0)
}

/// Where `value` stands among `sorted` (ascending), 0 to 1.
fn percentile(sorted: &[f64], value: f64) -> f64 {
    if sorted.is_empty() {
        return 0.5;
    }
    let below = sorted.partition_point(|v| *v < value);
    below as f64 / sorted.len() as f64
}

/// Every traded fund measured and ranked, from the CVM's months (any order, every fund) and B3's quotes of a day.
pub fn build(months: &[FiiMonth], quotes: &[Quote], today: Date) -> Radar {
    let started = std::time::Instant::now();
    // The fund's own share on the regular market, by ISIN.
    let mut by_isin: HashMap<&str, &Quote> = HashMap::new();
    for q in quotes.iter().filter(|q| q.market == 10 && q.close > 0.0 && b3::kind_of(q) == "fii") {
        by_isin.entry(q.isin.as_str()).or_insert(q);
    }
    let mut by_fund: HashMap<&str, Vec<&FiiMonth>> = HashMap::new();
    for m in months {
        by_fund.entry(m.cnpj.as_str()).or_default().push(m);
    }
    let mut funds: Vec<RadarFund> = Vec::new();
    for (cnpj, mut list) in by_fund.iter().map(|(k, v)| (*k, v.clone())) {
        list.sort_by_key(|m| m.date);
        let Some(last) = list.last() else { continue };
        // The ISIN the fund gave last, or any it gave that trades.
        let Some(quote) = list.iter().rev().find_map(|m| by_isin.get(m.isin.as_str())) else { continue };
        let Some(vp) = list.iter().rev().find_map(|m| m.vp_share.filter(|v| *v > 0.0)) else { continue };
        let year: Vec<&&FiiMonth> = list.iter().rev().take(12).collect();
        let dy12_vp: f64 = year.iter().filter_map(|m| m.dividend_yield).filter(|d| d.is_finite() && *d > -50.0 && *d < 50.0).sum();
        let paid = year.iter().filter(|m| m.dividend_yield.is_some_and(|d| d > 0.0)).count() as u32;
        let pvp = quote.close / vp;
        let before = list.iter().rev().nth(12);
        let vp_change = before.and_then(|b| b.vp_share.filter(|v| *v > 0.0)).map(|b| (vp / b - 1.0) * 100.0);
        let holders = list.iter().rev().find_map(|m| m.holders.filter(|h| *h > 0.0));
        let holders_change = match (holders, before.and_then(|b| b.holders.filter(|h| *h > 0.0))) {
            (Some(now), Some(then)) => Some((now / then - 1.0) * 100.0),
            _ => None,
        };
        funds.push(RadarFund {
            ticker: quote.ticker.clone(),
            cnpj: cnpj.to_string(),
            name: last.name.clone(),
            kind: cvm_fii::subtype(last).to_string(),
            segment: last.segment.clone(),
            price: quote.close,
            vp,
            pvp,
            dy12_vp,
            dy12: if pvp > 0.0 { dy12_vp / pvp } else { 0.0 },
            vp_change,
            holders,
            holders_change,
            net_assets: last.net_assets,
            volume: quote.volume,
            paid_months: paid,
            months: list.len() as u32,
            reported: Some(last.date),
            score: 0.0,
            flags: Vec::new(),
        });
    }
    // The income is judged against the others': where each year of yield stands among the ranked.
    let ranked = |f: &RadarFund| f.months as usize >= MIN_MONTHS && f.volume >= MIN_VOLUME && f.pvp > 0.2 && f.pvp < 3.0;
    let mut yields: Vec<f64> = funds.iter().filter(|f| ranked(f)).map(|f| f.dy12).collect();
    yields.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = yields.get(yields.len() / 2).copied().unwrap_or(0.0);
    for f in funds.iter_mut() {
        if !ranked(f) {
            continue;
        }
        let discount = ramp(1.10 - f.pvp, 0.0, 0.40);
        let income = percentile(&yields, f.dy12);
        let stability = f.vp_change.map_or(0.5, |c| ramp(c, -15.0, -2.0));
        let people = f.holders_change.map_or(0.5, |c| ramp(c, -10.0, 10.0));
        let liquidity = ramp(f.volume.max(1.0).log10(), 50_000f64.log10(), 1_000_000f64.log10());
        let regular = f.paid_months as f64 / 12.0;
        let mut score = 100.0 * (0.25 * discount + 0.25 * income + 0.20 * stability + 0.10 * people + 0.10 * liquidity + 0.10 * regular);
        if f.dy12 > 1.3 * median && f.vp_change.is_some_and(|c| c < -8.0) {
            f.flags.push("trap".into());
            score *= 0.8;
        }
        if f.pvp < 0.65 {
            f.flags.push("deep".into());
            score *= 0.9;
        }
        if f.holders_change.is_some_and(|c| c < -10.0) {
            f.flags.push("shrinking".into());
        }
        f.score = (score * 10.0).round() / 10.0;
    }
    funds.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.ticker.cmp(&b.ticker)));
    let scanned_funds = by_fund.len() as u32;
    funds.truncate(MOST);
    Radar {
        read: Some(today),
        quotes_day: quotes.first().map(|q| q.date),
        funds,
        scanned_funds,
        scanned_months: months.len() as u32,
        scanned_quotes: quotes.len() as u32,
        took_ms: started.elapsed().as_millis() as u32,
    }
}

/// The radar as the app shows it: the ranked funds (best first), the user's own among them with where each stands,
/// and how much was gone through. `kind` keeps one kind of fund ("tijolo", "papel", "fof", "hibrido").
pub fn view(store: &Store, market: &Market, kind: Option<&str>, most: usize) -> Value {
    let Some(radar) = market.radar.as_ref().filter(|r| !r.funds.is_empty()) else {
        return json!({ "read": null, "funds": [], "mine": [], "kinds": [] });
    };
    let ranked: Vec<&RadarFund> = radar.funds.iter().filter(|f| f.score > 0.0).collect();
    let held: Vec<String> = store.portfolio.tickers(Some(AssetClass::Fii));
    let fund = |f: &RadarFund, rank: Option<usize>| {
        json!({
            "ticker": f.ticker, "name": f.name, "kind": f.kind, "segment": f.segment, "price": f.price, "vp": f.vp,
            "pvp": f.pvp, "dy12": f.dy12, "dy12Vp": f.dy12_vp, "vpChange": f.vp_change, "holders": f.holders,
            "holdersChange": f.holders_change, "netAssets": f.net_assets, "volume": f.volume, "paidMonths": f.paid_months,
            "reported": f.reported, "score": f.score, "flags": f.flags, "rank": rank, "held": held.contains(&f.ticker),
        })
    };
    let mine: Vec<Value> = held
        .iter()
        .filter_map(|t| {
            let position = ranked.iter().position(|f| &f.ticker == t);
            let f = radar.funds.iter().find(|f| &f.ticker == t)?;
            let peers: Vec<&&RadarFund> = ranked.iter().filter(|p| p.kind == f.kind).collect();
            let among = peers.iter().position(|p| p.ticker == f.ticker);
            let mut v = fund(f, position.map(|p| p + 1));
            v["of"] = json!(ranked.len());
            v["kindRank"] = json!(among.map(|p| p + 1));
            v["kindOf"] = json!(peers.len());
            Some(v)
        })
        .collect();
    let mut kinds: Vec<(String, usize)> = Vec::new();
    for f in &ranked {
        match kinds.iter_mut().find(|(k, _)| k == &f.kind) {
            Some(k) => k.1 += 1,
            None => kinds.push((f.kind.clone(), 1)),
        }
    }
    kinds.sort_by(|a, b| b.1.cmp(&a.1));
    let shown: Vec<Value> = ranked
        .iter()
        .enumerate()
        .filter(|(_, f)| kind.is_none_or(|k| f.kind == k))
        .take(most.clamp(1, MOST))
        .map(|(i, f)| fund(f, Some(i + 1)))
        .collect();
    json!({
        "read": radar.read, "quotesDay": radar.quotes_day, "funds": shown, "mine": mine,
        "kinds": kinds.iter().map(|(k, n)| json!({ "kind": k, "count": n })).collect::<Vec<_>>(),
        "ranked": ranked.len(), "scannedFunds": radar.scanned_funds, "scannedMonths": radar.scanned_months,
        "scannedQuotes": radar.scanned_quotes, "tookMs": radar.took_ms,
    })
}

/// The radar in words, for the brain.
pub fn for_model(v: &Value) -> String {
    let funds = v["funds"].as_array().cloned().unwrap_or_default();
    if funds.is_empty() {
        return "The market radar has not been read on this device yet: the user runs it in the AXYON app (Investments → Market radar), which reads every listed fund from the CVM and B3.".into();
    }
    let line = |f: &Value| {
        let flags: Vec<&str> = f["flags"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).collect();
        format!(
            "{}. {} ({}) score {} — P/VP {}, DY 12m {} on price, VP {} in 12m, holders {}, traded {}{}{}",
            f["rank"].as_u64().unwrap_or(0),
            f["ticker"].as_str().unwrap_or(""),
            f["kind"].as_str().filter(|k| !k.is_empty()).unwrap_or("fund"),
            grouped(f["score"].as_f64().unwrap_or(0.0), 1),
            grouped(f["pvp"].as_f64().unwrap_or(0.0), 2),
            pct(f["dy12"].as_f64().unwrap_or(0.0), 1),
            f["vpChange"].as_f64().map(|c| pct(c, 1)).unwrap_or_else(|| "unknown".into()),
            f["holdersChange"].as_f64().map(|c| pct(c, 1)).unwrap_or_else(|| "unknown".into()),
            brl(f["volume"].as_f64().unwrap_or(0.0)),
            if flags.is_empty() { String::new() } else { format!(" [{}]", flags.join(", ")) },
            if f["held"].as_bool() == Some(true) { " (held)" } else { "" },
        )
    };
    let mut out = format!(
        "Market radar of {} (quotes of {}): {} funds ranked of {} in the CVM's files ({} months gone through). A score from the official figures (discount to net asset value, a year of reported yield, stability of the net asset value, holders, liquidity, regularity), not a recommendation: \"trap\" marks a high yield on a falling net asset value, \"deep\" a discount past 35 %.\n",
        v["read"].as_str().unwrap_or("?"),
        v["quotesDay"].as_str().unwrap_or("?"),
        v["ranked"].as_u64().unwrap_or(0),
        v["scannedFunds"].as_u64().unwrap_or(0),
        v["scannedMonths"].as_u64().unwrap_or(0),
    );
    for f in funds.iter().take(25) {
        out.push_str(&line(f));
        out.push('\n');
    }
    let mine = v["mine"].as_array().cloned().unwrap_or_default();
    if !mine.is_empty() {
        out.push_str("The user's own funds:\n");
        for f in &mine {
            out.push_str(&format!(
                "{} — {} of {} overall, {} of {} among {} funds\n",
                line(f),
                f["rank"].as_u64().map(|r| r.to_string()).unwrap_or_else(|| "unranked".into()),
                f["of"].as_u64().unwrap_or(0),
                f["kindRank"].as_u64().map(|r| r.to_string()).unwrap_or_else(|| "unranked".into()),
                f["kindOf"].as_u64().unwrap_or(0),
                f["kind"].as_str().filter(|k| !k.is_empty()).unwrap_or("its kind of"),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn month(cnpj: &str, isin: &str, date: Date, vp: f64, dy: f64, holders: f64) -> FiiMonth {
        FiiMonth {
            cnpj: cnpj.into(),
            date,
            name: format!("FUNDO {cnpj}"),
            isin: isin.into(),
            segment: "Logística".into(),
            vp_share: Some(vp),
            dividend_yield: Some(dy),
            holders: Some(holders),
            net_assets: Some(1e9),
            in_buildings: Some(9e8),
            invested: Some(1e9),
            ..FiiMonth::default()
        }
    }

    fn quote(ticker: &str, isin: &str, close: f64, volume: f64) -> Quote {
        Quote {
            ticker: ticker.into(),
            date: Date::new(2026, 11, 9).unwrap(),
            open: close,
            high: close,
            low: close,
            average: close,
            close,
            trades: 100,
            volume,
            bdi: "12".into(),
            kind: "CI".into(),
            name: ticker.into(),
            isin: isin.into(),
            market: 10,
        }
    }

    fn year(cnpj: &str, isin: &str, vp_from: f64, vp_to: f64, dy: f64, holders_from: f64, holders_to: f64) -> Vec<FiiMonth> {
        (0..13)
            .map(|i| {
                let t = i as f64 / 12.0;
                month(cnpj, isin, Date::new(2025, 10, 1).unwrap().add_months(i), vp_from + (vp_to - vp_from) * t, dy, holders_from + (holders_to - holders_from) * t)
            })
            .collect()
    }

    #[test]
    fn the_whole_market_is_measured_and_ranked() {
        let mut months = Vec::new();
        // Steady, at a discount, paying well.
        months.extend(year("1", "BRAAAACTF001", 10.0, 10.1, 0.9, 1000.0, 1200.0));
        // Expensive, paying little.
        months.extend(year("2", "BRBBBBCTF002", 10.0, 10.0, 0.5, 1000.0, 1000.0));
        // A very high yield on a net asset value that falls: a trap.
        months.extend(year("3", "BRCCCCCTF003", 12.0, 9.5, 1.6, 1000.0, 800.0));
        // Not traded: left out. And one that barely trades: listed, not ranked.
        months.extend(year("4", "BRDDDDCTF004", 10.0, 10.0, 0.9, 1000.0, 1000.0));
        months.extend(year("5", "BREEEECTF005", 10.0, 10.0, 0.9, 1000.0, 1000.0));
        let quotes = vec![
            quote("AAAA11", "BRAAAACTF001", 8.5, 900_000.0),
            quote("BBBB11", "BRBBBBCTF002", 11.5, 900_000.0),
            quote("CCCC11", "BRCCCCCTF003", 9.0, 900_000.0),
            quote("EEEE11", "BREEEECTF005", 9.0, 3_000.0),
            // An odd-lot line and a share of a company never count.
            Quote { market: 20, ..quote("AAAA11F", "BRAAAACTF001", 8.4, 10.0) },
            Quote { bdi: "02".into(), kind: "ON".into(), isin: "BRXXXXACNOR1".into(), ..quote("XXXX3", "BRXXXXACNOR1", 20.0, 9e6) },
        ];
        let radar = build(&months, &quotes, Date::new(2026, 11, 10).unwrap());
        assert_eq!((radar.scanned_funds, radar.scanned_months), (5, 65));
        let tickers: Vec<&str> = radar.funds.iter().map(|f| f.ticker.as_str()).collect();
        assert!(!tickers.contains(&"DDDD11") && !tickers.contains(&"XXXX3"), "{tickers:?}");
        assert_eq!(tickers[0], "AAAA11", "{:?}", radar.funds.iter().map(|f| (&f.ticker, f.score)).collect::<Vec<_>>());
        let a = &radar.funds[0];
        assert!((a.pvp - 8.5 / 10.1).abs() < 1e-9 && (a.dy12_vp - 10.8).abs() < 1e-9 && a.paid_months == 12);
        assert!((a.dy12 - 10.8 / a.pvp).abs() < 1e-9 && a.holders_change.is_some_and(|c| (c - 20.0).abs() < 1e-6));
        let trap = radar.funds.iter().find(|f| f.ticker == "CCCC11").unwrap();
        assert!(trap.flags.contains(&"trap".to_string()) && trap.flags.contains(&"shrinking".to_string()) && trap.score < a.score);
        let thin = radar.funds.iter().find(|f| f.ticker == "EEEE11").unwrap();
        assert_eq!(thin.score, 0.0, "a fund that barely trades is listed, not ranked");

        let mut market = Market::default();
        market.radar = Some(radar);
        let mut store = Store::default();
        store.portfolio.positions.push(crate::model::Position { ticker: "BBBB11".into(), class: AssetClass::Fii, quantity: 10.0, average_price: 11.0, subtype: String::new(), current_value: None, note: String::new() });
        let v = view(&store, &market, None, 50);
        assert_eq!(v["ranked"], 3);
        assert_eq!(v["funds"].as_array().unwrap().len(), 3);
        assert_eq!(v["mine"][0]["ticker"], "BBBB11");
        assert_eq!(v["mine"][0]["of"], 3);
        assert!(v["mine"][0]["rank"].as_u64().unwrap() > 1);
        let said = for_model(&v);
        assert!(said.contains("1. AAAA11") && said.contains("[trap") && said.contains("BBBB11") && said.contains("(held)"), "{said}");
        assert!(for_model(&view(&store, &Market::default(), None, 10)).contains("has not been read"));
    }
}
