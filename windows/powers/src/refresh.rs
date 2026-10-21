//! When to read the official sources again, and the reading itself, without holding the user's data: each function
//! takes what it needs and returns what it read, for the app to merge. Quotes once B3 publishes a new day; the funds'
//! monthly reports once a week; their documents and dividends once a day (or when asked); the economy once a day; a
//! company's statements once a month.

use std::collections::{BTreeMap, HashMap};

use crate::date::{previous_business_day, Date};
use crate::model::{AssetClass, CompanyData, FundData, Market, Store};
use crate::sources::{self, b3, cvm_fii, fetch::Http, fnet};

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

pub fn due(store: &Store, market: &Market, today: Date, evening: bool, force: bool) -> Due {
    let tickers = quoted(store);
    let latest_day = if evening { today } else { previous_business_day(today, true) };
    let quotes_stale = market.quotes_day.is_none_or(|d| d < previous_business_day(latest_day.add_days(1), true)) || tickers.iter().any(|t| !market.quotes.contains_key(t));
    let funds: Vec<String> = store.portfolio.tickers(Some(AssetClass::Fii));
    let funds_due: Vec<String> = funds
        .iter()
        .filter(|t| force || market.funds.get(*t).and_then(|f| f.checked).is_none_or(|d| d < today))
        .cloned()
        .collect();
    let missing_cnpj = funds.iter().any(|t| !market.fund_cnpjs.contains_key(crate::text::ticker_root(t)));
    Due {
        quotes: if force || quotes_stale { tickers } else { Vec::new() },
        funds: funds_due,
        fund_months: !funds.is_empty() && (missing_cnpj || market.funds_read.is_none_or(|d| d.until(today) >= 7) || (force && market.funds_read != Some(today))),
        economy: force || market.economy_read.is_none_or(|d| d < today),
    }
}

/// What was read about the funds.
#[derive(Debug, Clone, Default)]
pub struct Funds {
    pub roots: BTreeMap<String, String>,
    pub months: HashMap<String, Vec<cvm_fii::FiiMonth>>,
    pub documents: HashMap<String, Vec<fnet::Document>>,
    pub payouts: HashMap<String, Vec<fnet::Payout>>,
    pub months_read: bool,
    pub errors: Vec<String>,
}

/// Reads the funds of `tickers`: their monthly reports when `months` (all funds read, to learn each ticker's CNPJ),
/// their latest documents, and the dividends of the structured notices not read yet (`known_documents`).
pub async fn funds(http: &Http, tickers: &[String], roots: &BTreeMap<String, String>, months: bool, known_documents: &HashMap<String, Vec<u64>>, today: Date) -> Funds {
    let mut out = Funds { roots: roots.clone(), ..Funds::default() };
    if months {
        match sources::fii_months(http, &[today.y, today.y - 1], &[]).await {
            Ok(all) => {
                for (root, cnpj) in cvm_fii::cnpj_by_root(&all) {
                    out.roots.insert(root, cnpj);
                }
                for t in tickers {
                    if let Some(cnpj) = out.roots.get(crate::text::ticker_root(t)) {
                        let mut mine: Vec<cvm_fii::FiiMonth> = all.iter().filter(|m| &m.cnpj == cnpj).cloned().collect();
                        mine.sort_by_key(|m| m.date);
                        out.months.insert(t.clone(), mine);
                    }
                }
                out.months_read = true;
            }
            Err(e) => out.errors.push(e),
        }
    }
    for t in tickers {
        let Some(cnpj) = out.roots.get(crate::text::ticker_root(t)).cloned() else {
            out.errors.push(format!("{t}: its CNPJ is not known yet (the CVM's reports could not be read)"));
            continue;
        };
        match sources::fund_documents(http, &cnpj, 60).await {
            Ok(docs) => {
                let known = known_documents.get(t).cloned().unwrap_or_default();
                let mut payouts = Vec::new();
                for d in docs.iter().filter(|d| d.structured && d.topic == fnet::Topic::Payout && !known.contains(&d.id)).take(15) {
                    match sources::payouts_of(http, d).await {
                        Ok(p) => payouts.extend(p),
                        Err(e) => out.errors.push(format!("{t}: {e}")),
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
    let tickers: Vec<String> = read.months.keys().chain(read.documents.keys()).cloned().collect();
    for t in tickers {
        let f = market.funds.entry(t.clone()).or_insert_with(|| FundData { ticker: t.clone(), ..FundData::default() });
        if let Some(cnpj) = read.roots.get(crate::text::ticker_root(&t)) {
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
                if !f.payouts.iter().any(|x| x.document == payout.document && x.record == payout.record && x.amortization == payout.amortization) {
                    f.payouts.push(payout.clone());
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
    let figures = sources::company_figures(http, &cnpj, &[today.y - 1, today.y - 2], &[today.y, today.y - 1]).await?;
    Ok((CompanyData { ticker: ticker.to_string(), cnpj, name, figures, checked: Some(today) }, register))
}

/// Classes guessed at import (a ticker ending in 11) set from what the sources say: a fund in the CVM's reports is a
/// FII, a fund's shares at B3 under BDI 14 an ETF. Returns the tickers changed.
pub fn fix_classes(store: &mut Store, market: &Market) -> Vec<String> {
    let mut changed = Vec::new();
    for p in store.portfolio.positions.iter_mut().filter(|p| p.class.quoted()) {
        let root = crate::text::ticker_root(&p.ticker);
        let from_quote = market.quotes.get(&p.ticker).map(b3::kind_of).unwrap_or("");
        let class = if market.fund_cnpjs.contains_key(root) && p.ticker.ends_with("11") {
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
        if p.class == AssetClass::Fii && p.subtype.is_empty() {
            if let Some(m) = market.funds.get(&p.ticker).and_then(|f| f.months.last()) {
                p.subtype = cvm_fii::subtype(m).to_string();
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
        let mut store = Store::default();
        store.portfolio.positions.push(Position { ticker: "GGRC11".into(), class: AssetClass::Etf, quantity: 1.0, average_price: 9.0, subtype: String::new(), current_value: None, note: "guessed".into() });
        assert_eq!(fix_classes(&mut store, &market), vec!["GGRC11"]);
        assert_eq!(store.portfolio.positions[0].class, AssetClass::Fii);
        assert_eq!(store.portfolio.positions[0].subtype, "tijolo");
        // A class the user set is never overridden.
        store.portfolio.positions[0].class = AssetClass::Etf;
        store.portfolio.positions[0].note = String::new();
        assert!(fix_classes(&mut store, &market).is_empty());
    }
}
