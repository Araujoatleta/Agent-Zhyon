//! When to read the official sources again, and the reading itself, without holding the user's data: each function
//! takes what it needs and returns what it read, for the app to merge. Quotes once B3 publishes a new day; the funds'
//! monthly reports once a week; their documents and dividends once a day (or when asked); the economy once a day; a
//! company's statements once a month.

use std::collections::{BTreeMap, HashMap};

use crate::date::{previous_business_day, Date};
use crate::model::{AssetClass, CompanyData, FundData, Market, Store};
use crate::sources::{self, b3, cvm_fii, fetch::Http, fnet};

/// The dividend notices read for a fund at one reading, newest first.
pub const PAYOUTS_PER_READING: usize = 8;

/// What is due for a refresh.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Due {
    pub quotes: Vec<String>,
    pub funds: Vec<String>,
    /// The funds' monthly reports (a week old, or a fund without its CNPJ yet).
    pub fund_months: bool,
    pub economy: bool,
}

/// The tickers to quote: the portfolio's, and those of the decisions still under review.
pub fn quoted(store: &Store) -> Vec<String> {
    let mut t = store.portfolio.tickers(None);
    for d in store.decisions.iter().filter(|d| d.status == "taken") {
        if crate::text::ticker(&d.ticker).is_some() && !t.contains(&d.ticker) {
            t.push(d.ticker.clone());
        }
    }
    t
}

/// The CNPJ a fund's ticker is read under: the one the user gave, or the one learned from the sources.
pub fn cnpj_of<'a>(store: &'a Store, market: &'a Market, ticker: &str) -> Option<&'a String> {
    let root = crate::text::ticker_root(ticker);
    store.fund_cnpjs.get(root).or_else(|| market.fund_cnpjs.get(root))
}

/// A fund whose CNPJ the user gave and that was not read under it yet.
pub fn cnpj_changed(store: &Store, market: &Market, ticker: &str) -> bool {
    store.fund_cnpjs.get(crate::text::ticker_root(ticker)).is_some_and(|c| market.funds.get(ticker).is_none_or(|f| &f.cnpj != c))
}

pub fn due(store: &Store, market: &Market, today: Date, evening: bool, force: bool) -> Due {
    let tickers = quoted(store);
    let latest_day = if evening { today } else { previous_business_day(today, true) };
    let quotes_stale = market.quotes_day.is_none_or(|d| d < previous_business_day(latest_day.add_days(1), true)) || tickers.iter().any(|t| !market.quotes.contains_key(t));
    let funds: Vec<String> = store.portfolio.tickers(Some(AssetClass::Fii));
    let funds_due: Vec<String> = funds
        .iter()
        .filter(|t| force || cnpj_changed(store, market, t) || market.funds.get(*t).and_then(|f| f.checked).is_none_or(|d| d < today))
        .cloned()
        .collect();
    let missing_cnpj = funds.iter().any(|t| cnpj_of(store, market, t).is_none());
    let given = funds.iter().any(|t| cnpj_changed(store, market, t));
    Due {
        quotes: if force || quotes_stale { tickers } else { Vec::new() },
        funds: funds_due,
        // A fund the reports do not name (a mistyped ticker) is looked for again the next day, not at every refresh;
        // one whose CNPJ the user just gave, at once.
        fund_months: !funds.is_empty() && (given || market.funds_read != Some(today) && (missing_cnpj || force || market.funds_read.is_none_or(|d| d.until(today) >= 7))),
        economy: force || market.economy_read.is_none_or(|d| d < today),
    }
}

/// What was read about the funds.
#[derive(Debug, Clone, Default)]
pub struct Funds {
    /// Ticker roots to CNPJs, learned from the sources (and those known before).
    pub roots: BTreeMap<String, String>,
    /// The CNPJ each ticker was read under.
    pub cnpjs: BTreeMap<String, String>,
    /// The tickers whose fund's CNPJ no one knows.
    pub unknown: Vec<String>,
    pub months: HashMap<String, Vec<cvm_fii::FiiMonth>>,
    pub documents: HashMap<String, Vec<fnet::Document>>,
    pub payouts: HashMap<String, Vec<fnet::Payout>>,
    pub months_read: bool,
    pub errors: Vec<String>,
}

