// Zheus' superpowers in the app (coucou-powers does the work; docs/ZHEUS_POWERS.md). The user's own data lives in
// powers.json (Coucou's settings folder: the portfolio, the B3 history, the decisions, the studies, English and
// training) and what the official sources said in powers-market.json (its local folder). While the superpowers are
// on, what is due is read again in the background — B3's quotes once it publishes a day, the funds' documents and
// the Banco Central's figures once a day, the funds' monthly reports once a week, a company's statements once a
// month — and the brain gets them as tools: Mochi's (agent/tools.rs) and Hermes' over MCP (agent/mcp.rs). The island
// shows the portfolio and the day, and says the morning's briefing.
//
// During B3's session the prices come from Google Finance every few minutes, and what they make worth telling (an asset
// under the user's ceiling, a strong day) is told at once on the island. With the user's Google Sheet linked, AXYON
// keeps its own tab there (coucou_powers::workspace, through gog): the user's edits are read back, and AXYON writes
// its figures and every change made from the chat.
//
// Off by default (Settings → Zheus' superpowers). Only B3, the CVM, FundosNet, the Banco Central and Google Finance
// are contacted, for public data, with no account and nothing about the user sent; the user's own sheet only through
// their own gog. Nothing here buys, sells, pays, sends or posts.

mod google;
mod sheet;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use coucou_powers::date::Date;
use coucou_powers::day::{self, Item};
use coucou_powers::engine::{self, Alert, Summary};
use coucou_powers::model::{AssetClass, LivePrice, Market, Position, Store};
use coucou_powers::sheet::SheetLink;
use coucou_powers::sources::{self, fetch::Http};
use coucou_powers::{alerts, import, refresh, workspace};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::agent::{BoxFuture, HomeResult, PowersRunner};
use crate::i18n::{t, tf};
use crate::{island, log, settings, Shared};

/// How often the background reading wakes to see what is due (it reads nothing when nothing is).
const EVERY: Duration = Duration::from_secs(30 * 60);
/// The longest a tool waits for what is due before it answers with what Coucou has (the reading goes on, and the next
/// call has it). With the asset's below, a call stays within a minute, under what an agent waits for a tool.
const READ_BUDGET: Duration = Duration::from_secs(20);
/// The longest a tool waits for an asset the user does not hold: its quote, its fund's reports and notices, or its
/// company's statements.
const ASSET_BUDGET: Duration = Duration::from_secs(40);
/// The session's prices (Google Finance) are read again this often while B3 trades…
const LIVE_EVERY: Duration = Duration::from_secs(5 * 60);
/// …and this often while the ★ tab or Settings shows them.
const LIVE_LOOKED: Duration = Duration::from_secs(60);
/// How long a look at the ★ tab counts as looking.
const LOOK_LASTS: Duration = Duration::from_secs(150);
/// A pause between two of Google's pages: one asset after the other, gently.
const LIVE_PAUSE: Duration = Duration::from_millis(400);
/// The most assets read in one round.
const LIVE_MOST: usize = 40;
/// The longest a tool waits for the session's prices.
const LIVE_BUDGET: Duration = Duration::from_secs(15);
/// The user's sheet is read again this often (and after each reading of the session's prices, so AXYON's columns follow
/// them).
const SHEET_EVERY: Duration = Duration::from_secs(10 * 60);
/// The longest a tool that changes the portfolio waits for the user's latest edits in the sheet…
const SHEET_READ_BUDGET: Duration = Duration::from_secs(15);
/// …and for its change to be written there (the writing goes on after).
const SHEET_BUDGET: Duration = Duration::from_secs(25);
/// What the portfolio's source says when it comes from the sheet.
const SHEET_SOURCE: &str = "Google Sheets";
/// Copies of the user's data kept before a list is replaced.
const BACKUPS: usize = 10;
/// The largest image the island may save into the content folder.
const MAX_IMAGE: usize = 12 * 1024 * 1024;

/// What Coucou holds for the superpowers.
pub struct Powers {
    store: Mutex<Store>,
    market: Mutex<Market>,
    /// One reading of the sources at a time.
    turn: tokio::sync::Mutex<()>,
    http: Http,
    state: Mutex<RefreshState>,
    /// The background reading is running.
    watching: AtomicBool,
    /// The day the morning briefing was said ("2026-10-21").
    briefed: Mutex<String>,
    /// Companies whose statements could not be read today: not asked again before tomorrow.
    failed: Mutex<Vec<(String, Date)>>,
    /// The session's prices: the last reading, one at a time, and the loop that reads them.
    live: Mutex<LiveState>,
    live_read: Mutex<Option<Instant>>,
    live_running: AtomicBool,
    live_watching: AtomicBool,
    /// When the ★ tab or Settings last asked for the view.
    looked_at: Mutex<Option<Instant>>,
    /// The user's sheet: the last round, one at a time.
    sheet: Mutex<SheetState>,
    sheet_read: Mutex<Option<Instant>>,
    sheet_turn: tokio::sync::Mutex<()>,
    /// When the sheet's other tables (the dividends month by month, the targets by asset) were last read.
    extras_read: Mutex<Option<Instant>>,
    /// Counts the changes to the portfolio and the user's ceilings: a reading of the sheet that began before one does
    /// not undo it.
    version: AtomicU64,
}

/// The last reading of the session's prices.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveState {
    /// When it ended ("15:20").
    pub at: Option<String>,
    /// The assets Google Finance gave no price for.
    pub missed: Vec<String>,
}

/// The last round with the user's sheet.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetState {
    pub running: bool,
    /// When it was last read ("15:20").
    pub at: Option<String>,
    /// When AXYON last wrote its tab ("15:20").
    pub written: Option<String>,
    /// The tab, and whether it is AXYON's own (read and written) rather than one of the user's (only read).
    pub tab: String,
    pub ours: bool,
    pub positions: usize,
    /// Assets watched with the user's ceiling, not held.
    pub watched: usize,
    /// Why the last round failed, in the user's language.
    pub problem: Option<String>,
    /// Rows left out, and why.
    pub skipped: Vec<String>,
}

/// The last reading of the official sources.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshState {
    pub running: bool,
    /// When the last reading ended ("14:05"), today.
    pub at: Option<String>,
    /// What could not be read, as the sources said it.
    pub problems: Vec<String>,
}

impl Powers {
    pub fn load() -> Powers {
        Powers {
            store: Mutex::new(read_file(&store_path())),
            market: Mutex::new(read_file(&market_path())),
            turn: tokio::sync::Mutex::new(()),
            http: Http::new(),
            state: Mutex::new(RefreshState::default()),
            watching: AtomicBool::new(false),
            briefed: Mutex::new(String::new()),
            failed: Mutex::new(Vec::new()),
            live: Mutex::new(LiveState::default()),
            live_read: Mutex::new(None),
            live_running: AtomicBool::new(false),
            live_watching: AtomicBool::new(false),
            looked_at: Mutex::new(None),
            sheet: Mutex::new(SheetState::default()),
            sheet_read: Mutex::new(None),
            extras_read: Mutex::new(None),
            sheet_turn: tokio::sync::Mutex::new(()),
            version: AtomicU64::new(0),
        }
    }
}

// ── Files ─────────────────────────────────────────────────────────────────────

fn store_path() -> PathBuf {
    settings::config_dir().join("powers.json")
}

fn market_path() -> PathBuf {
    settings::local_dir().join("powers-market.json")
}

fn backups_dir() -> PathBuf {
    settings::config_dir().join("powers-backups")
}

/// Where the content kits go: Documents\Coucou\Zheus.
pub fn content_root() -> PathBuf {
    let home = crate::platform::home_dir();
    let documents = home.join("Documents");
    if documents.is_dir() { documents } else { home }.join("Coucou").join("Zheus")
}

fn stamp() -> String {
    let t = crate::platform::local_time();
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second)
}

/// A file as written by `write_file`; a missing file is the default, and one that cannot be read is set aside
/// (never overwritten) and the default used.
fn read_file<T: DeserializeOwned + Default>(path: &Path) -> T {
    let Ok(bytes) = std::fs::read(path) else { return T::default() };
    match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(err) => {
            let aside = path.with_extension(format!("unreadable-{}.json", stamp()));
            let _ = std::fs::rename(path, &aside);
            log::line(format!("powers: {} could not be read ({err}), kept as {}", path.display(), aside.display()));
            T::default()
        }
    }
}

/// Written beside, then moved over: a crash never leaves half a file.
fn write_file<T: Serialize>(path: &Path, value: &T, pretty: bool) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let bytes = if pretty { serde_json::to_vec_pretty(value) } else { serde_json::to_vec(value) }.map_err(std::io::Error::other)?;
    let temp = path.with_extension(format!("json.coucou-{}", std::process::id()));
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
}

fn save_store(store: &Store) {
    if let Err(err) = write_file(&store_path(), store, true) {
        log::line(format!("powers: could not save the user's data: {err}"));
    }
}

fn save_market(market: &Market) {
    if let Err(err) = write_file(&market_path(), market, false) {
        log::line(format!("powers: could not save the official data: {err}"));
    }
}

/// A dated copy of the user's data, before a list of theirs is replaced; the newest `BACKUPS` are kept.
fn backup(store: &Store) {
    let dir = backups_dir();
    if let Err(err) = write_file(&dir.join(format!("powers-{}.json", stamp())), store, true) {
        log::line(format!("powers: no copy before the change: {err}"));
        return;
    }
    let mut copies: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| entries.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("powers-"))).collect())
        .unwrap_or_default();
    copies.sort();
    let extra = copies.len().saturating_sub(BACKUPS);
    for old in copies.into_iter().take(extra) {
        let _ = std::fs::remove_file(old);
    }
}

// ── The clock ─────────────────────────────────────────────────────────────────

fn today() -> Date {
    let t = crate::platform::local_time();
    Date::new(t.year as i32, t.month, t.day).unwrap_or_default()
}

/// B3 publishes the day's quotes in the evening.
fn evening() -> bool {
    crate::platform::local_time().hour >= 21
}

fn clock_now() -> String {
    let t = crate::platform::local_time();
    format!("{:02}:{:02}", t.hour, t.minute)
}

/// Now in Brasília (UTC−3, B3's own time): its day and its minute of the day.
fn brasilia() -> (Date, u32) {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) - 3 * 3600;
    (Date::from_days(secs.div_euclid(86_400)), (secs.rem_euclid(86_400) / 60) as u32)
}

/// B3 trades, with a margin for its closing call and Google's 15 minutes: a trading day, 10:00 to 18:30 in Brasília.
fn session_open() -> bool {
    let (day, minute) = brasilia();
    in_session(day, minute)
}

fn in_session(day: Date, minute: u32) -> bool {
    coucou_powers::date::is_business_day(day, true) && (10 * 60..18 * 60 + 30).contains(&minute)
}

fn since(lock: &Mutex<Option<Instant>>, longer_than: Duration) -> bool {
    held(lock).is_none_or(|t| t.elapsed() >= longer_than)
}

// ── Reading the sources ───────────────────────────────────────────────────────

pub fn enabled(app: &AppHandle) -> bool {
    app.state::<Shared>().settings.lock().unwrap().powers_enabled
}

/// The shares whose statements are due (refresh::company_due): those held, those of a thesis, and those the user set
/// a ceiling price for (a unit such as TAEE11 when B3 quotes it as a share).
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

