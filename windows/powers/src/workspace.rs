//! AXYON's own tab in the user's spreadsheet: its workspace for the investments. The user and AXYON both write it —
//! the user in the sheet, AXYON from the chat (a trade, a ceiling) — and AXYON reads it back before each change. Its
//! first five columns are the record (the asset, its class, the quantity, the average price and the user's own
//! ceiling); in the others AXYON writes what it works out: the price now, the day's change, the value, the cost, the
//! result, the share of the portfolio, its own ceiling and how, the margin to the ceiling, and what to look at; beside
//! them (columns R and S), the portfolio's summary. Nothing outside this tab is ever written.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::ceiling::{self, Given};
use crate::date::Date;
use crate::model::{AssetClass, Market, Store};
use crate::portfolio;
use crate::sheet::Read;

/// The tab AXYON makes when the user names none.
pub const TAB: &str = "AXYON";

/// The columns, in order: the first `INPUTS` are the record (read back), the others AXYON's.
pub const HEADERS: [&str; 16] = [
    "Ativo",
    "Classe",
    "Quantidade",
    "Preço médio",
    "Meu teto",
    "Cotação",
    "Dia %",
    "Valor",
    "Custo",
    "Resultado",
    "Resultado %",
    "% da carteira",
    "Teto AXYON",
    "Método",
    "Margem até o teto %",
    "Aviso",
];
pub const INPUTS: usize = 5;
/// The summary's first column: R.
pub const SUMMARY_COLUMN: usize = 17;

/// What AXYON keeps of its work in the tab.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Sync {
    /// The spreadsheet it is about (its ID).
    pub id: String,
    /// A change from the chat (or a file) the tab does not have yet: written before the tab is read again, so the
    /// reading does not undo it.
    pub pending: bool,
    /// The assets the tab had when AXYON last read or wrote it: one AXYON knew there and no longer follows was sold or
    /// removed from the chat, one it did not know was just added by the user.
    pub known: Vec<String>,
}

/// Whether the rows are AXYON's own layout: its header row, first.
pub fn is_ours(rows: &[Vec<Value>]) -> bool {
    rows.first().is_some_and(|r| {
        HEADERS.iter().enumerate().all(|(i, h)| r.get(i).and_then(Value::as_str).is_some_and(|c| c.trim() == *h))
    })
}

/// Whether a tab has nothing at all in its cells.
pub fn is_blank(rows: &[Vec<Value>]) -> bool {
    rows.iter().all(|r| r.iter().all(empty))
}

fn empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        _ => false,
    }
}

/// The class as the sheet says it, in words the reader knows back.
pub fn class_word(class: AssetClass) -> &'static str {
    match class {
        AssetClass::Acao => "Ação",
        AssetClass::Fii => "FII",
        AssetClass::Etf => "ETF",
        AssetClass::Bdr => "BDR",
        AssetClass::RendaFixa => "Renda Fixa",
        AssetClass::Tesouro => "Tesouro",
        AssetClass::Cripto => "Cripto",
        AssetClass::Outro => "Outro",
    }
}

/// A ceiling method's name for the sheet.
pub fn method_word(method: &str) -> &'static str {
    match method {
        "bazin" => "Bazin",
        "bazin_projected" => "Bazin projetivo",
        "graham" => "Graham",
        "lynch" => "Peter Lynch",
        "pvp" => "P/VP",
        _ => "",
    }
}

/// Whether a reading has anything to take in: a position or a ceiling. A tab without either is filled from the user's
/// data rather than taken for a portfolio sold off (clearing it all never clears the portfolio; the chat does that).
pub fn has_assets(read: &Read) -> bool {
    !read.positions.is_empty() || !read.ceilings.is_empty()
}

/// What a tab says, taken into the user's data. `whole` (AXYON's own tab, the record): its positions and the user's
/// ceilings are all there are. Else (a tab of the user's, only read): its positions replace the portfolio's when it
/// lists some, and the ceilings it gives replace those of the assets it lists. The classes AXYON worked out stay.
/// Whether anything changed.
pub fn merge(store: &mut Store, read: &Read, whole: bool) -> bool {
    let before = (store.portfolio.positions.clone(), store.ceilings.manual.clone());
    if whole || !read.positions.is_empty() {
        store.portfolio.positions = crate::sheet::with_known(read.positions.clone(), &store.portfolio.positions);
    }
    if whole {
        store.ceilings.manual = read.ceilings.iter().cloned().collect();
    } else if read.has_ceilings {
        for t in &read.order {
            store.ceilings.manual.remove(t);
        }
        for (t, c) in &read.ceilings {
            store.ceilings.manual.insert(t.clone(), *c);
        }
    }
    (store.portfolio.positions.clone(), store.ceilings.manual.clone()) != before
}