/// Reads the latest documents of the funds of `tickers`, and the dividends of their structured notices not read yet
/// (`known_documents`); and, when `months` names funds, the CVM's monthly reports (all funds read, to learn each
/// ticker's CNPJ), kept for those. A CNPJ the user gave (`given`, by ticker root) wins over what the sources say.
pub async fn funds(http: &Http, tickers: &[String], roots: &BTreeMap<String, String>, given: &BTreeMap<String, String>, months: Option<&[String]>, known_documents: &HashMap<String, Vec<u64>>, today: Date) -> Funds {
    let mut out = Funds { roots: roots.clone(), ..Funds::default() };
    let mut all = Vec::new();
    let mut concerned: Vec<String> = tickers.to_vec();
    for t in months.unwrap_or_default() {
        if !concerned.contains(t) {
            concerned.push(t.clone());
        }
    }
    if months.is_some() {
        match sources::fii_months(http, &[today.y, today.y - 1], &[]).await {
            Ok(read) => {
                for (root, cnpj) in cvm_fii::cnpj_by_root(&read) {
                    out.roots.insert(root, cnpj);
                }
                all = read;
                out.months_read = true;
            }
            Err(e) => out.errors.push(e),
        }
    }
    // A fund whose shares the CVM's reports leave without an ISIN: its CNPJ from B3's register of listed funds.
    for t in &concerned {
        let root = crate::text::ticker_root(t);
        if !out.roots.contains_key(root) && !given.contains_key(root) {
            if let Some(cnpj) = http.text(&b3::fund_search_url(root)).await.ok().and_then(|body| b3::cnpj_in_search(&body, root)) {
                out.roots.insert(root.to_string(), cnpj);
            }
        }
    }
    for t in &concerned {
        let root = crate::text::ticker_root(t);
        match given.get(root).or_else(|| out.roots.get(root)) {
            Some(cnpj) => {
                out.cnpjs.insert(t.clone(), cnpj.clone());
            }
            None => {
                out.unknown.push(t.clone());
                out.errors.push(format!("{t}: its CNPJ is not in the CVM's reports nor in B3's register of funds"));
            }
        }
    }
    if out.months_read {
        for t in months.unwrap_or_default() {
            if let Some(cnpj) = out.cnpjs.get(t) {
                let mut mine: Vec<cvm_fii::FiiMonth> = all.iter().filter(|m| &m.cnpj == cnpj).cloned().collect();
                mine.sort_by_key(|m| m.date);
                out.months.insert(t.clone(), mine);
            }
        }
    }
    let reading: Vec<(String, String)> = tickers.iter().filter_map(|t| out.cnpjs.get(t).map(|c| (t.clone(), c.clone()))).collect();
    for (t, cnpj) in &reading {
        match sources::fund_documents(http, cnpj, 60).await {
            Ok(docs) => {
                let known = known_documents.get(t).cloned().unwrap_or_default();
                let mut payouts = Vec::new();
                // The newest notices first, a few at a time: FundosNet can be slow, and the rest come at the next
                // reading. A notice that does not come stops this fund's for now.
                for d in docs.iter().filter(|d| d.structured && d.topic == fnet::Topic::Payout && !known.contains(&d.id)).take(PAYOUTS_PER_READING) {
                    match sources::payouts_of(http, d).await {
                        Ok(p) => payouts.extend(p),
                        Err(e) => {
                            out.errors.push(format!("{t}: {e}"));
                            break;
                        }
                    }
                }
                out.payouts.insert(t.clone(), payouts);
                out.documents.insert(t.clone(), docs);
            }
            Err(e) => out.errors.push(format!("{t}: {e}")),
        }
    }
    out
}

/// The documents of each fund whose dividends were read already.
pub fn known_documents(market: &Market) -> HashMap<String, Vec<u64>> {
    market.funds.iter().map(|(t, f)| (t.clone(), f.payouts.iter().map(|p| p.document).collect())).collect()
}

