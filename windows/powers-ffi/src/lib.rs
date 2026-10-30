//! The superpowers inside the iPhone (NotchBuddy/Hermes/README.md): the same engine as the computer's (coucou-powers:
//! the portfolio against B3, the CVM, FundosNet and the Banco Central, the ceilings, taxes, the committee, and the
//! studies, English and training), for Hermes inside the iPhone app, behind two C functions the app links in. The app
//! answers Hermes' wealth_ and life_ tools with them (Hermes/Swift/HermesArms.swift, "powers") on a thread of its own:
//! a call may read the official sources first, as on the computer (src-tauri/src/powers/mod.rs, `call`, which this
//! follows, without the sheet and the island).
//!
//! What is kept is the computer's two files, in the folder the app gives: the user's data (powers.json) and what the
//! sources said (powers-market.json), and the iPhone's own third, the documents its reader read (powers-docs.json:
//! the app downloads and reads their PDFs, coucou_powers::library). The positions, the cash and the user's own ceilings follow the copy a paired
//! computer gives the iPhone (Hermes' portfolio.json, src-tauri/src/powers/mod.rs `phone_copy`) each time a newer one
//! comes: the computer and its sheet stay the record.

use std::collections::BTreeMap;
use std::ffi::{c_char, CStr, CString};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use coucou_powers::date::{is_business_day, Date};
use coucou_powers::library::{self, Library};
use coucou_powers::model::{AssetClass, LivePrice, Market, Position, Store};
use coucou_powers::sources::fetch::Http;
use coucou_powers::{alerts, ceiling, committee, day, engine, invest, learn, refresh, sources, text, thesis};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

/// How long an answer waits for the readings it rests on (a price of an asset held, the economy), as on the computer.
const READ_BUDGET: Duration = Duration::from_secs(20);
/// What is due but not needed for the answer: a short look, the rest at the next call.
const QUICK_BUDGET: Duration = Duration::from_secs(6);
/// An asset asked about and not held: its quote, its fund's documents or its company's statements.
const ASSET_BUDGET: Duration = Duration::from_secs(40);
const LIVE_EVERY: Duration = Duration::from_secs(5 * 60);
const LIVE_PAUSE: Duration = Duration::from_millis(400);
const LIVE_MOST: usize = 40;
const LIVE_BUDGET: Duration = Duration::from_secs(15);

struct State {
    folder: PathBuf,
    store: Store,
    market: Market,
    /// Shares whose statements could not be read today: not asked again before tomorrow.
    failed: Vec<(String, Date)>,
    live_read: Option<Instant>,
    /// The documents read, opened when first needed.
    library: Option<Library>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn http() -> &'static Http {
    static HTTP: OnceLock<Http> = OnceLock::new();
    HTTP.get_or_init(Http::new)
}

/// One runtime for every reading, with a thread of its own that looks after the client's open connections between
/// calls.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(1).thread_name("axyon-powers").enable_all().build().expect("a runtime"))
}

/// `job` within what is left until `deadline`; None when the time is up.
fn within<F: Future>(deadline: Instant, job: F) -> Option<F::Output> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return None;
    }
    runtime().block_on(async { tokio::time::timeout(left, job).await.ok() })
}

// ── Files ─────────────────────────────────────────────────────────────────────

fn read_file<T: DeserializeOwned + Default>(path: &Path) -> T {
    let Ok(bytes) = std::fs::read(path) else { return T::default() };
    match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => {
            // Set aside, never overwritten, as on the computer.
            let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let _ = std::fs::rename(path, path.with_extension(format!("unreadable-{secs}.json")));
            T::default()
        }
    }
}

/// Written beside, then moved over: a stop never leaves half a file.
fn write_file<T: Serialize>(path: &Path, value: &T) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(bytes) = serde_json::to_vec(value) {
        let temp = path.with_extension("json.axyon");
        if std::fs::write(&temp, bytes).is_ok() {
            let _ = std::fs::rename(&temp, path);
        }
    }
}

impl State {
    fn open(folder: &Path) -> State {
        State {
            folder: folder.to_path_buf(),
            store: read_file(&folder.join("powers.json")),
            market: read_file(&folder.join("powers-market.json")),
            failed: Vec::new(),
            live_read: None,
            library: None,
        }
    }

    fn load_library(&mut self) {
        if self.library.is_none() {
            self.library = Some(read_file(&self.folder.join("powers-docs.json")));
        }
    }

    fn save_library(&self) {
        if let Some(lib) = &self.library {
            write_file(&self.folder.join("powers-docs.json"), lib);
        }
    }

    fn save_store(&self) {
        write_file(&self.folder.join("powers.json"), &self.store);
    }

    fn save_market(&self) {
        write_file(&self.folder.join("powers-market.json"), &self.market);
    }

    /// The computer's copy of the portfolio, when a newer one came: its positions, cash and the user's ceilings.
    fn take_copy(&mut self, copy: &Path) {
        let Ok(bytes) = std::fs::read(copy) else { return };
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else { return };
        let at = v["receivedAt"].as_f64().unwrap_or(0.0);
        let mark = self.folder.join("powers-copy.txt");
        let taken = std::fs::read_to_string(&mark).ok().and_then(|t| t.trim().parse::<f64>().ok()).unwrap_or(0.0);
        if at <= taken {
            return;
        }
        let Ok(positions) = serde_json::from_value::<Vec<Position>>(v["positions"].clone()) else { return };
        self.store.portfolio.positions = positions;
        if let Ok(ceilings) = serde_json::from_value::<BTreeMap<String, f64>>(v["ceilings"].clone()) {
            self.store.ceilings.manual = ceilings;
        }
        if let Some(cash) = v["cash"].as_f64() {
            self.store.portfolio.cash = cash;
        }
        self.store.portfolio.updated = v["updated"].as_str().and_then(Date::parse).or(self.store.portfolio.updated);
        self.store.portfolio.source = COMPUTER.into();
        self.save_store();
        let _ = std::fs::write(mark, at.to_string());
    }
}