/// What one asset shows, for the "Aviso" column.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Signal {
    /// How far below the ceiling the price is, percent (negative: above it), and whether it is the user's own.
    pub below: Option<(f64, bool)>,
    /// The change since the previous close, percent, while the price is the session's.
    pub day: Option<f64>,
    /// A fund event today: "record" (data-com) or "payment".
    pub event: Option<String>,
}

/// The table AXYON writes: its rows' record (A:E) and AXYON's columns (F:P), in the same order, and each row's signal.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    pub tickers: Vec<String>,
    pub record: Vec<Vec<Value>>,
    pub computed: Vec<Vec<Value>>,
    pub signals: Vec<Signal>,
}

fn round(v: f64, digits: i32) -> Value {
    let k = 10f64.powi(digits);
    if v.is_finite() {
        json!((v * k).round() / k)
    } else {
        json!("")
    }
}

fn blank() -> Value {
    json!("")
}

/// The rows of the table: the assets in `order` first (the sheet's own order), then the rest held, then those only
/// watched (a ceiling of the user's, no position). `say` writes a row's signal in the user's words.
pub fn table(store: &Store, market: &Market, today: Date, order: &[String], say: &dyn Fn(&Signal) -> String) -> Table {
    let p = &store.portfolio;
    let v = portfolio::value(p, market);
    let mut tickers: Vec<String> = order.iter().filter(|t| p.position(t).is_some() || store.ceilings.manual.contains_key(*t)).cloned().collect();
    for t in p.positions.iter().map(|x| &x.ticker).chain(store.ceilings.manual.keys()) {
        if !tickers.contains(t) {
            tickers.push(t.clone());
        }
    }
    let events: Vec<(String, String)> = crate::engine::day_plan(store, market, today)
        .items
        .into_iter()
        .filter(|i| matches!(i.code.as_str(), "record" | "payment") && i.vars.get("when").is_some_and(|w| w == "today"))
        .filter_map(|i| Some((i.vars.get("ticker")?.clone(), i.code)))
        .collect();
    let mut out = Table::default();
    for t in tickers {
        let position = p.position(&t);
        let holding = v.holdings.iter().find(|h| h.ticker == t);
        let class = position.map(|x| x.class).or_else(|| crate::text::ticker(&t).map(|x| crate::import::guess_class(&x).0)).unwrap_or(AssetClass::Outro);
        let mine = store.ceilings.manual.get(&t).copied().filter(|c| *c > 0.0);
        out.record.push(vec![
            json!(t),
            json!(class_word(class)),
            position.map(|x| round(x.quantity, 6)).unwrap_or_else(blank),
            position.filter(|x| x.average_price > 0.0).map(|x| round(x.average_price, 4)).unwrap_or_else(blank),
            mine.map(|c| round(c, 2)).unwrap_or_else(blank),
        ]);
        let quoted = class.quoted();
        let price = if quoted { market.price(&t) } else { None };
        let day = if quoted { market.day_change(&t) } else { None };
        let assessment = (quoted && matches!(class, AssetClass::Acao | AssetClass::Fii)).then(|| ceiling::assess(store, market, &t, &Given::default(), today));
        let axyon = assessment.as_ref().and_then(|a| {
            ["bazin_projected", "bazin", "graham", "pvp", "lynch"].iter().find_map(|m| a.ceilings.iter().find(|c| c.method == *m && c.value > 0.0))
        });
        let target = mine.map(|c| (c, true)).or_else(|| axyon.map(|c| (c.value, false)));
        let signal = Signal {
            below: price.zip(target).map(|(now, (c, own))| (ceiling::below(now, c), own)),
            day,
            event: events.iter().find(|(x, _)| *x == t).map(|(_, e)| e.clone()),
        };
        let money = |x: Option<f64>| x.map(|x| round(x, 2)).unwrap_or_else(blank);
        out.computed.push(vec![
            money(price),
            day.map(|d| round(d, 2)).unwrap_or_else(blank),
            money(holding.map(|h| h.value)),
            money(holding.map(|h| h.cost)),
            money(holding.map(|h| h.gain)),
            holding.and_then(|h| h.gain_pct).map(|g| round(g, 2)).unwrap_or_else(blank),
            holding.map(|h| round(h.weight, 2)).unwrap_or_else(blank),
            money(axyon.map(|c| c.value)),
            axyon.map(|c| json!(method_word(c.method))).unwrap_or_else(blank),
            price.zip(target).map(|(now, (c, _))| round((c / now - 1.0) * 100.0, 2)).unwrap_or_else(blank),
            json!(say(&signal)),
        ]);
        out.signals.push(signal);
        out.tickers.push(t);
    }
    out
}