/// Reads what is due (everything that may change when `force`, as the Refresh button asks) and merges it in, one
/// reading at a time. The user's data is never held while waiting on a site.
pub async fn refresh(app: &AppHandle, force: bool) -> RefreshState {
    let powers = app.state::<Powers>();
    let _turn = powers.turn.lock().await;
    let (today, evening) = (today(), evening());
    let (store, market) = (held(&powers.store).clone(), held(&powers.market).clone());
    let due = refresh::due(&store, &market, today, evening, force);
    let companies = companies_due(&store, &market, today, &held(&powers.failed));
    if due.quotes.is_empty() && due.funds.is_empty() && !due.fund_months && !due.economy && companies.is_empty() {
        return held(&powers.state).clone();
    }
    held(&powers.state).running = true;
    changed(app);
    let mut problems = Vec::new();
    if !due.quotes.is_empty() {
        // Every ticker quoted already: only a newer day than the one read is worth a download.
        let have = market.quotes_day.filter(|_| due.quotes.iter().all(|t| market.quotes.contains_key(t)));
        match sources::quotes(&powers.http, today, evening, &due.quotes, have).await {
            Ok((day, quotes)) => {
                let missing: Vec<&str> = due.quotes.iter().filter(|t| !quotes.contains_key(*t) && !market.quotes.contains_key(*t)).map(String::as_str).collect();
                if !quotes.is_empty() && !missing.is_empty() {
                    problems.push(format!("B3 has no quote for {}", missing.join(", ")));
                }
                refresh::merge_quotes(&mut held(&powers.market), day, quotes);
            }
            Err(e) => problems.push(e),
        }
    }
    if due.economy {
        match sources::economy(&powers.http).await {
            Ok(economy) => {
                let mut m = held(&powers.market);
                m.economy = economy;
                m.economy_read = Some(today);
            }
            Err(e) => problems.push(e),
        }
    }
    if due.fund_months || !due.funds.is_empty() {
        // The documents of the funds due; the CVM's months for all of them when those are due.
        let mine = store.portfolio.tickers(Some(AssetClass::Fii));
        let read = refresh::funds(&powers.http, &due.funds, &market.fund_cnpjs, &store.fund_cnpjs, due.fund_months.then_some(mine.as_slice()), &refresh::known_documents(&market), today).await;
        problems.extend(read.errors.iter().cloned());
        refresh::merge_funds(&mut held(&powers.market), read, today);
    }
    for ticker in companies {
        let known = held(&powers.market).company_cnpjs.clone();
        match refresh::company(&powers.http, &ticker, &known, today).await {
            Ok((company, register)) => {
                let mut m = held(&powers.market);
                if let Some(register) = register {
                    m.company_cnpjs.extend(register);
                }
                coucou_powers::refresh::merge_company(&mut m, ticker, company);
            }
            Err(e) => {
                held(&powers.failed).push((ticker.clone(), today));
                problems.push(format!("{ticker}: {e}"));
            }
        }
    }
    {
        let mut store = held(&powers.store);
        let market = held(&powers.market);
        let fixed = refresh::fix_classes(&mut store, &market);
        if !fixed.is_empty() {
            log::line(format!("powers: classes set from the sources for {}", fixed.join(", ")));
        }
        engine::snapshot(&mut store, &market, today);
        save_store(&store);
        save_market(&market);
    }
    let state = RefreshState { running: false, at: Some(clock_now()), problems };
    log::line(format!("powers: sources read{}", if state.problems.is_empty() { String::new() } else { format!(", {} problem(s)", state.problems.len()) }));
    *held(&powers.state) = state.clone();
    changed(app);
    state
}

/// `job` on a thread of its own rather than on the async threads the whole app shares (the hooks' and AXYON Core's
/// pipe, the island, the phone): what B3, the CVM and Google send is unpacked and read there, which can take a while,
/// and a call from AXYON Core is still taken at once meanwhile.
fn apart<T: Send + 'static>(job: impl std::future::Future<Output = T> + Send + 'static) -> tauri::async_runtime::JoinHandle<T> {
    tauri::async_runtime::spawn_blocking(move || tauri::async_runtime::block_on(job))
}

/// What is due, read in the background; waits for it `budget` at most (the reading goes on after).
async fn fresh(app: &AppHandle, budget: Duration) {
    let reading = app.clone();
    let job = apart(async move {
        refresh(&reading, false).await;
    });
    let _ = tokio::time::timeout(budget, job).await;
}

/// Whether an answer about the portfolio would lack what it rests on — a price for an asset held that B3 quotes, or
/// the economy's figures —, so that it waits for the reading a while; else it answers at once and the reading goes on.
fn needs_reading(store: &Store, market: &Market) -> bool {
    store.portfolio.tickers(None).iter().any(|t| market.price(t).is_none()) || market.economy_read.is_none()
}

/// An asset the user asks about and may not hold: its last close, and its fund's reports and documents or its
/// company's statements. Waits `ASSET_BUDGET` at most; what was read is kept.
async fn ensure_asset(app: &AppHandle, ticker: &str) {
    let reading = app.clone();
    let ticker = ticker.to_string();
    let job = apart(async move {
        let powers = reading.state::<Powers>();
        let _turn = powers.turn.lock().await;
        let today = today();
        let market = held(&powers.market).clone();
        if !market.quotes.contains_key(&ticker) {
            if let Ok((day, quotes)) = sources::quotes(&powers.http, today, evening(), std::slice::from_ref(&ticker), None).await {
                refresh::merge_quotes(&mut held(&powers.market), day, quotes);
            }
        }
        let kind = held(&powers.market).quotes.get(&ticker).map(|q| sources::b3::kind_of(q)).unwrap_or("");
        let have = market.funds.get(&ticker);
        if ticker.ends_with("11") && kind != "etf" && kind != "acao" {
            // A fund: its months come from the CVM's yearly reports (read for the user's own funds too, as the files
            // are the same), its documents and dividends from FundosNet. Once a day, or at once under a CNPJ the user
            // just gave.
            let store = held(&powers.store).clone();
            let given = refresh::cnpj_changed(&store, &market, &ticker);
            let read_today = !given && have.and_then(|f| f.checked) == Some(today);
            let months = given || (!read_today && have.is_none_or(|f| f.months.is_empty()));
            if months || !read_today {
                let mut all = vec![ticker.clone()];
                all.extend(store.portfolio.tickers(Some(AssetClass::Fii)).into_iter().filter(|t| *t != ticker));
                let read = refresh::funds(&powers.http, std::slice::from_ref(&ticker), &market.fund_cnpjs, &store.fund_cnpjs, months.then_some(all.as_slice()), &refresh::known_documents(&market), today).await;
                refresh::merge_funds(&mut held(&powers.market), read, today);
            }
        } else if kind != "etf" && kind != "bdr" && refresh::company_due(market.companies.get(&ticker), today) {
            if let Ok((company, register)) = refresh::company(&powers.http, &ticker, &market.company_cnpjs, today).await {
                let mut m = held(&powers.market);
                if let Some(register) = register {
                    m.company_cnpjs.extend(register);
                }
                coucou_powers::refresh::merge_company(&mut m, ticker.clone(), company);
            }
        }
        save_market(&held(&powers.market));
    });
    let _ = tokio::time::timeout(ASSET_BUDGET, job).await;
}

/// The background reading, while the superpowers are on: what is due every half hour (nothing is read when nothing
/// is), and the morning's briefing once a day.
pub fn start(app: &AppHandle) {
    if !enabled(app) || app.state::<Powers>().watching.swap(true, Ordering::SeqCst) {
        return;
    }
    let app_for_live = app.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            if !enabled(&app) {
                app.state::<Powers>().watching.store(false, Ordering::SeqCst);
                log::line("powers: resting");
                return;
            }
            // Each round in a task of its own: one that stops on something unexpected (written down by crash.rs)
            // ends that round only, and the next comes half an hour later as usual.
            let round = app.clone();
            if apart(async move {
                refresh(&round, false).await;
                brief(&round);
                tell(&round);
            })
            .await
            .is_err()
            {
                held(&app.state::<Powers>().state).running = false;
                log::line("powers: a reading stopped on a panic; the next one is in half an hour");
            }
            tokio::time::sleep(EVERY).await;
        }
    });
    log::line("powers: reading the official data while on");
    start_live(&app_for_live);
}

// ── The session's prices and the user's sheet ─────────────────────────────────

/// The assets priced during the session: those held that B3 quotes, and those the user set a ceiling for.
fn live_tickers(store: &Store) -> Vec<String> {
    let mut out = store.portfolio.tickers(None);
    for t in store.ceilings.manual.keys() {
        if !out.contains(t) {
            out.push(t.clone());
        }
    }
    out.truncate(LIVE_MOST);
    out
}

/// The session's prices of the assets, from Google Finance, one page after the other; kept, shown, and the
/// opportunities they make told. One reading at a time.
async fn read_live(app: &AppHandle) {
    let powers = app.state::<Powers>();
    if powers.live_running.swap(true, Ordering::SeqCst) {
        return;
    }
    let tickers = read(app, |s, _| live_tickers(s));
    let mut got = Vec::new();
    let mut missed = Vec::new();
    for (i, ticker) in tickers.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(LIVE_PAUSE).await;
        }
        match sources::live_price(&powers.http, ticker).await {
            Ok(live) => got.push((ticker.clone(), LivePrice { price: live.price, at: live.at, previous: live.previous_close })),
            Err(_) => missed.push(ticker.clone()),
        }
    }
    if !got.is_empty() {
        let mut market = held(&powers.market);
        market.keep_live(got.iter().cloned());
        save_market(&market);
    }
    if !missed.is_empty() {
        log::line(format!("powers: Google Finance gave no price for {}", missed.join(", ")));
    }
    *held(&powers.live) = LiveState { at: Some(clock_now()), missed };
    *held(&powers.live_read) = Some(Instant::now());
    powers.live_running.store(false, Ordering::SeqCst);
    changed(app);
    if !got.is_empty() {
        tell(app);
    }
}

/// What a round with the sheet did.
#[derive(Debug, Clone, Default)]
pub struct Round {
    /// AXYON's own tab, read and written; else a tab of the user's, only read.
    pub ours: bool,
    /// AXYON wrote cells of it.
    pub wrote: bool,
    /// AXYON made the tab.
    pub made: bool,
    pub skipped: Vec<String>,
}

/// AXYON's tab in the user's spreadsheet, one round at a time (coucou_powers::workspace): read back, the user's edits
/// taken in and AXYON's columns and summary written where they changed; or, after a change from the chat (`write`, or
/// one not written yet), written from the user's data. AXYON's tab is made when missing; a tab of the user's is only
/// read. None when no sheet is linked.
async fn sync_sheet(app: &AppHandle, write: bool) -> Option<Result<Round, String>> {
    let powers = app.state::<Powers>();
    if read(app, |s, _| s.sheet.id.trim().is_empty()) {
        return None;
    }
    let _turn = powers.sheet_turn.lock().await;
    // The link, as it is once the round before is over.
    let link = read(app, |s, _| s.sheet.clone());
    if link.id.trim().is_empty() {
        return None;
    }
    held(&powers.sheet).running = true;
    changed(app);
    let result = sheet_round(app, &link, write).await;
    if result.is_ok() && since(&powers.extras_read, EXTRAS_EVERY) {
        sheet_extras(app, &link).await;
    }
    let now = clock_now();
    let (positions, watched) = read(app, |s, _| (s.portfolio.positions.len(), s.ceilings.manual.keys().filter(|t| s.portfolio.position(t).is_none()).count()));
    {
        let mut state = held(&powers.sheet);
        let before = state.clone();
        *state = match &result {
            Ok(round) => SheetState {
                running: false,
                at: Some(now.clone()),
                written: if round.wrote { Some(now) } else { before.written },
                tab: sheet::tab_of(&link),
                ours: round.ours,
                positions,
                watched,
                problem: None,
                skipped: round.skipped.clone(),
            },
            Err(e) => SheetState { running: false, problem: Some(e.clone()), ..before },
        };
    }
    match &result {
        Ok(round) if round.made => log::line(format!("powers: AXYON's tab made in the user's sheet ({positions} positions)")),
        Ok(round) if round.wrote => log::line("powers: AXYON's tab written"),
        Ok(_) => {}
        Err(e) => log::line(format!("powers: the sheet: {e}")),
    }
    *held(&powers.sheet_read) = Some(Instant::now());
    changed(app);
    Some(result)
}