/// The source of positions that came from the computer's copy.
const COMPUTER: &str = "computer";

// ── The clock ─────────────────────────────────────────────────────────────────

/// Now in Brasília (UTC−3, B3's own time): its day and its minute of the day.
fn brasilia() -> (Date, u32) {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) - 3 * 3600;
    (Date::from_days(secs.div_euclid(86_400)), (secs.rem_euclid(86_400) / 60) as u32)
}

/// B3 trades, with a margin for its closing call and Google's 15 minutes: a trading day, 10:00 to 18:30 in Brasília.
fn session_open() -> bool {
    let (day, minute) = brasilia();
    is_business_day(day, true) && (10 * 60..18 * 60 + 30).contains(&minute)
}

// ── Reading the sources ───────────────────────────────────────────────────────

/// Whether an answer about the portfolio would lack what it rests on: a price for an asset held, or the economy.
fn needs_reading(store: &Store, market: &Market) -> bool {
    store.portfolio.tickers(None).iter().any(|t| market.price(t).is_none()) || market.economy_read.is_none()
}

/// The shares whose statements are due: those held, those of a thesis, and those the user set a ceiling for.
fn companies_due(store: &Store, market: &Market, today: Date, failed: &[(String, Date)]) -> Vec<String> {
    let mut wanted = store.portfolio.tickers(Some(AssetClass::Acao));
    let share = |t: &&String| market.quotes.get(*t).map_or(!t.ends_with("11"), |q| sources::b3::kind_of(q) == "acao");
    for t in store.theses.iter().map(|th| &th.ticker).chain(store.ceilings.manual.keys().filter(share)) {
        if !wanted.contains(t) {
            wanted.push(t.clone());
        }
    }
    wanted
        .into_iter()
        .filter(|t| !failed.iter().any(|(f, d)| f == t && *d == today))
        .filter(|t| refresh::company_due(market.companies.get(t), today))
        .collect()
}

/// What is due, read within `budget` and merged in, one source after the other.
fn read_due(st: &mut State, today: Date, evening: bool, budget: Duration) {
    let due = refresh::due(&st.store, &st.market, today, evening, false);
    let companies = companies_due(&st.store, &st.market, today, &st.failed);
    if due.quotes.is_empty() && due.funds.is_empty() && !due.fund_months && !due.economy && companies.is_empty() {
        return;
    }
    let deadline = Instant::now() + budget;
    if !due.quotes.is_empty() {
        let have = st.market.quotes_day.filter(|_| due.quotes.iter().all(|t| st.market.quotes.contains_key(t)));
        if let Some(Ok((day, quotes))) = within(deadline, sources::quotes(http(), today, evening, &due.quotes, have)) {
            refresh::merge_quotes(&mut st.market, day, quotes);
        }
    }
    if due.economy {
        if let Some(Ok(economy)) = within(deadline, sources::economy(http())) {
            st.market.economy = economy;
            st.market.economy_read = Some(today);
        }
    }
    if due.fund_months || !due.funds.is_empty() {
        let mine = st.store.portfolio.tickers(Some(AssetClass::Fii));
        let known = refresh::known_documents(&st.market);
        let job = refresh::funds(http(), &due.funds, &st.market.fund_cnpjs, &st.store.fund_cnpjs, due.fund_months.then_some(mine.as_slice()), &known, today);
        if let Some(read) = within(deadline, job) {
            refresh::merge_funds(&mut st.market, read, today);
        }
    }
    for ticker in companies {
        match within(deadline, refresh::company(http(), &ticker, &st.market.company_cnpjs, today)) {
            Some(Ok((company, register))) => {
                if let Some(register) = register {
                    st.market.company_cnpjs.extend(register);
                }
                refresh::merge_company(&mut st.market, ticker, company);
            }
            Some(Err(_)) => st.failed.push((ticker, today)),
            None => break,
        }
    }
    refresh::fix_classes(&mut st.store, &st.market);
    engine::snapshot(&mut st.store, &st.market, today);
    st.save_store();
    st.save_market();
}

/// An asset the user asks about and may not hold: its last close, and its fund's documents or its company's figures.
fn ensure_asset(st: &mut State, ticker: &str, today: Date, evening: bool) {
    let deadline = Instant::now() + ASSET_BUDGET;
    let one = [ticker.to_string()];
    if !st.market.quotes.contains_key(ticker) {
        if let Some(Ok((day, quotes))) = within(deadline, sources::quotes(http(), today, evening, &one, None)) {
            refresh::merge_quotes(&mut st.market, day, quotes);
        }
    }
    let kind = st.market.quotes.get(ticker).map(sources::b3::kind_of).unwrap_or("");
    let have = st.market.funds.get(ticker).cloned();
    if ticker.ends_with("11") && kind != "etf" && kind != "acao" {
        let given = refresh::cnpj_changed(&st.store, &st.market, ticker);
        let read_today = !given && have.as_ref().and_then(|f| f.checked) == Some(today);
        let months = given || (!read_today && have.as_ref().is_none_or(|f| f.months.is_empty()));
        if months || !read_today {
            let mut all = vec![ticker.to_string()];
            all.extend(st.store.portfolio.tickers(Some(AssetClass::Fii)).into_iter().filter(|t| t != ticker));
            let known = refresh::known_documents(&st.market);
            let job = refresh::funds(http(), &one, &st.market.fund_cnpjs, &st.store.fund_cnpjs, months.then_some(all.as_slice()), &known, today);
            if let Some(read) = within(deadline, job) {
                refresh::merge_funds(&mut st.market, read, today);
            }
        }
    } else if kind != "etf" && kind != "bdr" && refresh::company_due(st.market.companies.get(ticker), today) {
        if let Some(Ok((company, register))) = within(deadline, refresh::company(http(), ticker, &st.market.company_cnpjs, today)) {
            if let Some(register) = register {
                st.market.company_cnpjs.extend(register);
            }
            refresh::merge_company(&mut st.market, ticker.to_string(), company);
        }
    }
    st.save_market();
}