pub fn merge_funds(market: &mut Market, read: Funds, today: Date) {
    for (root, cnpj) in &read.roots {
        market.fund_cnpjs.insert(root.clone(), cnpj.clone());
    }
    if read.months_read {
        market.funds_read = Some(today);
    }
    // A fund no one knows the CNPJ of any more (the user took back the one they gave): what was read under it goes.
    for t in &read.unknown {
        market.funds.remove(t);
    }
    let tickers: std::collections::BTreeSet<String> = read.months.keys().chain(read.documents.keys()).chain(read.payouts.keys()).cloned().collect();
    for t in tickers {
        let f = market.funds.entry(t.clone()).or_insert_with(|| FundData { ticker: t.clone(), ..FundData::default() });
        if let Some(cnpj) = read.cnpjs.get(&t) {
            // Read under another CNPJ now (the user gave the right one): what came from the old one goes.
            if !f.cnpj.is_empty() && &f.cnpj != cnpj {
                *f = FundData { ticker: t.clone(), ..FundData::default() };
            }
            f.cnpj = cnpj.clone();
        }
        if let Some(m) = read.months.get(&t) {
            if !m.is_empty() {
                f.months = m.clone();
            }
        }
        if let Some(d) = read.documents.get(&t) {
            f.documents = d.clone();
            f.checked = Some(today);
        }
        if let Some(p) = read.payouts.get(&t) {
            for payout in p {
                // The same payout in another document (a notice published again) is kept once, from the newest one.
                let same = |x: &fnet::Payout| x.record == payout.record && x.amortization == payout.amortization && (x.value - payout.value).abs() < 1e-6;
                match f.payouts.iter_mut().find(|x| same(x)) {
                    Some(x) if x.document < payout.document => *x = payout.clone(),
                    Some(_) => {}
                    None => f.payouts.push(payout.clone()),
                }
            }
            f.payouts.sort_by_key(|x| x.record);
            // Two years of dividends are plenty.
            let cut = today.add_months(-24);
            f.payouts.retain(|x| x.record >= cut);
        }
    }
}

pub fn merge_quotes(market: &mut Market, day: Date, quotes: HashMap<String, b3::Quote>) {
    if quotes.is_empty() {
        return;
    }
    market.quotes_day = Some(market.quotes_day.map_or(day, |d| d.max(day)));
    for (t, q) in quotes {
        market.quotes.insert(t, q);
    }
}

/// A company's statements: its CNPJ from the CVM's register when not known (`known`), then its last three years.
pub async fn company(http: &Http, ticker: &str, known: &BTreeMap<String, (String, String)>, today: Date) -> Result<(CompanyData, Option<HashMap<String, (String, String)>>), String> {
    let mut register = None;
    let (cnpj, name) = match known.get(ticker) {
        Some(found) => found.clone(),
        None => {
            let mut map = sources::company_tickers(http, today.y).await;
            if map.as_ref().map(|m| !m.contains_key(ticker)).unwrap_or(true) {
                map = sources::company_tickers(http, today.y - 1).await;
            }
            let map = map?;
            let found = map.get(ticker).cloned().ok_or_else(|| format!("{ticker} is not in the CVM's register of listed companies (a fund, an ETF or a BDR?)"))?;
            register = Some(map);
            found
        }
    };
    let (figures, problems) = sources::company_figures(http, &cnpj, &[today.y - 1, today.y - 2], &[today.y, today.y - 1]).await?;
    Ok((CompanyData { ticker: ticker.to_string(), cnpj, name, figures, checked: Some(today), problems }, register))
}

