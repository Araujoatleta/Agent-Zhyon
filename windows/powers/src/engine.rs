//! What each of Zheus' superpowers does when the brain calls it: the arguments read leniently (numbers as numbers or
//! as Brazilian text, classes in any wording), the user's data changed when asked, and the answer written for the
//! brain (in English; the brain tells it in the user's language). The app reads the official sources before (see
//! refresh.rs) and saves after; everything here is plain and synchronous.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::date::Date;
use crate::fii::{self, FundEvent, Health};
use crate::import::{self, Imported};
use crate::model::{AssetClass, Condition, Decision, EnglishProfile, ExerciseLog, GymProfile, Market, Position, SetLog, Store, StudyLog, Subject, Thesis, Wellness, Workout};
use crate::portfolio::{self, Scenario};
use crate::text::{brl, grouped, pct, ticker};
use crate::{audit, ceiling, company, content, day, english, gym, journal, srs, study, trades};

// ── Reading arguments ─────────────────────────────────────────────────────────

pub fn f(v: &Value, key: &str) -> Option<f64> {
    match &v[key] {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => crate::text::num(s),
        _ => None,
    }
}

pub fn s(v: &Value, key: &str) -> Option<String> {
    match &v[key] {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub fn b(v: &Value, key: &str) -> Option<bool> {
    match &v[key] {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match crate::text::plain(s).as_str() {
            "true" | "sim" | "on" | "yes" | "1" | "ligado" | "ligar" => Some(true),
            "false" | "nao" | "off" | "no" | "0" | "desligado" | "desligar" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

pub fn list<'a>(v: &'a Value, key: &str) -> Vec<&'a Value> {
    v[key].as_array().map(|a| a.iter().collect()).unwrap_or_default()
}

fn date_arg(v: &Value, key: &str) -> Option<Date> {
    s(v, key).and_then(|d| Date::parse(&d))
}

// ── The portfolio ─────────────────────────────────────────────────────────────

pub fn portfolio(store: &Store, market: &Market, today: Date) -> String {
    let p = &store.portfolio;
    if p.positions.is_empty() {
        return "The user's portfolio is empty in Coucou. Ask them to paste it (a table from Investidor10 or their broker, or lines like \"150 GGRC11 a 9,20\"), to drop B3's position export on the island, or to tell it; then save it with wealth_update.".into();
    }
    let v = portfolio::value(p, market);
    let incomes = portfolio::income(&v, market, today);
    let mut out = portfolio::for_model(&v, p, &incomes, &market.economy);
    let unpriced: Vec<&str> = v.holdings.iter().filter(|h| h.unpriced && h.class.quoted()).map(|h| h.ticker.as_str()).collect();
    if !unpriced.is_empty() {
        out.push_str(&format!("\nNo B3 quote yet for {}: valued at cost.", unpriced.join(", ")));
    }
    let nameless = without_cnpj(store, market);
    let unread: Vec<&str> = v
        .holdings
        .iter()
        .filter(|h| h.class == AssetClass::Fii && !nameless.contains(&h.ticker) && market.funds.get(&h.ticker).is_none_or(|f| !f.payouts.iter().any(|x| !x.amortization)))
        .map(|h| h.ticker.as_str())
        .collect();
    if !unread.is_empty() {
        out.push_str(&format!("\nDividends not read yet for {} (FundosNet has not answered yet): the passive income above leaves them out.", unread.join(", ")));
    }
    if !nameless.is_empty() {
        out.push_str(&format!("\n{}", ask_cnpj(&nameless)));
    }
    let guessed: Vec<&str> = p.positions.iter().filter(|x| x.note.contains("guessed")).map(|x| x.ticker.as_str()).collect();
    if !guessed.is_empty() {
        out.push_str(&format!("\nClass guessed (ends in 11): {} — ask the user whether each is a FII, an ETF or a unit.", guessed.join(", ")));
    }
    out
}

/// The funds held whose CNPJ no source gave and the user has not given either, once the CVM's reports were read: the
/// ISIN of a fund's shares is the only link from a ticker to its fund there, and some funds give none, or still their
/// old ticker's (BTCI11's fund reports FEXC11's).
pub fn without_cnpj(store: &Store, market: &Market) -> Vec<String> {
    if market.funds_read.is_none() {
        return Vec::new();
    }
    store.portfolio.tickers(Some(AssetClass::Fii)).into_iter().filter(|t| crate::refresh::cnpj_of(store, market, t).is_none()).collect()
}

/// A CNPJ the user gave, against the name B3 trades the ticker under: FundosNet's documents name the fund's shares
/// the way B3's quotes do ("FII BTG CRI"), so another fund's CNPJ shows.
fn given_check(store: &Store, market: &Market, t: &str) -> Option<String> {
    let squash = |s: &str| crate::text::plain(s).split_whitespace().collect::<Vec<_>>().join(" ");
    let given = store.fund_cnpjs.get(crate::text::ticker_root(t))?;
    let fund = market.funds.get(t).filter(|f| &f.cnpj == given)?;
    let b3 = market.quotes.get(t).map(|q| q.name.trim().to_string()).filter(|n| !n.is_empty())?;
    let theirs = fund.documents.iter().map(|d| d.trading.trim().to_string()).find(|n| !n.is_empty())?;
    let cnpj = crate::text::cnpj_format(given);
    Some(if fund.documents.iter().any(|d| squash(&d.trading) == squash(&b3)) {
        format!("The CNPJ the user gave ({cnpj}) is {t}'s: FundosNet names that fund's shares \"{theirs}\", as B3 does.")
    } else {
        format!("Check the CNPJ the user gave for {t} ({cnpj}): FundosNet names that fund's shares \"{theirs}\", while B3 trades {t} as \"{b3}\". It may be another fund's: ask the user.")
    })
}

fn ask_cnpj(tickers: &[String]) -> String {
    let example = tickers.first().map(String::as_str).unwrap_or("BTCI11");
    format!(
        "Coucou does not know the CNPJ of {}'s fund: the CVM's reports do not link the ticker to its fund (the ISIN they give for its shares is missing, or still an old ticker's). Ask the user for it, or look it up (the fund's monthly report on FundosNet shows it), and save it with wealth_update {{\"fund_cnpjs\": {{\"{example}\": \"00.000.000/0000-00\"}}}}; until then its reports and dividends are left out.",
        tickers.join(", ")
    )
}

fn position_from(v: &Value) -> Option<Position> {
    let t = s(v, "ticker").or_else(|| s(v, "name"))?;
    let class = s(v, "class").and_then(|c| AssetClass::parse(&c));
    let quoted = ticker(&t);
    let (class, guessed) = match (class, &quoted) {
        (Some(c), _) => (c, false),
        (None, Some(q)) => import::guess_class(q),
        (None, None) => (AssetClass::RendaFixa, false),
    };
    let quantity = f(v, "quantity").or_else(|| f(v, "qty")).unwrap_or(if class.quoted() { 0.0 } else { 1.0 });
    let average = f(v, "average_price").or_else(|| f(v, "price")).or_else(|| f(v, "invested").map(|i| i / quantity.max(1.0))).unwrap_or(0.0);
    Some(Position {
        ticker: quoted.unwrap_or(t),
        class,
        quantity,
        average_price: average,
        subtype: s(v, "subtype").unwrap_or_default(),
        current_value: f(v, "current_value"),
        note: if guessed { "guessed".into() } else { String::new() },
    })
}

/// How positions coming in meet the list: merged by ticker, replacing the whole list (a table of the whole
/// portfolio), or replacing only the classes they hold (B3's position export: a bank's CDB it does not know stays).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fold {
    Merge,
    Replace,
    Classes,
}

fn merge_positions(store: &mut Store, incoming: Vec<Position>, fold: Fold) -> (usize, usize, usize) {
    let (mut added, mut changed, mut removed) = (0, 0, 0);
    let old = store.portfolio.positions.clone();
    let gone = |o: &Position| !incoming.iter().any(|n| n.ticker == o.ticker);
    match fold {
        Fold::Replace => {
            removed = old.iter().filter(|o| gone(o)).count();
            store.portfolio.positions.clear();
        }
        Fold::Classes => {
            let classes: Vec<AssetClass> = incoming.iter().map(|n| n.class).collect();
            removed = old.iter().filter(|o| classes.contains(&o.class) && gone(o)).count();
            store.portfolio.positions.retain(|o| !(classes.contains(&o.class) && gone(o)));
        }
        Fold::Merge => {}
    }
    for mut p in incoming {
        let before = old.iter().find(|o| o.ticker == p.ticker);
        // An export without average prices (B3's position) keeps the ones known.
        if p.average_price <= 0.0 {
            if let Some(b) = before {
                p.average_price = b.average_price;
            }
        }
        if let Some(b) = before {
            if p.subtype.is_empty() {
                p.subtype = b.subtype.clone();
            }
            if b.note.is_empty() && p.note == "guessed" {
                p.class = b.class;
                p.note = String::new();
            }
        }
        match store.portfolio.positions.iter_mut().find(|x| x.ticker == p.ticker) {
            Some(existing) => {
                if p.quantity <= 0.0 && p.class.quoted() {
                    removed += 1;
                    store.portfolio.positions.retain(|x| x.ticker != p.ticker);
                } else {
                    *existing = p;
                    changed += 1;
                }
            }
            None if p.quantity > 0.0 || !p.class.quoted() => {
                if before.is_some() {
                    changed += 1;
                } else {
                    added += 1;
                }
                store.portfolio.positions.push(p);
            }
            None => {}
        }
    }
    (added, changed, removed)
}

/// wealth_update: positions (merge by default), a pasted text or a file already read (replace by default), targets,
/// the income goal, the monthly contribution, the cash, tickers to remove.
pub fn update(store: &mut Store, args: &Value, file: Option<Imported>, today: Date) -> Result<String, String> {
    let mut notes = Vec::new();
    let mut touched = false;
    let mode = s(args, "mode").map(|m| crate::text::plain(&m));
    let mut incoming: Vec<Position> = list(args, "positions").into_iter().filter_map(position_from).collect();
    let mut fold = match mode.as_deref() {
        Some("replace") => Fold::Replace,
        Some("classes") => Fold::Classes,
        _ => Fold::Merge,
    };
    let from_file = file.as_ref().is_some_and(|f| !f.positions.is_empty());
    // A file of trades alone (a spreadsheet of contributions, B3's 'Negociação'): no position list in it.
    let trades_only = file.as_ref().is_some_and(|f| f.positions.is_empty() && !f.trades.is_empty());
    let mut imported = file.unwrap_or_default();
    if let Some(text) = s(args, "text") {
        let typed = import::positions_from_text(&text);
        if typed.positions.is_empty() {
            return Err("No position could be read from the text: each line needs a ticker and a quantity (and the average price).".into());
        }
        imported.positions.extend(typed.positions);
        imported.guessed.extend(typed.guessed);
        imported.warnings.extend(typed.warnings);
    }
    if !imported.positions.is_empty() {
        // A whole table replaces the list; B3's export, the classes it holds. Unless the user said otherwise.
        if mode.is_none() {
            fold = if from_file { Fold::Classes } else { Fold::Replace };
        }
        for mut p in imported.positions.drain(..) {
            if imported.guessed.contains(&p.ticker) {
                p.note = "guessed".into();
            }
            incoming.push(p);
        }
    }
    if !incoming.is_empty() {
        let (a, c, r) = merge_positions(store, incoming, fold);
        let how = match fold {
            Fold::Replace => " (the list replaced the old one)",
            Fold::Classes => " (the file's classes replaced their old positions; the others stay)",
            Fold::Merge => "",
        };
        notes.push(format!("Positions: {a} added, {c} updated, {r} removed{how}."));
        touched = true;
    }
    if !imported.trades.is_empty() || !imported.events.is_empty() {
        let (t0, e0) = (store.trades.len(), store.events.len());
        for t in imported.trades {
            if !store.trades.iter().any(|x| x.date == t.date && x.ticker == t.ticker && x.kind == t.kind && (x.quantity - t.quantity).abs() < 1e-9 && (x.price - t.price).abs() < 0.005) {
                store.trades.push(t);
            }
        }
        for e in imported.events {
            if !store.events.iter().any(|x| x.date == e.date && x.ticker == e.ticker && x.kind == e.kind && (x.value - e.value).abs() < 0.005) {
                store.events.push(e);
            }
        }
        store.trades.sort_by_key(|t| t.date);
        store.events.sort_by_key(|e| e.date);
        notes.push(format!("History: {} new trades, {} new payouts and events.", store.trades.len() - t0, store.events.len() - e0));
        touched = true;
    }
    // The positions worked out from the trades: when a file of trades alone comes to an empty portfolio, or when the
    // user asks for it (fixed income and the rest B3 does not trade stay).
    let rebuild = b(args, "rebuild_from_trades") == Some(true);
    if rebuild || (trades_only && store.portfolio.positions.is_empty()) {
        let built = trades::positions_from(&store.trades);
        if built.is_empty() {
            notes.push("No position comes out of the trades recorded (everything bought was sold, or there is no trade).".into());
        } else {
            store.portfolio.positions.retain(|p| !p.class.quoted());
            notes.push(format!("Positions worked out from {} trades: {} assets, each at its average price with its costs.", store.trades.len(), built.len()));
            store.portfolio.positions.extend(built);
            touched = true;
        }
    } else if trades_only {
        notes.push("The file had trades only, so the positions were kept: wealth_audit compares them with the trades, and wealth_update with rebuild_from_trades: true works the positions out from the trades instead.".into());
    }
    for w in imported.warnings {
        notes.push(format!("Not read: {w}"));
    }
    for t in list(args, "remove").into_iter().filter_map(|v| v.as_str()) {
        let before = store.portfolio.positions.len();
        store.portfolio.positions.retain(|p| !p.ticker.eq_ignore_ascii_case(t.trim()));
        if store.portfolio.positions.len() < before {
            notes.push(format!("Removed {t}."));
            touched = true;
        }
    }
    if let Some(obj) = args["targets"].as_object() {
        store.portfolio.targets.clear();
        for (k, v) in obj {
            let Some(class) = AssetClass::parse(k) else {
                notes.push(format!("Target \"{k}\" is not a class (ações, FIIs, ETFs, BDRs, renda fixa, Tesouro, cripto)."));
                continue;
            };
            if let Some(x) = v.as_f64().or_else(|| v.as_str().and_then(crate::text::num)).filter(|x| *x > 0.0) {
                store.portfolio.targets.insert(class, x);
            }
        }
        let sum: f64 = store.portfolio.targets.values().sum();
        notes.push(format!("Targets set ({} in all{}).", pct(sum, 0), if (sum - 100.0).abs() > 0.5 { "; read as proportions" } else { "" }));
        touched = true;
    }
    // A fund's CNPJ, for the funds the CVM lists without the ISIN of their shares; an empty one forgets it.
    let mut cnpjs = false;
    for (k, v) in args["fund_cnpjs"].as_object().into_iter().flatten() {
        let Some(t) = ticker(k) else {
            notes.push(format!("\"{k}\" is not a ticker (like BTCI11)."));
            continue;
        };
        let root = crate::text::ticker_root(&t).to_string();
        match v.as_str().map(str::trim).filter(|c| !c.is_empty()) {
            None => {
                if store.fund_cnpjs.remove(&root).is_some() {
                    notes.push(format!("{t}: the CNPJ given before is forgotten; Coucou goes back to what the CVM says."));
                    cnpjs = true;
                }
            }
            Some(typed) => match crate::text::cnpj(typed) {
                Ok(c) => {
                    notes.push(format!("{t}: its fund's CNPJ is {}; Coucou reads the fund under it.", crate::text::cnpj_format(&c)));
                    store.fund_cnpjs.insert(root, c);
                    cnpjs = true;
                }
                Err(e) => notes.push(format!("{t}: {e}")),
            },
        }
    }
    for (key, label) in [("income_goal", "Passive income goal"), ("monthly_contribution", "Monthly contribution"), ("cash", "Cash at the broker")] {
        if let Some(x) = f(args, key) {
            match key {
                "income_goal" => store.portfolio.income_goal = Some(x).filter(|x| *x > 0.0),
                "monthly_contribution" => store.portfolio.monthly_contribution = Some(x).filter(|x| *x > 0.0),
                _ => store.portfolio.cash = x.max(0.0),
            }
            notes.push(format!("{label}: {}.", brl(x)));
            touched = true;
        }
    }
    if !touched && !cnpjs {
        return Err(if notes.is_empty() { "Nothing to change: give positions, a text, a file, targets, income_goal, monthly_contribution, cash, remove or fund_cnpjs.".into() } else { notes.join(" ") });
    }
    if touched {
        store.portfolio.updated = Some(today);
        store.portfolio.source = s(args, "source").unwrap_or_else(|| if s(args, "text").is_some() { "text".into() } else { "chat".into() });
    }
    Ok(notes.join(" "))
}

// ── Funds, events, companies ─────────────────────────────────────────────────

pub fn funds(store: &Store, market: &Market, only: Option<&str>, today: Date) -> String {
    let wanted: Vec<String> = match only.and_then(ticker) {
        Some(t) => vec![t],
        None => store.portfolio.tickers(Some(AssetClass::Fii)),
    };
    if wanted.is_empty() {
        return "The user holds no real-estate fund (FII) in Coucou.".into();
    }
    let nameless = without_cnpj(store, market);
    let mut out = Vec::new();
    for t in wanted {
        match market.funds.get(&t) {
            Some(fund) => {
                let mut text = fii::report_for_model(&fii::report(fund, market.price(&t), &market.economy, today));
                if let Some(check) = given_check(store, market, &t) {
                    text.push_str(&format!("\n{check}"));
                }
                out.push(text);
            }
            None if nameless.contains(&t) => out.push(ask_cnpj(std::slice::from_ref(&t))),
            None => out.push(format!("{t}: not read yet from the CVM and FundosNet (they may be unreachable now, or the fund is not a FII).")),
        }
    }
    out.join("\n\n")
}

pub fn fund_events(store: &Store, market: &Market, days: i64, today: Date) -> Vec<FundEvent> {
    let mut all = Vec::new();
    for p in store.portfolio.positions.iter().filter(|p| p.class == AssetClass::Fii) {
        if let Some(f) = market.funds.get(&p.ticker) {
            all.extend(fii::events(f, p.quantity, today, days));
        }
    }
    all.sort_by(|a, b| a.importance.cmp(&b.importance).then(a.date.cmp(&b.date)));
    all
}

pub fn events(store: &Store, market: &Market, days: i64, today: Date) -> String {
    if store.portfolio.tickers(Some(AssetClass::Fii)).is_empty() {
        return "The user holds no real-estate fund: no fund event to watch.".into();
    }
    fii::events_for_model(&fund_events(store, market, days, today))
}

/// wealth_company: the company's figures against the user's thesis (set or replaced by `conditions` / `thesis`).
pub fn company_view(store: &mut Store, market: &Market, t: &str, args: &Value, today: Date) -> (String, bool) {
    let mut changed = false;
    let mut conditions: Vec<Condition> = list(args, "conditions")
        .into_iter()
        .filter_map(|c| {
            Some(Condition {
                metric: s(c, "metric")?,
                op: s(c, "op").unwrap_or_else(|| ">".into()),
                value: f(c, "value").unwrap_or(0.0),
                label: s(c, "label").unwrap_or_default(),
            })
        })
        .collect();
    let mut not_read = Vec::new();
    if let Some(text) = s(args, "thesis") {
        for sentence in text.split(['\n', ';', '.']).map(str::trim).filter(|x| !x.is_empty()) {
            match company::parse_condition(sentence) {
                Some(c) => conditions.push(c),
                None => not_read.push(sentence.to_string()),
            }
        }
    }
    if !conditions.is_empty() {
        store.theses.retain(|x| x.ticker != t);
        store.theses.push(Thesis { ticker: t.to_string(), conditions: conditions.clone(), written: today, text: s(args, "thesis").unwrap_or_default() });
        changed = true;
    }
    let thesis: Vec<Condition> = store.theses.iter().find(|x| x.ticker == t).map(|x| x.conditions.clone()).unwrap_or_default();
    let Some(c) = market.companies.get(t) else {
        return (format!("{t}: its statements could not be read from the CVM yet.{}", if changed { " The thesis was saved and will be checked when they are." } else { "" }), changed);
    };
    let m = company::metrics(&c.figures);
    let checks = company::check(&m, &thesis);
    let flags = company::red_flags(&m);
    let mut out = company::for_model(t, &c.name, &m, &checks, &flags);
    if !c.problems.is_empty() {
        out.push_str(&format!("\nNot read from the CVM this time: {}.", c.problems.join("; ")));
    }
    if let Some(price) = market.price(t) {
        out.push_str(&format!("\nLast close {}.", brl(price)));
    }
    if !not_read.is_empty() {
        out.push_str(&format!(
            "\nNot measurable as written: {}. Measurable metrics: {}.",
            not_read.join("; "),
            company::METRICS.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", ")
        ));
    }
    (out, changed)
}

pub fn economy(market: &Market, today: Date) -> String {
    let e = &market.economy;
    if e.selic.is_none() && e.selic_expected.is_empty() {
        return "The Banco Central's figures could not be read yet.".into();
    }
    let mut out = String::from("Economy (Banco Central):");
    if let Some(x) = e.selic {
        out.push_str(&format!(" Selic target {}", pct(x, 2)));
    }
    if let Some(x) = e.cdi {
        out.push_str(&format!(", CDI {}", pct(x, 2)));
    }
    if let Some(x) = e.ipca_12m {
        out.push_str(&format!(", IPCA 12 months {}", pct(x, 2)));
    }
    if let Some(x) = e.usd {
        out.push_str(&format!(", dollar {}", brl(x)));
    }
    if let Some(d) = e.date {
        out.push_str(&format!(" ({})", d.br()));
    }
    out.push('.');
    let years: Vec<String> = [today.y, today.y + 1]
        .iter()
        .filter_map(|y| match (e.selic_at(*y), e.ipca_at(*y)) {
            (None, None) => None,
            (s, i) => Some(format!("end of {y}: Selic {}, IPCA {}", s.map(|v| pct(v, 2)).unwrap_or_else(|| "—".into()), i.map(|v| pct(v, 2)).unwrap_or_else(|| "—".into()))),
        })
        .collect();
    if !years.is_empty() {
        out.push_str(&format!(" Focus survey medians — {}.", years.join("; ")));
    }
    if let Some(r) = e.real_rate(today.y + 1) {
        out.push_str(&format!(" Real rate about {}.", pct(r, 1)));
    }
    out
}

// ── The user's trades, and ceiling prices ─────────────────────────────────────

/// wealth_trade: record the trades the user tells (each moves its position and its average price), list them month
/// by month, or undo the last one told.
pub fn trade_view(store: &mut Store, market: &Market, args: &Value, today: Date) -> Result<String, String> {
    if b(args, "undo_last") == Some(true) {
        return trades::undo_last(store, today);
    }
    let told: Vec<trades::Told> = list(args, "trades")
        .into_iter()
        .filter_map(|t| {
            let ticker = s(t, "ticker").and_then(|x| ticker(&x))?;
            let side = s(t, "side").or_else(|| s(t, "kind")).map(|k| crate::text::plain(&k)).unwrap_or_default();
            let kind = if side.starts_with("sell") || side.starts_with("vend") || side == "v" { crate::model::TradeKind::Sell } else { crate::model::TradeKind::Buy };
            Some(trades::Told {
                ticker,
                kind,
                quantity: f(t, "quantity").or_else(|| f(t, "qty")).unwrap_or(0.0),
                price: f(t, "price"),
                fees: f(t, "fees").unwrap_or(0.0),
                date: date_arg(t, "date"),
                time: s(t, "time").map(|x| trades::time_of(&x)).unwrap_or_default(),
                class: s(t, "class").and_then(|c| AssetClass::parse(&c)),
            })
        })
        .collect();
    if told.is_empty() {
        if b(args, "list") == Some(true) || list(args, "trades").is_empty() {
            let months = f(args, "months").map(|m| m.clamp(1.0, 36.0) as usize).unwrap_or(6);
            return Ok(trades::history(store, months, today));
        }
        return Err("No trade could be read: each one needs a B3 ticker, buy or sell, and the quantity.".into());
    }
    Ok(trades::record(store, market, &told, today)?.join("\n"))
}

/// wealth_ceiling: the ceiling prices of one asset or of all the watched ones, the opportunities among them, and the
/// user's choices (the yields, the P/VP, the margin, the methods, their own ceilings).
pub fn ceiling_view(store: &mut Store, market: &Market, args: &Value, today: Date) -> Result<String, String> {
    let mut notes = Vec::new();
    let c = &mut store.ceilings;
    if let Some(y) = f(args, "stock_yield").filter(|y| *y > 0.0 && *y < 50.0) {
        c.stock_yield = y;
        notes.push(format!("Shares' Bazin yield: {}.", pct(y, 1)));
    }
    let fund_yield = f(args, "fund_yield");
    if fund_yield == Some(0.0) || args["fund_yield"].as_str().is_some_and(|x| x.trim().is_empty()) {
        c.fund_yield = None;
        notes.push("Funds' yield: back to the CDI after the 15 % income tax.".into());
    } else if let Some(y) = fund_yield.filter(|y| *y > 0.0 && *y < 50.0) {
        c.fund_yield = Some(y);
        notes.push(format!("Funds' yield: {}.", pct(y, 1)));
    }
    if let Some(x) = f(args, "max_pvp").filter(|x| *x > 0.0 && *x < 5.0) {
        c.max_pvp = x;
        notes.push(format!("Funds' most P/VP: {}.", grouped(x, 2)));
    }
    if let Some(m) = f(args, "margin").filter(|m| (0.0..50.0).contains(m)) {
        c.margin = m;
        notes.push(format!("An opportunity from {} below a ceiling.", pct(m, 1)));
    }
    if args["methods"].is_array() {
        c.methods = list(args, "methods").into_iter().filter_map(|m| m.as_str()).map(|m| crate::text::plain(m).replace(' ', "_")).filter(|m| ceiling::METHODS.contains(&m.as_str())).collect();
        notes.push(format!("Methods watched: {}.", if c.methods.is_empty() { "the usual ones".to_string() } else { c.methods.iter().map(|m| ceiling::label(m)).collect::<Vec<_>>().join(", ") }));
    }
    let asked = s(args, "ticker").and_then(|t| ticker(&t));
    if let (Some(t), Some(v)) = (&asked, f(args, "ceiling")) {
        if v > 0.0 {
            c.manual.insert(t.clone(), v);
            notes.push(format!("{t}: the user's own ceiling is {} (watched from now on).", brl(v)));
        } else {
            c.manual.remove(t);
            notes.push(format!("{t}: the user's own ceiling is gone."));
        }
    }
    let given = ceiling::Given { eps: f(args, "eps"), bvps: f(args, "bvps"), dps: f(args, "dps"), growth: f(args, "growth") };
    let tickers = match &asked {
        Some(t) => vec![t.clone()],
        None => ceiling::watched_tickers(store),
    };
    if tickers.is_empty() {
        return Err("The user holds no share or fund in Coucou to price: save the portfolio first (wealth_update), or name a ticker.".into());
    }
    let margin = store.ceilings.margin;
    let mut out: Vec<String> = notes;
    for t in &tickers {
        let g = if asked.is_some() { given.clone() } else { ceiling::Given::default() };
        out.push(ceiling::for_model(&ceiling::assess(store, market, t, &g, today), margin));
    }
    let found = ceiling::opportunities(store, market, today);
    if found.is_empty() {
        out.push("No watched asset is below its ceiling now.".into());
    } else {
        out.push(format!(
            "Opportunities now (the close below a watched ceiling): {}.",
            found.iter().map(|o| format!("{} {} below its {} ceiling ({})", o.ticker, pct(o.below, 1), ceiling::label(o.method), brl(o.ceiling))).collect::<Vec<_>>().join("; ")
        ));
    }
    let (y, why) = ceiling::fund_yield(&store.ceilings, &market.economy);
    out.push(format!(
        "The assumptions: shares' Bazin at {}, funds' at {} ({why}), funds' P/VP up to {}. These are rules of thumb on the user's own choices, not a recommendation: tell them which method and figures each ceiling comes from.",
        pct(store.ceilings.stock_yield, 1),
        pct(y, 1),
        grouped(store.ceilings.max_pvp, 2)
    ));
    Ok(out.join("\n\n"))
}

// ── The plan (digital twin), the audit, decisions and the committee ──────────

pub fn plan(store: &Store, market: &Market, args: &Value, today: Date) -> String {
    let p = &store.portfolio;
    if p.positions.is_empty() {
        return portfolio(store, market, today);
    }
    let v = portfolio::value(p, market);
    let mut out = String::new();
    match portfolio::road(&v, p) {
        Some(r) => {
            out.push_str(&format!(
                "Road to the targets without selling: {}.",
                if r.to_add < 0.5 { "already on target".to_string() } else { format!("add about {}{}", brl(r.to_add), r.months.map(|m| format!(" (~{} months at {} a month)", grouped(m.ceil(), 0), brl(p.monthly_contribution.unwrap_or(0.0)))).unwrap_or_default()) }
            ));
            if !r.untargeted.is_empty() {
                out.push_str(&format!(" Held with no target: {}.", r.untargeted.iter().map(|c| c.label()).collect::<Vec<_>>().join(", ")));
            }
        }
        None => out.push_str("No targets are set: ask the user how they want to split the portfolio (e.g. 25 % each of shares, FIIs, ETFs and fixed income) and save it with wealth_update (targets)."),
    }
    let amount = f(args, "contribution").or_else(|| f(args, "aporte")).or(p.monthly_contribution);
    if let Some(a) = amount.filter(|a| *a > 0.0) {
        let slices = portfolio::allocate(&v, p, a);
        if !slices.is_empty() {
            out.push_str(&format!("\nWhere {} goes (math on the user's targets, not a recommendation):", brl(a)));
            for sl in slices {
                let options: Vec<String> = sl
                    .options
                    .iter()
                    .take(3)
                    .map(|(t, w, shares)| format!("{t} ({} of the portfolio{})", pct(*w, 1), shares.map(|n| format!(", {} shares", grouped(n, 0))).unwrap_or_default()))
                    .collect();
                out.push_str(&format!(
                    "\n- {}: {}{}",
                    sl.class.label(),
                    brl(sl.amount),
                    if options.is_empty() { " — the user holds nothing of this class yet".to_string() } else { format!(" — their holdings, least weighted first: {}", options.join(", ")) }
                ));
            }
        }
    }
    let incomes = portfolio::income(&v, market, today);
    let monthly: f64 = incomes.iter().map(|i| i.monthly).sum();
    if monthly > 0.0 {
        out.push_str(&format!("\nPassive income {} a month. Magic numbers (shares whose dividend buys one more):", brl(monthly)));
        for i in &incomes {
            if let Some(m) = i.magic_number {
                let held = v.holdings.iter().find(|h| h.ticker == i.ticker).map(|h| h.quantity).unwrap_or(0.0);
                out.push_str(&format!(" {} {} (has {});", i.ticker, grouped(m, 0), grouped(held, 0)));
            }
        }
    }
    let sc = &args["scenario"];
    if sc.is_object() {
        let scenario = Scenario { selic: f(sc, "selic").unwrap_or(0.0), ipca: f(sc, "ipca").unwrap_or(0.0), dollar: f(sc, "dollar").unwrap_or(0.0), stocks: f(sc, "stocks").unwrap_or(0.0) };
        let lines = portfolio::scenario(&v, &market.economy, &scenario);
        let (now, after, income): (f64, f64, f64) = lines.iter().fold((0.0, 0.0, 0.0), |acc, l| (acc.0 + l.now, acc.1 + l.after, acc.2 + l.income_change));
        out.push_str(&format!(
            "\nScenario (Selic {:+} pt, IPCA {:+} pt, dollar {:+} %, stocks {:+} %), rough rules of thumb over 12 months, not a forecast:",
            scenario.selic, scenario.ipca, scenario.dollar, scenario.stocks
        ));
        for l in &lines {
            out.push_str(&format!("\n- {}: {} → {}{} ({})", l.label, brl(l.now), brl(l.after), if l.income_change.abs() > 0.5 { format!(", income {:+.0} R$/year", l.income_change) } else { String::new() }, l.why));
        }
        out.push_str(&format!("\nTotal {} → {} ({}), income change {:+.0} R$/year.", brl(now), brl(after), pct((after / now.max(1e-9) - 1.0) * 100.0, 1), income));
    }
    if let Some(sh) = portfolio::shadow(&store.snapshots, p, market.economy.cdi) {
        out.push_str(&format!(
            "\nShadow portfolio since {} (same money always on the targets): real {} vs shadow {} — {}.",
            sh.since.br(),
            pct(sh.real, 2),
            pct(sh.shadow, 2),
            if sh.difference > 0.0 { format!("following the targets would have earned {} points more", grouped(sh.difference, 2)) } else { format!("straying from them earned {} points more", grouped(-sh.difference, 2)) }
        ));
    }
    out
}

pub fn audit_view(store: &Store, market: &Market, args: &Value, today: Date) -> String {
    let positions = &store.portfolio.positions;
    if s(args, "report").map(|r| crate::text::plain(&r)).as_deref().is_some_and(|r| r.starts_with("decl") || r.starts_with("irpf")) {
        let year = f(args, "year").map(|y| y as i32).unwrap_or(today.y - 1);
        if store.trades.is_empty() {
            return "The declaration needs the history: ask the user to drop B3's 'Negociação' and 'Movimentação' exports (Área do Investidor → Extratos, every year since they started) on the island.".into();
        }
        return audit::declaration_for_model(&audit::declaration(year, &store.trades, &store.events, positions, market));
    }
    let l = audit::ledger(&store.trades, &store.events, positions);
    let months = audit::months(&l.sales);
    let missing = audit::missing_payouts(&l, &store.events, market, today);
    let diffs = audit::reconcile(&l, positions);
    audit::for_model(&l, &months, &missing, &diffs, today)
}

/// The DARF of this month's due date (for last month's gains), if any.
pub fn darf_due(store: &Store, today: Date) -> Option<f64> {
    if store.trades.is_empty() {
        return None;
    }
    let l = audit::ledger(&store.trades, &store.events, &store.portfolio.positions);
    let last = today.first_of_month().add_days(-1).month_key();
    audit::months(&l.sales).into_iter().find(|m| m.month == last).map(|m| m.darf).filter(|d| *d > 0.0)
}

pub fn decision(store: &mut Store, market: &Market, args: &Value, today: Date) -> Result<String, String> {
    let action = s(args, "action").map(|a| crate::text::plain(&a)).unwrap_or_else(|| "list".into());
    match action.as_str() {
        "record" | "registrar" | "add" => {
            let t = s(args, "ticker").and_then(|t| ticker(&t)).ok_or("record needs the ticker")?;
            let what = s(args, "decision").map(|d| crate::text::plain(&d)).unwrap_or_else(|| "buy".into());
            let what = match what.as_str() {
                w if w.starts_with("compr") || w == "buy" => "buy",
                w if w.starts_with("vend") || w == "sell" => "sell",
                w if w.starts_with("mant") || w == "hold" => "hold",
                _ => "avoid",
            };
            let reasons = s(args, "reasons").ok_or("record needs the user's reasons, in their words")?;
            let n = store.decisions.iter().filter(|d| d.date == today && d.ticker == t).count();
            let d = Decision {
                id: journal::new_id(today, &t, n),
                date: today,
                ticker: t.clone(),
                action: what.into(),
                quantity: f(args, "quantity"),
                price: f(args, "price").or_else(|| market.price(&t)),
                reasons,
                expectation: s(args, "expectation").unwrap_or_default(),
                confidence: f(args, "confidence").map(|c| c.clamp(0.0, 100.0) as u8),
                committee: s(args, "committee").unwrap_or_default(),
                reviews: Vec::new(),
                status: if b(args, "proposed").unwrap_or(false) { "proposed".into() } else { "taken".into() },
            };
            let id = d.id.clone();
            let next = journal::next_review(&d).map(|x| x.br()).unwrap_or_default();
            store.decisions.push(d);
            Ok(format!("Decision {id} recorded ({what} {t}). It comes back for a review on {next}, then at six and twelve months."))
        }
        "review" | "revisar" => {
            let id = s(args, "id").ok_or("review needs the decision's id")?;
            let price_now = s(args, "ticker").and_then(|t| market.price(&t));
            let d = store.decisions.iter_mut().find(|d| d.id == id).ok_or_else(|| format!("no decision {id} in the journal"))?;
            let price = f(args, "price").or(price_now).or_else(|| market.price(&d.ticker));
            journal::review(d, today, &s(args, "verdict").unwrap_or_else(|| "mixed".into()), &s(args, "note").unwrap_or_default(), price);
            Ok(format!("Review of {id} saved ({}).", d.reviews.last().map(|r| r.verdict.clone()).unwrap_or_default()))
        }
        "take" | "drop" => {
            let id = s(args, "id").ok_or("needs the decision's id")?;
            let d = store.decisions.iter_mut().find(|d| d.id == id).ok_or_else(|| format!("no decision {id} in the journal"))?;
            d.status = if action == "take" { "taken".into() } else { "dropped".into() };
            Ok(format!("{id}: {}.", d.status))
        }
        _ => {
            let cal = journal::calibration(&store.decisions, |t| market.price(t), market.economy.cdi, today);
            Ok(journal::for_model(&store.decisions, today, &cal))
        }
    }
}

/// wealth_committee: the committee's brief for the chat (committee.rs), without the documents (the apps add the reader's
/// when they keep one).
pub fn committee(store: &Store, market: &Market, args: &Value, today: Date) -> Result<String, String> {
    crate::committee::brief(store, market, None, args, today, false).map(|(text, _, _)| text)
}

// ── Studies, cards, English, training, the day, content ──────────────────────

pub fn study_view(store: &mut Store, market: &Market, args: &Value, today: Date) -> Result<String, String> {
    let action = s(args, "action").map(|a| crate::text::plain(&a)).unwrap_or_else(|| "show".into());
    let mut note = String::new();
    if action == "set" {
        let p = &mut store.study;
        if let Some(x) = s(args, "exam") {
            p.exam = x;
        }
        if let Some(x) = s(args, "board") {
            p.board = x;
        }
        if let Some(x) = date_arg(args, "date") {
            p.date = Some(x);
        }
        if let Some(x) = f(args, "salary") {
            p.salary = Some(x);
        }
        if let Some(x) = f(args, "current_income") {
            p.current_income = Some(x);
        }
        if let Some(x) = f(args, "hours_per_week") {
            p.hours_per_week = x.clamp(1.0, 80.0);
        }
        if let Some(x) = f(args, "chance") {
            p.chance = Some(x.clamp(0.1, 100.0));
        }
        let subjects: Vec<Subject> = list(args, "subjects")
            .into_iter()
            .filter_map(|v| {
                Some(Subject {
                    name: s(v, "name")?,
                    weight: f(v, "weight").unwrap_or(1.0),
                    level: f(v, "level").map(|l| l.clamp(0.0, 5.0) as u8).unwrap_or(2),
                    topics: list(v, "topics").into_iter().filter_map(|t| t.as_str().map(String::from)).collect(),
                })
            })
            .collect();
        if !subjects.is_empty() {
            p.subjects = subjects;
        }
        note = "Study profile saved.\n".into();
    } else if action == "log" {
        let subject = s(args, "subject").ok_or("log needs the subject")?;
        let minutes = f(args, "minutes").map(|m| m.max(0.0) as u32).unwrap_or(0);
        let questions = f(args, "questions").map(|q| q.max(0.0) as u32).unwrap_or(0);
        let correct = f(args, "correct").map(|q| q.max(0.0) as u32).unwrap_or(0).min(questions);
        store.study_logs.push(StudyLog { date: date_arg(args, "date").unwrap_or(today), subject: subject.clone(), minutes, questions, correct });
        note = format!("Logged: {subject}, {minutes} min{}.\n", if questions > 0 { format!(", {correct}/{questions} questions") } else { String::new() });
    }
    let plan = study::plan(&store.study, &store.study_logs, today);
    let progress = study::progress(&store.study, &store.study_logs, today);
    let value = study::value(&store.study, today);
    let v = portfolio::value(&store.portfolio, market);
    let mut targets = vec![100_000.0, 1_000_000.0];
    if let Some(goal) = store.portfolio.income_goal {
        let incomes = portfolio::income(&v, market, today);
        if let Some((capital, _)) = portfolio::income_goal(&incomes, &v, goal) {
            targets.push(capital.round());
        }
    }
    let proj = if v.value > 0.0 { study::projection(v.value, store.portfolio.monthly_contribution.unwrap_or(0.0), value.as_ref().map(|x| x.monthly_gain), &targets) } else { Vec::new() };
    Ok(format!("{note}{}", study::for_model(&store.study, &plan, &progress, value.as_ref(), &proj, today)))
}

pub fn cards(store: &mut Store, args: &Value, today: Date) -> Result<String, String> {
    let action = s(args, "action").map(|a| crate::text::plain(&a)).unwrap_or_else(|| "due".into());
    let deck = s(args, "deck").unwrap_or_default();
    match action.as_str() {
        "add" => {
            let mut added = 0;
            for c in list(args, "cards") {
                let (Some(front), Some(back)) = (s(c, "front"), s(c, "back")) else { continue };
                if store.cards.iter().any(|x| x.front.eq_ignore_ascii_case(&front) && x.deck == deck.to_lowercase()) {
                    continue;
                }
                let id = format!("c{}", store.cards.len() + 1 + added);
                let tags = list(c, "tags").into_iter().filter_map(|t| t.as_str().map(String::from)).collect();
                store.cards.push(srs::new_card(id, if deck.is_empty() { "concurso" } else { &deck }, &front, &back, tags, today));
                added += 1;
            }
            Ok(format!("{added} card(s) added to {}.", if deck.is_empty() { "concurso" } else { &deck }))
        }
        "grade" => {
            let mut done = Vec::new();
            for g in list(args, "grades") {
                let Some(id) = s(g, "id") else { continue };
                let q = s(g, "grade").and_then(|x| srs::quality(&x)).unwrap_or(4);
                if let Some(card) = store.cards.iter_mut().find(|c| c.id == id) {
                    srs::grade(card, q, today);
                    done.push(format!("{id} → back on {}", card.due.br()));
                }
            }
            if done.is_empty() {
                return Err("grade needs grades: [{id, grade}] with ids from action due".into());
            }
            Ok(format!("Graded: {}.", done.join(", ")))
        }
        "stats" => {
            let st = srs::stats(&store.cards, today);
            if st.is_empty() {
                return Ok("No card yet.".into());
            }
            Ok(st.iter().map(|d| format!("{}: {} cards, {} due, {} learned, {} mature", d.deck, d.cards, d.due, d.learned, d.mature)).collect::<Vec<_>>().join("\n"))
        }
        _ => {
            let limit = f(args, "limit").map(|l| l.clamp(1.0, 50.0) as usize).unwrap_or(10);
            let due = srs::due(&store.cards, &deck, today, limit);
            if due.is_empty() {
                return Ok("No card due now.".into());
            }
            let mut out = String::from("Cards due (ask the front, wait for the answer, show the back, then grade each with action grade):");
            for c in due {
                out.push_str(&format!("\n- {} [{}]: {} → {}", c.id, c.deck, c.front, c.back));
            }
            Ok(out)
        }
    }
}

pub fn english_view(store: &mut Store, args: &Value, today: Date) -> Result<String, String> {
    let action = s(args, "action").map(|a| crate::text::plain(&a)).unwrap_or_else(|| "lesson".into());
    let mut note = String::new();
    match action.as_str() {
        "set" => {
            let p: &mut EnglishProfile = &mut store.english;
            if let Some(x) = s(args, "level") {
                p.level = x.to_uppercase();
            }
            if let Some(x) = f(args, "minutes_per_day") {
                p.minutes_per_day = x.clamp(5.0, 180.0) as u32;
            }
            if let Some(x) = b(args, "mode") {
                p.mode = x;
            }
            note = format!("English settings saved (English mode {}).\n", if store.english.mode { "on" } else { "off" });
        }
        "mode" => {
            let on = b(args, "mode").unwrap_or(!store.english.mode);
            store.english.mode = on;
            return Ok(format!("English mode {}.", if on { "on: Zheus answers in English and corrects the user's sentences" } else { "off" }));
        }
        "log" => {
            let minutes = f(args, "minutes").map(|m| m.max(1.0) as u32).unwrap_or(10);
            let kind = s(args, "kind").unwrap_or_else(|| "practice".into());
            english::practice(&mut store.english, &mut store.practice, &kind, minutes, today);
            return Ok(format!("{minutes} min of English logged ({kind}); streak {} day(s).", store.english.streak_days));
        }
        _ => {}
    }
    let l = english::lesson(&store.english, &store.cards, today);
    Ok(format!("{note}{}", english::for_model(&store.english, &l, &store.practice, today)))
}

pub fn gym_view(store: &mut Store, args: &Value, today: Date, study_hours: f64) -> Result<String, String> {
    let action = s(args, "action").map(|a| crate::text::plain(&a)).unwrap_or_else(|| "today".into());
    let mut note = String::new();
    match action.as_str() {
        "set" => {
            let p: &mut GymProfile = &mut store.gym;
            if let Some(x) = f(args, "days_per_week") {
                p.days_per_week = x.clamp(1.0, 7.0) as u8;
            }
            if let Some(x) = s(args, "split") {
                p.split = crate::text::plain(&x).replace(' ', "_");
            }
            if let Some(x) = s(args, "goal") {
                p.goal = x;
            }
            if let Some(x) = f(args, "session_minutes") {
                p.session_minutes = x.clamp(15.0, 240.0) as u32;
            }
            note = "Training profile saved.\n".into();
        }
        "log" => {
            let w = &args["workout"];
            let source = if w.is_object() { w } else { args };
            let exercises: Vec<ExerciseLog> = list(source, "exercises")
                .into_iter()
                .filter_map(|e| {
                    let name = s(e, "name")?;
                    let mut sets: Vec<SetLog> = list(e, "sets").into_iter().filter_map(|x| Some(SetLog { reps: f(x, "reps")? as u32, kg: f(x, "kg").unwrap_or(0.0) })).collect();
                    // "3x10 com 60 kg": sets, reps and kg given once.
                    if sets.is_empty() {
                        if let (Some(n), Some(reps)) = (f(e, "sets").or_else(|| f(e, "sets_count")), f(e, "reps")) {
                            sets = (0..n.clamp(1.0, 20.0) as u32).map(|_| SetLog { reps: reps as u32, kg: f(e, "kg").unwrap_or(0.0) }).collect();
                        }
                    }
                    Some(ExerciseLog { name, sets })
                })
                .collect();
            if exercises.is_empty() {
                return Err("log needs the exercises: [{name, sets: [{reps, kg}]}] (or sets, reps and kg once)".into());
            }
            let session = s(source, "session").unwrap_or_else(|| gym::next_session(&store.gym, &store.workouts).0.to_string());
            let n = exercises.len();
            store.workouts.push(Workout {
                date: date_arg(source, "date").unwrap_or(today),
                session: session.clone(),
                exercises,
                minutes: f(source, "minutes").map(|m| m as u32).unwrap_or(0),
                effort: f(source, "effort").map(|e| e.clamp(1.0, 10.0) as u8),
                note: s(source, "note").unwrap_or_default(),
            });
            note = format!("Workout logged: {session}, {n} exercises.\n");
        }
        "wellness" | "sleep" => {
            let w = Wellness {
                date: date_arg(args, "date").unwrap_or(today),
                sleep_hours: f(args, "sleep_hours"),
                soreness: f(args, "soreness").map(|x| x.clamp(0.0, 3.0) as u8),
                energy: f(args, "energy").map(|x| x.clamp(1.0, 5.0) as u8),
            };
            store.wellness.retain(|x| x.date != w.date);
            store.wellness.push(w);
            note = "Noted.\n".into();
        }
        _ => {}
    }
    Ok(format!("{note}{}", gym::for_model(&store.gym, &store.workouts, &store.wellness, today, study_hours)))
}

/// Hours of study planned for today.
pub fn study_hours_today(store: &Store, today: Date) -> f64 {
    if store.study.subjects.is_empty() {
        return 0.0;
    }
    let plan = study::plan(&store.study, &store.study_logs, today);
    study::today_blocks(&plan, today).iter().map(|b| b.minutes as f64 / 60.0).sum()
}

pub fn day_plan(store: &Store, market: &Market, today: Date) -> day::DayPlan {
    let events = fund_events(store, market, 7, today);
    let reviews = journal::due(&store.decisions, today).into_iter().map(|d| (d.id.clone(), format!("{} {}", d.action, d.ticker))).collect();
    let (study_blocks, study_reviews) = if store.study.subjects.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        let plan = study::plan(&store.study, &store.study_logs, today);
        (study::today_blocks(&plan, today).iter().map(|b| format!("{} ({} min)", b.subject, b.minutes)).collect(), plan.reviews_today)
    };
    let study_hours = study_hours_today(store, today);
    let training = (!store.workouts.is_empty() || store.gym != GymProfile::default()).then(|| {
        let (session, _) = gym::next_session(&store.gym, &store.workouts);
        let (r, _) = gym::readiness(&store.wellness, today, study_hours);
        (session.to_string(), match r {
            gym::Readiness::Normal => "normal".to_string(),
            gym::Readiness::Light => "light".to_string(),
            gym::Readiness::Rest => "rest".to_string(),
        })
    });
    let english = (store.english.minutes_per_day > 0 || !store.english.level.is_empty()).then(|| {
        let l = english::lesson(&store.english, &store.cards, today);
        (l.company.1.clone(), l.cards.len())
    });
    let opportunities = ceiling::opportunities(store, market, today).into_iter().map(|o| (o.ticker, o.below, o.method.to_string(), o.ceiling)).collect();
    day::plan(day::Inputs { today, events: &events, reviews, darf: darf_due(store, today), study: study_blocks, study_reviews, training, english, opportunities })
}