/// The CVM's list of the documents of the companies followed (IPE: material facts, releases, assemblies), once a week
/// each, within `deadline`.
fn read_company_documents(st: &mut State, today: Date, deadline: Instant) {
    let due = refresh::company_documents_due(&st.store, &st.market, today);
    if due.is_empty() {
        return;
    }
    let cnpjs: Vec<String> = due.iter().map(|(_, c)| c.clone()).collect();
    if let Some(Ok(read)) = within(deadline, sources::company_documents(http(), &refresh::document_years(today), &cnpjs, 40)) {
        refresh::merge_company_documents(&mut st.market, &due, read, today);
        st.save_market();
    }
}

/// The session's prices of what is held and what has a ceiling, from Google Finance, one page after the other, until
/// `deadline`.
fn read_live(st: &mut State, deadline: Instant) {
    let mut tickers = st.store.portfolio.tickers(None);
    for t in st.store.ceilings.manual.keys() {
        if !tickers.contains(t) {
            tickers.push(t.clone());
        }
    }
    tickers.truncate(LIVE_MOST);
    let mut got = Vec::new();
    for (i, ticker) in tickers.iter().enumerate() {
        if i > 0 {
            std::thread::sleep(LIVE_PAUSE);
        }
        match within(deadline, sources::live_price(http(), ticker)) {
            Some(Ok(live)) => got.push((ticker.clone(), LivePrice { price: live.price, at: live.at, previous: live.previous_close })),
            Some(Err(_)) => {}
            None => break,
        }
    }
    if !got.is_empty() {
        st.market.keep_live(got);
        st.save_market();
    }
    st.live_read = Some(Instant::now());
}

// ── The tools ─────────────────────────────────────────────────────────────────

/// The tools that read the official data: what is due is read first.
fn reads_market(tool: &str) -> bool {
    matches!(tool, "wealth_portfolio" | "wealth_fii" | "wealth_events" | "wealth_plan" | "wealth_audit" | "wealth_macro" | "wealth_committee" | "wealth_documents" | "wealth_learn" | "wealth_ceiling" | "life_day")
}

/// An asset the call is about, when it names one.
fn asked_asset(tool: &str, input: &Value) -> Option<String> {
    let named = engine::s(input, "ticker").and_then(|t| text::ticker(&t));
    match tool {
        "wealth_fii" | "wealth_company" | "wealth_ceiling" | "wealth_documents" | "wealth_learn" => named,
        "wealth_committee" => named.or_else(|| engine::s(input, "proposal").and_then(|p| text::tickers_in(&p).into_iter().next())),
        // The app's own calls: the committee's brief, a thesis written, a pretend trade.
        "invest_committee" if engine::s(input, "do").as_deref() == Some("brief") => named.or_else(|| engine::s(input, "proposal").and_then(|p| text::tickers_in(&p).into_iter().next())),
        "invest_thesis" if engine::s(input, "do").as_deref() != Some("remove") => named,
        "invest_learn" if engine::s(input, "do").as_deref() == Some("paper") => named,
        _ => None,
    }
}

/// Where the positions come from and how old they are, for the brain: an old list is not today's portfolio.
fn record_note(s: &Store) -> String {
    let source = match s.portfolio.source.as_str() {
        COMPUTER => "from AXYON on the user's computer (its copy on this iPhone)",
        "paste" | "text" => "pasted by the user",
        "b3" => "imported from B3's file",
        "chat" => "told in the chat",
        _ => "saved in AXYON on this iPhone",
    };
    let when = s.portfolio.updated.map(|d| format!(", last changed on {}", d.br())).unwrap_or_default();
    let mut out = format!("\n\nPositions {source}{when}.");
    if s.portfolio.source == COMPUTER {
        out.push_str(" A change made here stays on this iPhone until the computer's next copy, which replaces it: the user's computer and its sheet are the record.");
    }
    out
}

