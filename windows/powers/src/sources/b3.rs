//! B3's official daily quotes (COTAHIST, "Séries Históricas"): one fixed-width line per instrument and market, the
//! day's open, high, low, average and close, in a ZIP published each evening for the day. Free, no key.
//! Layout: B3's "SeriesHistoricas_Layout" (positions 1-based in the comments).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::date::Date;

/// One instrument's day on the regular (lote padrão) or odd-lot market.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    pub ticker: String,
    pub date: Date,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub average: f64,
    pub close: f64,
    pub trades: u32,
    /// R$ traded.
    pub volume: f64,
    /// B3's BDI code: "02" lote padrão, "12" real-estate funds, "14" funds and certificates…
    pub bdi: String,
    /// "ON", "PN", "UNT", "CI"…
    pub kind: String,
    pub name: String,
    pub isin: String,
    /// 10 regular market, 20 odd lot.
    pub market: u16,
}

/// Where the quotes of `day` are.
pub fn url(day: Date) -> String {
    format!("https://bvmf.bmfbovespa.com.br/InstDados/SerHist/COTAHIST_D{:02}{:02}{:04}.ZIP", day.d, day.m, day.y)
}

fn field(line: &str, from: usize, to: usize) -> &str {
    line.get(from - 1..to).unwrap_or("").trim()
}

fn price(line: &str, from: usize, to: usize, factor: f64) -> f64 {
    field(line, from, to).parse::<f64>().map(|v| v / 100.0 / factor).unwrap_or(0.0)
}

/// The quotes of a COTAHIST text, for the tickers in `wanted` (all when None). Only the cash market (regular and odd
/// lot); options, futures and the rest are skipped.
pub fn parse(text: &str, wanted: Option<&HashSet<String>>) -> Vec<Quote> {
    let mut out = Vec::new();
    for line in text.lines() {
        if !line.starts_with("01") || line.len() < 245 {
            continue;
        }
        let market: u16 = field(line, 25, 27).parse().unwrap_or(0);
        if market != 10 && market != 20 {
            continue;
        }
        let raw = field(line, 13, 24);
        let ticker = raw.strip_suffix('F').filter(|_| market == 20).unwrap_or(raw).to_string();
        if let Some(w) = wanted {
            if !w.contains(&ticker) {
                continue;
            }
        }
        let Some(date) = Date::parse(field(line, 3, 10)) else { continue };
        let factor = field(line, 211, 217).parse::<f64>().ok().filter(|f| *f > 0.0).unwrap_or(1.0);
        out.push(Quote {
            ticker,
            date,
            open: price(line, 57, 69, factor),
            high: price(line, 70, 82, factor),
            low: price(line, 83, 95, factor),
            average: price(line, 96, 108, factor),
            close: price(line, 109, 121, factor),
            trades: field(line, 148, 152).parse().unwrap_or(0),
            volume: field(line, 171, 188).parse::<f64>().map(|v| v / 100.0).unwrap_or(0.0),
            bdi: field(line, 11, 12).to_string(),
            kind: field(line, 40, 49).to_string(),
            name: field(line, 28, 39).to_string(),
            isin: field(line, 231, 242).to_string(),
            market,
        });
    }
    out
}

/// One quote per ticker: the regular market's when there is one, else the odd lot's.
pub fn by_ticker(quotes: Vec<Quote>) -> HashMap<String, Quote> {
    let mut out: HashMap<String, Quote> = HashMap::new();
    for q in quotes {
        match out.get(&q.ticker) {
            Some(have) if have.market == 10 && q.market != 10 => {}
            _ => {
                out.insert(q.ticker.clone(), q);
            }
        }
    }
    out
}

/// What kind of asset B3 says a ticker is: "fii" (BDI 12, or a fund's "CI" shares with an ISIN of real-estate
/// funds), "etf" (BDI 14 "CI"), "acao" (ON, PN, UNT…), "bdr" (BDI 34/35, 3x/4x tickers), or "" when unsure.
pub fn kind_of(q: &Quote) -> &'static str {
    let k = q.kind.to_ascii_uppercase();
    match q.bdi.as_str() {
        "12" => "fii",
        "14" if k.starts_with("CI") => "etf",
        "34" | "35" => "bdr",
        "02" | "96" if k.starts_with("ON") || k.starts_with("PN") || k.starts_with("UNT") => "acao",
        _ if q.isin.len() >= 9 && &q.isin[6..9] == "CTF" => "fii",
        _ => "",
    }
}

/// B3's register of listed funds (what its site searches), for a fund whose shares the CVM's reports leave without
/// an ISIN: the funds whose code or name matches `root` ("BTCI").
pub fn fund_search_url(root: &str) -> String {
    let query = format!("{{\"typeFund\":7,\"pageNumber\":1,\"pageSize\":20,\"keyword\":\"{root}\"}}");
    format!("https://sistemaswebb3-listados.b3.com.br/fundsProxy/fundsCall/GetListedFundsSIG/{}", crate::text::base64(query.as_bytes()))
}