/// "27/10/2026 15:20": when AXYON wrote its tab, for the tab.
fn written_stamp() -> String {
    let t = crate::platform::local_time();
    format!("{:02}/{:02}/{:04} {:02}:{:02}", t.day, t.month, t.year, t.hour, t.minute)
}

async fn sheet_round(app: &AppHandle, link: &SheetLink, write: bool) -> Result<Round, String> {
    let powers = app.state::<Powers>();
    let tab = sheet::tab_of(link);
    let version = powers.version.load(Ordering::SeqCst);
    let (pending, known) = read(app, |s, _| {
        let same = s.sheet_sync.id == link.id;
        (same && s.sheet_sync.pending, if same { s.sheet_sync.known.clone() } else { Vec::new() })
    });
    let write = write || pending;
    let mut made = false;
    let rows = match sheet::get(link).await.map_err(|e| tf("The sheet could not be read: {error}", &[("error", &e)]))? {
        sheet::Got::Rows(rows) => rows,
        sheet::Got::NoTab if tab == workspace::TAB => {
            sheet::add_tab(link, &tab).await.map_err(|e| tf("AXYON's tab could not be made in the sheet: {error}", &[("error", &e)]))?;
            made = true;
            Vec::new()
        }
        sheet::Got::NoTab => return Err(tf("The spreadsheet has no tab named {tab}.", &[("tab", &tab)])),
    };
    let blank = workspace::is_blank(&rows);
    if !workspace::is_ours(&rows) && !(blank && tab == workspace::TAB) {
        return read_only(app, link, &rows, write, version);
    }
    let found = if blank { None } else { Some(sheet::positions(&rows)?) };
    // A tab with nothing in it is AXYON's to fill: the user's own tabs are read for it once, their ceilings and the
    // assets they list taken in, and what does not agree noted on its row.
    let mut made_notes = Vec::new();
    if blank {
        if let Some(theirs) = their_tabs(link, &tab).await {
            made_notes = take_theirs(app, &theirs, version);
        }
    }
    // The user's edits taken in, unless a change from the chat waits to be written (the reading would undo it).
    let taken = !write && found.as_ref().is_some_and(|f| workspace::has_assets(f) && take_in(app, f, true, version));
    let today = today();
    let order = found.as_ref().map(|f| f.order.clone()).unwrap_or_default();
    let stamp = written_stamp();
    let (mut table, summary, computed_at, old_notes) = read(app, |s, m| {
        (
            workspace::table(s, m, today, &order, &workspace::aviso),
            workspace::summary_rows(s, m, today, &stamp),
            powers.version.load(Ordering::SeqCst),
            if s.sheet_sync.id == link.id { s.sheet_sync.notes.clone() } else { Vec::new() },
        )
    });
    let mut notes = workspace::notes_kept(&old_notes, &table, today);
    notes.extend(workspace::notes_for(&table, &made_notes, today));
    workspace::with_notes(&mut table, &notes);
    let writes = if blank {
        workspace::seed(&table, &summary)
    } else if taken {
        workspace::after_reading(&rows, &table, &summary)
    } else {
        workspace::after_change(&rows, &table, &summary, &known)
    };
    for w in &writes {
        sheet::update(link, &sheet::in_tab(&tab, &w.cells), &w.values).await.map_err(|e| tf("The sheet could not be written: {error}", &[("error", &e)]))?;
    }
    // What the tab has now; nothing waits to be written, unless a change came meanwhile.
    {
        let mut store = held(&powers.store);
        let since = powers.version.load(Ordering::SeqCst) != computed_at;
        let sync = workspace::Sync { id: link.id.clone(), pending: since && store.sheet_sync.pending, known: table.tickers.clone(), notes: notes.clone() };
        if store.sheet_sync != sync {
            store.sheet_sync = sync;
            save_store(&store);
        }
    }
    if blank {
        tell_sheet_ready(app, &notes);
    }
    Ok(Round { ours: true, wrote: !writes.is_empty(), made, skipped: found.map(|f| f.skipped).unwrap_or_default() })
}

/// How often the sheet's other tables are read.
const EXTRAS_EVERY: Duration = Duration::from_secs(3 * 3600);

/// The sheet's tables beside the positions, every three hours: the dividends month by month (a "Proventos pagos"
/// table, in whichever tab has it: the record of what the user received) and the targets by asset (a "% Ideal" or
/// "Meta" column), from every tab (twelve at most). Kept with the user's data; what a tab could not give leaves what
/// was there.
async fn sheet_extras(app: &AppHandle, link: &SheetLink) {
    *held(&app.state::<Powers>().extras_read) = Some(Instant::now());
    let names = match sheet::tabs(link).await {
        Ok(names) => names,
        Err(e) => {
            log::line(format!("powers: the sheet's tabs could not be listed for its dividends: {e}"));
            return;
        }
    };
    let mut dividends: Option<(String, coucou_powers::sheet::SheetDividends)> = None;
    let mut targets: Vec<(String, f64)> = Vec::new();
    for name in names.iter().take(12) {
        let Ok(sheet::Got::Rows(rows)) = sheet::get_tab(link, name).await else { continue };
        if dividends.is_none() {
            dividends = coucou_powers::sheet::dividends(&rows).map(|d| (name.clone(), d));
        }
        if let Ok(found) = coucou_powers::sheet::positions(&rows) {
            for (t, x) in found.targets {
                if !targets.iter().any(|(y, _)| *y == t) {
                    targets.push((t, x));
                }
            }
        }
    }
    let today = today();
    let found = (dividends.as_ref().map(|d| d.1.months.len()).unwrap_or(0), targets.len());
    change(app, |s, _| {
        if let Some((tab, d)) = dividends {
            s.recorded_dividends = coucou_powers::model::RecordedDividends { source: format!("Sua planilha — aba {tab}"), read: Some(today), months: d.months, by_asset: d.by_asset };
        }
        for (t, x) in targets {
            s.portfolio.asset_targets.insert(t, x);
        }
    });
    log::line(format!("powers: the sheet gives {} month(s) of dividends and {} target(s) by asset", found.0, found.1));
}

/// The user's other tabs (ten at most), read once when AXYON makes its own: what they list, the positions they give
/// with a quantity, their ceilings. None when they could not be listed.
async fn their_tabs(link: &SheetLink, ours: &str) -> Option<workspace::Theirs> {
    let names = match sheet::tabs(link).await {
        Ok(names) => names,
        Err(e) => {
            log::line(format!("powers: the sheet's tabs could not be listed: {e}"));
            return None;
        }
    };
    let mut theirs = workspace::Theirs::default();
    for name in names.iter().filter(|n| !n.eq_ignore_ascii_case(ours)).take(10) {
        if let Ok(sheet::Got::Rows(rows)) = sheet::get_tab(link, name).await {
            if let Ok(found) = coucou_powers::sheet::positions(&rows) {
                theirs.add(&found);
            }
        }
    }
    log::line(format!("powers: the user's tabs list {} asset(s), {} ceiling(s)", theirs.listed.len(), theirs.ceilings.len()));
    Some(theirs)
}

/// The user's tabs taken into their data (a copy kept first when it changes it), unless the data changed since the
/// round began: the notes to put on the rows.
fn take_theirs(app: &AppHandle, theirs: &workspace::Theirs, version: u64) -> Vec<(String, &'static str)> {
    let powers = app.state::<Powers>();
    let mut store = held(&powers.store);
    if powers.version.load(Ordering::SeqCst) != version {
        return Vec::new();
    }
    let before = store.clone();
    let notes = workspace::take_theirs(&mut store, theirs);
    if *store != before {
        backup(&before);
        refresh::fix_classes(&mut store, &held(&powers.market));
        save_store(&store);
    }
    notes
}

/// AXYON's tab is ready in the user's sheet: said on the island, with what to look at in it.
fn tell_sheet_ready(app: &AppHandle, notes: &[workspace::Note]) {
    let text = if notes.is_empty() {
        t("Your portfolio is there with the price now, the result and the ceilings: edit it there or tell me in the chat.")
    } else {
        tf("Look at its rows: {list}.", &[("list", &notes_list(notes))])
    };
    let alert = PowersAlert { kind: "sheet".into(), ticker: String::new(), title: t("AXYON's tab is ready in your sheet"), text, tone: "info".into(), at: None };
    let _ = app.emit_to(island::WINDOW_LABEL, "powers-alert", [alert]);
}

