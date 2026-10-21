//! The official sources Zheus reads, and how each is fetched: B3's daily quotes, the CVM's open data on funds and
//! companies, FundosNet's documents and dividends, and the Banco Central's rates and survey. All free, no key, no
//! account; only these addresses are ever contacted, and only while the user has them on.

pub mod b3;
pub mod bcb;
pub mod cvm_cia;
pub mod cvm_fii;
pub mod fetch;
pub mod fnet;

use std::collections::{HashMap, HashSet};

use crate::date::{is_business_day, previous_business_day, Date};
use fetch::Http;

/// The latest closing quotes of `tickers`: the newest day B3 has published, looking back from `today` (today's own
/// file only once the evening has come, `evening`), and never back to `have` (a day already read: its file is not
/// downloaded again). Returns that day and one quote per ticker found.
pub async fn quotes(http: &Http, today: Date, evening: bool, tickers: &[String], have: Option<Date>) -> Result<(Date, HashMap<String, b3::Quote>), String> {
    let wanted: HashSet<String> = tickers.iter().cloned().collect();
    let mut day = if evening && is_business_day(today, true) { today } else { previous_business_day(today, true) };
    let mut last_error = String::new();
    for _ in 0..6 {
        if let Some(h) = have.filter(|h| day <= *h) {
            // Nothing newer yet: the day already read stands.
            return Ok((h, HashMap::new()));
        }
        match http.zip_entries(&b3::url(day), &[".txt".to_string()]).await {
            Ok(files) if !files.is_empty() => {
                let text = crate::text::decode(&files[0].1);
                return Ok((day, b3::by_ticker(b3::parse(&text, Some(&wanted)))));
            }
            Ok(_) => last_error = "B3's file was empty".into(),
            Err(e) => last_error = e,
        }
        day = previous_business_day(day, true);
    }
    Err(format!("B3's quotes could not be read: {last_error}"))
}

/// The monthly reports of the funds whose CNPJs are `cnpjs` (all when empty), for `years`.
pub async fn fii_months(http: &Http, years: &[i32], cnpjs: &[String]) -> Result<Vec<cvm_fii::FiiMonth>, String> {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for &year in years {
        match http.get(&cvm_fii::url(year)).await.and_then(|bytes| cvm_fii::parse_zip(&bytes, cnpjs)) {
            Ok(months) => out.extend(months),
            Err(e) => errors.push(e),
        }
    }
    if out.is_empty() && !errors.is_empty() {
        return Err(format!("the CVM's fund reports could not be read: {}", errors.join("; ")));
    }
    Ok(out)
}

/// A fund's latest documents (CNPJ digits), newest first.
pub async fn fund_documents(http: &Http, cnpj: &str, count: usize) -> Result<Vec<fnet::Document>, String> {
    let body = http.text(&fnet::list_url(cnpj, count)).await?;
    fnet::parse_list(&body)
}

/// The dividends and amortizations of one structured notice.
pub async fn payouts_of(http: &Http, document: &fnet::Document) -> Result<Vec<fnet::Payout>, String> {
    let body = http.get(&fnet::download_url(document.id)).await?;
    let xml = crate::text::decode(&fnet::unwrap_download(&body));
    Ok(fnet::parse_payouts(&xml, document.id))
}

/// The economy now: the latest Selic, CDI, IPCA over twelve months and dollar, and the Focus survey's medians. What
/// cannot be read is left empty; an error only when nothing could be.
pub async fn economy(http: &Http) -> Result<bcb::Macro, String> {
    let mut m = bcb::Macro::default();
    let mut errors = Vec::new();
    let latest = |code: u32| {
        let http = http.clone();
        async move { http.text(&bcb::series_url(code, 1)).await.and_then(|b| bcb::parse_series(&b)).map(|s| s.last().copied()) }
    };
    for (code, slot) in [(bcb::SELIC, 0), (bcb::CDI, 1), (bcb::IPCA_12M, 2), (bcb::USD, 3)] {
        match latest(code).await {
            Ok(Some((date, value))) => {
                match slot {
                    0 => m.selic = Some(value),
                    1 => m.cdi = Some(value),
                    2 => m.ipca_12m = Some(value),
                    _ => m.usd = Some(value),
                }
                // The Selic target's series runs ahead to the next Copom meeting, and the IPCA's is a month's:
                // the figures are as of the daily series' last day.
                if code == bcb::CDI || code == bcb::USD {
                    m.date = Some(m.date.map_or(date, |d: Date| d.max(date)));
                }
            }
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }
    for (indicator, selic) in [("Selic", true), ("IPCA", false)] {
        match http.text(&bcb::focus_url(indicator)).await.and_then(|b| bcb::parse_focus(&b)) {
            Ok(list) if selic => m.selic_expected = list,
            Ok(list) => m.ipca_expected = list,
            Err(e) => errors.push(e),
        }
    }
    if m.selic.is_none() && m.cdi.is_none() && m.selic_expected.is_empty() {
        return Err(format!("the Banco Central could not be read: {}", errors.join("; ")));
    }
    Ok(m)
}

/// Tickers to (company CNPJ, name), from the companies' register of `year`.
pub async fn company_tickers(http: &Http, year: i32) -> Result<HashMap<String, (String, String)>, String> {
    let files = http.zip_entries(&cvm_cia::fca_url(year), &[format!("valor_mobiliario_{year}.csv")]).await?;
    let (_, bytes) = files.first().ok_or("the CVM's register has no securities table")?;
    Ok(cvm_cia::tickers(&crate::text::decode(bytes)))
}

/// A company's figures per period (CNPJ digits): the yearly statements of `years` and the quarterly ones of
/// `quarter_years`, consolidated, oldest first; and the files that could not be read.
pub async fn company_figures(http: &Http, cnpj: &str, years: &[i32], quarter_years: &[i32]) -> Result<(Vec<cvm_cia::Figures>, Vec<String>), String> {
    let wanted = vec![cnpj.to_string()];
    let mut accounts = Vec::new();
    let mut errors = Vec::new();
    for (kind, list) in [("dfp", years), ("itr", quarter_years)] {
        for &year in list {
            let names: Vec<String> = cvm_cia::TABLES.iter().map(|t| cvm_cia::entry_name(kind, t, year)).collect();
            match http.zip_entries(&cvm_cia::url(kind, year), &names).await {
                Ok(files) => {
                    for (_, bytes) in files {
                        accounts.extend(cvm_cia::parse_accounts(&crate::text::decode(&bytes), &wanted));
                    }
                }
                Err(e) => errors.push(e),
            }
        }
    }
    if accounts.is_empty() {
        return Err(if errors.is_empty() {
            "the CVM has no statements for this company".into()
        } else {
            format!("the CVM's statements could not be read: {}", errors.join("; "))
        });
    }
    Ok((cvm_cia::figures(&accounts), errors))
}