pub fn content_kit(store: &Store, args: &Value, today: Date) -> (Vec<content::File>, String) {
    let end = s(args, "month").and_then(|m| Date::parse(&format!("{m}-01"))).map(|d| d.last_of_month().min(today)).unwrap_or(today);
    content::kit(store, end)
}

// ── What the island shows ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassLine {
    pub id: String,
    pub label: String,
    pub weight: f64,
    pub target: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    /// "fund" (a fund's dividends or value at risk), "thesis" (conditions of the user's thesis that broke) or "cnpj" (a
    /// fund Coucou needs the CNPJ of).
    pub kind: String,
    pub ticker: String,
    /// How many conditions broke (thesis).
    pub count: usize,
    /// Why, in English (for the brain, and as the island's tooltip).
    pub text: String,
    pub urgent: bool,
}

/// The island's and the TV's picture of it all.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub has_portfolio: bool,
    pub value: f64,
    pub gain_pct: Option<f64>,
    pub priced_on: Option<Date>,
    /// When the newest of the session's prices was (unix seconds), when the value uses some.
    pub live_at: Option<i64>,
    pub classes: Vec<ClassLine>,
    pub income: f64,
    pub income_goal: Option<f64>,
    pub alerts: Vec<Alert>,
    pub today: Vec<day::Item>,
    pub study_streak: u32,
    pub english_streak: u32,
    pub next_training: Option<String>,
    pub selic: Option<f64>,
}