/// "TIMS3 (not in your other tabs), KNSC11 (no quantity)": the notes, in the user's language.
fn notes_list(notes: &[workspace::Note]) -> String {
    notes
        .iter()
        .map(|n| {
            let why = if n.text == workspace::NOTE_NO_QUANTITY { t("no quantity") } else { t("not in your other tabs") };
            format!("{} ({why})", n.ticker)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A tab of the user's: only read, its positions and ceilings taken in, but not right after a change from the chat
/// (that would undo it at once; the next reading does). Nothing is written there.
fn read_only(app: &AppHandle, link: &SheetLink, rows: &[Vec<Value>], write: bool, version: u64) -> Result<Round, String> {
    let found = sheet::positions(rows)?;
    if !workspace::has_assets(&found) {
        return Err(t("The sheet has no position: a row needs a B3 ticker and a quantity."));
    }
    if !write {
        take_in(app, &found, false, version);
    }
    let powers = app.state::<Powers>();
    let mut store = held(&powers.store);
    let sync = workspace::Sync { id: link.id.clone(), ..workspace::Sync::default() };
    if store.sheet_sync != sync {
        store.sheet_sync = sync;
        save_store(&store);
    }
    Ok(Round { ours: false, wrote: false, made: false, skipped: found.skipped })
}

/// A reading of the sheet taken into the user's data (a copy kept first when it changes it), unless the data changed
/// since the round began (`version`): a change from the chat, to be written instead. Whether it was taken in.
fn take_in(app: &AppHandle, found: &coucou_powers::sheet::Read, whole: bool, version: u64) -> bool {
    let powers = app.state::<Powers>();
    let mut store = held(&powers.store);
    if powers.version.load(Ordering::SeqCst) != version {
        return false;
    }
    let before = store.clone();
    if workspace::merge(&mut store, found, whole) {
        backup(&before);
        store.portfolio.source = SHEET_SOURCE.into();
        store.portfolio.updated = Some(today());
        refresh::fix_classes(&mut store, &held(&powers.market));
        save_store(&store);
        log::line(format!("powers: the user's sheet changed the portfolio ({} positions, {} ceilings of theirs)", store.portfolio.positions.len(), store.ceilings.manual.len()));
    }
    true
}

/// What a round with the sheet, waited for a while, came to.
#[derive(Debug, Clone)]
pub enum SheetOutcome {
    NotLinked,
    Done(Round),
    Failed(String),
    /// Still going when the wait was over (it goes on).
    Going,
}

/// A round with the sheet in the background, waited for `budget` at most.
async fn sheet_now(app: &AppHandle, write: bool, budget: Duration) -> SheetOutcome {
    if read(app, |s, _| s.sheet.id.trim().is_empty()) {
        return SheetOutcome::NotLinked;
    }
    let reading = app.clone();
    let job = tauri::async_runtime::spawn(async move { sync_sheet(&reading, write).await });
    match tokio::time::timeout(budget, job).await {
        Ok(Ok(Some(Ok(round)))) => SheetOutcome::Done(round),
        Ok(Ok(Some(Err(e)))) => SheetOutcome::Failed(e),
        Ok(Ok(None)) => SheetOutcome::NotLinked,
        Ok(Err(e)) => {
            // A round that stopped on a panic (written down by crash.rs) is not still reading.
            held(&app.state::<Powers>().sheet).running = false;
            SheetOutcome::Failed(e.to_string())
        }
        Err(_) => SheetOutcome::Going,
    }
}

/// A round with the sheet in the background, not waited for.
fn sheet_later(app: &AppHandle, write: bool) {
    let reading = app.clone();
    tauri::async_runtime::spawn(async move {
        sync_sheet(&reading, write).await;
    });
}

/// While the superpowers are on: the user's sheet every ten minutes, and during B3's session the prices every five
/// minutes (every minute while the ★ tab or Settings shows them), the sheet after them. Outside the session nothing is
/// asked of Google Finance.
fn start_live(app: &AppHandle) {
    if app.state::<Powers>().live_watching.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(8)).await;
        loop {
            if !enabled(&app) {
                app.state::<Powers>().live_watching.store(false, Ordering::SeqCst);
                return;
            }
            let powers = app.state::<Powers>();
            let looked = !since(&powers.looked_at, LOOK_LASTS);
            let round = app.clone();
            let live_due = session_open() && since(&powers.live_read, if looked { LIVE_LOOKED } else { LIVE_EVERY } - Duration::from_secs(5));
            // During the session, AXYON's columns follow the prices, every five minutes at most.
            let sheet_due = since(&powers.sheet_read, SHEET_EVERY) || (live_due && since(&powers.sheet_read, LIVE_EVERY - Duration::from_secs(5)));
            if (sheet_due || live_due)
                && apart(async move {
                    if live_due {
                        read_live(&round).await;
                    }
                    if sheet_due {
                        sync_sheet(&round, false).await;
                    }
                })
                .await
                .is_err()
            {
                let powers = app.state::<Powers>();
                powers.live_running.store(false, Ordering::SeqCst);
                held(&powers.sheet).running = false;
                log::line("powers: a reading of the session's prices stopped on a panic");
            }
            tokio::time::sleep(Duration::from_secs(20)).await;
        }
    });
}

/// One thing AXYON tells about the portfolio on the island (src/island/powers.ts): its headline and what follows, in
/// the user's language, the asset, and its tone (its colour, AXYON's face and sound).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PowersAlert {
    /// "opportunity", "into_zone", "out_of_zone", "up", "down", "portfolio_up", "portfolio_down", or "sheet" (AXYON's
    /// tab made in the user's sheet).
    pub kind: String,
    pub ticker: String,
    pub title: String,
    pub text: String,
    /// "good", "bad" or "info".
    pub tone: String,
    /// When the price was, in Brasília ("15:16"), when it is the session's.
    pub at: Option<String>,
}

/// The most told at once.
const MOST_ALERTS: usize = 8;

/// "+R$ 20,40", "−R$ 45,00".
fn signed_money(v: f64) -> String {
    format!("{}{}", if v < 0.0 { "−" } else { "+" }, money(v.abs()))
}

/// The alerts of the moments and opportunities found, the most important first: an asset going under the user's own
/// ceiling, an opportunity under a ceiling, the portfolio's strong day, an asset's, an asset back above the ceiling.
fn alerts_of(moments: &[alerts::Moment], opportunities: &[coucou_powers::ceiling::Opportunity], market: &Market) -> Vec<PowersAlert> {
    let clock = |t: &str| market.live_now(t).map(|l| coucou_powers::portfolio::brasilia_clock(l.at));
    let mut out: Vec<(u8, PowersAlert)> = Vec::new();
    for m in moments {
        let pct = percent_text(m.day.abs());
        let price = money(m.now);
        let ceiling = money(m.ceiling.unwrap_or_default());
        let against_ceiling = tf("{price} now · your ceiling {ceiling}", &[("price", &price), ("ceiling", &ceiling)]);
        let day_text = match m.money {
            Some(made) => tf("{price} now · your position {money} today", &[("price", &price), ("money", &signed_money(made))]),
            None => tf("{price} now", &[("price", &price)]),
        };
        let whole = tf("{value} now · {money} today", &[("value", &price), ("money", &signed_money(m.money.unwrap_or_default()))]);
        let who = [("ticker", m.ticker.as_str()), ("pct", pct.as_str())];
        let (rank, kind, tone, title, text) = match m.kind {
            alerts::Kind::IntoZone => (0, "into_zone", "good", tf("{ticker} went below your ceiling", &who), against_ceiling),
            alerts::Kind::OutOfZone => (4, "out_of_zone", "info", tf("{ticker} went back above your ceiling", &who), against_ceiling),
            alerts::Kind::PortfolioUp => (2, "portfolio_up", "good", tf("Your portfolio is up {pct}% today", &who), whole),
            alerts::Kind::PortfolioDown => (2, "portfolio_down", "bad", tf("Your portfolio is down {pct}% today", &who), whole),
            alerts::Kind::Up => (3, "up", "good", tf("{ticker} up {pct}% today", &who), day_text),
            alerts::Kind::Down => (3, "down", "bad", tf("{ticker} down {pct}% today", &who), day_text),
        };
        let at = if m.ticker.is_empty() { market.live.values().map(|l| l.at).max().map(coucou_powers::portfolio::brasilia_clock) } else { clock(&m.ticker) };
        out.push((rank, PowersAlert { kind: kind.into(), ticker: m.ticker.clone(), title, text, tone: tone.into(), at }));
    }
    for o in opportunities {
        // An asset just gone under the user's own ceiling is told by that, once.
        if moments.iter().any(|m| m.kind == alerts::Kind::IntoZone && m.ticker == o.ticker) {
            continue;
        }
        out.push((
            1,
            PowersAlert {
                kind: "opportunity".into(),
                ticker: o.ticker.clone(),
                title: tf("{ticker} is {pct}% below its ceiling", &[("ticker", &o.ticker), ("pct", &percent_text(o.below))]),
                text: tf("{price} now · {method}: {ceiling}", &[("price", &money(o.price)), ("method", &method_name(o.method)), ("ceiling", &money(o.ceiling))]),
                tone: "good".into(),
                at: clock(&o.ticker),
            },
        ));
    }
    out.sort_by_key(|(rank, _)| *rank);
    out.into_iter().take(MOST_ALERTS).map(|(_, a)| a).collect()
}

/// What the session's prices (and B3's closes) just made worth telling: told on the island at once (src/island/
/// powers.ts: the ★ tab opens on it, with AXYON's face and sound, in the chat, and aloud when the user asked for it).
/// The record of what was told is saved only when it changed.
fn tell(app: &AppHandle) {
    let today = today();
    let found = {
        let powers = app.state::<Powers>();
        let mut store = held(&powers.store);
        let market = held(&powers.market);
        let before = (store.ceilings.told.clone(), store.told_today.clone());
        let opportunities = coucou_powers::ceiling::to_tell(&mut store, &market, today);
        let moments = alerts::to_tell(&mut store, &market, today);
        if (store.ceilings.told.clone(), store.told_today.clone()) != before {
            save_store(&store);
        }
        alerts_of(&moments, &opportunities, &market)
    };
    if found.is_empty() {
        return;
    }
    log::line(format!("powers: {} alert(s) told", found.len()));
    let _ = app.emit_to(island::WINDOW_LABEL, "powers-alert", &found);
}

/// A ceiling method's name, in the user's language.
fn method_name(method: &str) -> String {
    match method {
        "bazin" => "Bazin".into(),
        "bazin_projected" => t("projected Bazin"),
        "graham" => "Graham".into(),
        "lynch" => "Peter Lynch".into(),
        "pvp" => "P/VP".into(),
        _ => t("your own ceiling"),
    }
}

/// "10,5" (the user's decimal separator), for a percentage the text puts its sign after.
fn percent_text(v: f64) -> String {
    coucou_powers::text::grouped(v, 1)
}

/// The morning's briefing on the island (and aloud when the user asked for it): once a day, between 7 and 11.
fn brief(app: &AppHandle) {
    let now = crate::platform::local_time();
    if !(7..11).contains(&now.hour) {
        return;
    }
    let day_key = today().to_string();
    let powers = app.state::<Powers>();
    if *held(&powers.briefed) == day_key {
        return;
    }
    // Opportunities are told on their own (tell_opportunities), not said twice.
    let lines = read(app, |s, m| engine::day_plan(s, m, today()).items.iter().filter(|i| i.code != "opportunity").map(line).collect::<Vec<_>>());
    *held(&powers.briefed) = day_key;
    if lines.is_empty() {
        return;
    }
    crate::zheus_one::say(app, &format!("{}\n• {}", t("Good morning! Today:"), lines.join("\n• ")));
}

// ── The user's data, briefly held ─────────────────────────────────────────────

/// One of the superpowers' locks, taken even if a panic elsewhere left it poisoned: what it holds is plain data, still
/// whole, and a refused lock would only turn one stopped task into many.
fn held<T>(lock: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn read<R>(app: &AppHandle, f: impl FnOnce(&Store, &Market) -> R) -> R {
    let powers = app.state::<Powers>();
    let store = held(&powers.store);
    let market = held(&powers.market);
    f(&store, &market)
}

/// The part of the user's data AXYON's tab holds: the positions and the user's own ceilings.
fn record(store: &Store) -> (Vec<Position>, BTreeMap<String, f64>) {
    (store.portfolio.positions.clone(), store.ceilings.manual.clone())
}

/// A change to the user's data, saved at once. One to what AXYON's tab holds is counted (a reading of the sheet begun
/// before it does not undo it) and, with a sheet linked, waits to be written there (sync_sheet).
fn change<R>(app: &AppHandle, f: impl FnOnce(&mut Store, &Market) -> R) -> R {
    let powers = app.state::<Powers>();
    let mut store = held(&powers.store);
    let market = held(&powers.market);
    let before = record(&store);
    let out = f(&mut store, &market);
    if record(&store) != before {
        powers.version.fetch_add(1, Ordering::SeqCst);
        if !store.sheet.id.trim().is_empty() {
            if store.sheet_sync.id != store.sheet.id {
                store.sheet_sync = workspace::Sync { id: store.sheet.id.clone(), ..workspace::Sync::default() };
            }
            store.sheet_sync.pending = true;
        }
    }
    save_store(&store);
    out
}

// ── The tools ─────────────────────────────────────────────────────────────────

/// The superpowers for one turn of the brain, while they are on.
pub fn runner(app: &AppHandle) -> Option<Arc<dyn PowersRunner>> {
    enabled(app).then(|| Arc::new(AppPowers { app: app.clone() }) as Arc<dyn PowersRunner>)
}

struct AppPowers {
    app: AppHandle,
}

impl PowersRunner for AppPowers {
    fn call<'a>(&'a self, tool: &'a str, input: &'a Value) -> BoxFuture<'a, HomeResult> {
        Box::pin(call(&self.app, tool, input))
    }
}

