//! The official sources, for real: what each one sends today, and what Zheus makes of it. Ignored by default (they
//! need the internet, and the sites change on their own schedule); `.github/workflows/powers-live.yml` runs them:
//!
//!     cargo test -p coucou-powers --test live -- --ignored --nocapture --test-threads=1
//!
//! The raw_ tests print the first lines of every file, so a change of format shows in the log; the others print
//! what was read and check that it makes sense.

use std::collections::{BTreeMap, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

use coucou_powers::date::{previous_business_day, Date};
use coucou_powers::model::{AssetClass, Market, Store};
use coucou_powers::sources::{self, b3, bcb, cvm_cia, cvm_fii, fetch::Http, fnet};
use coucou_powers::{company, engine, fii, refresh, zip};
use serde_json::json;

/// A portfolio of real tickers (made-up quantities and prices), pasted the way Investidor10 shows it.
const PASTE: &str = "FIIs\nGGRC11 150 R$ 9,20\nGARE11 180 R$ 8,55\nBTCI11 140 R$ 8,90\nPORD11 120 R$ 8,45\nAções\nTIMS3 30 R$ 18,60\nETFs\nSPXB11 40 R$ 16,75";
const FUNDS: [&str; 4] = ["GGRC11", "GARE11", "BTCI11", "PORD11"];
/// BTCI11's fund, as its own monthly reports on FundosNet give it: the CVM's reports list the fund without the ISIN of
/// its shares, so the user gives it to Zheus.
const BTCI_CNPJ: &str = "09.552.812/0001-14";

fn given() -> BTreeMap<String, String> {
    [("BTCI".to_string(), coucou_powers::text::cnpj(BTCI_CNPJ).unwrap())].into()
}

/// Today in Brasília.
fn today() -> Date {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    Date::from_days((secs - 3 * 3600).div_euclid(86_400))
}

fn head(text: &str, lines: usize) -> String {
    text.lines().take(lines).map(|l| coucou_powers::text::clip(l, 400)).collect::<Vec<_>>().join("\n")
}

fn rows_with(text: &str, needle: &str, n: usize) -> String {
    text.lines().filter(|l| l.contains(needle)).take(n).map(|l| coucou_powers::text::clip(l, 400)).collect::<Vec<_>>().join("\n")
}

fn section(title: &str) {
    println!("\n══════ {title} ══════");
}

async fn fund_months(http: &Http, today: Date) -> Vec<cvm_fii::FiiMonth> {
    sources::fii_months(http, &[today.y, today.y - 1], &[]).await.expect("the CVM's fund reports")
}

// ── What each source sends ────────────────────────────────────────────────────

/// Google Finance's quote pages: the size, the data blocks the page carries (AF_initDataCallback), where the
/// ticker shows in them, and what gfinance.rs reads, for shares, funds, an ETF and a ticker that does not exist.
#[tokio::test]
#[ignore]
async fn live_raw_gfinance() {
    let http = Http::new();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let clip = |s: &str, n: usize| coucou_powers::text::clip(s, n);
    for (i, ticker) in ["GGRC11", "TIMS3", "BOVA11", "ZZZZ11"].into_iter().enumerate() {
        section(&format!("Google Finance {ticker}"));
        let url = sources::gfinance::url(ticker);
        match http.page(&url).await {
            Ok(page) => {
                println!("{} bytes", page.len());
                let title = page.split("<title>").nth(1).and_then(|t| t.split("</title>").next()).unwrap_or("(no title)");
                println!("title: {}", clip(title, 200));
                let full = i == 0;
                for (at, _) in page.match_indices("AF_initDataCallback(").take(if full { 40 } else { 12 }) {
                    let block = &page[at..(at + if full { 1800 } else { 500 }).min(page.len())];
                    println!("--- AF at {at}: {}", clip(block, if full { 1800 } else { 500 }));
                }
                for (at, _) in page.match_indices(ticker).take(if full { 14 } else { 6 }) {
                    println!("~~~ {ticker} at {at}: {}", clip(&page[at.saturating_sub(300)..(at + 500).min(page.len())], 800));
                }
                println!("read: {:?}", sources::gfinance::parse(&page, ticker));
            }
            Err(e) => println!("error: {e}"),
        }
    }
    let read = sources::live_price(&http, "GGRC11").await.expect("GGRC11 on Google Finance");
    assert!(read.price > 1.0 && read.price < 100.0 && read.at > now - 10 * 86_400, "{read:?}");
    assert!(sources::live_price(&http, "ZZZZ11").await.is_err(), "a ticker that does not exist has no price");
}

#[tokio::test]
#[ignore]
async fn live_raw_b3() {
    let http = Http::new();
    let mut day = previous_business_day(today(), true);
    for _ in 0..6 {
        section(&format!("B3 COTAHIST {}", day.br()));
        match http.zip_entries(&b3::url(day), &[".txt".to_string()]).await {
            Ok(files) if !files.is_empty() => {
                let (name, bytes) = &files[0];
                let text = coucou_powers::text::decode(bytes);
                println!("{name}: {} bytes, {} lines", bytes.len(), text.lines().count());
                println!("{}", head(&text, 2));
                for t in FUNDS.iter().chain(["TIMS3", "SPXB11"].iter()) {
                    println!("{}", rows_with(&text, &format!("{t:<12}"), 2));
                }
                return;
            }
            Ok(_) => println!("empty"),
            Err(e) => println!("error: {e}"),
        }
        day = previous_business_day(day, true);
    }
    panic!("no COTAHIST file in six business days");
}

#[tokio::test]
#[ignore]
async fn live_raw_cvm_fii() {
    let http = Http::new();
    let year = today().y;
    section(&format!("CVM FII monthly reports {year}"));
    let bytes = http.get(&cvm_fii::url(year)).await.expect("the year's ZIP");
    let archive = zip::Archive::new(&bytes).expect("a ZIP");
    println!("{} bytes", bytes.len());
    for e in &archive.entries {
        println!("entry {} ({} → {} bytes)", e.name, e.compressed, e.size);
    }
    for e in &archive.entries {
        let text = coucou_powers::text::decode(&archive.read(e).expect("entry"));
        println!("\n— {} ({} lines)\n{}", e.name, text.lines().count(), head(&text, 2));
        // GGRC11's fund, by its ISIN or its name.
        let rows = rows_with(&text, "GGRC", 2);
        println!("{}", if rows.is_empty() { rows_with(&text, "GGR COVEPI", 2) } else { rows });
    }
    // Each fund of the portfolio as the general table names it: its ISIN, or the rows of its name.
    let general = archive.entries.iter().find(|e| e.name.contains("geral")).map(|e| coucou_powers::text::decode(&archive.read(e).unwrap())).unwrap_or_default();
    let blank_isin = general.lines().skip(1).filter(|l| l.split(';').nth(8).is_some_and(|i| i.trim().is_empty())).count();
    println!("\n{blank_isin} rows of the general table have no ISIN");
    for (root, names) in [("BTCI", ["BTG PACTUAL CR", "BTG CRED"]), ("GARE", ["GUARDIAN", "GUARDIAL"]), ("PORD", ["POLO CR", "POLO CRÉD"]), ("GGRC", ["ZAGROS", "GGR COVEPI"])] {
        let by_isin = rows_with(&general, root, 1);
        let by_name: Vec<String> = names.iter().map(|n| rows_with(&general.to_uppercase(), n, 2)).filter(|r| !r.is_empty()).collect();
        println!("{root}: by ISIN {}\n  by name {}", if by_isin.is_empty() { "none".into() } else { coucou_powers::text::clip(&by_isin, 300) }, coucou_powers::text::clip(&by_name.join(" | "), 900));
    }
    // BTCI11's fund by its CNPJ, in each table, and the names of BTG's funds.
    let cnpj = &BTCI_CNPJ[..10];
    for e in &archive.entries {
        let text = coucou_powers::text::decode(&archive.read(e).expect("entry"));
        println!("{} rows of {cnpj} in {}: {}", text.lines().filter(|l| l.contains(cnpj)).count(), e.name, rows_with(&text, cnpj, 2));
    }
    // What each fund holds, column by column: the assets table's names as the CVM writes them now.
    if let Some(e) = archive.entries.iter().find(|e| e.name.contains("ativo_passivo")) {
        let text = coucou_powers::text::decode(&archive.read(e).expect("entry"));
        let header: Vec<&str> = text.lines().next().unwrap_or("").split(';').collect();
        println!("\n{} columns in {}: {}", header.len(), e.name, header.iter().enumerate().map(|(i, h)| format!("{i} {h}")).collect::<Vec<_>>().join(" | "));
        for (fund, cnpj) in [("BTCI11", "09.552.812"), ("PORD11", "17.156.502"), ("GGRC11", "26.614.291"), ("GARE11", "37.295.919")] {
            if let Some(row) = text.lines().filter(|l| l.starts_with(cnpj)).last() {
                let cells: Vec<String> = row.split(';').zip(&header).filter(|(v, _)| !v.trim().is_empty() && v.trim() != "0").map(|(v, h)| format!("{h}={v}")).collect();
                println!("{fund}: {}", cells.join("; "));
            }
        }
    }
}

#[tokio::test]
#[ignore]
async fn live_raw_fnet() {
    let http = Http::new();
    let today = today();
    let months = fund_months(&http, today).await;
    let roots = cvm_fii::cnpj_by_root(&months);
    let cnpj = roots.get("GGRC").expect("GGRC11's CNPJ from the ISINs").clone();
    section(&format!("FundosNet list for GGRC11 ({cnpj})"));
    let body = http.text(&fnet::list_url(&cnpj, 20)).await.expect("the list");
    println!("{}", coucou_powers::text::clip(&body, 3000));
    let docs = fnet::parse_list(&body).expect("parsed");
    for d in docs.iter().take(20) {
        println!("{} {:?} [{} / {} / {}] ref {} delivered {:?} structured {} — {}", d.id, d.topic, d.category, d.kind, d.species, d.reference, d.delivered, d.structured, coucou_powers::text::clip(&d.info, 80));
    }
    if let Some(d) = docs.iter().find(|d| d.structured && d.topic == fnet::Topic::Payout) {
        section(&format!("Structured payout notice {}", d.id));
        let raw = http.get(&fnet::download_url(d.id)).await.expect("the document");
        println!("raw: {}", coucou_powers::text::clip(&String::from_utf8_lossy(&raw[..raw.len().min(200)]), 200));
        let xml = coucou_powers::text::decode(&fnet::unwrap_download(&raw));
        println!("{}", coucou_powers::text::clip(&xml, 4000));
        println!("parsed: {:?}", fnet::parse_payouts(&xml, d.id));
    } else {
        println!("no structured payout notice among the last 20 documents");
    }
    let btci = coucou_powers::text::cnpj(BTCI_CNPJ).unwrap();
    section(&format!("FundosNet list for BTCI11 ({btci}, its CNPJ as its reports give it)"));
    match http.text(&fnet::list_url(&btci, 10)).await {
        Ok(body) => {
            println!("{}", coucou_powers::text::clip(&body, 1500));
            for d in fnet::parse_list(&body).unwrap_or_default().iter().take(10) {
                println!("{} {:?} [{} / {}] ref {} delivered {:?} — {}", d.id, d.topic, d.category, d.kind, d.reference, d.delivered, coucou_powers::text::clip(&d.info, 80));
            }
        }
        Err(e) => println!("error: {e}"),
    }
}

#[tokio::test]
#[ignore]
async fn live_raw_bcb() {
    let http = Http::new();
    for code in [bcb::SELIC, bcb::CDI, bcb::IPCA_12M, bcb::USD] {
        section(&format!("SGS {code}"));
        match http.text(&bcb::series_url(code, 2)).await {
            Ok(b) => println!("{}", coucou_powers::text::clip(&b, 600)),
            Err(e) => println!("error: {e}"),
        }
    }
    for indicator in ["Selic", "IPCA"] {
        section(&format!("Focus {indicator}"));
        match http.text(&bcb::focus_url(indicator)).await {
            Ok(b) => println!("{}", coucou_powers::text::clip(&b, 1500)),
            Err(e) => println!("error: {e}"),
        }
    }
}

#[tokio::test]
#[ignore]
async fn live_raw_cvm_companies() {
    let http = Http::new();
    let today = today();
    let mut tim = String::new();
    for year in [today.y, today.y - 1] {
        section(&format!("CVM FCA {year}: valor_mobiliario"));
        match http.zip_entries(&cvm_cia::fca_url(year), &[format!("valor_mobiliario_{year}.csv")]).await {
            Ok(files) => {
                for (name, bytes) in &files {
                    let text = coucou_powers::text::decode(bytes);
                    println!("{name}: {} lines\n{}\n{}", text.lines().count(), head(&text, 2), rows_with(&text, "TIMS3", 3));
                    let map = cvm_cia::tickers(&text);
                    println!("{} tickers read; TIMS3 → {:?}; PETR4 → {:?}", map.len(), map.get("TIMS3"), map.get("PETR4"));
                    if let Some((cnpj, _)) = map.get("TIMS3") {
                        tim = cnpj.clone();
                    }
                }
                if !tim.is_empty() {
                    break;
                }
            }
            Err(e) => println!("error: {e}"),
        }
    }
    assert!(!tim.is_empty(), "TIMS3 in the register");
    for (kind, year) in [("dfp", today.y - 1), ("itr", today.y)] {
        let names: Vec<String> = cvm_cia::TABLES.iter().map(|t| cvm_cia::entry_name(kind, t, year)).collect();
        section(&format!("CVM {kind} {year}"));
        match http.zip_entries(&cvm_cia::url(kind, year), &names).await {
            Ok(files) => {
                for (name, bytes) in &files {
                    let text = coucou_powers::text::decode(bytes);
                    let mine: Vec<&str> = text.lines().filter(|l| coucou_powers::text::cnpj_chars(l).contains(&tim)).collect();
                    println!("\n— {name}: {} lines, {} of TIM\n{}", text.lines().count(), mine.len(), head(&text, 1));
                    for l in mine.iter().take(6) {
                        println!("{}", coucou_powers::text::clip(l, 400));
                    }
                }
            }
            Err(e) => println!("error: {e}"),
        }
    }
}

// ── What Zheus makes of it ────────────────────────────────────────────────────

#[tokio::test]
#[ignore]
async fn live_quotes() {
    let http = Http::new();
    let tickers: Vec<String> = FUNDS.iter().chain(["TIMS3", "SPXB11"].iter()).map(|t| t.to_string()).collect();
    let (day, quotes) = sources::quotes(&http, today(), false, &tickers, None).await.expect("quotes");
    section(&format!("Quotes of {}", day.br()));
    for t in &tickers {
        match quotes.get(t) {
            Some(q) => println!("{t}: close {} (open {}, low {}, high {}), {} trades, bdi {} kind {} isin {} → {}", q.close, q.open, q.low, q.high, q.trades, q.bdi, q.kind, q.isin, b3::kind_of(q)),
            None => println!("{t}: not found"),
        }
    }
    assert!(today().until(day) >= -10, "a recent day");
    for t in &tickers {
        let q = quotes.get(t).unwrap_or_else(|| panic!("{t} quoted"));
        assert!(q.close > 1.0 && q.close < 1000.0, "{t} at {}", q.close);
    }
    assert_eq!(b3::kind_of(&quotes["GGRC11"]), "fii");
    assert_eq!(b3::kind_of(&quotes["SPXB11"]), "etf");
    assert_eq!(b3::kind_of(&quotes["TIMS3"]), "acao");
}

#[tokio::test]
#[ignore]
async fn live_economy() {
    let m = sources::economy(&Http::new()).await.expect("the Banco Central");
    section("Economy");
    println!("{m:#?}");
    let selic = m.selic.expect("Selic");
    assert!((2.0..30.0).contains(&selic), "Selic {selic}");
    assert!(m.cdi.is_some_and(|c| (c - selic).abs() < 1.0), "CDI next to the Selic: {:?}", m.cdi);
    assert!(m.ipca_12m.is_some_and(|i| (-3.0..20.0).contains(&i)));
    assert!(m.usd.is_some_and(|u| (2.0..15.0).contains(&u)));
    let y = today().y;
    assert!(m.selic_at(y).is_some() || m.selic_at(y + 1).is_some(), "Focus Selic");
    assert!(m.ipca_at(y + 1).is_some(), "Focus IPCA");
}

#[tokio::test]
#[ignore]
async fn live_funds() {
    let http = Http::new();
    let today = today();
    let tickers: Vec<String> = FUNDS.iter().map(|t| t.to_string()).collect();
    let read = refresh::funds(&http, &tickers, &BTreeMap::new(), &given(), Some(&tickers), &HashMap::new(), today).await;
    section("Funds read");
    println!("errors: {:?}", read.errors);
    println!("{} ticker roots known", read.roots.len());
    for t in &tickers {
        let root = coucou_powers::text::ticker_root(t);
        println!("{t}: CNPJ {:?} (from the sources {:?}), {} months, {} documents, {} payouts", read.cnpjs.get(t), read.roots.get(root), read.months.get(t).map_or(0, |m| m.len()), read.documents.get(t).map_or(0, |d| d.len()), read.payouts.get(t).map_or(0, |p| p.len()));
        if let Some(last) = read.months.get(t).and_then(|m| m.last()) {
            println!(
                "  last month {}: VP/cota {:?}, DY {:?} %, holders {:?}, segment {:?}, mandate {:?}; invested {:?}: buildings {:?}, receivables {:?}, funds {:?} → {} (from the assets: {:?})",
                last.date.br(),
                last.vp_share,
                last.dividend_yield,
                last.holders,
                last.segment,
                last.mandate,
                last.invested,
                last.in_buildings,
                last.in_receivables,
                last.in_funds,
                cvm_fii::subtype(last),
                cvm_fii::subtype_from_assets(last)
            );
        }
        for p in read.payouts.get(t).into_iter().flatten().take(6) {
            println!("  payout record {} pay {:?} R$ {} amort {} ({})", p.record.br(), p.payment.map(|d| d.br()), p.value, p.amortization, p.period);
        }
    }
    let mut market = Market::default();
    refresh::merge_funds(&mut market, read, today);
    market.economy = sources::economy(&http).await.unwrap_or_default();
    if let Ok((day, quotes)) = sources::quotes(&http, today, false, &tickers, None).await {
        refresh::merge_quotes(&mut market, day, quotes);
    }
    let mut with_months = 0;
    for t in &tickers {
        let Some(fund) = market.funds.get(t) else {
            section(&format!("{t}: nothing read"));
            continue;
        };
        let r = fii::report(fund, market.price(t), &market.economy, today);
        section(&format!("{t} for the brain"));
        println!("{}", fii::report_for_model(&r));
        let ev = fii::events(fund, 100.0, today, 60);
        println!("{}", fii::events_for_model(&ev));
        assert!(!fund.cnpj.is_empty(), "{t}'s CNPJ");
        if fund.months.is_empty() {
            println!("{t}: no month in the CVM's reports under {}", fund.cnpj);
            continue;
        }
        with_months += 1;
        assert!(fund.months.len() >= 6, "{t}: months of reports");
        assert!(r.vp_share.is_some_and(|v| v > 1.0 && v < 1000.0), "{t}: VP/cota {:?}", r.vp_share);
    }
    assert!(with_months >= 3, "the CVM's months for {with_months} of the 4 funds");
    // FundosNet is slow from abroad: at least half the funds' documents and dividends in one reading.
    let with_documents = tickers.iter().filter(|t| market.funds.get(*t).is_some_and(|f| !f.documents.is_empty())).count();
    let with_dividends = tickers.iter().filter(|t| market.funds.get(*t).is_some_and(|f| f.payouts.iter().any(|p| !p.amortization && p.value > 0.0 && p.value < 10.0))).count();
    println!("documents for {with_documents} funds, dividends for {with_dividends}");
    assert!(with_documents >= 2 && with_dividends >= 1, "FundosNet gave documents for {with_documents} and dividends for {with_dividends} of the 4 funds");
}

#[tokio::test]
#[ignore]
async fn live_company() {
    let http = Http::new();
    let today = today();
    let (c, register) = refresh::company(&http, "TIMS3", &BTreeMap::new(), today).await.expect("TIM's statements");
    section(&format!("{} ({}), {} periods; register of {} tickers; not read: {:?}", c.name, c.cnpj, c.figures.len(), register.as_ref().map_or(0, |r| r.len()), c.problems));
    for f in &c.figures {
        println!("{:?} → {}: revenue {:?}, ebit {:?}, net {:?}, dep {:?}, cash flow {:?}, capex {:?}, dividends {:?}, debt {:?}, cash {:?}, equity {:?}", f.start.map(|d| d.br()), f.end.br(), f.revenue, f.ebit, f.net_income, f.depreciation, f.operating_cash, f.capex, f.dividends_paid, f.debt, f.cash, f.equity);
    }
    let m = company::metrics(&c.figures);
    let thesis: Vec<_> = ["margem EBITDA acima de 40%", "dívida líquida/EBITDA abaixo de 2", "receita crescendo"].iter().filter_map(|s| company::parse_condition(s)).collect();
    let checks = company::check(&m, &thesis);
    println!("{}", company::for_model("TIMS3", &c.name, &m, &checks, &company::red_flags(&m)));
    // The share count from the archives' "composição do capital", and the ceilings it gives.
    println!("shares {:?} on {:?}; per share: {:?}", c.shares, c.shares_date.map(|d| d.br()), coucou_powers::ceiling::per_share(&c, Some(19.0)));
    let a = coucou_powers::ceiling::assess_stock("TIMS3", Some(&c), Some(19.0), &Default::default(), &Default::default());
    println!("{}", coucou_powers::ceiling::for_model(&a, 0.0));
    assert!(c.name.to_uppercase().contains("TIM"));
    let revenue = m.now.get("revenue").copied().expect("twelve months of revenue");
    assert!(revenue > 1e10 && revenue < 1e11, "TIM's yearly revenue {revenue}");
    let margin = m.now.get("ebitda_margin").copied().expect("EBITDA margin");
    assert!((20.0..70.0).contains(&margin), "EBITDA margin {margin}");
    assert_eq!(thesis.len(), 3);
}

/// The reader, as the iPhone runs it: a fund's latest manager's report and a company's documents listed by the CVM
/// (IPE), each downloaded and told apart (a PDF for the app's PDFKit, or its text), and a report's figures found.
#[tokio::test]
#[ignore]
async fn live_reader() {
    use coucou_powers::library::{self, File};
    let http = Http::new();
    let today = today();
    let months = fund_months(&http, today).await;
    let cnpj = cvm_fii::cnpj_by_root(&months).get("GGRC").expect("GGRC11's CNPJ").clone();
    let docs = sources::fund_documents(&http, &cnpj, 30).await.expect("GGRC11's documents");
    let report = docs.iter().find(|d| d.topic == fnet::Topic::Report && !d.structured).expect("a manager's report among the last 30");
    section(&format!("GGRC11's report {} ({} {}, {:?})", report.id, report.category, report.reference, report.delivered));
    let body = http.get(&fnet::download_url(report.id)).await.expect("the report's file");
    println!("{} bytes, starting {:?}", body.len(), String::from_utf8_lossy(&body[..body.len().min(40)]));
    match library::file_of(&body).expect("a file") {
        File::Pdf(pdf) => {
            println!("a PDF of {} bytes", pdf.len());
            assert!(pdf.starts_with(b"%PDF"));
        }
        File::Text(pages) => {
            println!("text, {} pages:\n{}", pages.len(), head(&pages.join("\n"), 30));
            println!("figures: {:?}", library::figures(&pages));
        }
    }
    section("TIM's documents at the CVM (IPE)");
    let tim = coucou_powers::text::cnpj_chars("02.421.421/0001-11");
    let listed = sources::company_documents(&http, &refresh::document_years(today), std::slice::from_ref(&tim), 15).await.expect("the IPE list");
    let tim_docs = listed.get(&tim).cloned().unwrap_or_default();
    for d in &tim_docs {
        println!("{} [{} / {} / {}] {} {:?} kind {:?} — {}", d.key, d.category, d.kind, d.subject, d.reference, d.delivered.map(|x| x.br()), d.doc_kind(), d.download);
    }
    assert!(!tim_docs.is_empty(), "TIM files documents with the CVM every month");
    if let Some(d) = tim_docs.iter().find(|d| d.doc_kind().is_some() && !d.download.is_empty()) {
        section(&format!("Its document {} downloaded", d.key));
        match http.get(&d.download).await {
            Ok(body) => {
                println!("{} bytes, starting {:?}", body.len(), String::from_utf8_lossy(&body[..body.len().min(40)]));
                match library::file_of(&body) {
                    Ok(File::Pdf(pdf)) => println!("a PDF of {} bytes", pdf.len()),
                    Ok(File::Text(pages)) => println!("text, {} pages:\n{}", pages.len(), head(&pages.join("\n"), 20)),
                    Err(e) => println!("not a document: {e}"),
                }
            }
            Err(e) => println!("download error: {e}"),
        }
    }
}

/// The facts the committee is given, from today's sources: each with its source, its reference date and how the code
/// computed it, for GARE11 (a made-up portfolio with a 7,5 % target for it) and GGRC11.
#[tokio::test]
#[ignore]
async fn live_committee_facts() {
    let http = Http::new();
    let today = today();
    let mut store = Store::default();
    engine::update(&mut store, &json!({"text": PASTE, "asset_targets": {"GARE11": 7.5}}), None, today).expect("the paste");
    let mut market = Market::default();
    let held = store.portfolio.tickers(None);
    let (day, quotes) = sources::quotes(&http, today, false, &held, None).await.expect("quotes");
    refresh::merge_quotes(&mut market, day, quotes);
    let funds: Vec<String> = ["GARE11", "GGRC11"].iter().map(|t| t.to_string()).collect();
    let read = refresh::funds(&http, &funds, &BTreeMap::new(), &given(), Some(&funds), &HashMap::new(), today).await;
    println!("fund errors: {:?}", read.errors);
    refresh::merge_funds(&mut market, read, today);
    market.economy = sources::economy(&http).await.unwrap_or_default();
    market.economy_read = Some(today);
    for t in &funds {
        let b = coucou_powers::committee::brief_full(&store, &market, None, &json!({"proposal": format!("comprar 50 cotas de {t}")}), today, true).expect("the brief");
        section(&format!("{t}: the committee's facts"));
        for f in &b.facts {
            println!("{}", f.line());
        }
        // P/VP names the day of the price and the month of the book value it divides.
        if let Some(pvp) = b.facts.iter().find(|f| f.text.starts_with(&format!("P/VP de {t}"))) {
            assert!(pvp.reference.starts_with("preço ") && pvp.reference.contains("; VP "), "{}", pvp.line());
        }
        // A payout keeps the places it was announced with, into the yield.
        if let Some(last) = b.facts.iter().find(|f| f.text.starts_with("Último rendimento:")) {
            let announced = market.funds[t.as_str()].payouts.iter().filter(|p| !p.amortization && p.value > 0.0).max_by_key(|p| p.record).map(|p| p.value).unwrap();
            assert_eq!(last.value, Some(announced), "{}", last.line());
        }
    }
    let b = coucou_powers::committee::brief_full(&store, &market, None, &json!({"proposal": "comprar 50 cotas de GARE11"}), today, true).unwrap();
    assert!(b.facts.iter().any(|f| f.text.starts_with("Sua meta para GARE11: 7,5%")), "the target from the portfolio");
    assert!(b.facts.iter().any(|f| f.text.contains("GARE11 vale") && f.text.contains("pesa")), "its weight in the portfolio");
}

/// Everything at once, the way the app does it, and what the brain reads back.
#[tokio::test]
#[ignore]
async fn live_zheus_end_to_end() {
    let http = Http::new();
    let today = today();
    let mut store = Store::default();
    let note = engine::update(&mut store, &json!({"text": PASTE, "targets": {"ações": 25, "fiis": 25, "etfs": 25, "renda fixa": 25}, "monthly_contribution": 500, "income_goal": 1000}), None, today).expect("the paste");
    println!("{note}");
    let mut market = Market::default();
    let due = refresh::due(&store, &market, today, false, true);
    println!("{due:?}");
    let (day, quotes) = sources::quotes(&http, today, false, &due.quotes, None).await.expect("quotes");
    refresh::merge_quotes(&mut market, day, quotes);
    let held = store.portfolio.tickers(Some(AssetClass::Fii));
    let read = refresh::funds(&http, &due.funds, &market.fund_cnpjs, &store.fund_cnpjs, due.fund_months.then_some(held.as_slice()), &refresh::known_documents(&market), today).await;
    println!("fund errors: {:?}", read.errors);
    refresh::merge_funds(&mut market, read, today);
    market.economy = sources::economy(&http).await.expect("economy");
    market.economy_read = Some(today);
    // A fund the CVM lists without the ISIN of its shares: Zheus asks for its CNPJ, the user gives it.
    let nameless = engine::without_cnpj(&store, &market);
    println!("funds without a CNPJ: {nameless:?}");
    if !nameless.is_empty() {
        section("wealth_fii before the CNPJ");
        println!("{}", engine::funds(&store, &market, Some(&nameless[0]), today));
        let given: serde_json::Map<String, serde_json::Value> = nameless.iter().filter(|t| t.starts_with("BTCI")).map(|t| (t.clone(), json!(BTCI_CNPJ))).collect();
        println!("{}", engine::update(&mut store, &json!({"fund_cnpjs": given}), None, today).expect("the CNPJ"));
        let due = refresh::due(&store, &market, today, false, false);
        println!("{due:?}");
        let read = refresh::funds(&http, &due.funds, &market.fund_cnpjs, &store.fund_cnpjs, due.fund_months.then_some(held.as_slice()), &refresh::known_documents(&market), today).await;
        println!("fund errors: {:?}", read.errors);
        refresh::merge_funds(&mut market, read, today);
        assert!(!refresh::due(&store, &market, today, false, false).fund_months, "read under it: not due again");
    }
    println!("classes fixed: {:?}", refresh::fix_classes(&mut store, &market));
    for t in store.portfolio.tickers(Some(AssetClass::Acao)) {
        match refresh::company(&http, &t, &market.company_cnpjs, today).await {
            Ok((c, register)) => {
                if let Some(r) = register {
                    market.company_cnpjs.extend(r);
                }
                market.companies.insert(t, c);
            }
            Err(e) => println!("{t}: {e}"),
        }
    }
    engine::snapshot(&mut store, &market, today);
    for (title, text) in [
        ("wealth_portfolio", engine::portfolio(&store, &market, today)),
        ("wealth_fii", engine::funds(&store, &market, None, today)),
        ("wealth_events", engine::events(&store, &market, 30, today)),
        ("wealth_plan", engine::plan(&store, &market, &json!({"contribution": 500, "scenario": {"selic": -2, "ipca": 1}}), today)),
        ("wealth_macro", engine::economy(&market, today)),
        ("wealth_company", engine::company_view(&mut store, &market, "TIMS3", &json!({"thesis": "margem EBITDA acima de 40%; dívida líquida/EBITDA abaixo de 2"}), today).0),
        ("wealth_committee", engine::committee(&store, &market, &json!({"proposal": "comprar 50 cotas de GARE11"}), today).unwrap()),
        ("wealth_audit", engine::audit_view(&store, &market, &json!({}), today)),
        ("life_day", coucou_powers::day::for_model(&engine::day_plan(&store, &market, today))),
    ] {
        section(title);
        println!("{text}");
    }
    section("wealth_ceiling");
    println!("{}", engine::ceiling_view(&mut store, &market, &json!({}), today).unwrap_or_else(|e| e));
    let summary = engine::summary(&store, &market, today);
    section("island summary");
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    assert!(summary.value > 3_000.0 && summary.value < 30_000.0, "the portfolio's value {}", summary.value);
    assert!(summary.income > 20.0 && summary.income < 400.0, "monthly income {}", summary.income);
    let (files, brief) = engine::content_kit(&store, &json!({}), today);
    section("content kit");
    println!("{brief}");
    for f in &files {
        println!("{} ({} bytes): {}", f.name, f.svg.len(), f.about);
    }
}