/// The CNPJ of the fund coded `root` in B3's answer: the entry whose code ("acronym", or any field) is the root, and
/// its field named like a CNPJ (a valid one, as text or as a number).
pub fn cnpj_in_search(body: &str, root: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    fn walk(v: &serde_json::Value, root: &str) -> Option<String> {
        match v {
            serde_json::Value::Object(map) => {
                let coded = map.iter().any(|(k, x)| {
                    let k = k.to_ascii_lowercase();
                    (k.contains("acronym") || k.contains("codigo") || k.contains("code") || k.contains("sigla")) && x.as_str().is_some_and(|s| s.trim().eq_ignore_ascii_case(root))
                });
                if coded {
                    let cnpj = map.iter().filter(|(k, _)| k.to_ascii_lowercase().contains("cnpj")).find_map(|(_, x)| {
                        let typed = x.as_str().map(str::to_string).or_else(|| x.as_u64().map(|n| format!("{n:014}")))?;
                        crate::text::cnpj(&typed).ok()
                    });
                    if cnpj.is_some() {
                        return cnpj;
                    }
                }
                map.values().find_map(|x| walk(x, root))
            }
            serde_json::Value::Array(list) => list.iter().find_map(|x| walk(x, root)),
            _ => None,
        }
    }
    walk(&v, root)
}

/// A COTAHIST line, for tests and fixtures: the fields that matter, the rest blank or zero.
pub fn line(day: Date, ticker: &str, bdi: &str, kind: &str, isin: &str, close: f64, market: u16) -> String {
    let mut l = vec![b' '; 245];
    let mut put = |from: usize, s: &str| {
        for (i, b) in s.bytes().enumerate() {
            if from - 1 + i < 245 {
                l[from - 1 + i] = b;
            }
        }
    };
    let p = |v: f64| format!("{:013}", (v * 100.0).round() as i64);
    put(1, "01");
    put(3, &format!("{:04}{:02}{:02}", day.y, day.m, day.d));
    put(11, bdi);
    put(13, &format!("{ticker:<12}"));
    put(25, &format!("{market:03}"));
    put(28, &format!("{:<12}", "NOME"));
    put(40, &format!("{kind:<10}"));
    put(57, &p(close * 0.99));
    put(70, &p(close * 1.01));
    put(83, &p(close * 0.98));
    put(96, &p(close));
    put(109, &p(close));
    put(148, "00100");
    put(153, &format!("{:018}", 1000));
    put(171, &format!("{:018}", (close * 1000.0 * 100.0).round() as i64));
    put(211, "0000001");
    put(231, &format!("{isin:<12}"));
    String::from_utf8(l).expect("ascii")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_funds_cnpj_from_b3s_register() {
        assert!(fund_search_url("BTCI").ends_with(&crate::text::base64(br#"{"typeFund":7,"pageNumber":1,"pageSize":20,"keyword":"BTCI"}"#)));
        let body = r#"{"page":{"pageNumber":1,"pageSize":20,"totalRecords":2,"totalPages":1},"results":[{"id":1,"typeName":"FII","acronym":"BTCR","fundName":"OUTRO","cnpj":"11.111.111/0001-11"},{"id":2,"typeName":"FII","acronym":"BTCI","fundName":"BTG PACTUAL CREDITO IMOBILIARIO","cnpj":"09.552.812/0001-14"}]}"#;
        assert_eq!(cnpj_in_search(body, "BTCI").as_deref(), Some("09552812000114"));
        assert_eq!(cnpj_in_search(body, "XXXX"), None);
        assert_eq!(cnpj_in_search("not json", "BTCI"), None);
    }

    #[test]
    fn the_days_quotes_are_read_by_position() {
        let day = Date::new(2026, 10, 20).unwrap();
        let text = [
            "00COTAHIST.2026BOVESPA 20261020".to_string(),
            line(day, "GGRC11", "12", "CI", "BRGGRCCTF006", 9.12, 10),
            line(day, "TIMS3", "02", "ON      NM", "BRTIMSACNOR5", 19.20, 10),
            line(day, "TIMS3F", "96", "ON      NM", "BRTIMSACNOR5", 19.25, 20),
            line(day, "SPXB11", "14", "CI", "BRSPXBCTF000", 17.12, 10),
            line(day, "PETRJ420", "78", "OPC", "BRPETRACNPR6", 0.42, 70),
            "99COTAHIST.2026BOVESPA 2026102000000000006".to_string(),
        ]
        .join("\n");
        let all = parse(&text, None);
        assert_eq!(all.len(), 4, "options are not the cash market");
        let q = &all[0];
        assert_eq!(q.ticker, "GGRC11");
        assert_eq!(q.date, day);
        assert!((q.close - 9.12).abs() < 1e-9);
        assert_eq!(kind_of(q), "fii");
        let map = by_ticker(all.clone());
        assert!((map["TIMS3"].close - 19.20).abs() < 1e-9, "the regular market wins over the odd lot");
        assert_eq!(kind_of(&map["TIMS3"]), "acao");
        assert_eq!(kind_of(&map["SPXB11"]), "etf");
        let only: HashSet<String> = ["SPXB11".to_string()].into();
        assert_eq!(parse(&text, Some(&only)).len(), 1);
        assert_eq!(url(day), "https://bvmf.bmfbovespa.com.br/InstDados/SerHist/COTAHIST_D20102026.ZIP");
    }
}