fn answer(text: String) -> HomeResult {
    HomeResult { ok: true, text, detail: String::new() }
}

fn refused(text: String) -> HomeResult {
    HomeResult { ok: false, detail: coucou_powers::text::clip(&text, 120), text }
}

/// Where the positions come from and how old they are, and what AXYON's tab asks the user to check: for the brain, so
/// that an old list is not taken for today's portfolio.
fn record_note(s: &Store) -> String {
    let source = match s.portfolio.source.as_str() {
        SHEET_SOURCE => "read from the AXYON tab of the user's Google Sheet",
        "paste" | "text" => "pasted by the user",
        "b3" => "imported from B3's file",
        "chat" => "told in the chat",
        _ => "saved in AXYON",
    };
    let when = s.portfolio.updated.map(|d| format!(", last changed on {}", d.br())).unwrap_or_default();
    let mut out = format!("\n\nPositions {source}{when}. If the user no longer holds one of them, or holds one not listed, change it with them (wealth_trade for a buy or a sale; wealth_update with remove for one gone).");
    if !s.sheet_sync.notes.is_empty() {
        let list: Vec<String> = s
            .sheet_sync
            .notes
            .iter()
            .map(|n| format!("{} ({})", n.ticker, if n.text == workspace::NOTE_NO_QUANTITY { "in their own tabs with no quantity" } else { "held here but not in their own tabs: sold?" }))
            .collect();
        out.push_str(&format!(" AXYON's tab asks the user to check: {}.", list.join(", ")));
    }
    out
}

/// The tools that change what AXYON's tab holds (the positions, the user's own ceilings): the user's latest edits in
/// the sheet are read first, and the change is written there after.
fn changes_record(tool: &str) -> bool {
    matches!(tool, "wealth_update" | "wealth_trade" | "wealth_ceiling")
}

/// The answer, with where the change went: the sheet's round after it (`tab`, the tab named).
fn sheet_note(text: String, outcome: &SheetOutcome, tab: &str) -> String {
    let note = match outcome {
        SheetOutcome::NotLinked => return text,
        SheetOutcome::Done(round) if round.ours => format!("Written to the \"{tab}\" tab of the user's Google Sheet, AXYON's workspace there (the user sees it and edits it too)."),
        SheetOutcome::Done(_) => format!("Note: the user's portfolio comes from the \"{tab}\" tab of their own Google Sheet, which AXYON only reads (again every 10 minutes), so a change made here is replaced then: to keep it, it goes into that tab. With no tab named in Settings, AXYON keeps its own \"AXYON\" tab, which it reads and writes."),
        SheetOutcome::Failed(e) => format!("Not written to the user's Google Sheet yet ({e}): AXYON writes it there before it reads the sheet again."),
        SheetOutcome::Going => format!("Being written to the \"{tab}\" tab of the user's Google Sheet."),
    };
    format!("{text}\n\n{note}")
}

/// The tools that read the official data: what is due is read first.
fn reads_market(tool: &str) -> bool {
    matches!(tool, "wealth_portfolio" | "wealth_fii" | "wealth_events" | "wealth_plan" | "wealth_audit" | "wealth_macro" | "wealth_committee" | "wealth_documents" | "wealth_learn" | "wealth_ceiling" | "life_day")
}

/// An asset the call is about, when it names one.
fn asked_asset(tool: &str, input: &Value) -> Option<String> {
    let named = engine::s(input, "ticker").and_then(|t| coucou_powers::text::ticker(&t));
    match tool {
        "wealth_fii" | "wealth_company" | "wealth_ceiling" | "wealth_documents" | "wealth_learn" => named,
        "wealth_committee" => named.or_else(|| engine::s(input, "proposal").and_then(|p| coucou_powers::text::tickers_in(&p).into_iter().next())),
        _ => None,
    }
}

/// One superpower, as the brain called it. The answer is English, for the brain to tell in the user's language.
pub async fn call(app: &AppHandle, tool: &str, input: &Value) -> HomeResult {
    if !enabled(app) {
        return refused(crate::agent::NO_POWERS.into());
    }
    let version = app.state::<Powers>().version.load(Ordering::SeqCst);
    let sheet_first = async {
        if changes_record(tool) {
            sheet_now(app, false, SHEET_READ_BUDGET).await;
        }
    };
    let market_first = async {
        if reads_market(tool) {
            // What is due is read either way; the answer waits for it only when it would lack a price or the economy.
            if read(app, needs_reading) {
                fresh(app, READ_BUDGET).await;
            } else {
                let reading = app.clone();
                apart(async move { refresh(&reading, false).await });
            }
            // The session's prices, when the last reading is older than the background loop's pace (AXYON just started).
            if session_open() && since(&app.state::<Powers>().live_read, LIVE_EVERY) {
                let reading = app.clone();
                let job = apart(async move { read_live(&reading).await });
                let _ = tokio::time::timeout(LIVE_BUDGET, job).await;
            }
        }
    };
    tokio::join!(sheet_first, market_first);
    if let Some(asset) = asked_asset(tool, input) {
        ensure_asset(app, &asset).await;
    }
    let today = today();
    let mut result = match tool {
        "wealth_portfolio" => answer(read(app, |s, m| format!("{}{}", engine::portfolio(s, m, today), record_note(s)))),
        "wealth_update" => {
            let mode = engine::s(input, "mode");
            let replaces = (engine::s(input, "text").is_some() && mode.as_deref() != Some("merge")) || mode.as_deref() == Some("replace") || engine::b(input, "rebuild_from_trades") == Some(true);
            if replaces {
                read(app, |s, _| backup(s));
            }
            match change(app, |s, _| engine::update(s, input, None, today)) {
                Ok(note) => {
                    // The new tickers' quotes and funds, so the portfolio below is priced.
                    fresh(app, READ_BUDGET).await;
                    let mut text = format!("{note}\n\nThe portfolio now:\n{}", read(app, |s, m| engine::portfolio(s, m, today)));
                    // A fund whose CNPJ was just given: what was read under it, its name first, for the user to check.
                    for t in input["fund_cnpjs"].as_object().into_iter().flat_map(|o| o.keys()).filter_map(|k| coucou_powers::text::ticker(k)) {
                        text.push_str(&format!("\n\n{}", read(app, |s, m| engine::funds(s, m, Some(&t), today))));
                    }
                    answer(text)
                }
                Err(e) => refused(e),
            }
        }
        "wealth_fii" => {
            let only = engine::s(input, "ticker");
            answer(read(app, |s, m| engine::funds(s, m, only.as_deref(), today)))
        }
        "wealth_events" => {
            let days = engine::f(input, "days").map(|d| d.clamp(1.0, 365.0) as i64).unwrap_or(30);
            answer(read(app, |s, m| engine::events(s, m, days, today)))
        }
        "wealth_company" => match engine::s(input, "ticker").and_then(|t| coucou_powers::text::ticker(&t)) {
            Some(ticker) => answer(change(app, |s, m| engine::company_view(s, m, &ticker, input, today).0)),
            None => refused("wealth_company needs the share's ticker (\"TIMS3\").".into()),
        },
        "wealth_trade" => change(app, |s, m| engine::trade_view(s, m, input, today)).map_or_else(refused, answer),
        "wealth_ceiling" => change(app, |s, m| engine::ceiling_view(s, m, input, today)).map_or_else(refused, answer),
        "wealth_plan" => answer(read(app, |s, m| engine::plan(s, m, input, today))),
        "wealth_audit" => answer(read(app, |s, m| engine::audit_view(s, m, input, today))),
        "wealth_decision" => change(app, |s, m| engine::decision(s, m, input, today)).map_or_else(refused, answer),
        "wealth_committee" => read(app, |s, m| engine::committee(s, m, input, today)).map_or_else(refused, answer),
        // The documents are read by the iPhone app: on this computer the reader's library is empty, the theses work.
        "wealth_documents" => change(app, |s, m| coucou_powers::invest::documents_tool(s, m, None, input, today)).map_or_else(refused, answer),
        "wealth_learn" => change(app, |s, m| coucou_powers::learn::tool(s, m, None, input, today)).map_or_else(refused, answer),
        "wealth_macro" => answer(read(app, |_, m| engine::economy(m, today))),
        "life_study" => change(app, |s, m| engine::study_view(s, m, input, today)).map_or_else(refused, answer),
        "life_cards" => change(app, |s, _| engine::cards(s, input, today)).map_or_else(refused, answer),
        "life_english" => change(app, |s, _| engine::english_view(s, input, today)).map_or_else(refused, answer),
        "life_gym" => change(app, |s, _| {
            let hours = engine::study_hours_today(s, today);
            engine::gym_view(s, input, today, hours)
        })
        .map_or_else(refused, answer),
        "life_day" => answer(read(app, |s, m| day::for_model(&engine::day_plan(s, m, today)))),
        "life_content" => match make_kit(app, engine::s(input, "month").as_deref()) {
            Ok((folder, brief)) => answer(format!("{brief}\nSaved in {} (PNG copies are added there by AXYON in a moment).", folder.display())),
            Err(e) => refused(e),
        },
        other => refused(format!("There is no superpower named \"{other}\".")),
    };
    // A change to what AXYON's tab holds: written there now.
    if result.ok && changes_record(tool) && app.state::<Powers>().version.load(Ordering::SeqCst) != version {
        let outcome = sheet_now(app, true, SHEET_BUDGET).await;
        let tab = read(app, |s, _| sheet::tab_of(&s.sheet));
        result.text = sheet_note(std::mem::take(&mut result.text), &outcome, &tab);
    }
    log::line(format!("powers: {tool} {}", if result.ok { "done" } else { "refused" }));
    changed(app);
    result
}

// ── The content kit ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KitFile {
    pub name: String,
    pub svg: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Kit {
    pub folder: String,
    pub files: Vec<KitFile>,
}

/// "2026-10", from what the brain or the user asked (this month by default).
fn month_of(asked: Option<&str>, today: Date) -> Result<Date, String> {
    match asked.map(str::trim).filter(|m| !m.is_empty()) {
        None => Ok(today.first_of_month()),
        Some(m) => Date::parse(&format!("{m}-01")).filter(|d| *d <= today).ok_or_else(|| format!("\"{m}\" is not a month (YYYY-MM) up to this one.")),
    }
}

/// The month's charts written as SVG in Documents\Coucou\Zheus\<month>; the island then adds a PNG of each.
fn make_kit(app: &AppHandle, month: Option<&str>) -> Result<(PathBuf, String), String> {
    let today = today();
    let first = month_of(month, today)?;
    let (files, brief) = read(app, |s, _| engine::content_kit(s, &json!({ "month": first.month_key() }), today));
    if files.is_empty() {
        return Err(brief);
    }
    let folder = content_root().join(first.month_key());
    std::fs::create_dir_all(&folder).map_err(|e| format!("The folder {} could not be made: {e}", folder.display()))?;
    let mut kit = Kit { folder: folder.display().to_string(), files: Vec::new() };
    for f in &files {
        let name = safe_name(&f.name).ok_or_else(|| format!("bad file name {}", f.name))?;
        std::fs::write(folder.join(format!("{name}.svg")), &f.svg).map_err(|e| format!("{name}.svg could not be written: {e}"))?;
        kit.files.push(KitFile { name, svg: f.svg.clone() });
    }
    let _ = app.emit_to(island::WINDOW_LABEL, "powers-kit", kit);
    log::line(format!("powers: content kit of {} saved", first.month_key()));
    Ok((folder, brief))
}