/// One superpower, as Hermes called it: whether it was done, and the answer in English, for Hermes to tell.
fn run(st: &mut State, tool: &str, input: &Value, today: Date, evening: bool, read: bool) -> (bool, String) {
    if read && reads_market(tool) {
        let budget = if needs_reading(&st.store, &st.market) { READ_BUDGET } else { QUICK_BUDGET };
        read_due(st, today, evening, budget);
        if session_open() && st.live_read.is_none_or(|t| t.elapsed() >= LIVE_EVERY) {
            read_live(st, Instant::now() + LIVE_BUDGET);
        }
    }
    if read {
        if let Some(asset) = asked_asset(tool, input) {
            ensure_asset(st, &asset, today, evening);
        }
    }
    let changed = |st: &State, out: Result<String, String>| -> (bool, String) {
        if out.is_ok() {
            st.save_store();
        }
        match out {
            Ok(text) => (true, text),
            Err(text) => (false, text),
        }
    };
    match tool {
        "wealth_portfolio" => (true, format!("{}{}", engine::portfolio(&st.store, &st.market, today), record_note(&st.store))),
        "wealth_update" => match engine::update(&mut st.store, input, None, today) {
            Ok(note) => {
                st.save_store();
                if read {
                    read_due(st, today, evening, READ_BUDGET);
                }
                let mut text = format!("{note}\n\nThe portfolio now:\n{}", engine::portfolio(&st.store, &st.market, today));
                for t in input["fund_cnpjs"].as_object().into_iter().flat_map(|o| o.keys()).filter_map(|k| text::ticker(k)) {
                    text.push_str(&format!("\n\n{}", engine::funds(&st.store, &st.market, Some(&t), today)));
                }
                (true, text)
            }
            Err(e) => (false, e),
        },
        "wealth_fii" => {
            let only = engine::s(input, "ticker");
            (true, engine::funds(&st.store, &st.market, only.as_deref(), today))
        }
        "wealth_events" => {
            let days = engine::f(input, "days").map(|d| d.clamp(1.0, 365.0) as i64).unwrap_or(30);
            (true, engine::events(&st.store, &st.market, days, today))
        }
        "wealth_company" => match engine::s(input, "ticker").and_then(|t| text::ticker(&t)) {
            Some(ticker) => {
                let text = engine::company_view(&mut st.store, &st.market, &ticker, input, today).0;
                st.save_store();
                (true, text)
            }
            None => (false, "wealth_company needs the share's ticker (\"TIMS3\").".into()),
        },
        "wealth_trade" => {
            let out = engine::trade_view(&mut st.store, &st.market, input, today);
            changed(st, out)
        }
        "wealth_ceiling" => {
            let out = engine::ceiling_view(&mut st.store, &st.market, input, today);
            changed(st, out)
        }
        "wealth_plan" => (true, engine::plan(&st.store, &st.market, input, today)),
        "wealth_audit" => (true, engine::audit_view(&st.store, &st.market, input, today)),
        "wealth_decision" => {
            let out = engine::decision(&mut st.store, &st.market, input, today);
            changed(st, out)
        }
        "wealth_committee" => {
            // With the documents the reader read, cited by their pages.
            st.load_library();
            match committee::brief(&st.store, &st.market, st.library.as_ref(), input, today, false) {
                Ok((text, _, _)) => (true, text),
                Err(text) => (false, text),
            }
        }
        "wealth_documents" => {
            st.load_library();
            let out = invest::documents_tool(&mut st.store, &st.market, st.library.as_ref(), input, today);
            changed(st, out)
        }
        "wealth_learn" => {
            st.load_library();
            let out = learn::tool(&mut st.store, &st.market, st.library.as_ref(), input, today);
            changed(st, out)
        }
        "wealth_macro" => (true, engine::economy(&st.market, today)),
        "life_study" => {
            let out = engine::study_view(&mut st.store, &st.market, input, today);
            changed(st, out)
        }
        "life_cards" => {
            let out = engine::cards(&mut st.store, input, today);
            changed(st, out)
        }
        "life_english" => {
            let out = engine::english_view(&mut st.store, input, today);
            changed(st, out)
        }
        "life_gym" => {
            let hours = engine::study_hours_today(&st.store, today);
            let out = engine::gym_view(&mut st.store, input, today, hours);
            changed(st, out)
        }
        "life_day" => (true, day::for_model(&engine::day_plan(&st.store, &st.market, today))),
        "life_content" => (false, "The month's content kit is made by AXYON on the computer, where its files are saved: ask for it there.".into()),
        other => (false, format!("There is no superpower named \"{other}\".")),
    }
}

// ── What to tell ──────────────────────────────────────────────────────────────

/// The most alerts at once, as on the computer.
const MOST_ALERTS: usize = 8;

/// What the session's prices and B3's closes make worth telling now, each once, as the computer's island tells it
/// (src-tauri/src/powers/mod.rs, `tell`): an asset gone under the user's own ceiling or back above it, a strong day
/// for an asset or the whole portfolio, an asset under one of its ceilings by the user's margin. The record of what
/// was told is the store's.
fn tell(st: &mut State, today: Date) -> Vec<Value> {
    let before = (st.store.ceilings.told.clone(), st.store.told_today.clone());
    let opportunities = ceiling::to_tell(&mut st.store, &st.market, today);
    let moments = alerts::to_tell(&mut st.store, &st.market, today);
    if (st.store.ceilings.told.clone(), st.store.told_today.clone()) != before {
        st.save_store();
    }
    let mut out: Vec<(u8, Value)> = Vec::new();
    for m in &moments {
        let (rank, kind) = match m.kind {
            alerts::Kind::IntoZone => (0, "into_zone"),
            alerts::Kind::OutOfZone => (4, "out_of_zone"),
            alerts::Kind::PortfolioUp => (2, "portfolio_up"),
            alerts::Kind::PortfolioDown => (2, "portfolio_down"),
            alerts::Kind::Up => (3, "up"),
            alerts::Kind::Down => (3, "down"),
        };
        out.push((rank, json!({ "kind": kind, "ticker": m.ticker, "price": m.now, "day": m.day, "ceiling": m.ceiling, "money": m.money })));
    }
    for o in &opportunities {
        // An asset just gone under the user's own ceiling is told by that, once.
        if moments.iter().any(|m| m.kind == alerts::Kind::IntoZone && m.ticker == o.ticker) {
            continue;
        }
        out.push((1, json!({ "kind": "opportunity", "ticker": o.ticker, "price": o.price, "ceiling": o.ceiling, "below": o.below, "method": o.method })));
    }
    out.sort_by_key(|(rank, _)| *rank);
    out.into_iter().take(MOST_ALERTS).map(|(_, a)| a).collect()
}

