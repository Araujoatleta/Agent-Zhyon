//! The superpowers inside the iPhone (NotchBuddy/Hermes/README.md): the same engine as the computer's (coucou-powers:
//! the portfolio against B3, the CVM, FundosNet and the Banco Central, the ceilings, taxes, the committee, and the
//! studies, English and training), for Hermes inside the iPhone app, behind two C functions the app links in. The app
//! answers Hermes' wealth_ and life_ tools with them (Hermes/Swift/HermesArms.swift, "powers") on a thread of its own:
//! a call may read the official sources first, as on the computer (src-tauri/src/powers/mod.rs, `call`, which this
//! follows, without the sheet and the island).
//!
//! What is kept is the computer's two files, in the folder the app gives: the user's data (powers.json) and what the
//! sources said (powers-market.json). The positions, the cash and the user's own ceilings follow the copy a paired
//! computer gives the iPhone (Hermes' portfolio.json, src-tauri/src/powers/mod.rs `phone_copy`) each time a newer one
//! comes: the computer and its sheet stay the record.

use std::collections::BTreeMap;
use std::ffi::{c_char, CStr, CString};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use coucou_powers::date::{is_business_day, Date};
use coucou_powers::model::{AssetClass, LivePrice, Market, Position, Store};
use coucou_powers::sources::fetch::Http;
use coucou_powers::{day, engine, refresh, sources, text};
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
                st.market.companies.insert(ticker, company);
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
            st.market.companies.insert(ticker.to_string(), company);
        }
    }
    st.save_market();
}

/// The session's prices of what is held and what has a ceiling, from Google Finance, one page after the other.
fn read_live(st: &mut State) {
    let mut tickers = st.store.portfolio.tickers(None);
    for t in st.store.ceilings.manual.keys() {
        if !tickers.contains(t) {
            tickers.push(t.clone());
        }
    }
    tickers.truncate(LIVE_MOST);
    let deadline = Instant::now() + LIVE_BUDGET;
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
    matches!(tool, "wealth_portfolio" | "wealth_fii" | "wealth_events" | "wealth_plan" | "wealth_audit" | "wealth_macro" | "wealth_committee" | "wealth_ceiling" | "life_day")
}

/// An asset the call is about, when it names one.
fn asked_asset(tool: &str, input: &Value) -> Option<String> {
    let named = engine::s(input, "ticker").and_then(|t| text::ticker(&t));
    match tool {
        "wealth_fii" | "wealth_company" | "wealth_ceiling" => named,
        "wealth_committee" => named.or_else(|| engine::s(input, "proposal").and_then(|p| text::tickers_in(&p).into_iter().next())),
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
            read_live(st);
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
        "wealth_committee" => match engine::committee(&st.store, &st.market, input, today) {
            Ok(text) => (true, text),
            Err(text) => (false, text),
        },
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

/// One call, from the strings the app gives: the answer as JSON (`{"ok": …, "text": …}`).
fn answer(tool: &str, input: &str, folder: &str, copy: &str, today: &str, evening: bool, read: bool) -> Value {
    let Some(today) = Date::parse(today.trim()) else {
        return json!({ "ok": false, "text": "The iPhone's date was not given." });
    };
    let input: Value = serde_json::from_str(input).unwrap_or_else(|_| json!({}));
    if folder.trim().is_empty() {
        return json!({ "ok": false, "text": "No folder for the superpowers' files." });
    }
    let folder = PathBuf::from(folder);
    let mut held = STATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if held.as_ref().is_none_or(|st| st.folder != folder) {
        *held = Some(State::open(&folder));
    }
    let st = held.as_mut().expect("opened above");
    if !copy.trim().is_empty() {
        st.take_copy(Path::new(copy));
    }
    let (ok, text) = run(st, tool, &input, today, evening, read);
    json!({ "ok": ok, "text": text })
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

/// Gives back an answer of `axyon_powers_call`.
///
/// # Safety
/// `text` is an answer of `axyon_powers_call`, given back once, or null.
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