/// The asset a row names (its first cell) as the reader takes it: a ticker, or a fixed income's name.
fn row_key(row: &[Value]) -> Option<String> {
    let first = match row.first()? {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    if first.is_empty() {
        return None;
    }
    Some(crate::text::ticker(&first).unwrap_or_else(|| first.chars().take(60).collect()))
}

fn padded(row: &[Value], width: usize) -> Vec<Value> {
    (0..width).map(|i| row.get(i).cloned().unwrap_or_else(blank)).collect()
}

fn full_row(table: &Table, i: usize) -> Vec<Value> {
    table.record[i].iter().chain(&table.computed[i]).cloned().collect()
}

/// The rows of a tab that hold its table (the header first): up to the last with something in columns A to P, each
/// cut there.
fn body(rows: &[Vec<Value>]) -> Vec<Vec<Value>> {
    let cut: Vec<Vec<Value>> = rows.iter().map(|r| r.iter().take(HEADERS.len()).cloned().collect()).collect();
    let end = cut.iter().rposition(|r| r.iter().any(|c| !empty(c))).map_or(0, |i| i + 1);
    cut.into_iter().take(end.max(1)).collect()
}

/// The table laid against the tab's rows (`body`, its header first), each row by the asset it names: what to write on
/// every row under the header, and the rows to add after them for the assets the tab does not list. With `record`
/// (the user's data is the truth: a change from the chat), a row gets its record too, and a row the reader takes for
/// an asset AXYON knew there (`known`) and no longer follows, or for one written twice, is emptied; any other row (a
/// note, an asset the user just added) stays as it is. Without, only AXYON's columns are written (`F` on): blank on a
/// row that names no asset AXYON follows.
pub fn aligned(table: &Table, body: &[Vec<Value>], record: bool, known: &[String]) -> (Vec<Vec<Value>>, Vec<Vec<Value>>) {
    let read = crate::sheet::positions(body).unwrap_or_default();
    let taken: Vec<&String> = read.positions.iter().map(|p| &p.ticker).chain(read.ceilings.iter().map(|(t, _)| t)).collect();
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    let index = |key: &str| table.tickers.iter().position(|t| same(t, key));
    let mut used = vec![false; table.tickers.len()];
    let mut out = Vec::new();
    for row in body.iter().skip(1) {
        let key = row_key(row);
        // The same asset twice: its figures on its first row (the reader adds the rows up).
        let found = key.as_deref().and_then(index).filter(|i| !std::mem::replace(&mut used[*i], true));
        let gone = key.as_deref().is_some_and(|k| taken.iter().any(|t| same(t, k)) && (known.iter().any(|t| same(t, k)) || index(k).is_some()));
        out.push(match (found, record) {
            (Some(i), true) => full_row(table, i),
            (Some(i), false) => table.computed[i].clone(),
            (None, true) if gone => vec![blank(); HEADERS.len()],
            (None, true) => padded(row, HEADERS.len()),
            (None, false) => vec![blank(); HEADERS.len() - INPUTS],
        });
    }
    let added = (0..table.tickers.len()).filter(|i| !used[*i]).map(|i| full_row(table, i)).collect();
    (out, added)
}

/// Whether `new` says something else than the cells read (`old`, from column `from`): numbers by value, empty cells
/// as empty strings.
pub fn differs(old: &[Vec<Value>], from: usize, new: &[Vec<Value>]) -> bool {
    let same = |a: &Value, b: &Value| match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x.as_f64().unwrap_or(f64::NAN) - y.as_f64().unwrap_or(f64::NAN)).abs() < 1e-9,
        _ if empty(a) && empty(b) => true,
        _ => a == b,
    };
    new.iter().enumerate().any(|(r, row)| {
        let had = old.get(r);
        row.iter().enumerate().any(|(c, v)| !same(had.and_then(|h| h.get(from + c)).unwrap_or(&Value::Null), v))
    })
}