/// What the reader read that matters (a material fact, an assembly, an offering, or a document the brain judged a risk
/// to a thesis) and the theses that broke, each once (coucou_powers::thesis, `to_tell`): `kind` "document" or
/// "thesis", `ticker`, `what` (the document's kind, or the conditions broken), `text`, `key`.
fn notices(st: &mut State, today: Date) -> Vec<Value> {
    st.load_library();
    let lib = st.library.as_ref().expect("loaded above");
    let told = thesis::to_tell(&mut st.store, &st.market, lib, today);
    if !told.is_empty() {
        st.save_store();
    }
    told.into_iter().map(|n| json!({ "kind": n.kind, "ticker": n.ticker, "what": n.what, "text": n.text, "key": n.key })).collect()
}

/// The morning's briefing, once a day (`powers-briefed.txt` keeps the day it was given): the day's items as the
/// computer's island says them (src-tauri/src/powers/mod.rs, `brief`), without the opportunities, told on their own.
fn briefing(st: &State, today: Date) -> Vec<Value> {
    let mark = st.folder.join("powers-briefed.txt");
    if std::fs::read_to_string(&mark).is_ok_and(|d| d.trim() == today.to_string()) {
        return Vec::new();
    }
    let items = engine::day_plan(&st.store, &st.market, today)
        .items
        .into_iter()
        .filter(|i| i.code != "opportunity")
        .map(|i| json!({ "code": i.code, "vars": i.vars, "title": i.title, "urgent": i.urgent }))
        .collect();
    let _ = std::fs::write(mark, today.to_string());
    items
}

/// A look for the app's background refresh: what is due read first (half of `budget`), the session's prices while B3
/// trades (the rest), then what to tell; in the `morning`, the day's briefing too. Nothing is read for a user with
/// nothing held or watched.
fn watch(st: &mut State, today: Date, evening: bool, morning: bool, budget: Duration) -> Value {
    let mut alerts = Vec::new();
    if !st.store.portfolio.positions.is_empty() || !st.store.ceilings.manual.is_empty() || !st.store.theses.is_empty() {
        let deadline = Instant::now() + budget;
        read_due(st, today, evening, budget / 2);
        if session_open() {
            read_live(st, deadline);
        }
        alerts = tell(st, today);
        alerts.extend(notices(st, today));
    }
    let day = if morning { briefing(st, today) } else { Vec::new() };
    json!({ "ok": true, "alerts": alerts, "day": day })
}

/// The largest document file read.
const MOST_DOCUMENT: usize = 40 * 1024 * 1024;

/// A document's file, downloaded with the engine's client (FundosNet's and the CVM's): a PDF saved for the app to read
/// (`{"pdf": path}`, in the folder's documents/), or its text's pages (`{"pages": […]}`).
fn fetch_document(st: &State, input: &Value) -> Value {
    let Some(url) = input["pending"]["download"].as_str().filter(|u| u.starts_with("https://")) else {
        return json!({ "ok": false, "text": "The document has no address to download it from." });
    };
    let key = input["pending"]["key"].as_str().unwrap_or("document");
    let body = match within(Instant::now() + ASSET_BUDGET, http().get(url)) {
        None => return json!({ "ok": false, "text": "The download took too long." }),
        Some(Err(e)) => return json!({ "ok": false, "text": e }),
        Some(Ok(body)) if body.len() > MOST_DOCUMENT => return json!({ "ok": false, "text": "The document is too large." }),
        Some(Ok(body)) => body,
    };
    match library::file_of(&body) {
        Ok(library::File::Pdf(bytes)) => {
            let dir = st.folder.join("documents");
            let _ = std::fs::create_dir_all(&dir);
            let name: String = key.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
            let path = dir.join(format!("{name}.pdf"));
            match std::fs::write(&path, bytes) {
                Ok(()) => json!({ "ok": true, "data": { "pdf": path.to_string_lossy() } }),
                Err(e) => json!({ "ok": false, "text": e.to_string() }),
            }
        }
        Ok(library::File::Text(pages)) => json!({ "ok": true, "data": { "pages": pages } }),
        Err(e) => json!({ "ok": false, "text": e }),
    }
}

