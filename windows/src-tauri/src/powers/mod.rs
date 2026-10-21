// Zheus' superpowers in the app (coucou-powers does the work; docs/ZHEUS_POWERS.md). The user's own data lives in
// powers.json (Coucou's settings folder: the portfolio, the B3 history, the decisions, the studies, English and
// training) and what the official sources said in powers-market.json (its local folder). While the superpowers are
// on, what is due is read again in the background — B3's quotes once it publishes a day, the funds' documents and
// the Banco Central's figures once a day, the funds' monthly reports once a week, a company's statements once a
// month — and the brain gets them as tools: Mochi's (agent/tools.rs) and Hermes' over MCP (agent/mcp.rs). The island
// shows the portfolio and the day, and says the morning's briefing.
//
// Off by default (Settings → Zheus' superpowers). Only B3, the CVM, FundosNet and the Banco Central are contacted, for
// public data, with no account and nothing about the user sent. Nothing here buys, sells, pays, sends or posts.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use coucou_powers::date::Date;
use coucou_powers::day::{self, Item};
use coucou_powers::engine::{self, Alert, Summary};
use coucou_powers::model::{AssetClass, Market, Store};
use coucou_powers::sources::{self, fetch::Http};
use coucou_powers::{import, refresh};
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

// ── Reading the sources ───────────────────────────────────────────────────────

pub fn enabled(app: &AppHandle) -> bool {
    app.state::<Shared>().settings.lock().unwrap().powers_enabled
}

/// The shares whose statements are a month old (or never read): those held, and those of a thesis.
fn companies_due(store: &Store, market: &Market, today: Date, failed: &[(String, Date)]) -> Vec<String> {
    let mut wanted = store.portfolio.tickers(Some(AssetClass::Acao));
    for th in &store.theses {
        if !wanted.contains(&th.ticker) {
            wanted.push(th.ticker.clone());
        }
    }
    wanted
        .into_iter()
        .filter(|t| !failed.iter().any(|(f, d)| f == t && *d == today))
        .filter(|t| market.companies.get(t).and_then(|c| c.checked).is_none_or(|d| d.until(today) >= 30))
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
                m.companies.insert(ticker, company);
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

/// What is due, read in the background; waits for it `budget` at most (the reading goes on after).
async fn fresh(app: &AppHandle, budget: Duration) {
    let reading = app.clone();
    let job = tauri::async_runtime::spawn(async move {
        refresh(&reading, false).await;
    });
    let _ = tokio::time::timeout(budget, job).await;
}

/// An asset the user asks about and may not hold: its last close, and its fund's reports and documents or its
/// company's statements. Waits `ASSET_BUDGET` at most; what was read is kept.
async fn ensure_asset(app: &AppHandle, ticker: &str) {
    let reading = app.clone();
    let ticker = ticker.to_string();
    let job = tauri::async_runtime::spawn(async move {
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
        } else if kind != "etf" && kind != "bdr" && market.companies.get(&ticker).and_then(|c| c.checked).is_none_or(|d| d.until(today) >= 30) {
            if let Ok((company, register)) = refresh::company(&powers.http, &ticker, &market.company_cnpjs, today).await {
                let mut m = held(&powers.market);
                if let Some(register) = register {
                    m.company_cnpjs.extend(register);
                }
                m.companies.insert(ticker.clone(), company);
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
            if tauri::async_runtime::spawn(async move {
                refresh(&round, false).await;
                brief(&round);
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
    let lines = read(app, |s, m| engine::day_plan(s, m, today()).items.iter().map(line).collect::<Vec<_>>());
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

/// A change to the user's data, saved at once.
fn change<R>(app: &AppHandle, f: impl FnOnce(&mut Store, &Market) -> R) -> R {
    let powers = app.state::<Powers>();
    let mut store = held(&powers.store);
    let market = held(&powers.market);
    let out = f(&mut store, &market);
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

/// The tools that read the official data: what is due is read first.
fn reads_market(tool: &str) -> bool {
    matches!(tool, "wealth_portfolio" | "wealth_fii" | "wealth_events" | "wealth_plan" | "wealth_audit" | "wealth_macro" | "wealth_committee" | "life_day")
}

/// An asset the call is about, when it names one.
fn asked_asset(tool: &str, input: &Value) -> Option<String> {
    let named = engine::s(input, "ticker").and_then(|t| coucou_powers::text::ticker(&t));
    match tool {
        "wealth_fii" | "wealth_company" => named,
        "wealth_committee" => named.or_else(|| engine::s(input, "proposal").and_then(|p| coucou_powers::text::tickers_in(&p).into_iter().next())),
        _ => None,
    }
}

/// One superpower, as the brain called it. The answer is English, for the brain to tell in the user's language.
pub async fn call(app: &AppHandle, tool: &str, input: &Value) -> HomeResult {
    if !enabled(app) {
        return refused(crate::agent::NO_POWERS.into());
    }
    if reads_market(tool) {
        fresh(app, READ_BUDGET).await;
    }
    if let Some(asset) = asked_asset(tool, input) {
        ensure_asset(app, &asset).await;
    }
    let today = today();
    let result = match tool {
        "wealth_portfolio" => answer(read(app, |s, m| engine::portfolio(s, m, today))),
        "wealth_update" => {
            let mode = engine::s(input, "mode");
            let replaces = (engine::s(input, "text").is_some() && mode.as_deref() != Some("merge")) || mode.as_deref() == Some("replace");
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
        "wealth_plan" => answer(read(app, |s, m| engine::plan(s, m, input, today))),
        "wealth_audit" => answer(read(app, |s, m| engine::audit_view(s, m, input, today))),
        "wealth_decision" => change(app, |s, m| engine::decision(s, m, input, today)).map_or_else(refused, answer),
        "wealth_committee" => read(app, |s, m| engine::committee(s, m, input, today)).map_or_else(refused, answer),
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
            Ok((folder, brief)) => answer(format!("{brief}\nSaved in {} (PNG copies are added there by Coucou in a moment).", folder.display())),
            Err(e) => refused(e),
        },
        other => refused(format!("There is no superpower named \"{other}\".")),
    };
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
    let alerts = summary.alerts.iter().map(|a| Line { area: a.kind.clone(), text: alert_line(a), urgent: a.urgent }).collect();
    View {
        on,
        summary,
        today: today_lines,
        alerts,
        refresh: held(&app.state::<Powers>().state).clone(),
        profile,
        hermes: hermes_state(app),
        command: settings::hook_exe_path().to_string_lossy().to_string(),
    }
}

/// Every window redraws from the new view.
fn changed(app: &AppHandle) {
    let _ = app.emit("powers-changed", view(app));
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
    Ok(view(&app))
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
        tauri::async_runtime::spawn(async move {
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
    tauri::async_runtime::spawn(async move {
        refresh(&reading, true).await;
    });
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
        assert!(companies_due(&store, &market, today, &[]).is_empty());
        market.companies.get_mut("TIMS3").unwrap().checked = Some(today.add_days(-31));
        assert_eq!(companies_due(&store, &market, today, &[]).len(), 1);
    }
}