/// A range of the tab ("F2:P9") and what AXYON writes there.
#[derive(Debug, Clone, PartialEq)]
pub struct Write {
    pub cells: String,
    pub values: Vec<Vec<Value>>,
}

fn letter(column: usize) -> char {
    (b'A' + column.min(25) as u8) as char
}

/// `values` from `column` (A is 0) and `row` (the first is 1), `width` columns wide.
fn write_at(column: usize, row: usize, width: usize, values: Vec<Vec<Value>>) -> Write {
    let cells = format!("{}{row}:{}{}", letter(column), letter(column + width - 1), row + values.len().max(1) - 1);
    Write { cells, values }
}

fn header() -> Vec<Value> {
    HEADERS.iter().map(|h| json!(h)).collect()
}

/// A tab with nothing in it: the header, the table and the summary.
pub fn seed(table: &Table, summary: &[Vec<Value>]) -> Vec<Write> {
    let mut rows = vec![header()];
    rows.extend((0..table.tickers.len()).map(|i| full_row(table, i)));
    vec![write_at(0, 1, HEADERS.len(), rows), write_at(SUMMARY_COLUMN, 1, 2, summary.to_vec())]
}

/// After a reading (the user's edits taken in): AXYON's columns of the rows there, where they say something new, the
/// assets the tab does not list after them, and the summary when its figures changed.
pub fn after_reading(rows: &[Vec<Value>], table: &Table, summary: &[Vec<Value>]) -> Vec<Write> {
    let body = body(rows);
    let (computed, added) = aligned(table, &body, false, &[]);
    let mut out = Vec::new();
    if differs(body.get(1..).unwrap_or(&[]), INPUTS, &computed) {
        out.push(write_at(INPUTS, 2, HEADERS.len() - INPUTS, computed));
    }
    if !added.is_empty() {
        out.push(write_at(0, body.len() + 1, HEADERS.len(), added));
    }
    with_summary(out, rows, summary)
}

/// After a change from the chat (the user's data is the truth): each row's record and AXYON's columns, the assets the
/// tab does not list after them, and the summary. `known`: the assets the tab had at the last round.
pub fn after_change(rows: &[Vec<Value>], table: &Table, summary: &[Vec<Value>], known: &[String]) -> Vec<Write> {
    let body = body(rows);
    let (mut all, added) = aligned(table, &body, true, known);
    all.extend(added);
    let mut out = Vec::new();
    if differs(body.get(1..).unwrap_or(&[]), 0, &all) {
        out.push(write_at(0, 2, HEADERS.len(), all));
    }
    with_summary(out, rows, summary)
}

/// The summary, added when the table changed or its own figures did ("Atualizado" aside: when it was written).
fn with_summary(mut out: Vec<Write>, rows: &[Vec<Value>], summary: &[Vec<Value>]) -> Vec<Write> {
    let figures = summary.len().saturating_sub(1);
    if !out.is_empty() || differs(rows, SUMMARY_COLUMN, &summary[..figures]) {
        out.push(write_at(SUMMARY_COLUMN, 1, 2, summary.to_vec()));
    }
    out
}

/// The portfolio's figures for the summary: value, cost, result, result in percent, the passive income a month, and
/// when the prices were (the session's newest, unix seconds; else B3's close day).
pub fn summary(store: &Store, market: &Market, today: Date) -> (f64, f64, f64, Option<f64>, f64, Option<i64>, Option<Date>) {
    let v = portfolio::value(&store.portfolio, market);
    let income = portfolio::income(&v, market, today).iter().map(|i| i.monthly).sum();
    (v.value, v.cost, v.gain, v.gain_pct, income, v.live_at, v.priced_on)
}

/// What to look at, in the sheet's own words (the tab is in Portuguese, as its columns): how far below or above the
/// ceiling, a strong day, a fund event today.
pub fn aviso(s: &Signal) -> String {
    let mut parts = Vec::new();
    match s.below {
        Some((b, own)) if b > 0.0 => parts.push(format!("🟢 {} abaixo do {}", crate::text::pct(b, 1), if own { "seu teto" } else { "teto" })),
        Some((b, _)) if b < 0.0 => parts.push(format!("🔴 {} acima do teto", crate::text::pct(-b, 1))),
        _ => {}
    }
    if let Some(d) = s.day.filter(|d| d.abs() >= 3.0) {
        parts.push(format!("⚡ {}{} hoje", if d > 0.0 { "+" } else { "−" }, crate::text::pct(d.abs(), 1)));
    }
    match s.event.as_deref() {
        Some("record") => parts.push("📅 data-com hoje".into()),
        Some("payment") => parts.push("💰 pagamento hoje".into()),
        _ => {}
    }
    parts.join(" · ")
}