/// A kit file's name: lowercase letters, digits and dashes only, without its extension.
fn safe_name(name: &str) -> Option<String> {
    let stem = name.trim().trim_end_matches(".svg").trim_end_matches(".png").to_ascii_lowercase();
    (!stem.is_empty() && stem.len() <= 60 && stem.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')).then_some(stem)
}

// ── What the island and Settings show ─────────────────────────────────────────

/// One line of the day or an alert, in the user's language.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub area: String,
    pub text: String,
    pub urgent: bool,
}

/// The settings the user sees in Settings → Zheus' superpowers.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub positions: usize,
    pub source: String,
    pub updated: Option<Date>,
    /// Class id ("fii") to percent.
    pub targets: std::collections::BTreeMap<String, f64>,
    pub income_goal: Option<f64>,
    pub monthly_contribution: Option<f64>,
    pub cash: f64,
    pub trades: usize,
    pub payouts: usize,
    pub decisions: usize,
    pub cards: usize,
    pub exam: String,
    pub exam_date: Option<Date>,
    pub hours_per_week: f64,
    pub salary: Option<f64>,
    pub current_income: Option<f64>,
    pub subjects: Vec<String>,
    pub english_level: String,
    pub english_minutes: u32,
    pub english_mode: bool,
    pub gym_days: u8,
    pub gym_split: String,
    pub gym_goal: String,
    pub private_content: bool,
    pub content_folder: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub on: bool,
    pub summary: Summary,
    pub today: Vec<Line>,
    pub alerts: Vec<Line>,
    pub refresh: RefreshState,
    pub profile: Profile,
    /// Hermes, as Coucou runs it: "ready" (it has these tools), "missing" (Coucou adds them), or "" (Coucou does
    /// not run Hermes: the user adds coucou-hook to their own).
    pub hermes: String,
    /// The program an MCP client starts, with `--mcp`.
    pub command: String,
    /// The session's prices: the last reading.
    pub live: LiveState,
    /// The user's sheet: which one, its last reading, and whether AXYON has a way to it (`way`: "gog", "core" for AXYON
    /// Core's Google sign-in, or "").
    pub sheet_link: coucou_powers::sheet::SheetLink,
    pub sheet: SheetState,
    pub gog: bool,
    pub way: String,
}

fn money(v: f64) -> String {
    coucou_powers::text::brl(v)
}

/// A day's item in the user's language (`day::Item`'s code and values).
pub fn line(item: &Item) -> String {
    let v = |k: &str| item.vars.get(k).cloned().unwrap_or_default();
    let amount = item.vars.get("amount").and_then(|a| a.parse::<f64>().ok()).map(money);
    match item.code.as_str() {
        "record" | "payment" | "amortization" | "assembly" | "offering" | "material_fact" | "notice" | "report" => {
            let what = match item.code.as_str() {
                "record" => t("data-com"),
                "payment" => t("dividend paid"),
                "amortization" => t("amortization"),
                "assembly" => t("assembly: your vote"),
                "offering" => t("offering: your decision"),
                "material_fact" => t("material fact"),
                "notice" => t("notice to holders"),
                _ => t("manager's report"),
            };
            let when = match v("when").as_str() {
                "today" => t("today"),
                "tomorrow" => t("tomorrow"),
                _ => t("soon"),
            };
            let mut parts = vec![v("ticker"), what, when];
            parts.extend(amount);
            parts.join(" · ")
        }
        "darf" => {
            let due = Date::parse(&v("due")).map(|d| d.br()).unwrap_or_default();
            tf("DARF of {amount} due {date}", &[("amount", &amount.unwrap_or_default()), ("date", &due)])
        }
        "decision" => {
            let what = v("what");
            let (action, ticker) = what.split_once(' ').unwrap_or(("", what.as_str()));
            let action = match action {
                "buy" => t("buy"),
                "sell" => t("sell"),
                "hold" => t("hold"),
                _ => t("avoid"),
            };
            tf("Look again at your decision: {action} {ticker}", &[("action", &action), ("ticker", ticker)])
        }
        "study" => tf("Study: {subjects}", &[("subjects", &v("subjects"))]),
        "study_review" => tf("Review: {subjects}", &[("subjects", &v("subjects"))]),
        "training" => tf("Training: {session}", &[("session", &v("session"))]),
        "training_light" => tf("Training: {session}, light", &[("session", &v("session"))]),
        "rest" => t("Training: rest or a walk today"),
        "opportunity" => tf(
            "{ticker}: {pct}% below its ceiling ({method})",
            &[("ticker", &v("ticker")), ("pct", &percent_text(v("pct").parse().unwrap_or(0.0))), ("method", &method_name(&v("method")))],
        ),
        "english" => match v("cards").parse::<u32>().unwrap_or(0) {
            0 => tf("English: {company}'s results", &[("company", &v("company"))]),
            n => tf("English: {company}'s results and {cards} cards", &[("company", &v("company")), ("cards", &n.to_string())]),
        },
        _ => item.title.clone(),
    }
}

fn alert_line(a: &Alert) -> String {
    match a.kind.as_str() {
        "fund" => tf("{ticker}: its dividend is at risk", &[("ticker", &a.ticker)]),
        "cnpj" => tf("{ticker}: tell Zheus its fund's CNPJ", &[("ticker", &a.ticker)]),
        _ => tf("{ticker}: {count} condition(s) of your thesis broke", &[("ticker", &a.ticker), ("count", &a.count.to_string())]),
    }
}

fn hermes_state(app: &AppHandle) -> String {
    let managed = app.state::<Shared>().settings.lock().unwrap().hermes_managed;
    if !managed || !crate::hermes_setup::installed() {
        return String::new();
    }
    let config = std::fs::read_to_string(crate::hermes_setup::hermes_home().join("config.yaml")).unwrap_or_default();
    if crate::hermes_setup::has_coucou_mcp(&config) { "ready" } else { "missing" }.into()
}

pub fn view(app: &AppHandle) -> View {
    let on = enabled(app);
    let today = today();
    let (summary, profile) = read(app, |s, m| {
        let p = &s.portfolio;
        let profile = Profile {
            positions: p.positions.len(),
            source: p.source.clone(),
            updated: p.updated,
            targets: p.targets.iter().map(|(c, v)| (c.id().to_string(), *v)).collect(),
            income_goal: p.income_goal,
            monthly_contribution: p.monthly_contribution,
            cash: p.cash,
            trades: s.trades.len(),
            payouts: s.events.len(),
            decisions: s.decisions.len(),
            cards: s.cards.len(),
            exam: s.study.exam.clone(),
            exam_date: s.study.date,
            hours_per_week: s.study.hours_per_week,
            salary: s.study.salary,
            current_income: s.study.current_income,
            subjects: s.study.subjects.iter().map(|x| x.name.clone()).collect(),
            english_level: s.english.level.clone(),
            english_minutes: s.english.minutes_per_day,
            english_mode: s.english.mode,
            gym_days: s.gym.days_per_week,
            gym_split: s.gym.split.clone(),
            gym_goal: s.gym.goal.clone(),
            private_content: s.content.private,
            content_folder: content_root().display().to_string(),
        };
        (engine::summary(s, m, today), profile)
    });
    let today_lines = summary.today.iter().map(|i| Line { area: i.area.clone(), text: line(i), urgent: i.urgent }).collect();
    let mut alerts: Vec<Line> = summary.alerts.iter().map(|a| Line { area: a.kind.clone(), text: alert_line(a), urgent: a.urgent }).collect();
    let notes = read(app, |s, _| s.sheet_sync.notes.clone());
    if !notes.is_empty() {
        alerts.insert(0, Line { area: "money".into(), text: tf("Check in AXYON's tab: {list}", &[("list", &notes_list(&notes))]), urgent: true });
    }
    View {
        on,
        summary,
        today: today_lines,
        alerts,
        refresh: held(&app.state::<Powers>().state).clone(),
        profile,
        hermes: hermes_state(app),
        command: settings::hook_exe_path().to_string_lossy().to_string(),
        live: held(&app.state::<Powers>().live).clone(),
        sheet_link: read(app, |s, _| s.sheet.clone()),
        sheet: held(&app.state::<Powers>().sheet).clone(),
        gog: !sheet::way_name().is_empty(),
        way: sheet::way_name().into(),
    }
}

/// Every window redraws from the new view, and the paired phones get the portfolio's new copy.
fn changed(app: &AppHandle) {
    let _ = app.emit("powers-changed", view(app));
    crate::phone_link::portfolio_changed(app);
}

// ── The phones' copy ──────────────────────────────────────────────────────────

/// The most positions (and prices) a phone is given, and the most notes: with the notes' length below, the copy stays
/// well under the 64 KiB a phone reads in one line.
const PHONE_MOST: usize = 120;
const PHONE_NOTES: usize = 30;
const PHONE_NOTE_CHARS: usize = 160;

/// The portfolio as the paired phones keep it, for AXYON on the iPhone to work with the computer off
/// (docs/IPHONE_PC.md): the positions, the user's own ceilings, the last price known of each asset and the notes of
/// AXYON's tab. None while the superpowers are off: the phones are told to forget their copy.
pub fn phone_copy(app: &AppHandle) -> Option<Value> {
    enabled(app).then(|| read(app, phone_copy_of))
}

fn phone_copy_of(s: &Store, m: &Market) -> Value {
    let p = &s.portfolio;
    let mut tickers: Vec<String> = p.tickers(None).iter().map(|t| t.to_uppercase()).collect();
    for t in s.ceilings.manual.keys() {
        if !tickers.contains(&t.to_uppercase()) {
            tickers.push(t.to_uppercase());
        }
    }
    let prices: serde_json::Map<String, Value> = tickers
        .iter()
        .take(PHONE_MOST)
        .filter_map(|t| {
            let live = m.live_now(t);
            let close = m.quotes.get(t).filter(|q| q.close > 0.0);
            let price = m.price(t)?;
            Some((
                t.clone(),
                json!({
                    "price": price,
                    "at": live.map(|l| l.at),
                    "previousClose": live.and_then(|l| l.previous),
                    "close": close.map(|q| q.close),
                    "closeDay": close.map(|q| q.date.to_string()),
                }),
            ))
        })
        .collect();
    let positions: Vec<Value> = p
        .positions
        .iter()
        .take(PHONE_MOST)
        .map(|x| {
            json!({
                "ticker": x.ticker,
                "class": x.class.id(),
                "subtype": x.subtype,
                "quantity": x.quantity,
                "averagePrice": x.average_price,
                "currentValue": x.current_value,
                "note": x.note.chars().take(PHONE_NOTE_CHARS).collect::<String>(),
            })
        })
        .collect();
    let notes: Vec<Value> = s
        .sheet_sync
        .notes
        .iter()
        .take(PHONE_NOTES)
        .map(|n| json!({ "ticker": n.ticker, "text": n.text.chars().take(PHONE_NOTE_CHARS).collect::<String>() }))
        .collect();
    json!({
        "type": "portfolio",
        "positions": positions,
        "assetTargets": s.portfolio.asset_targets,
        "dividends": s.recorded_dividends,
        "ceilings": s.ceilings.manual,
        "prices": prices,
        "cash": p.cash,
        "source": p.source,
        "updated": p.updated.map(|d| d.to_string()),
        "notes": notes,
    })
}

// ── What the chat adds to the user's message ──────────────────────────────────