pub fn summary(store: &Store, market: &Market, today: Date) -> Summary {
    let p = &store.portfolio;
    let v = portfolio::value(p, market);
    let incomes = portfolio::income(&v, market, today);
    let mut alerts = Vec::new();
    for t in p.tickers(Some(AssetClass::Fii)) {
        if let Some(fund) = market.funds.get(&t) {
            let r = fii::report(fund, market.price(&t), &market.economy, today);
            if r.health == Health::Risk {
                alerts.push(Alert { kind: "fund".into(), ticker: t.clone(), count: r.signals.len(), text: r.signals.join(" "), urgent: true });
            }
        }
    }
    for t in without_cnpj(store, market) {
        alerts.push(Alert { kind: "cnpj".into(), ticker: t, count: 0, text: "Coucou needs this fund's CNPJ: the CVM's reports do not link its ticker to it.".into(), urgent: false });
    }
    for th in &store.theses {
        if let Some(c) = market.companies.get(&th.ticker) {
            let m = company::metrics(&c.figures);
            let broken: Vec<String> = company::check(&m, &th.conditions).into_iter().filter(|c| c.holds == Some(false)).map(|c| if c.condition.label.is_empty() { c.condition.metric } else { c.condition.label }).collect();
            if !broken.is_empty() {
                alerts.push(Alert { kind: "thesis".into(), ticker: th.ticker.clone(), count: broken.len(), text: broken.join("; "), urgent: false });
            }
        }
    }
    let plan = day_plan(store, market, today);
    Summary {
        has_portfolio: !p.positions.is_empty(),
        value: v.value,
        gain_pct: v.gain_pct,
        priced_on: v.priced_on,
        live_at: v.live_at,
        // Only what is held: a class sold out is no bar, whatever its target.
        classes: v.classes.iter().filter(|c| c.value > 0.0).map(|c| ClassLine { id: c.class.id().into(), label: c.label.clone(), weight: c.weight, target: c.target }).collect(),
        income: incomes.iter().map(|i| i.monthly).sum(),
        income_goal: p.income_goal,
        alerts,
        today: plan.items,
        study_streak: study::progress(&store.study, &store.study_logs, today).streak,
        english_streak: if store.english.last_day.is_some_and(|d| d.add_days(1) >= today) { store.english.streak_days } else { 0 },
        next_training: (!store.workouts.is_empty()).then(|| gym::next_session(&store.gym, &store.workouts).0.to_string()),
        selic: market.economy.selic,
    }
}