/// The summary AXYON writes beside the table (columns R and S): the portfolio's value, cost and result, its passive
/// income, where the prices come from, and when it was written (`written`, "27/10/2026 15:20").
pub fn summary_rows(store: &Store, market: &Market, today: Date, written: &str) -> Vec<Vec<Value>> {
    let (value, cost, gain, gain_pct, income, live_at, priced_on) = summary(store, market, today);
    let prices = match (live_at, priced_on) {
        (Some(at), _) => format!("Google Finance {} (≈15 min)", portfolio::brasilia_clock(at)),
        (None, Some(d)) => format!("Fechamento B3 {}", d.br()),
        (None, None) => "—".into(),
    };
    vec![
        vec![json!("Resumo AXYON"), json!("")],
        vec![json!("Patrimônio"), round(value, 2)],
        vec![json!("Custo"), round(cost, 2)],
        vec![json!("Resultado"), round(gain, 2)],
        vec![json!("Resultado %"), gain_pct.map(|g| round(g, 2)).unwrap_or_else(blank)],
        vec![json!("Renda passiva / mês"), round(income, 2)],
        vec![json!("Preços"), json!(prices)],
        vec![json!("Atualizado"), json!(written)],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LivePrice, Position};
    use crate::sheet;

    fn pos(t: &str, class: AssetClass, q: f64, avg: f64) -> Position {
        Position { ticker: t.into(), class, quantity: q, average_price: avg, subtype: String::new(), current_value: None, note: String::new() }
    }

    #[test]
    fn the_tab_is_read_back_as_the_record() {
        let rows = vec![
            header(),
            vec![json!("GGRC11"), json!("FII"), json!(150), json!(9.2), json!(9.5), json!(8.69), json!(""), json!(1303.5)],
            vec![json!("KNRI11"), json!("FII"), json!(""), json!(""), json!(150)],
            vec![json!("RDB Nubank"), json!("Renda Fixa"), json!(1), json!(500), json!(""), json!(""), json!(""), json!(512.3)],
        ];
        assert!(is_ours(&rows));
        assert!(!is_ours(&rows[1..]));
        let read = sheet::positions(&rows).unwrap();
        assert!(has_assets(&read));
        let mut store = Store::default();
        store.ceilings.manual.insert("GGRC11".into(), 8.0);
        store.ceilings.manual.insert("TAEE11".into(), 40.0);
        let mut theirs = store.clone();
        assert!(merge(&mut store, &read, true));
        assert_eq!(store.portfolio.positions.len(), 2);
        let rdb = &store.portfolio.positions[1];
        assert_eq!((rdb.average_price, rdb.current_value), (500.0, Some(512.3)), "a fixed income: what was put in, and its amount now");
        assert_eq!(store.ceilings.manual.get("GGRC11"), Some(&9.5), "the sheet's ceiling wins");
        assert_eq!(store.ceilings.manual.get("KNRI11"), Some(&150.0), "a watched asset's ceiling");
        assert_eq!(store.ceilings.manual.get("TAEE11"), None, "AXYON's tab is the record: TAEE11's row is gone");
        assert!(!merge(&mut store, &read, true), "read again: nothing changes");
        // A tab of the user's, only read: the ceilings of what it lists.
        assert!(merge(&mut theirs, &read, false));
        assert_eq!(theirs.ceilings.manual.get("TAEE11"), Some(&40.0), "an asset the sheet does not list keeps its own");
        let watchlist = sheet::Read { ceilings: vec![("XPLG11".into(), 106.67)], has_ceilings: true, order: vec!["XPLG11".into()], ..sheet::Read::default() };
        assert!(merge(&mut theirs, &watchlist, false));
        assert_eq!(theirs.portfolio.positions.len(), 2, "a list of ceilings leaves the positions alone");
        assert!(!has_assets(&sheet::positions(&[header(), vec![json!("VALE3")]]).unwrap()), "a ticker alone is no asset to take in");
        assert!(is_blank(&[vec![], vec![json!(""), Value::Null]]) && !is_blank(&rows));
    }

    fn quote(t: &str, c: f64) -> crate::sources::b3::Quote {
        crate::sources::b3::Quote {
            ticker: t.into(),
            date: Date::new(2026, 10, 26).unwrap(),
            open: c,
            high: c,
            low: c,
            average: c,
            close: c,
            trades: 1,
            volume: 1.0,
            bdi: "02".into(),
            kind: "ON".into(),
            name: t.into(),
            isin: String::new(),
            market: 10,
        }
    }

    /// GGRC11 and TIMS3 held, a fixed income, KNRI11 watched; prices of 27/10 15:16 in Brasília against B3's close of
    /// the 26th.
    fn sample() -> (Store, Market, Date) {
        let mut store = Store::default();
        store.portfolio.positions = vec![pos("GGRC11", AssetClass::Fii, 150.0, 9.2), pos("TIMS3", AssetClass::Acao, 30.0, 18.6)];
        let mut rdb = pos("RDB Nubank", AssetClass::RendaFixa, 1.0, 500.0);
        rdb.current_value = Some(512.3);
        store.portfolio.positions.push(rdb);
        store.ceilings.manual.insert("GGRC11".into(), 9.5);
        store.ceilings.manual.insert("KNRI11".into(), 150.0);
        let mut market = Market::default();
        let at = 1_793_125_002;
        market.quotes.insert("GGRC11".into(), quote("GGRC11", 8.85));
        market.quotes.insert("TIMS3".into(), quote("TIMS3", 18.1));
        let live = |t: &str, price: f64| (t.to_string(), LivePrice { price, at, previous: None });
        market.keep_live([live("GGRC11", 8.69), live("TIMS3", 18.78), live("KNRI11", 140.0)]);
        (store, market, Date::new(2026, 10, 27).unwrap())
    }

    #[test]
    fn the_table_has_the_record_then_what_axyon_works_out() {
        let (store, market, today) = sample();
        let say = |s: &Signal| format!("{:?}", s.below.map(|(b, own)| (b.round(), own)));
        let t = table(&store, &market, today, &["TIMS3".to_string(), "GONE11".to_string()], &say);
        assert_eq!(t.tickers, ["TIMS3", "GGRC11", "RDB Nubank", "KNRI11"], "the sheet's order, then the rest held, then the watched");
        assert_eq!(t.record[1], vec![json!("GGRC11"), json!("FII"), json!(150.0), json!(9.2), json!(9.5)]);
        assert_eq!(t.record[3], vec![json!("KNRI11"), json!("FII"), json!(""), json!(""), json!(150.0)]);
        let ggrc = &t.computed[1];
        assert_eq!((&ggrc[0], &ggrc[1], &ggrc[2], &ggrc[3]), (&json!(8.69), &json!(-1.81), &json!(1303.5), &json!(1380.0)));
        assert_eq!(ggrc[9], json!(9.32), "the margin to the user's own ceiling: 9,50 / 8,69 − 1");
        assert_eq!(t.signals[1].below.map(|(b, own)| (b.round(), own)), Some((9.0, true)));
        assert_eq!(ggrc[10], json!("Some((9.0, true))"));
        let rdb = &t.computed[2];
        assert_eq!((&rdb[0], &rdb[2], &rdb[3]), (&json!(""), &json!(512.3), &json!(500.0)), "fixed income: its amount, no price");
        let knri = &t.computed[3];
        assert_eq!((&knri[0], &knri[2]), (&json!(140.0), &json!("")), "watched: a price, no value");
        assert_eq!(t.record.len(), t.computed.len());
        assert!(t.computed.iter().all(|r| r.len() == HEADERS.len() - INPUTS));
        assert_eq!(aviso(&t.signals[1]), "🟢 8,5% abaixo do seu teto");
        assert_eq!(aviso(&Signal { below: Some((-4.0, false)), day: Some(-3.4), event: Some("record".into()) }), "🔴 4,0% acima do teto · ⚡ −3,4% hoje · 📅 data-com hoje");
        assert_eq!(aviso(&Signal::default()), "");
        let rows = summary_rows(&store, &market, today, "27/10/2026 15:20");
        assert_eq!((rows.len(), &rows[6][1], &rows[7][1]), (8, &json!("Google Finance 15:16 (≈15 min)"), &json!("27/10/2026 15:20")));
        let (value, cost, ..) = summary(&store, &market, today);
        assert!((value - (1303.5 + 30.0 * 18.78 + 512.3)).abs() < 1e-6 && (cost - (1380.0 + 558.0 + 500.0)).abs() < 1e-6);
    }

    #[test]
    fn axyons_columns_go_on_each_assets_own_row() {
        let (store, market, today) = sample();
        let t = table(&store, &market, today, &[], &aviso);
        // A tab in another order, with a note and a blank row in between, an asset twice and one no longer followed.
        let tab = vec![
            header(),
            vec![json!("GGRC11"), json!("FII"), json!(100)],
            vec![json!("minhas notas")],
            vec![],
            vec![json!("TIMS3"), json!("Ação"), json!(30)],
            vec![json!("SOLD11"), json!("FII"), json!(5)],
            vec![json!("GGRC11"), json!("FII"), json!(50)],
            vec![json!("VALE3"), json!("Ação"), json!(10), json!(60)],
        ];
        let at = |name: &str| t.tickers.iter().position(|x| x == name).unwrap();
        let (rows, added) = aligned(&t, &tab, false, &[]);
        assert_eq!(rows.len(), 7);
        assert_eq!(rows[0], t.computed[at("GGRC11")], "GGRC11's own figures on GGRC11's row");
        assert_eq!(rows[3], t.computed[at("TIMS3")]);
        for blank_row in [1, 2, 4, 5, 6] {
            assert!(rows[blank_row].iter().all(|c| c == &json!("")), "row {blank_row}");
        }
        assert_eq!(added.iter().map(|r| r[0].clone()).collect::<Vec<_>>(), [json!("RDB Nubank"), json!("KNRI11")], "what the tab does not list yet");
        // A change from the chat: SOLD11 was known there and sold; VALE3 the user just typed in.
        let (rows, _) = aligned(&t, &tab, true, &["GGRC11".into(), "TIMS3".into(), "SOLD11".into()]);
        assert_eq!(rows[0][..5], t.record[at("GGRC11")][..], "with the record");
        assert_eq!(rows[1][0], json!("minhas notas"), "a note stays");
        assert!(rows[4].iter().all(|c| c == &json!("")), "an asset sold is emptied");
        assert!(rows[5].iter().all(|c| c == &json!("")), "GGRC11's second row: its quantity is on the first now");
        assert_eq!(rows[6][..4], [json!("VALE3"), json!("Ação"), json!(10), json!(60)], "an asset the user just added stays");
        assert!(!differs(&[vec![json!("x"), json!(8.69), Value::Null]], 1, &[vec![json!(8.69), json!("")]]));
        assert!(differs(&[vec![json!(8.69)]], 0, &[vec![json!(8.7)]]));
    }

    /// `w` written into `tab` as Google writes it: its range's cells, the tab grown as needed.
    fn apply(tab: &mut Vec<Vec<Value>>, w: &Write) {
        let start = w.cells.split(':').next().unwrap();
        let column = (start.as_bytes()[0] - b'A') as usize;
        let row: usize = start[1..].parse().unwrap();
        for (r, values) in w.values.iter().enumerate() {
            let at = row - 1 + r;
            if tab.len() <= at {
                tab.resize(at + 1, Vec::new());
            }
            let line = &mut tab[at];
            if line.len() < column + values.len() {
                line.resize(column + values.len(), Value::Null);
            }
            for (c, v) in values.iter().enumerate() {
                line[column + c] = v.clone();
            }
        }
    }

    fn row_of(tab: &[Vec<Value>], ticker: &str) -> usize {
        tab.iter().position(|r| r.first() == Some(&json!(ticker))).unwrap()
    }

    #[test]
    fn the_user_and_axyon_stay_in_step_through_the_tabs_life() {
        let (mut store, market, today) = sample();
        let mut tab: Vec<Vec<Value>> = Vec::new();
        // A new tab, filled from what AXYON knows; read back, it is the same portfolio.
        let t = table(&store, &market, today, &[], &aviso);
        for w in seed(&t, &summary_rows(&store, &market, today, "1")) {
            apply(&mut tab, &w);
        }
        assert!(is_ours(&tab));
        assert!(!merge(&mut store.clone(), &sheet::positions(&tab).unwrap(), true), "what AXYON wrote reads back as what it knows");
        // The user, in the sheet: GGRC11 to 200, a new row for VALE3, TIMS3's row deleted (the rows under it move up).
        let ggrc = row_of(&tab, "GGRC11");
        tab[ggrc][2] = json!(200);
        tab.push(vec![json!("VALE3"), json!("Ação"), json!(10), json!(60)]);
        let tims = row_of(&tab, "TIMS3");
        tab.remove(tims);
        let read = sheet::positions(&tab).unwrap();
        assert!(merge(&mut store, &read, true));
        assert_eq!(store.portfolio.position("GGRC11").unwrap().quantity, 200.0);
        assert!(store.portfolio.position("TIMS3").is_none());
        assert_eq!(store.portfolio.position("VALE3").map(|p| (p.quantity, p.average_price)), Some((10.0, 60.0)));
        let t = table(&store, &market, today, &read.order, &aviso);
        for w in after_reading(&tab, &t, &summary_rows(&store, &market, today, "2")) {
            apply(&mut tab, &w);
        }
        assert_eq!(tab[row_of(&tab, "GGRC11")][INPUTS + 2], json!(1738.0), "GGRC11's value follows its new quantity: 200 × 8,69");
        assert_eq!(tab[row_of(&tab, "KNRI11")][INPUTS], json!(140.0), "each row its own figures, after the rows moved");
        // From the chat: 20 more GGRC11, and the RDB taken out.
        let known = t.tickers.clone();
        store.portfolio.positions.iter_mut().find(|p| p.ticker == "GGRC11").unwrap().quantity = 220.0;
        store.portfolio.positions.retain(|p| p.ticker != "RDB Nubank");
        let t = table(&store, &market, today, &sheet::positions(&tab).unwrap().order, &aviso);
        for w in after_change(&tab, &t, &summary_rows(&store, &market, today, "3"), &known) {
            apply(&mut tab, &w);
        }
        let read = sheet::positions(&tab).unwrap();
        assert!(!merge(&mut store.clone(), &read, true), "the tab has every change from the chat");
        assert_eq!(read.positions.iter().map(|p| p.ticker.as_str()).collect::<Vec<_>>(), ["GGRC11", "VALE3"]);
        assert_eq!(read.ceilings, [("GGRC11".to_string(), 9.5), ("KNRI11".to_string(), 150.0)]);
        assert!(is_ours(&tab) && tab[0][SUMMARY_COLUMN] == json!("Resumo AXYON"));
        // Read again with nothing new: nothing to write.
        let t = table(&store, &market, today, &read.order, &aviso);
        assert!(after_reading(&tab, &t, &summary_rows(&store, &market, today, "4")).is_empty());
    }

    #[test]
    fn each_round_writes_only_what_changed() {
        let (store, market, today) = sample();
        let t = table(&store, &market, today, &[], &aviso);
        let summary = summary_rows(&store, &market, today, "27/10/2026 15:20");
        // A new tab: everything, from A1 and from R1.
        let new = seed(&t, &summary);
        assert_eq!(new.iter().map(|w| w.cells.as_str()).collect::<Vec<_>>(), ["A1:P5", "R1:S8"]);
        assert_eq!(new[0].values[0], header());
        // Written, then read back: nothing to write again.
        let mut tab: Vec<Vec<Value>> = new[0].values.clone();
        tab.resize(new[1].values.len().max(tab.len()), Vec::new());
        for (i, row) in new[1].values.iter().enumerate() {
            tab[i].resize(SUMMARY_COLUMN, json!(""));
            tab[i].extend(row.iter().cloned());
        }
        assert!(after_reading(&tab, &t, &summary).is_empty());
        assert!(after_reading(&tab, &t, &summary_rows(&store, &market, today, "27/10/2026 15:25")).is_empty(), "only its time: not written");
        // The price moved: AXYON's columns of the rows there and the summary.
        let mut moved = market.clone();
        moved.keep_live([("GGRC11".to_string(), LivePrice { price: 8.75, at: 1_793_125_302, previous: Some(8.85) })]);
        let t2 = table(&store, &moved, today, &[], &aviso);
        let writes = after_reading(&tab, &t2, &summary_rows(&store, &moved, today, "27/10/2026 15:25"));
        assert_eq!(writes.iter().map(|w| w.cells.as_str()).collect::<Vec<_>>(), ["F2:P5", "R1:S8"]);
        assert_eq!(writes[0].values.len(), 4);
        // A change from the chat: the rows' record too, from A2.
        let mut bought = store.clone();
        bought.portfolio.positions[0].quantity = 160.0;
        let t3 = table(&bought, &market, today, &[], &aviso);
        let writes = after_change(&tab, &t3, &summary_rows(&bought, &market, today, "27/10/2026 15:26"), &t.tickers);
        assert_eq!(writes[0].cells, "A2:P5");
        assert_eq!(writes[0].values[0][2], json!(160.0));
        // The tab with its header only: the rows written under it.
        let writes = after_change(&tab[..1], &t, &summary, &[]);
        assert_eq!(writes[0].cells, "A2:P5");
    }
}