/// Notes for the brain about the user's message (zheus_one::route): English mode, and a portfolio pasted.
pub fn notes(app: &AppHandle, message: &str) -> Vec<String> {
    if !enabled(app) {
        return Vec::new();
    }
    let mut out = Vec::new();
    if read(app, |s, _| s.english.mode) {
        out.push("English mode is on: answer in English, then correct the user's English briefly at the end (life_english).".to_string());
    }
    if looks_like_portfolio(message) {
        out.push("This looks like the user's portfolio: if they want it kept, save it with wealth_update (text), then tell them what was saved.".into());
    }
    out
}

/// A message that is a list of positions: at least two lines read as a ticker and a quantity.
pub fn looks_like_portfolio(message: &str) -> bool {
    message.lines().filter(|l| !l.trim().is_empty()).count() >= 2 && import::positions_from_text(message).positions.len() >= 2
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Only Coucou's own windows: the island and Settings.
fn ours(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == island::WINDOW_LABEL || window.label() == "settings" {
        Ok(())
    } else {
        Err("only the island and Settings may do this".into())
    }
}

fn save_setting(app: &AppHandle, on: bool) {
    let shared = app.state::<Shared>();
    let saved = {
        let mut s = shared.settings.lock().unwrap();
        s.powers_enabled = on;
        s.clone()
    };
    if let Err(err) = settings::save(&saved) {
        log::line(format!("could not save settings: {err}"));
    }
    let _ = app.emit("settings-changed", saved);
}

#[tauri::command]
pub fn powers_status(app: AppHandle, window: WebviewWindow) -> Result<View, String> {
    ours(&window)?;
    looked(&app);
    Ok(view(&app))
}

/// The ★ tab or Settings shows the portfolio: its prices are read every minute for a while, and now when the last
/// reading is a minute old.
fn looked(app: &AppHandle) {
    let powers = app.state::<Powers>();
    *held(&powers.looked_at) = Some(Instant::now());
    if enabled(app) && session_open() && since(&powers.live_read, LIVE_LOOKED) {
        let reading = app.clone();
        apart(async move {
            read_live(&reading).await;
        });
    }
}

/// Settings → Zheus' superpowers: on or off. On starts the background reading and gives Hermes the tools (when Coucou
/// runs it).
#[tauri::command]
pub fn powers_set(app: AppHandle, window: WebviewWindow, on: bool) -> Result<View, String> {
    crate::phone_link::only_settings(&window)?;
    save_setting(&app, on);
    log::line(format!("powers: {}", if on { "on" } else { "off" }));
    if on {
        start(&app);
        crate::hermes_setup::offer_tools(&app);
        let reading = app.clone();
        apart(async move {
            refresh(&reading, false).await;
        });
    }
    changed(&app);
    Ok(view(&app))
}

/// The Refresh button: everything that may have changed, read again now (in the background).
#[tauri::command]
pub fn powers_refresh(app: AppHandle, window: WebviewWindow) -> Result<View, String> {
    ours(&window)?;
    if !enabled(&app) {
        return Err(t("Zheus' superpowers are off."));
    }
    let reading = app.clone();
    apart(async move {
        if session_open() {
            read_live(&reading).await;
        }
        sync_sheet(&reading, false).await;
        refresh(&reading, true).await;
    });
    Ok(view(&app))
}

/// Settings → Zheus' superpowers → Your Google Sheet: which spreadsheet AXYON keeps the portfolio in (its link or ID,
/// the tab — empty: AXYON's own, made there — and gog's account), read at once. An empty link stops it (the portfolio
/// stays as it is).
#[tauri::command]
pub fn powers_sheet(app: AppHandle, window: WebviewWindow, id: String, tab: String, account: String) -> Result<View, String> {
    crate::phone_link::only_settings(&window)?;
    let given = id.trim();
    let link = if given.is_empty() {
        coucou_powers::sheet::SheetLink::default()
    } else {
        let id = coucou_powers::sheet::spreadsheet_id(given).ok_or_else(|| t("That is not a Google Sheets link or ID."))?;
        let clean = |s: &str, n: usize| s.trim().chars().filter(|c| !c.is_control()).take(n).collect::<String>();
        coucou_powers::sheet::SheetLink { id, tab: clean(&tab, 100), account: clean(&account, 120) }
    };
    let linked = !link.id.is_empty();
    change(&app, |s, _| s.sheet = link);
    log::line(format!("powers: the portfolio's sheet {}", if linked { "set" } else { "unlinked" }));
    *held(&app.state::<Powers>().sheet) = SheetState::default();
    if linked && enabled(&app) {
        let reading = app.clone();
        apart(async move {
            if session_open() {
                read_live(&reading).await;
            }
            sync_sheet(&reading, false).await;
        });
    }
    changed(&app);
    Ok(view(&app))
}

/// What an import or a paste did, in the user's language.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    pub text: String,
    pub view: View,
}

fn imported_line(positions: usize, trades: usize, events: usize) -> String {
    tf("Saved: {positions} positions, {trades} trades, {events} payouts and events.", &[("positions", &positions.to_string()), ("trades", &trades.to_string()), ("events", &events.to_string())])
}

/// A file from B3's Área do Investidor (position, movements, trades; XLSX or CSV) or a broker's table, dropped or
/// picked in Settings.
#[tauri::command]
pub async fn powers_import(app: AppHandle, window: WebviewWindow, name: String, bytes: Vec<u8>) -> Result<Imported, String> {
    ours(&window)?;
    if !enabled(&app) {
        return Err(t("Zheus' superpowers are off."));
    }
    if bytes.len() > 30 * 1024 * 1024 {
        return Err(t("This file is too large."));
    }
    let file = import::from_file(&name, &bytes).map_err(|e| tf("This file could not be read ({error}).", &[("error", &e)]))?;
    let counts = (file.positions.len(), file.trades.len(), file.events.len());
    read(&app, |s, _| backup(s));
    change(&app, |s, _| engine::update(s, &json!({ "source": "b3" }), Some(file), today())).map_err(|e| tf("Nothing was saved ({error}).", &[("error", &e)]))?;
    fresh(&app, READ_BUDGET).await;
    sheet_later(&app, true);
    changed(&app);
    Ok(Imported { text: imported_line(counts.0, counts.1, counts.2), view: view(&app) })
}

/// A table pasted in Settings (Investidor10, a broker): it replaces the list.
#[tauri::command]
pub async fn powers_paste(app: AppHandle, window: WebviewWindow, text: String) -> Result<Imported, String> {
    ours(&window)?;
    if !enabled(&app) {
        return Err(t("Zheus' superpowers are off."));
    }
    let count = import::positions_from_text(&text).positions.len();
    if count == 0 {
        return Err(t("No position could be read: each line needs a ticker and a quantity, and the average price."));
    }
    read(&app, |s, _| backup(s));
    change(&app, |s, _| engine::update(s, &json!({ "text": text, "source": "paste" }), None, today())).map_err(|e| tf("Nothing was saved ({error}).", &[("error", &e)]))?;
    fresh(&app, READ_BUDGET).await;
    sheet_later(&app, true);
    changed(&app);
    Ok(Imported { text: imported_line(count, 0, 0), view: view(&app) })
}

/// The tools Settings' forms use, with the user's own clicks: the portfolio's targets and amounts, the study, English
/// and training profiles, the content's privacy.
const SETTINGS_TOOLS: &[&str] = &["wealth_update", "life_study", "life_english", "life_gym"];

/// Settings → Zheus' superpowers saves a form through the same tool the brain uses.
#[tauri::command]
pub async fn powers_save(app: AppHandle, window: WebviewWindow, tool: String, input: Value) -> Result<View, String> {
    crate::phone_link::only_settings(&window)?;
    if !enabled(&app) {
        return Err(t("Zheus' superpowers are off."));
    }
    if tool == "content_private" {
        let private = input.as_bool().unwrap_or(true);
        change(&app, |s, _| s.content.private = private);
        changed(&app);
        return Ok(view(&app));
    }
    if !SETTINGS_TOOLS.contains(&tool.as_str()) {
        return Err(format!("{tool} is not saved from Settings"));
    }
    let result = call(&app, &tool, &input).await;
    if !result.ok {
        return Err(result.text);
    }
    Ok(view(&app))
}

/// Settings → Zheus' superpowers → the month's content kit: the charts saved, and the folder shown.
#[tauri::command]
pub fn powers_kit(app: AppHandle, window: WebviewWindow, month: Option<String>) -> Result<String, String> {
    ours(&window)?;
    if !enabled(&app) {
        return Err(t("Zheus' superpowers are off."));
    }
    let (folder, _) = make_kit(&app, month.as_deref())?;
    Ok(folder.display().to_string())
}