/// One of the app's own calls (never Hermes' tools): the investing screen and what it does (coucou_powers::invest,
/// `app`), plus `invest_refresh`, what is due read first (the CVM's lists of documents too), then the screen, and
/// `invest_docs` `fetch`, a document's file downloaded (`fetch_document`). What an
/// answer rests on is read first, as for Hermes' tools. `{"ok": true, "data": …}`, or `{"ok": false, "text": …}`.
fn app(st: &mut State, tool: &str, input: &Value, today: Date, evening: bool, read: bool) -> Value {
    if tool == "invest_docs" && engine::s(input, "do").as_deref() == Some("fetch") {
        return if read { fetch_document(st, input) } else { json!({ "ok": false, "text": "Nothing is downloaded now." }) };
    }
    if read {
        match tool {
            "invest_refresh" => {
                let deadline = Instant::now() + READ_BUDGET;
                read_due(st, today, evening, READ_BUDGET / 2);
                if session_open() && st.live_read.is_none_or(|t| t.elapsed() >= LIVE_EVERY) {
                    read_live(st, Instant::now() + LIVE_BUDGET.min(READ_BUDGET / 4));
                }
                read_company_documents(st, today, deadline);
            }
            "invest_committee" if engine::s(input, "do").as_deref() == Some("brief") => {
                let budget = if needs_reading(&st.store, &st.market) { READ_BUDGET } else { QUICK_BUDGET };
                read_due(st, today, evening, budget);
            }
            _ => {}
        }
        if let Some(asset) = asked_asset(tool, input) {
            ensure_asset(st, &asset, today, evening);
        }
    }
    st.load_library();
    let tool = if tool == "invest_refresh" { "invest_view" } else { tool };
    let lib = st.library.as_mut().expect("loaded above");
    match invest::app(&mut st.store, &st.market, lib, tool, input, today) {
        Ok((data, saved)) => {
            if saved.store {
                st.save_store();
            }
            if saved.library {
                st.save_library();
            }
            json!({ "ok": true, "data": data })
        }
        Err(text) => json!({ "ok": false, "text": text }),
    }
}

/// One call, from the strings the app gives: the answer as JSON (`{"ok": …, "text": …}`; the app's own calls,
/// `invest_…`, `{"ok": …, "data": …}`).
fn answer(tool: &str, input: &str, folder: &str, copy: &str, today: &str, evening: bool, read: bool) -> Value {
    let Some(today) = Date::parse(today.trim()) else {
        return json!({ "ok": false, "text": "The iPhone's date was not given." });
    };
    let input: Value = serde_json::from_str(input).unwrap_or_else(|_| json!({}));
    if folder.trim().is_empty() {
        return json!({ "ok": false, "text": "No folder for the superpowers' files." });
    }
    with_state(folder, copy, |st| {
        if tool.starts_with("invest_") {
            return app(st, tool, &input, today, evening, read);
        }
        let (ok, text) = run(st, tool, &input, today, evening, read);
        json!({ "ok": ok, "text": text })
    })
}

/// `job` on the state of `folder`, with the computer's copy taken first when a newer one came.
fn with_state(folder: &str, copy: &str, job: impl FnOnce(&mut State) -> Value) -> Value {
    let folder = PathBuf::from(folder);
    let mut held = STATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if held.as_ref().is_none_or(|st| st.folder != folder) {
        *held = Some(State::open(&folder));
    }
    let st = held.as_mut().expect("opened above");
    if !copy.trim().is_empty() {
        st.take_copy(Path::new(copy));
    }
    job(st)
}

/// What is worth telling now, from the strings the app gives (`{"ok": …, "alerts": […], "day": […]}`).
fn look(folder: &str, copy: &str, today: &str, evening: bool, morning: bool, budget: Duration) -> Value {
    let Some(today) = Date::parse(today.trim()) else {
        return json!({ "ok": false, "alerts": [], "day": [] });
    };
    if folder.trim().is_empty() {
        return json!({ "ok": false, "alerts": [], "day": [] });
    }
    with_state(folder, copy, |st| watch(st, today, evening, morning, budget))
}

/// One of Hermes' wealth_ or life_ tools: `tool`, its `input` (a JSON object), the folder of the files, the
/// computer's copy of the portfolio (or ""), the iPhone's own date (YYYY-MM-DD) and whether its evening has come
/// (21:00 on: B3's day is published). Blocks while the sources are read (up to a minute). Returns
/// `{"ok": bool, "text": string}` as a string to give back to `axyon_powers_free`.
///
/// # Safety
/// Each pointer is a NUL-terminated UTF-8 string or null.
#[no_mangle]
pub unsafe extern "C" fn axyon_powers_call(
    tool: *const c_char,
    input: *const c_char,
    folder: *const c_char,
    copy: *const c_char,
    today: *const c_char,
    evening: bool,
) -> *mut c_char {
    let owned = |p: *const c_char| if p.is_null() { String::new() } else { CStr::from_ptr(p).to_string_lossy().into_owned() };
    let (tool, input, folder, copy, today) = (owned(tool), owned(input), owned(folder), owned(copy), owned(today));
    let out = std::panic::catch_unwind(|| answer(&tool, &input, &folder, &copy, &today, evening, true))
        .unwrap_or_else(|_| json!({ "ok": false, "text": "The superpowers stopped on an unexpected error." }));
    CString::new(out.to_string()).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
}

/// What is worth telling now, for the app's background refresh (at most `budget_ms` reading the sources):
/// `{"ok": bool, "alerts": [...], "day": [...]}`, each alert's `kind` (into_zone, out_of_zone, up, down, portfolio_up,
/// portfolio_down, opportunity), `ticker` ("" for the portfolio), `price` (for the portfolio, its value), `day`
/// (percent), `ceiling`, `money` (what the user's position made today), `below` (percent) and `method`, each told
/// once; the reader's and the theses' (`notices`: kind document or thesis, with `what`, `text` and `key`), once; in the `morning`, once a day, the day's items (`code`, `vars`, `title`, `urgent`: engine::day_plan). To give
/// back to `axyon_powers_free`.
///
/// # Safety
/// Each pointer is a NUL-terminated UTF-8 string or null.
#[no_mangle]
pub unsafe extern "C" fn axyon_powers_watch(folder: *const c_char, copy: *const c_char, today: *const c_char, evening: bool, morning: bool, budget_ms: u32) -> *mut c_char {
    let owned = |p: *const c_char| if p.is_null() { String::new() } else { CStr::from_ptr(p).to_string_lossy().into_owned() };
    let (folder, copy, today) = (owned(folder), owned(copy), owned(today));
    let budget = Duration::from_millis(u64::from(budget_ms.clamp(1_000, 120_000)));
    let out = std::panic::catch_unwind(|| look(&folder, &copy, &today, evening, morning, budget)).unwrap_or_else(|_| json!({ "ok": false, "alerts": [], "day": [] }));
    CString::new(out.to_string()).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
}