/// Today's snapshot, once the quotes are in.
pub fn snapshot(store: &mut Store, market: &Market, today: Date) {
    if store.portfolio.positions.is_empty() {
        return;
    }
    let v = portfolio::value(&store.portfolio, market);
    let incomes = portfolio::income(&v, market, today);
    portfolio::keep_snapshot(&mut store.snapshots, portfolio::snapshot(&v, &incomes, today));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::fixtures;
    use serde_json::json;

    fn today() -> Date {
        Date::new(2026, 10, 21).unwrap()
    }

    #[test]
    fn the_portfolio_from_a_paste_then_targets_and_a_plan() {
        let mut store = Store::default();
        let market = fixtures::market();
        assert!(portfolio(&store, &market, today()).contains("empty"));
        let paste = "FIIs\nGGRC11 150 R$ 9,20\nGARE11 180 R$ 8,55\nBTCI11 140 R$ 8,90\nPORD11 120 R$ 8,45\nAções\nTIMS3 30 R$ 18,60\nETFs\nSPXB11 40 R$ 16,75";
        let note = update(&mut store, &json!({"text": paste, "targets": {"Ações": 25, "FIIs": "25", "ETFs": 25, "renda fixa": 25}, "monthly_contribution": "500", "income_goal": 1000}), None, today()).unwrap();
        assert!(note.contains("6 added") && note.contains("Targets set (100%"), "{note}");
        assert_eq!(store.portfolio.positions.len(), 6);
        assert_eq!(store.portfolio.monthly_contribution, Some(500.0));
        let text = portfolio(&store, &market, today());
        assert!(text.contains("FIIs: R$") && text.contains("vs target 25%"));
        // Merge one change in, with numbers as text.
        update(&mut store, &json!({"positions": [{"ticker": "gare11", "quantity": "254", "average_price": "8,37"}]}), None, today()).unwrap();
        let gare = store.portfolio.position("GARE11").unwrap();
        assert_eq!((gare.quantity, gare.average_price), (254.0, 8.37));
        assert_eq!(store.portfolio.positions.len(), 6, "merge keeps the others");
        let plan_text = plan(&store, &market, &json!({"contribution": 500, "scenario": {"selic": -2}}), today());
        assert!(plan_text.contains("Road to the targets without selling") && plan_text.contains("Where R$ 500,00 goes") && plan_text.contains("Scenario"), "{plan_text}");
        assert!(plan_text.contains("Magic numbers"));
        assert!(update(&mut store, &json!({}), None, today()).is_err());
        assert!(update(&mut store, &json!({"text": "nada"}), None, today()).is_err());
        update(&mut store, &json!({"remove": ["SPXB11"]}), None, today()).unwrap();
        assert!(store.portfolio.position("SPXB11").is_none());
    }

    #[test]
    fn a_b3_position_keeps_the_average_prices_known() {
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let file = Imported {
            positions: vec![Position { ticker: "GGRC11".into(), class: AssetClass::Fii, quantity: 200.0, average_price: 0.0, subtype: String::new(), current_value: None, note: String::new() }],
            ..Imported::default()
        };
        update(&mut store, &json!({"mode": "merge"}), Some(file.clone()), today()).unwrap();
        let g = store.portfolio.position("GGRC11").unwrap();
        assert_eq!((g.quantity, g.average_price, g.subtype.as_str()), (200.0, 9.20, "tijolo"));
        assert_eq!(store.portfolio.positions.len(), 7, "merged: the rest stays");
        // By default B3's export replaces the classes it holds: the FIIs sold are gone, the rest stays.
        let note = update(&mut store, &json!({}), Some(file), today()).unwrap();
        assert!(note.contains("3 removed") && note.contains("the others stay"), "{note}");
        let fiis = store.portfolio.tickers(Some(AssetClass::Fii));
        assert_eq!(fiis, vec!["GGRC11"]);
        assert!(store.portfolio.position("TIMS3").is_some() && store.portfolio.position("Tesouro Selic 2031").is_some());
    }

    #[test]
    fn a_fund_without_its_cnpj_is_asked_for_and_given() {
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let mut market = fixtures::market();
        market.funds.remove("BTCI11");
        for root in ["GGRC", "GARE", "PORD"] {
            market.fund_cnpjs.insert(root.into(), "1".into());
        }
        assert!(without_cnpj(&store, &market).is_empty(), "the CVM's reports not read yet: nothing to ask");
        market.funds_read = Some(today());
        assert_eq!(without_cnpj(&store, &market), vec!["BTCI11"]);
        let text = funds(&store, &market, Some("BTCI11"), today());
        assert!(text.contains("CNPJ of BTCI11's fund") && text.contains("fund_cnpjs"), "{text}");
        let p = portfolio(&store, &market, today());
        assert!(p.contains("CNPJ of BTCI11's fund") && !p.contains("not read yet for BTCI11"), "{p}");
        let s = summary(&store, &market, today());
        assert!(s.alerts.iter().any(|a| a.kind == "cnpj" && a.ticker == "BTCI11" && !a.urgent));
        // A wrong check digit is refused, and nothing else changes.
        let wrong = update(&mut store, &json!({"fund_cnpjs": {"BTCI11": "09.552.812/0001-15"}}), None, today()).unwrap_err();
        assert!(wrong.contains("not a valid CNPJ"), "{wrong}");
        assert!(store.fund_cnpjs.is_empty() && store.portfolio.updated.is_none());
        let note = update(&mut store, &json!({"fund_cnpjs": {"btci11": "09.552.812/0001-14"}}), None, today()).unwrap();
        assert!(note.contains("09.552.812/0001-14"), "{note}");
        assert_eq!(store.fund_cnpjs["BTCI"], "09552812000114");
        assert!(store.portfolio.updated.is_none(), "a CNPJ is not a change of the portfolio");
        assert!(without_cnpj(&store, &market).is_empty());
        // Read under it: its name from FundosNet, checked against the name B3 trades it under.
        let doc = |trading: &str| crate::sources::fnet::Document {
            id: 1,
            category: "Relatórios".into(),
            kind: "Relatório Gerencial".into(),
            species: String::new(),
            reference: "09/2026".into(),
            delivered: Some(today()),
            info: String::new(),
            topic: crate::sources::fnet::Topic::Report,
            structured: false,
            fund: "BTG PACTUAL CRÉDITO IMOBILIÁRIO FII".into(),
            trading: trading.into(),
        };
        market.quotes.get_mut("BTCI11").unwrap().name = "FII BTG CRI".into();
        market.funds.insert("BTCI11".into(), crate::model::FundData { ticker: "BTCI11".into(), cnpj: "09552812000114".into(), documents: vec![doc("FII BTG  CRI")], ..Default::default() });
        let text = funds(&store, &market, Some("BTCI11"), today());
        assert!(text.starts_with("BTCI11 — BTG PACTUAL CRÉDITO IMOBILIÁRIO FII") && text.contains("is BTCI11's"), "{text}");
        market.funds.get_mut("BTCI11").unwrap().documents = vec![doc("FII OUTRO")];
        let text = funds(&store, &market, Some("BTCI11"), today());
        assert!(text.contains("It may be another fund's"), "{text}");
        update(&mut store, &json!({"fund_cnpjs": {"BTCI11": ""}}), None, today()).unwrap();
        assert!(store.fund_cnpjs.is_empty(), "an empty one forgets it");
        assert!(!funds(&store, &market, Some("BTCI11"), today()).contains("CNPJ the user gave"));
    }

    #[test]
    fn trades_told_in_the_chat_and_ceilings_through_the_tools() {
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let mut market = fixtures::market();
        market.economy.cdi = Some(13.65);
        let note = trade_view(&mut store, &market, &json!({"trades": [{"ticker": "ggrc11", "side": "compra", "quantity": "50", "price": "9,00", "date": "21/10/2026", "time": "14h30"}]}), today()).unwrap();
        assert!(note.contains("Bought 50 GGRC11 at R$ 9,00 on 21/10/2026 14:30"), "{note}");
        assert_eq!(store.portfolio.position("GGRC11").unwrap().quantity, 200.0);
        let sold = trade_view(&mut store, &market, &json!({"trades": [{"ticker": "GGRC11", "side": "venda", "quantity": 20, "price": 10}]}), today()).unwrap();
        assert!(sold.contains("Sold 20 GGRC11"), "{sold}");
        let history = trade_view(&mut store, &market, &json!({"list": true}), today()).unwrap();
        assert!(history.starts_with("2026-10: R$ 450,00 put in, R$ 200,00 from sales"), "{history}");
        assert!(trade_view(&mut store, &market, &json!({"undo_last": true}), today()).unwrap().contains("Undone: the sale"));
        assert_eq!(store.portfolio.position("GGRC11").unwrap().quantity, 200.0);
        assert!(trade_view(&mut store, &market, &json!({"trades": [{"side": "buy"}]}), today()).is_err(), "no ticker");
        // Ceilings: the user's own, the yields, and an opportunity.
        let text = ceiling_view(&mut store, &market, &json!({"ticker": "GGRC11", "ceiling": 12, "fund_yield": 10}), today()).unwrap();
        assert!(text.contains("the user's own ceiling is R$ 12,00") && text.contains("Funds' yield: 10,0%"), "{text}");
        assert!(text.contains("GGRC11 (a real-estate fund) — price R$ 9,12") && text.contains("projected Bazin: ceiling R$ 10,56"), "{text}");
        assert!(text.contains("Opportunities now") && text.contains("GGRC11"), "{text}");
        assert_eq!(store.ceilings.manual["GGRC11"], 12.0);
        assert_eq!(store.ceilings.fund_yield, Some(10.0));
        ceiling_view(&mut store, &market, &json!({"fund_yield": 0}), today()).unwrap();
        assert_eq!(store.ceilings.fund_yield, None, "0 goes back to the CDI's");
        let all = ceiling_view(&mut store, &market, &json!({}), today()).unwrap();
        assert!(all.contains("TIMS3 (a share)") && !all.contains("SPXB11"), "shares and funds held, not the ETF: {all}");
        let day = day_plan(&store, &market, today());
        assert!(day.items.iter().any(|i| i.code == "opportunity" && i.vars["ticker"] == "GGRC11"));
    }

    #[test]
    fn a_contributions_file_builds_the_positions_of_an_empty_portfolio() {
        let mut store = Store::default();
        let file = crate::import::from_file("aportes.csv", "Data;Ativo;Quantidade;Preço\n10/09/2026;GGRC11;100;9,00\n21/10/2026;GGRC11;50;9,60\n".as_bytes()).unwrap();
        let note = update(&mut store, &json!({}), Some(file.clone()), today()).unwrap();
        assert!(note.contains("Positions worked out from 2 trades"), "{note}");
        let g = store.portfolio.position("GGRC11").unwrap();
        assert_eq!(g.quantity, 150.0);
        assert!((g.average_price - 9.2).abs() < 1e-9);
        // A portfolio already there is kept, unless the user asks to rebuild it.
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let note = update(&mut store, &json!({}), Some(file.clone()), today()).unwrap();
        assert!(note.contains("the positions were kept"), "{note}");
        assert_eq!(store.portfolio.position("GGRC11").unwrap().quantity, 150.0);
        let note = update(&mut store, &json!({"rebuild_from_trades": true}), None, today()).unwrap();
        assert!(note.contains("Positions worked out"), "{note}");
        assert!(store.portfolio.position("TIMS3").is_none() && store.portfolio.position("Tesouro Selic 2031").is_some(), "the trades' assets, and fixed income kept");
    }

    #[test]
    fn decisions_committee_studies_cards_english_training_and_the_day() {
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let market = fixtures::market();
        let r = decision(&mut store, &market, &json!({"action": "record", "ticker": "GARE11", "decision": "comprar", "reasons": "P/VP abaixo de 1", "confidence": "70", "quantity": 50}), today()).unwrap();
        assert!(r.contains("recorded") && r.contains("review on"));
        assert_eq!(store.decisions[0].price, Some(8.40), "the last close when no price is given");
        assert!(decision(&mut store, &market, &json!({"action": "record", "ticker": "GARE11"}), today()).is_err());
        let list = decision(&mut store, &market, &json!({}), today()).unwrap();
        assert!(list.contains("buy GARE11"));
        let c = committee(&store, &market, &json!({"proposal": "comprar 50 cotas de GARE11"}), today()).unwrap();
        assert!(c.contains("[valuation]") && c.contains("[risk]") && c.contains("Proposal: comprar 50 cotas de GARE11") && c.contains("would go from"), "{c}");
        let st = study_view(
            &mut store,
            &market,
            &json!({"action": "set", "exam": "Analista do BACEN", "board": "Cebraspe", "date": "18/04/2027", "salary": "20924,80", "hours_per_week": 15, "subjects": [{"name": "Economia", "weight": 30, "level": 2}, {"name": "Português", "weight": 15, "level": 4}]}),
            today(),
        )
        .unwrap();
        assert!(st.contains("Study profile saved") && st.contains("Analista do BACEN"), "{st}");
        let logged = study_view(&mut store, &market, &json!({"action": "log", "subject": "Economia", "minutes": 50, "questions": 10, "correct": 7}), today()).unwrap();
        assert!(logged.contains("7/10"));
        let added = cards(&mut store, &json!({"action": "add", "deck": "english", "cards": [{"front": "guidance", "back": "projeção"}, {"front": "yield", "back": "rendimento"}]}), today()).unwrap();
        assert!(added.contains("2 card(s)"));
        let due = cards(&mut store, &json!({"action": "due", "deck": "english"}), today()).unwrap();
        assert!(due.contains("guidance"));
        let graded = cards(&mut store, &json!({"action": "grade", "grades": [{"id": "c1", "grade": "acertei"}]}), today()).unwrap();
        assert!(graded.contains("c1 → back on"));
        let en = english_view(&mut store, &json!({"action": "set", "level": "b1", "minutes_per_day": 15, "mode": "sim"}), today()).unwrap();
        assert!(en.contains("English mode on") && store.english.mode);
        assert!(english_view(&mut store, &json!({"action": "log", "minutes": 10, "kind": "speaking"}), today()).unwrap().contains("streak 1"));
        let g = gym_view(&mut store, &json!({"action": "log", "workout": {"session": "Upper A", "exercises": [{"name": "Supino reto", "sets": 3, "reps": 10, "kg": "60"}, {"name": "Remada curvada", "sets_count": 3, "reps": 10, "kg": 50}]}}), today(), 2.0).unwrap();
        assert_eq!(store.workouts[0].exercises[1].sets.len(), 3, "sets_count with reps and kg once");
        assert!(g.contains("Workout logged: Upper A") && g.contains("Next session: Lower A"), "{g}");
        let well = gym_view(&mut store, &json!({"action": "wellness", "sleep_hours": 5, "energy": 2, "soreness": 2}), today(), 5.0).unwrap();
        assert!(well.contains("rest or walk"), "{well}");
        let d = day_plan(&store, &market, today());
        assert!(d.items.iter().any(|i| i.area == "study") && d.items.iter().any(|i| i.area == "english") && d.items.iter().any(|i| i.area == "training"));
        let sum = summary(&store, &market, today());
        assert!(sum.has_portfolio && sum.income > 0.0 && !sum.classes.is_empty());
        // A class the user holds nothing of is no bar, even with a target set for it.
        let mut sold = store.clone();
        sold.portfolio.positions.retain(|p| p.class != AssetClass::Etf);
        sold.portfolio.targets.insert(AssetClass::Etf, 0.1);
        assert!(!summary(&sold, &market, today()).classes.iter().any(|c| c.id == AssetClass::Etf.id()));
        assert_eq!(sum.english_streak, 1);
        snapshot(&mut store, &market, today());
        assert_eq!(store.snapshots.len(), 1);
        let (files, brief) = content_kit(&store, &json!({}), today());
        assert!(!files.is_empty() && brief.contains("Content kit"), "{brief}");
    }

    #[test]
    fn a_company_and_its_thesis() {
        let mut store = Store::default();
        let mut market = Market::default();
        let (text, changed) = company_view(&mut store, &market, "TIMS3", &json!({"thesis": "margem EBITDA acima de 45%; dívida caindo; gosto do CEO"}), today());
        assert!(changed && text.contains("could not be read") && text.contains("saved"));
        market.companies.insert("TIMS3".into(), crate::model::CompanyData { ticker: "TIMS3".into(), cnpj: "1".into(), name: "TIM S.A.".into(), figures: crate::company::fixtures::figures(0.1), checked: Some(today()), ..Default::default() });
        let (text, changed) = company_view(&mut store, &market, "TIMS3", &json!({"thesis": "margem EBITDA acima de 45%; dívida caindo; gosto do CEO"}), today());
        assert!(changed);
        assert!(text.contains("holds") && text.contains("Not measurable as written: gosto do CEO"), "{text}");
        let (text, changed) = company_view(&mut store, &market, "TIMS3", &json!({}), today());
        assert!(!changed && text.contains("The user's thesis"), "the thesis is kept");
        assert!(economy(&market, today()).contains("could not be read"));
    }
}