/// The island's PNG of a kit's chart (drawn from its SVG), saved beside it.
#[tauri::command]
pub fn powers_save_png(window: WebviewWindow, folder: String, name: String, bytes: Vec<u8>) -> Result<(), String> {
    ours(&window)?;
    let root = content_root();
    let folder = PathBuf::from(folder);
    let inside = folder.parent() == Some(root.as_path()) && folder.file_name().and_then(|n| n.to_str()).is_some_and(|n| Date::parse(&format!("{n}-01")).is_some());
    let name = safe_name(&name).ok_or("bad file name")?;
    if !inside || bytes.len() > MAX_IMAGE || !bytes.starts_with(b"\x89PNG") {
        return Err("not a chart of the content kit".into());
    }
    std::fs::write(folder.join(format!("{name}.png")), bytes).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn powers_open_folder(window: WebviewWindow, month: Option<String>) -> Result<(), String> {
    ours(&window)?;
    let mut folder = content_root();
    if let Some(m) = month.as_deref().and_then(|m| month_of(Some(m), today()).ok()) {
        folder = folder.join(m.month_key());
    }
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    crate::platform::reveal_folder(&folder.to_string_lossy());
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn prices_are_read_while_b3_trades_only() {
        let tuesday = Date::new(2026, 10, 27).unwrap();
        assert!(in_session(tuesday, 15 * 60 + 16));
        assert!(!in_session(tuesday, 9 * 60 + 59) && !in_session(tuesday, 18 * 60 + 30), "before the opening, after the closing call");
        assert!(!in_session(Date::new(2026, 10, 31).unwrap(), 12 * 60), "a Saturday");
    }

    #[test]
    fn a_reading_apart_runs_on_a_thread_of_its_own_and_still_reaches_the_runtime() {
        let main = std::thread::current().id();
        let (thread, slept) = tauri::async_runtime::block_on(async {
            apart(async {
                // A timer of the shared runtime, from that thread.
                tokio::time::sleep(Duration::from_millis(10)).await;
                (std::thread::current().id(), true)
            })
            .await
            .unwrap()
        });
        assert!(slept && thread != main);
        let record = |q: f64| {
            let mut store = Store::default();
            store.portfolio.positions.push(coucou_powers::model::Position { ticker: "TIMS3".into(), class: AssetClass::Acao, quantity: q, average_price: 18.0, subtype: String::new(), current_value: None, note: String::new() });
            store
        };
        let mut market = Market::default();
        market.economy_read = Some(Date::new(2026, 10, 27).unwrap());
        assert!(needs_reading(&record(30.0), &market), "TIMS3 has no price yet: the answer waits for the reading");
        market.keep_live([("TIMS3".to_string(), LivePrice { price: 18.78, at: 1_793_125_002, previous: None })]);
        assert!(!needs_reading(&record(30.0), &market), "priced: it answers at once");
        market.economy_read = None;
        assert!(needs_reading(&record(30.0), &market));
    }

    #[test]
    fn alerts_say_the_moment_the_most_important_first() {
        let moment = |kind, ticker: &str, now: f64, day: f64, ceiling: Option<f64>, money: Option<f64>| alerts::Moment { kind, ticker: ticker.into(), now, day, ceiling, money, step: None };
        let moments = [
            moment(alerts::Kind::Up, "TIMS3", 18.78, 3.76, None, Some(20.4)),
            moment(alerts::Kind::IntoZone, "GGRC11", 8.69, -1.81, Some(8.8), Some(-24.0)),
            moment(alerts::Kind::PortfolioDown, "", 7420.0, -2.1, None, Some(-160.0)),
            moment(alerts::Kind::Down, "KNRI11", 140.0, -3.2, None, None),
        ];
        let opportunity = |ticker: &str, method: &'static str, ceiling: f64, price: f64| coucou_powers::ceiling::Opportunity { ticker: ticker.into(), method, ceiling, price, below: (1.0 - price / ceiling) * 100.0 };
        // GGRC11's is told by its crossing.
        let opportunities = [opportunity("GGRC11", "manual", 8.8, 8.69), opportunity("XPML11", "bazin_projected", 120.0, 105.0)];
        let told = alerts_of(&moments, &opportunities, &Market::default());
        assert_eq!(told.iter().map(|a| a.kind.as_str()).collect::<Vec<_>>(), ["into_zone", "opportunity", "portfolio_down", "up", "down"]);
        assert_eq!((told[0].title.as_str(), told[0].text.as_str(), told[0].tone.as_str()), ("GGRC11 went below your ceiling", "R$ 8,69 now · your ceiling R$ 8,80", "good"));
        assert_eq!((told[1].title.as_str(), told[1].text.as_str()), ("XPML11 is 12,5% below its ceiling", "R$ 105,00 now · projected Bazin: R$ 120,00"));
        assert_eq!((told[2].title.as_str(), told[2].text.as_str(), told[2].tone.as_str()), ("Your portfolio is down 2,1% today", "R$ 7.420,00 now · −R$ 160,00 today", "bad"));
        assert_eq!((told[3].title.as_str(), told[3].text.as_str()), ("TIMS3 up 3,8% today", "R$ 18,78 now · your position +R$ 20,40 today"));
        assert_eq!(told[4].text, "R$ 140,00 now", "watched, not held: no position to speak of");
        assert!(told.iter().all(|a| a.at.is_none()), "no session's price in this market");
    }

    #[test]
    fn the_answer_says_where_a_change_went() {
        let said = |outcome: &SheetOutcome, tab: &str| sheet_note("Recorded.".into(), outcome, tab);
        assert_eq!(said(&SheetOutcome::NotLinked, "AXYON"), "Recorded.");
        let ours = Round { ours: true, wrote: true, ..Round::default() };
        assert_eq!(said(&SheetOutcome::Done(ours), "AXYON"), "Recorded.\n\nWritten to the \"AXYON\" tab of the user's Google Sheet, AXYON's workspace there (the user sees it and edits it too).");
        assert!(said(&SheetOutcome::Done(Round::default()), "Carteira").contains("the \"Carteira\" tab of their own Google Sheet, which AXYON only reads"));
        assert!(said(&SheetOutcome::Failed("token expired".into()), "AXYON").contains("Not written to the user's Google Sheet yet (token expired): AXYON writes it there before it reads the sheet again."));
        assert!(said(&SheetOutcome::Going, "AXYON").ends_with("Being written to the \"AXYON\" tab of the user's Google Sheet."));
        assert!(changes_record("wealth_trade") && changes_record("wealth_ceiling") && !changes_record("wealth_portfolio"));
        assert_eq!(signed_money(-45.0), "−R$ 45,00");
    }

    #[test]
    fn the_assets_priced_are_those_held_and_those_watched() {
        let mut store = Store::default();
        store.portfolio.positions.push(coucou_powers::model::Position { ticker: "GGRC11".into(), class: AssetClass::Fii, quantity: 1.0, average_price: 9.0, subtype: String::new(), current_value: None, note: String::new() });
        store.portfolio.positions.push(coucou_powers::model::Position { ticker: "CDB X".into(), class: AssetClass::RendaFixa, quantity: 1.0, average_price: 1000.0, subtype: String::new(), current_value: Some(1010.0), note: String::new() });
        store.ceilings.manual.insert("TAEE11".into(), 40.0);
        store.ceilings.manual.insert("GGRC11".into(), 10.0);
        assert_eq!(live_tickers(&store), ["GGRC11", "TAEE11"], "what B3 does not quote is not asked for");
    }

    #[test]
    fn the_phones_copy_has_the_positions_the_users_ceilings_and_the_last_prices() {
        let mut store = Store::default();
        store.portfolio.positions.push(coucou_powers::model::Position { ticker: "GARE11".into(), class: AssetClass::Fii, quantity: 204.0, average_price: 8.95, subtype: "tijolo".into(), current_value: None, note: String::new() });
        store.portfolio.positions.push(coucou_powers::model::Position { ticker: "RDB Nubank".into(), class: AssetClass::RendaFixa, quantity: 1.0, average_price: 100.0, subtype: "pos".into(), current_value: Some(104.5), note: String::new() });
        store.ceilings.manual.insert("GARE11".into(), 8.0);
        store.ceilings.manual.insert("KNSC11".into(), 9.0);
        let mut market = Market::default();
        market.keep_live([("GARE11".to_string(), LivePrice { price: 8.31, at: 1_793_125_002, previous: Some(8.4) })]);
        let copy = phone_copy_of(&store, &market);
        assert_eq!(copy["type"], "portfolio");
        assert_eq!(copy["positions"][0], json!({ "ticker": "GARE11", "class": "fii", "subtype": "tijolo", "quantity": 204.0, "averagePrice": 8.95, "currentValue": null, "note": "" }));
        assert_eq!(copy["positions"][1]["currentValue"], 104.5);
        assert_eq!(copy["ceilings"], json!({ "GARE11": 8.0, "KNSC11": 9.0 }));
        assert_eq!(copy["prices"]["GARE11"], json!({ "price": 8.31, "at": 1_793_125_002, "previousClose": 8.4, "close": null, "closeDay": null }));
        assert!(copy["prices"].get("KNSC11").is_none(), "no price known: none given");
        assert!(copy["prices"].get("RDB NUBANK").is_none(), "what B3 does not quote has no price");
    }

    use super::*;
    use std::collections::BTreeMap;

    fn item(code: &str, vars: &[(&str, &str)]) -> Item {
        Item {
            area: "money".into(),
            title: "the title".into(),
            detail: String::new(),
            urgent: false,
            code: code.into(),
            vars: vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn the_days_lines_are_said_from_their_codes() {
        assert_eq!(line(&item("record", &[("ticker", "GGRC11"), ("when", "today"), ("amount", "13.20")])), "GGRC11 · data-com · today · R$ 13,20");
        assert_eq!(line(&item("assembly", &[("ticker", "BTCI11"), ("when", "soon")])), "BTCI11 · assembly: your vote · soon");
        assert_eq!(line(&item("darf", &[("amount", "199.68"), ("due", "2026-10-30")])), "DARF of R$ 199,68 due 30/10/2026");
        assert_eq!(line(&item("decision", &[("id", "d-1"), ("what", "buy GARE11")])), "Look again at your decision: buy GARE11");
        assert_eq!(line(&item("english", &[("company", "TIM"), ("cards", "4")])), "English: TIM's results and 4 cards");
        assert_eq!(line(&item("english", &[("company", "TIM"), ("cards", "0")])), "English: TIM's results");
        assert_eq!(line(&item("rest", &[])), "Training: rest or a walk today");
        assert_eq!(line(&item("opportunity", &[("ticker", "GGRC11"), ("pct", "10.5"), ("method", "bazin_projected"), ("ceiling", "10.34")])), "GGRC11: 10,5% below its ceiling (projected Bazin)");
        assert_eq!(method_name("graham"), "Graham");
        assert_eq!(method_name("manual"), "your own ceiling");
        assert_eq!(line(&item("something new", &[])), "the title", "an unknown code says its title");
        let a = Alert { kind: "thesis".into(), ticker: "TIMS3".into(), count: 2, text: String::new(), urgent: false };
        assert_eq!(alert_line(&a), "TIMS3: 2 condition(s) of your thesis broke");
        let a = Alert { kind: "cnpj".into(), ticker: "BTCI11".into(), count: 0, text: String::new(), urgent: false };
        assert_eq!(alert_line(&a), "BTCI11: tell Zheus its fund's CNPJ");
    }

    #[test]
    fn a_pasted_portfolio_is_noticed_and_a_question_is_not() {
        assert!(looks_like_portfolio("GGRC11 150 R$ 9,20\nGARE11 180 R$ 8,55\nTIMS3 30 18,60"));
        assert!(!looks_like_portfolio("como está o GGRC11 hoje?"));
        assert!(!looks_like_portfolio("GGRC11 150"), "one line is a question about it, not a list");
    }

    #[test]
    fn kit_names_and_months_stay_in_their_folder() {
        assert_eq!(safe_name("patrimonio-2026-10.svg").as_deref(), Some("patrimonio-2026-10"));
        assert_eq!(safe_name("../../evil"), None);
        assert_eq!(safe_name("a b"), None);
        let today = Date::new(2026, 10, 21).unwrap();
        assert_eq!(month_of(None, today).unwrap(), Date::new(2026, 10, 1).unwrap());
        assert_eq!(month_of(Some("2026-09"), today).unwrap(), Date::new(2026, 9, 1).unwrap());
        assert!(month_of(Some("2027-01"), today).is_err(), "not a month to come");
        assert!(month_of(Some("../x"), today).is_err());
    }

    #[test]
    fn a_file_that_cannot_be_read_is_set_aside_not_lost() {
        let dir = std::env::temp_dir().join(format!("coucou-powers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("powers.json");
        assert_eq!(read_file::<Store>(&path), Store::default(), "missing: the defaults");
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(read_file::<Store>(&path), Store::default());
        assert!(!path.exists());
        let kept: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        assert!(kept.iter().any(|n| n.starts_with("powers.unreadable-")), "{kept:?}");
        let mut store = Store::default();
        store.english.mode = true;
        write_file(&path, &store, true).unwrap();
        assert_eq!(read_file::<Store>(&path), store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn companies_are_read_once_a_month_and_a_failure_waits_a_day() {
        let today = Date::new(2026, 10, 21).unwrap();
        let mut store = Store::default();
        store.portfolio.positions.push(coucou_powers::model::Position {
            ticker: "TIMS3".into(),
            class: AssetClass::Acao,
            quantity: 30.0,
            average_price: 18.6,
            subtype: String::new(),
            current_value: None,
            note: String::new(),
        });
        let mut market = Market::default();
        assert_eq!(companies_due(&store, &market, today, &[]), vec!["TIMS3"]);
        assert!(companies_due(&store, &market, today, &[("TIMS3".into(), today)]).is_empty(), "failed today");
        market.companies.insert("TIMS3".into(), coucou_powers::model::CompanyData { ticker: "TIMS3".into(), checked: Some(today.add_days(-10)), ..Default::default() });
        assert_eq!(companies_due(&store, &market, today, &[]).len(), 1, "read by an older Coucou, without its share count: once more");
        market.companies.get_mut("TIMS3").unwrap().reader = refresh::COMPANY_READER;
        assert!(companies_due(&store, &market, today, &[]).is_empty());
        market.companies.get_mut("TIMS3").unwrap().checked = Some(today.add_days(-31));
        assert_eq!(companies_due(&store, &market, today, &[]).len(), 1);
    }
}