/// Gives back an answer of `axyon_powers_call` or `axyon_powers_watch`.
///
/// # Safety
/// `text` is an answer of `axyon_powers_call` or `axyon_powers_watch`, given back once, or null.
#[no_mangle]
pub unsafe extern "C" fn axyon_powers_free(text: *mut c_char) {
    if !text.is_null() {
        drop(CString::from_raw(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("axyon-powers-ffi-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn position(t: &str, class: AssetClass, quantity: f64) -> Position {
        Position { ticker: t.into(), class, quantity, average_price: 10.0, subtype: String::new(), current_value: None, note: String::new() }
    }

    fn quote(t: &str, close: f64) -> coucou_powers::sources::b3::Quote {
        coucou_powers::sources::b3::Quote {
            ticker: t.into(),
            date: Date::parse("2026-10-26").unwrap(),
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
    fn what_to_tell_comes_once_as_data_for_the_notifications() {
        let dir = folder("tell");
        let today = Date::parse("2026-10-27").unwrap();
        // 27/10 15:16 in Brasília, during the session.
        let at = 1_793_125_002;
        let mut st = State::open(&dir);
        st.store.portfolio.positions = vec![position("GGRC11", AssetClass::Fii, 150.0), position("TIMS3", AssetClass::Acao, 30.0)];
        st.store.ceilings.manual.insert("GGRC11".into(), 8.80);
        for (t, c) in [("GGRC11", 8.85), ("TIMS3", 18.1)] {
            st.market.quotes.insert(t.into(), quote(t, c));
        }
        st.market.keep_live([("GGRC11".to_string(), LivePrice { price: 8.69, at, previous: None }), ("TIMS3".to_string(), LivePrice { price: 18.78, at, previous: None })]);
        let told = tell(&mut st, today);
        let kinds: Vec<&str> = told.iter().map(|a| a["kind"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["into_zone", "up"], "8,85 → 8,69 under the ceiling of 8,80; TIMS3 +3,8 %: {told:?}");
        assert_eq!((told[0]["ticker"].as_str(), told[0]["price"].as_f64(), told[0]["ceiling"].as_f64()), (Some("GGRC11"), Some(8.69), Some(8.80)));
        assert!((told[1]["money"].as_f64().unwrap() - 30.0 * (18.78 - 18.1)).abs() < 1e-6, "what the user's 30 shares made today");
        assert!(tell(&mut st, today).is_empty(), "told once");
        assert!(State::open(&dir).store.told_today.keys.contains(&"into:GGRC11".to_string()), "the record kept: not told again after a relaunch");
        // Nothing held or watched: nothing read, nothing told.
        let mut none = State::open(&folder("tell-none"));
        assert_eq!(watch(&mut none, today, false, false, Duration::from_secs(1)), json!({ "ok": true, "alerts": [], "day": [] }));
        let out = look(folder("tell-none").to_str().unwrap(), "", "2026-10-27", false, false, Duration::from_secs(1));
        assert_eq!(out, json!({ "ok": true, "alerts": [], "day": [] }));
        assert_eq!(look("", "", "2026-10-27", false, false, Duration::from_secs(1))["ok"], json!(false));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_mornings_briefing_comes_once_a_day() {
        let dir = folder("brief");
        let today = Date::parse("2026-10-28").unwrap();
        let mut st = State::open(&dir);
        st.store.english.level = "B1".into();
        st.store.english.minutes_per_day = 15;
        let first = watch(&mut st, today, false, true, Duration::from_secs(1));
        let day = first["day"].as_array().unwrap();
        assert!(day.iter().any(|i| i["code"] == "english"), "the English lesson is in the day: {first}");
        assert!(day.iter().all(|i| i["code"] != "opportunity"), "opportunities are told on their own");
        assert_eq!(watch(&mut st, today, false, true, Duration::from_secs(1))["day"], json!([]), "once a day");
        assert_eq!(watch(&mut st, today.add_days(1), false, false, Duration::from_secs(1))["day"], json!([]), "only in the morning");
        assert!(!watch(&mut st, today.add_days(1), false, true, Duration::from_secs(1))["day"].as_array().unwrap().is_empty(), "the next morning again");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_computers_copy_gives_the_positions_and_ceilings_once_per_copy() {
        let dir = folder("copy");
        let copy = dir.join("portfolio.json");
        std::fs::write(&copy, json!({
            "type": "portfolio", "receivedAt": 1_793_000_000.0, "cash": 150.0, "updated": "2026-10-27", "source": "sheet",
            "positions": [{ "ticker": "GARE11", "class": "fii", "subtype": "", "quantity": 100.0, "averagePrice": 9.0, "currentValue": null, "note": "" }],
            "ceilings": { "TIMS3": 21.5 }, "prices": {}, "notes": []
        }).to_string()).unwrap();
        let mut st = State::open(&dir);
        st.take_copy(&copy);
        assert_eq!(st.store.portfolio.positions.len(), 1);
        assert_eq!(st.store.portfolio.positions[0].class, AssetClass::Fii);
        assert_eq!(st.store.ceilings.manual.get("TIMS3"), Some(&21.5));
        assert_eq!(st.store.portfolio.cash, 150.0);
        assert_eq!(st.store.portfolio.source, COMPUTER);
        // A change on the iPhone stays until a newer copy comes.
        st.store.portfolio.positions.clear();
        st.take_copy(&copy);
        assert!(st.store.portfolio.positions.is_empty());
        let reopened = State::open(&dir);
        assert_eq!(reopened.store.portfolio.positions.len(), 1, "the copy was saved");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_studies_and_english_keep_their_records_on_the_iphone() {
        let dir = folder("life");
        let today = Date::parse("2026-10-28").unwrap();
        let mut st = State::open(&dir);
        let (ok, text) = run(&mut st, "life_english", &json!({ "action": "status" }), today, false, false);
        assert!(!text.is_empty(), "{ok} {text}");
        let (ok, text) = run(&mut st, "life_english", &json!({ "action": "log", "minutes": 15 }), today, false, false);
        assert!(ok && text.starts_with("15 min of English logged"), "{text}");
        assert_eq!(State::open(&dir).store.english.streak_days, 1, "kept in powers.json");
        assert!(std::fs::read_to_string(dir.join("powers.json")).unwrap().contains("\"streakDays\":1"), "as the Simulator test reads it");
        let (ok, text) = run(&mut st, "life_content", &json!({}), today, false, false);
        assert!(!ok && text.contains("computer"));
        let (ok, text) = run(&mut st, "wealth_nothing", &json!({}), today, false, false);
        assert!(!ok && text.contains("no superpower"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_apps_investing_calls_keep_what_the_reader_read_and_tell_its_news_once() {
        let dir = folder("invest");
        let today = Date::parse("2026-10-28").unwrap();
        let mut st = State::open(&dir);
        st.store.portfolio.positions = vec![position("GGRC11", AssetClass::Fii, 150.0)];
        st.market.quotes.insert("GGRC11".into(), quote("GGRC11", 9.1));
        // A material fact the app downloaded and read.
        let pending = json!({ "key": "fnet:905", "ticker": "GGRC11", "kind": "material_fact", "title": "Fato Relevante", "reference": "27/10/2026", "delivered": "2026-10-27", "url": "https://fnet.example/905", "download": "https://fnet.example/905" });
        let out = app(&mut st, "invest_docs", &json!({ "do": "ingest", "pending": pending, "pages": ["O fundo vendeu o imóvel de Campinas."] }), today, false, false);
        assert_eq!(out["ok"], json!(true), "{out}");
        assert_eq!(out["data"]["key"], "fnet:905");
        assert!(dir.join("powers-docs.json").exists(), "the library is saved");
        let told = notices(&mut st, today);
        assert_eq!(told.len(), 1, "{told:?}");
        assert_eq!((told[0]["kind"].as_str(), told[0]["what"].as_str(), told[0]["ticker"].as_str()), (Some("document"), Some("material_fact"), Some("GGRC11")));
        assert!(notices(&mut st, today).is_empty(), "told once");
        // A thesis broken by the price is told once too.
        let out = app(&mut st, "invest_thesis", &json!({ "ticker": "GGRC11", "text": "P/VP abaixo de 0,1" }), today, false, false);
        assert_eq!(out["ok"], json!(true), "{out}");
        // Reopened (the app relaunched): the reader's library is still there, and the screen shows it.
        let shown = answer("invest_view", "{}", dir.to_str().unwrap(), "", "2026-10-28", false, false);
        assert_eq!(shown["ok"], json!(true));
        assert_eq!(shown["data"]["feed"].as_array().map(Vec::len), Some(1), "{shown}");
        let mut again = State::open(&dir);
        again.load_library();
        assert_eq!(again.library.as_ref().map(|l| l.docs.len()), Some(1));
        assert_eq!(answer("invest_nothing", "{}", dir.to_str().unwrap(), "", "2026-10-28", false, false)["ok"], json!(false));
        // Hermes' own tools read the same library.
        let (ok, text) = run(&mut st, "wealth_documents", &json!({}), today, false, false);
        assert!(ok && text.contains("fnet:905"), "{text}");
        let (ok, text) = run(&mut st, "wealth_learn", &json!({ "do": "glossary", "term": "P/VP" }), today, false, false);
        assert!(ok && text.contains("P/VP"), "{text}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_call_through_the_c_interface_answers_json_and_frees() {
        let dir = folder("ffi");
        let c = |s: &str| CString::new(s).unwrap();
        let (tool, input, folder, copy, today) = (c("life_day"), c("{}"), c(dir.to_str().unwrap()), c(""), c("2026-10-28"));
        // life_day reads the market first: in the test, through the same path as the app but without the network.
        let out = answer("life_day", "{}", dir.to_str().unwrap(), "", "2026-10-28", false, false);
        assert_eq!(out["ok"], json!(true));
        unsafe {
            let raw = axyon_powers_call(c("wealth_nothing").as_ptr(), input.as_ptr(), folder.as_ptr(), copy.as_ptr(), today.as_ptr(), false);
            assert!(!raw.is_null());
            let v: Value = serde_json::from_str(CStr::from_ptr(raw).to_str().unwrap()).unwrap();
            assert_eq!(v["ok"], json!(false));
            axyon_powers_free(raw);
            let bad_day = axyon_powers_call(tool.as_ptr(), input.as_ptr(), folder.as_ptr(), copy.as_ptr(), c("today").as_ptr(), false);
            let v: Value = serde_json::from_str(CStr::from_ptr(bad_day).to_str().unwrap()).unwrap();
            assert!(v["text"].as_str().unwrap().contains("date"));
            axyon_powers_free(bad_day);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