/// Classes guessed at import (a ticker ending in 11) set from what the sources say: a fund in the CVM's reports is a
/// FII, a fund's shares at B3 under BDI 14 an ETF. Returns the tickers changed.
pub fn fix_classes(store: &mut Store, market: &Market) -> Vec<String> {
    let mut changed = Vec::new();
    for p in store.portfolio.positions.iter_mut().filter(|p| p.class.quoted()) {
        let root = crate::text::ticker_root(&p.ticker);
        let from_quote = market.quotes.get(&p.ticker).map(b3::kind_of).unwrap_or("");
        let class = if (market.fund_cnpjs.contains_key(root) || store.fund_cnpjs.contains_key(root)) && p.ticker.ends_with("11") {
            Some(AssetClass::Fii)
        } else {
            match from_quote {
                "etf" => Some(AssetClass::Etf),
                "fii" => Some(AssetClass::Fii),
                "bdr" => Some(AssetClass::Bdr),
                "acao" => Some(AssetClass::Acao),
                _ => None,
            }
        };
        if let Some(c) = class {
            if c != p.class && (p.note.contains("guessed") || p.class == AssetClass::Outro) {
                p.class = c;
                changed.push(p.ticker.clone());
            }
        }
        if p.class == AssetClass::Fii {
            if let Some(m) = market.funds.get(&p.ticker).and_then(|f| f.months.last()) {
                // What the fund holds, from its own report, decides; its segment and mandate only fill an empty one.
                match cvm_fii::subtype_from_assets(m) {
                    Some(kind) => p.subtype = kind.to_string(),
                    None if p.subtype.is_empty() => p.subtype = cvm_fii::subtype(m).to_string(),
                    None => {}
                }
            }
        }
        if !changed.contains(&p.ticker) && class == Some(p.class) {
            p.note = p.note.replace("guessed", "").trim().to_string();
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Position;
    use crate::portfolio::fixtures;

    #[test]
    fn what_is_due_and_when() {
        let today = Date::new(2026, 10, 21).unwrap();
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let mut market = fixtures::market();
        let d = due(&store, &market, today, false, false);
        assert!(d.quotes.is_empty(), "yesterday's close is in");
        assert!(d.fund_months, "no CNPJ known yet");
        market.funds_read = Some(today);
        assert!(!due(&store, &market, today, false, false).fund_months, "looked for today already: tomorrow again");
        market.funds_read = None;
        assert_eq!(d.funds.len(), 4);
        assert!(d.economy);
        let evening = due(&store, &market, today, true, false);
        assert!(!evening.quotes.is_empty(), "tonight B3 publishes today's");
        for root in ["GGRC", "GARE", "BTCI", "PORD"] {
            market.fund_cnpjs.insert(root.into(), "1".into());
        }
        market.funds_read = Some(today.add_days(-2));
        for f in market.funds.values_mut() {
            f.checked = Some(today);
        }
        market.economy_read = Some(today);
        let calm = due(&store, &market, today, false, false);
        assert!(!calm.fund_months && calm.funds.is_empty() && !calm.economy);
        assert!(due(&store, &market, today, false, true).funds.len() == 4, "asked: read again");
        store.portfolio.positions.push(Position { ticker: "VALE3".into(), class: AssetClass::Acao, quantity: 1.0, average_price: 60.0, subtype: String::new(), current_value: None, note: String::new() });
        assert_eq!(due(&store, &market, today, false, false).quotes.len(), 7, "a ticker without a quote");
    }

    #[test]
    fn funds_read_merge_without_duplicates_and_classes_get_fixed() {
        let today = Date::new(2026, 10, 21).unwrap();
        let mut market = Market::default();
        let payout = |doc: u64, rec: &str| fnet::Payout {
            record: Date::parse(rec).unwrap(),
            payment: None,
            value: 0.09,
            amortization: false,
            tax_free: None,
            period: String::new(),
            document: doc,
        };
        let read = Funds {
            roots: [("GGRC".to_string(), "111".to_string())].into(),
            cnpjs: [("GGRC11".to_string(), "111".to_string())].into(),
            unknown: vec![],
            months: [("GGRC11".to_string(), vec![cvm_fii::FiiMonth { cnpj: "111".into(), date: today.first_of_month(), segment: "Logística".into(), ..Default::default() }])].into(),
            documents: [("GGRC11".to_string(), vec![])].into(),
            payouts: [("GGRC11".to_string(), vec![payout(1, "2026-09-14"), payout(2, "2026-10-14")])].into(),
            months_read: true,
            errors: vec![],
        };
        merge_funds(&mut market, read.clone(), today);
        merge_funds(&mut market, read, today);
        let f = &market.funds["GGRC11"];
        assert_eq!(f.payouts.len(), 2, "read twice, kept once");
        assert_eq!(f.cnpj, "111");
        assert_eq!(f.checked, Some(today));
        assert_eq!(known_documents(&market)["GGRC11"], vec![1, 2]);
        // The same payout in a notice published again: kept once, from the newer document.
        let again = Funds { payouts: [("GGRC11".to_string(), vec![payout(5, "2026-10-14")])].into(), ..Funds::default() };
        merge_funds(&mut market, again, today);
        let f = &market.funds["GGRC11"];
        assert_eq!(f.payouts.len(), 2);
        assert_eq!(f.payouts.iter().find(|p| p.record == Date::parse("2026-10-14").unwrap()).unwrap().document, 5);
        let mut store = Store::default();
        store.portfolio.positions.push(Position { ticker: "GGRC11".into(), class: AssetClass::Etf, quantity: 1.0, average_price: 9.0, subtype: String::new(), current_value: None, note: "guessed".into() });
        assert_eq!(fix_classes(&mut store, &market), vec!["GGRC11"]);
        assert_eq!(store.portfolio.positions[0].class, AssetClass::Fii);
        assert_eq!(store.portfolio.positions[0].subtype, "tijolo");
        // A class the user set is never overridden.
        store.portfolio.positions[0].class = AssetClass::Etf;
        store.portfolio.positions[0].note = String::new();
        assert!(fix_classes(&mut store, &market).is_empty());
        // What the fund holds, once its assets are read, sets the kind the segment had guessed.
        store.portfolio.positions[0].class = AssetClass::Fii;
        let month = market.funds.get_mut("GGRC11").unwrap().months.last_mut().unwrap();
        (month.invested, month.in_receivables) = (Some(100.0), Some(90.0));
        fix_classes(&mut store, &market);
        assert_eq!(store.portfolio.positions[0].subtype, "papel");
    }

    #[test]
    fn a_cnpj_the_user_gives_is_read_at_once_and_wins() {
        let today = Date::new(2026, 10, 21).unwrap();
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let mut market = fixtures::market();
        for root in ["GGRC", "GARE", "PORD"] {
            market.fund_cnpjs.insert(root.into(), format!("{root}-cnpj"));
        }
        market.funds_read = Some(today);
        market.economy_read = Some(today);
        for f in market.funds.values_mut() {
            f.checked = Some(today);
            f.cnpj = format!("{}-cnpj", crate::text::ticker_root(&f.ticker));
        }
        market.funds.remove("BTCI11");
        let d = due(&store, &market, today, false, false);
        assert!(!d.fund_months, "BTCI looked for today already");
        assert_eq!(cnpj_of(&store, &market, "BTCI11"), None);
        store.fund_cnpjs.insert("BTCI".into(), "09552812000114".into());
        let d = due(&store, &market, today, false, false);
        assert!(d.fund_months && d.funds == vec!["BTCI11".to_string()], "given: read now, {d:?}");
        assert_eq!(cnpj_of(&store, &market, "BTCI11").map(String::as_str), Some("09552812000114"));
        // Read under it (no month in the CVM's reports: its documents still count), it is not due again today.
        let read = Funds {
            cnpjs: [("BTCI11".to_string(), "09552812000114".to_string())].into(),
            months: [("BTCI11".to_string(), vec![])].into(),
            documents: [("BTCI11".to_string(), vec![])].into(),
            months_read: true,
            ..Funds::default()
        };
        merge_funds(&mut market, read, today);
        assert!(!market.fund_cnpjs.contains_key("BTCI"), "what the user gave stays theirs");
        let d = due(&store, &market, today, false, false);
        assert!(!d.fund_months && d.funds.is_empty(), "{d:?}");
        // Another CNPJ given for GGRC: what was read under the old one goes.
        store.fund_cnpjs.insert("GGRC".into(), "26614291000100".into());
        assert!(cnpj_changed(&store, &market, "GGRC11"));
        let read = Funds { cnpjs: [("GGRC11".to_string(), "26614291000100".to_string())].into(), documents: [("GGRC11".to_string(), vec![])].into(), ..Funds::default() };
        merge_funds(&mut market, read, today);
        let f = &market.funds["GGRC11"];
        assert_eq!(f.cnpj, "26614291000100");
        assert!(f.months.is_empty() && f.payouts.is_empty(), "the old fund's months and dividends are gone");
        // Taken back, and known nowhere: the fund's data goes too.
        merge_funds(&mut market, Funds { unknown: vec!["BTCI11".into()], ..Funds::default() }, today);
        assert!(!market.funds.contains_key("BTCI11"));
    }
}
