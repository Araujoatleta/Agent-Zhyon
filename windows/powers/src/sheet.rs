//! The portfolio from the user's own spreadsheet: a Google Sheet read through `gog` (the Google Workspace command the
//! user's assistant already uses), rows under a header that names the asset and its quantity, with the average price,
//! the class and, for what B3 does not quote, the amount when the sheet has them. The sheet is the record: what it
//! holds replaces the positions (the targets, goals and history stay as they are).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::import::guess_class;
use crate::model::{AssetClass, Position};
use crate::text::{num, plain, ticker};

/// Which sheet: what the user gave in Settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SheetLink {
    /// The spreadsheet's ID (or its link, as pasted).
    pub id: String,
    /// The tab to read; empty: the first one.
    pub tab: String,
    /// gog's account (an e-mail or an alias); empty: gog's own default.
    pub account: String,
}

/// What a sheet with no header the reader knows says.
pub const NO_HEADER: &str = "No header with the asset and its quantity: name the columns \"Ativo\" and \"Quantidade\" (and \"Preço médio\", \"Classe\" if you like).";

/// What a reading of the sheet found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Read {
    pub positions: Vec<Position>,
    /// Rows left out, and why ("ROW 12: 'Total' is not an asset"), for the user.
    pub skipped: Vec<String>,
    /// The ceilings the user wrote ("Meu teto"), by ticker, held or only watched.
    pub ceilings: Vec<(String, f64)>,
    /// The sheet has a column for them: the tickers it lists without one have none.
    pub has_ceilings: bool,
    /// The assets in the order the sheet lists them (positions and watched ones).
    pub order: Vec<String>,
    /// The share of the portfolio the user wants in each asset, percent ("% Ideal", "Meta").
    pub targets: Vec<(String, f64)>,
}

/// The dividends a sheet records month by month (its "Proventos pagos" table): month ("2026-10") to R$, every month the
/// table has a column for (an empty one is R$ 0); and month to ticker to R$ when it lists them asset by asset.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SheetDividends {
    pub months: std::collections::BTreeMap<String, f64>,
    pub by_asset: std::collections::BTreeMap<String, std::collections::BTreeMap<String, f64>>,
    /// Its "Total de Proventos", when it has one.
    pub total: Option<f64>,
}

/// The spreadsheet ID from a link ("https://docs.google.com/spreadsheets/d/<ID>/edit#gid=0") or the ID itself.
pub fn spreadsheet_id(given: &str) -> Option<String> {
    let given = given.trim();
    let id = match given.find("/d/") {
        Some(at) => given[at + 3..].split(['/', '?', '#']).next().unwrap_or(""),
        None => given,
    };
    (id.len() >= 20 && id.len() <= 100 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).then(|| id.to_string())
}

/// The range gog reads: the tab's first 400 rows and 26 columns ("'Meu Patrimônio'!A1:Z400"), the first tab without one.
pub fn range(tab: &str) -> String {
    let tab = tab.trim();
    if tab.is_empty() {
        "A1:Z400".to_string()
    } else {
        format!("'{}'!A1:Z400", tab.replace('\'', "''"))
    }
}

/// `values` of gog's `sheets get --json` (`{"range": …, "values": [[…]]}`).
pub fn values_of(json: &Value) -> Vec<Vec<Value>> {
    let rows = json.get("values").or(Some(json)).and_then(Value::as_array);
    rows.map(|rows| rows.iter().map(|r| r.as_array().cloned().unwrap_or_default()).collect()).unwrap_or_default()
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn cell_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => num(s),
        _ => None,
    }
    .filter(|n| n.is_finite())
}

#[derive(Clone, Copy, PartialEq)]
enum Column {
    Asset,
    Quantity,
    AveragePrice,
    Class,
    Value,
    Invested,
    Ceiling,
    Status,
    Target,
}

/// What a header cell names, by its words (any case, accents or not).
fn column(header: &str) -> Option<Column> {
    let p = plain(header);
    let p = p.trim().trim_end_matches([':', '.']);
    let is = |words: &[&str]| words.iter().any(|w| p == *w);
    let starts = |words: &[&str]| words.iter().any(|w| p.starts_with(w));
    if p.starts_with("teto axyon") {
        // AXYON's own ceiling: written by AXYON, never read as the user's.
        return None;
    }
    Some(if is(&["ativo", "ativos", "ticker", "papel", "acao", "simbolo", "cod", "codigo", "asset", "symbol"]) || starts(&["codigo do", "ticker ", "ativo "]) {
        Column::Asset
    } else if starts(&["quantidade", "qtd", "qtde", "quant", "n de cotas", "numero de cotas", "num de cotas", "cotas", "quantity", "shares"]) || is(&["qt"]) {
        Column::Quantity
    } else if p.contains("preco medio") || p.contains("custo medio") || p.contains("preco de compra") || p.contains("valor medio") || is(&["pm", "p.m", "p. medio", "average price", "avg price", "medio"]) {
        Column::AveragePrice
    } else if is(&["classe", "tipo", "categoria", "classe do ativo", "tipo de ativo", "class", "type"]) {
        Column::Class
    } else if starts(&["meu teto", "teto (meu)", "preco teto", "preco-teto", "teto"]) {
        Column::Ceiling
    } else if starts(&["valor atual", "saldo", "valor de mercado", "patrimonio atual"]) || is(&["valor", "valor (r$)"]) {
        Column::Value
    } else if starts(&["valor investido", "total investido", "valor aplicado", "investido", "aplicado"]) {
        Column::Invested
    } else if is(&["status", "situacao", "estado"]) {
        Column::Status
    } else if p.contains("ideal") || p.contains("alvo") || p.contains("objetivo") || is(&["meta", "meta %", "% meta", "meta (%)", "meta de alocacao", "target", "target %"]) || p.starts_with("meta ") {
        Column::Target
    } else {
        return None;
    })
}

/// A status that says the asset is no longer held ("Vendido", "Zerado", "Encerrado").
fn is_gone(status: &str) -> bool {
    let p = plain(status);
    ["vend", "zerad", "encerr", "liquidad", "sold", "closed"].iter().any(|w| p.trim().starts_with(w))
}

/// Names that are a sum, not an asset.
fn is_total(name: &str) -> bool {
    let p = plain(name);
    ["total", "subtotal", "soma", "saldo total", "patrimonio", "carteira"].iter().any(|w| p.trim() == *w || p.starts_with(&format!("{w} ")))
}

/// What B3 does not quote, by its name: fixed income, the Treasury, savings, crypto (whole words: "Something" is no
/// Ether).
fn unquoted_class(name: &str) -> Option<AssetClass> {
    let p = plain(name);
    let words: Vec<&str> = p.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let has = |list: &[&str]| list.iter().any(|w| words.contains(w));
    if p.contains("tesouro") {
        Some(AssetClass::Tesouro)
    } else if p.contains("renda fixa") || has(&["cdb", "lci", "lca", "rdb", "lc", "debenture", "debentures", "cri", "cra", "poupanca", "selic", "cdi", "ipca", "prefixado", "caixinha", "nubank", "reserva"]) {
        Some(AssetClass::RendaFixa)
    } else if has(&["bitcoin", "btc", "ethereum", "eth", "cripto", "criptomoeda", "usdt", "solana"]) {
        Some(AssetClass::Cripto)
    } else {
        None
    }
}

/// The positions of a sheet's rows: the first row (of the first 30) that names an asset column and a quantity, amount
/// or ceiling column is the header; each row under it with a B3 ticker and a quantity above zero is a position (one
/// with none is watched, with its ceiling), and a row with a fixed-income name and an amount (or what was put in) is
/// one too. The same ticker twice is one position (quantities added, the average price weighted). Err when no such
/// header is there.
pub fn positions(rows: &[Vec<Value>]) -> Result<Read, String> {
    let header = rows.iter().take(30).enumerate().find_map(|(i, row)| {
        let columns: Vec<Option<Column>> = row.iter().map(|c| column(&cell_text(c))).collect();
        let has = |k: Column| columns.iter().any(|c| *c == Some(k));
        (has(Column::Asset) && (has(Column::Quantity) || has(Column::Value) || has(Column::Ceiling))).then_some((i, columns))
    });
    let Some((at, columns)) = header else {
        return Err(NO_HEADER.into());
    };
    let index = |k: Column| columns.iter().position(|c| *c == Some(k));
    let (asset, quantity, average, class, value, invested, ceiling, status, target) = (
        index(Column::Asset).unwrap_or(0),
        index(Column::Quantity),
        index(Column::AveragePrice),
        index(Column::Class),
        index(Column::Value),
        index(Column::Invested),
        index(Column::Ceiling),
        index(Column::Status),
        index(Column::Target),
    );
    let mut read = Read { has_ceilings: ceiling.is_some(), ..Read::default() };
    for (i, row) in rows.iter().enumerate().skip(at + 1) {
        let get = |k: Option<usize>| k.and_then(|k| row.get(k));
        let name = get(Some(asset)).map(cell_text).unwrap_or_default();
        // A row marked sold is neither held nor watched.
        if name.is_empty() || is_total(&name) || get(status).map(cell_text).is_some_and(|s| is_gone(&s)) {
            continue;
        }
        let line = i + 1;
        let given_class = get(class).map(cell_text).and_then(|c| AssetClass::parse(&c));
        let amount = get(value).and_then(cell_number).filter(|v| *v > 0.0);
        match ticker(&name).filter(|_| given_class.is_none_or(AssetClass::quoted)) {
            Some(t) => {
                if let Some(c) = get(ceiling).and_then(cell_number).filter(|c| *c > 0.0) {
                    read.ceilings.retain(|(x, _)| *x != t);
                    read.ceilings.push((t.clone(), c));
                }
                if !read.order.contains(&t) {
                    read.order.push(t.clone());
                }
                if let Some(x) = get(target).and_then(percent_of).filter(|x| *x > 0.0 && *x <= 100.0) {
                    read.targets.retain(|(y, _)| *y != t);
                    read.targets.push((t.clone(), x));
                }
                let Some(q) = get(quantity).and_then(cell_number).filter(|q| *q > 0.0) else {
                    // A ticker with no quantity: watched, not held.
                    continue;
                };
                let price = get(average).and_then(cell_number).filter(|p| *p > 0.0).or_else(|| get(invested).and_then(cell_number).filter(|v| *v > 0.0).map(|v| v / q)).unwrap_or(0.0);
                let (class, guessed) = given_class.map_or_else(|| guess_class(&t), |c| (c, false));
                match read.positions.iter_mut().find(|p| p.ticker == t) {
                    Some(p) => {
                        let total = p.quantity + q;
                        p.average_price = (p.cost() + q * price) / total;
                        p.quantity = total;
                    }
                    None => read.positions.push(Position {
                        ticker: t,
                        class,
                        quantity: q,
                        average_price: price,
                        subtype: String::new(),
                        current_value: None,
                        note: if guessed { "guessed".into() } else { String::new() },
                    }),
                }
            }
            None => {
                let class = given_class.filter(|c| !c.quoted()).or_else(|| unquoted_class(&name));
                // What was put in: its own column, else the average price times the quantity (one when none).
                let invested_amount = get(invested).and_then(cell_number).filter(|v| *v > 0.0).or_else(|| {
                    let q = get(quantity).and_then(cell_number).filter(|q| *q > 0.0).unwrap_or(1.0);
                    get(average).and_then(cell_number).filter(|p| *p > 0.0).map(|p| p * q)
                });
                match (class, amount.or(invested_amount)) {
                    (Some(class), Some(now)) => {
                        let name: String = name.chars().take(60).collect();
                        if !read.order.contains(&name) {
                            read.order.push(name.clone());
                        }
                        read.positions.push(Position {
                        ticker: name,
                        class,
                        quantity: 1.0,
                        average_price: invested_amount.unwrap_or(now),
                        subtype: String::new(),
                        current_value: Some(now),
                        note: String::new(),
                    })
                    }
                    _ => read.skipped.push(format!("row {line}: \"{}\" is not a B3 ticker", name.chars().take(40).collect::<String>())),
                }
            }
        }
    }
    Ok(read)
}

/// A percent as a cell holds it: "7,5%" written, or 0.075 from a cell formatted as a percent (read unformatted).
fn percent_of(v: &Value) -> Option<f64> {
    let n = cell_number(v)?;
    let written = matches!(v, Value::String(s) if s.contains('%'));
    Some(if !written && n > 0.0 && n <= 1.0 { n * 100.0 } else { n })
}

/// "out./26", "Outubro/2026", "10/2026", "2026-10", or a date the sheet keeps as its serial number: the month.
fn month_of(v: &Value) -> Option<String> {
    const NAMES: [&str; 12] = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];
    match v {
        Value::Number(n) => {
            // Google Sheets' dates: days since 30/12/1899, between 2000 and 2100.
            let serial = n.as_f64()?;
            (36_526.0..=73_051.0).contains(&serial).then(|| crate::date::Date::from_days(serial.floor() as i64 - 25_569).month_key())
        }
        Value::String(s) => {
            let p = plain(s);
            let words: Vec<&str> = p.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
            let year = |w: &str| -> Option<i32> {
                let y: i32 = w.parse().ok()?;
                match w.len() {
                    2 => Some(2000 + y),
                    4 if (2000..=2100).contains(&y) => Some(y),
                    _ => None,
                }
            };
            match words.as_slice() {
                [m, y] => {
                    let month = NAMES.iter().position(|n| m.starts_with(n)).map(|i| i as u32 + 1).or_else(|| m.parse::<u32>().ok().filter(|m| (1..=12).contains(m)));
                    let (month, year) = match (month, year(y)) {
                        (Some(mo), Some(ye)) => (mo, ye),
                        _ => {
                            // "2026-10".
                            let (ye, mo) = (year(m)?, y.parse::<u32>().ok().filter(|x| (1..=12).contains(x))?);
                            (mo, ye)
                        }
                    };
                    Some(format!("{year:04}-{month:02}"))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// The sheet's "Proventos pagos" (or "Proventos recebidos", "Dividendos recebidos"…) table: a title, a row of months
/// under it, and under those a row of amounts (the month's total) or a row per asset (its ticker first). None when the
/// tab has no such table.
pub fn dividends(rows: &[Vec<Value>]) -> Option<SheetDividends> {
    let title = rows.iter().enumerate().find_map(|(i, r)| r.iter().any(dividends_title).then_some(i))?;
    if let Some(found) = dividends_in_row(rows, title) {
        return Some(found);
    }
    let (header, columns) = (title..(title + 5).min(rows.len())).find_map(|i| month_row(&rows[i], 0).map(|cols| (i, cols)))?;
    let first = columns.iter().map(|(c, _)| *c).min().unwrap_or(0);
    let mut found = SheetDividends::default();
    let mut totals: Option<std::collections::BTreeMap<String, f64>> = None;
    let mut blank = 0;
    for row in rows.iter().skip(header + 1).take(60) {
        let values: Vec<(String, Option<f64>)> = columns.iter().map(|(c, m)| (m.clone(), row.get(*c).and_then(cell_number))).collect();
        if values.iter().all(|(_, v)| v.is_none()) {
            blank += 1;
            if blank >= 3 {
                break;
            }
            continue;
        }
        blank = 0;
        let label = row.iter().take(first).map(cell_text).find(|t| !t.is_empty()).unwrap_or_default();
        match ticker(&label) {
            Some(t) => {
                for (m, v) in &values {
                    if let Some(v) = v {
                        *found.by_asset.entry(m.clone()).or_default().entry(t.clone()).or_insert(0.0) += v;
                    }
                }
            }
            None if totals.is_none() && (label.is_empty() || is_total(&label) || plain(&label).starts_with("total")) => {
                totals = Some(values.iter().map(|(m, v)| (m.clone(), v.unwrap_or(0.0))).collect());
            }
            None => {}
        }
    }
    found.months = match totals {
        Some(t) => t,
        None if !found.by_asset.is_empty() => columns.iter().map(|(_, m)| (m.clone(), found.by_asset.get(m).map(|x| x.values().sum()).unwrap_or(0.0))).collect(),
        None => return None,
    };
    found.total = total_of(rows);
    Some(found)
}

/// The share of the portfolio the user wants in each class, percent: rows that are classes (Ações, FIIs, Renda
/// Fixa…) under a "Percentual Ideal" (or "% Ideal", "Meta") column.
pub fn class_targets(rows: &[Vec<Value>]) -> Vec<(AssetClass, f64)> {
    let mut out: Vec<(AssetClass, f64)> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let Some(col) = row.iter().position(|c| column(&cell_text(c)) == Some(Column::Target)) else { continue };
        for below in rows.iter().skip(i + 1).take(15) {
            let label = below.iter().take(col).map(cell_text).find(|t| !t.is_empty());
            let Some(class) = label.filter(|t| ticker(t).is_none()).and_then(|t| AssetClass::parse(&t)) else { continue };
            if let Some(x) = below.get(col).and_then(percent_of).filter(|x| *x > 0.0 && *x <= 100.0) {
                if !out.iter().any(|(c, _)| *c == class) {
                    out.push((class, x));
                }
            }
        }
    }
    out
}

/// "Proventos pagos", "Dividendos recebidos", "Proventos mês a mês"… (never a total).
fn dividends_title(c: &Value) -> bool {
    let p = plain(&cell_text(c));
    let p = p.trim();
    !p.contains("total") && (p.contains("proventos") || p.contains("dividendos") || p.contains("rendimentos")) && (p.contains("pago") || p.contains("recebid") || p.contains("mes a mes") || p.contains("historico") || p == "proventos" || p == "dividendos")
}

/// The months of a row, right of column `after`: at least two, in order, about a month apart (a row of amounts in
/// the tens of thousands reads as dates years apart, and is not one).
fn month_row(row: &[Value], after: usize) -> Option<Vec<(usize, String)>> {
    let cols: Vec<(usize, String)> = row.iter().enumerate().skip(after).filter_map(|(c, v)| month_of(v).map(|m| (c, m))).collect();
    let index = |m: &str| m[..4].parse::<i32>().unwrap_or(0) * 12 + m[5..].parse::<i32>().unwrap_or(0);
    let ordered = cols.windows(2).all(|w| index(&w[0].1) < index(&w[1].1));
    let span = cols.last().map(|l| index(&l.1)).unwrap_or(0) - cols.first().map(|f| index(&f.1)).unwrap_or(0);
    (cols.len() >= 2 && ordered && span <= cols.len() as i32 + 12).then_some(cols)
}

/// "Proventos pagos" as one row of a table with the months across above it (Patrimônio, Aportes, Proventos pagos…):
/// that row's amounts are the months' totals.
fn dividends_in_row(rows: &[Vec<Value>], title: usize) -> Option<SheetDividends> {
    let row = &rows[title];
    let at = row.iter().position(dividends_title)?;
    let columns = (title.saturating_sub(40)..title).rev().find_map(|i| month_row(&rows[i], at + 1))?;
    let values: Vec<(String, Option<f64>)> = columns.iter().map(|(c, m)| (m.clone(), row.get(*c).and_then(cell_number))).collect();
    if values.iter().all(|(_, v)| v.is_none()) {
        return None;
    }
    Some(SheetDividends { months: values.into_iter().map(|(m, v)| (m, v.unwrap_or(0.0))).collect(), by_asset: Default::default(), total: total_of(rows) })
}

/// "Total de Proventos = R$ 82,27": the first number to its right.
fn total_of(rows: &[Vec<Value>]) -> Option<f64> {
    rows.iter().find_map(|r| {
        let at = r.iter().position(|c| plain(&cell_text(c)).starts_with("total de proventos"))?;
        r.iter().skip(at + 1).find_map(cell_number)
    })
}

/// The sheet's positions with what AXYON already knows of each: the class it worked out (B3's own codes, the CVM's
/// reports) where the sheet gives none, a fund's kind, a note. Read again unchanged, the sheet changes nothing.
pub fn with_known(new: Vec<Position>, old: &[Position]) -> Vec<Position> {
    new.into_iter()
        .map(|mut p| {
            if let Some(o) = old.iter().find(|o| o.ticker == p.ticker) {
                if p.note == "guessed" {
                    p.class = o.class;
                    p.note = o.note.clone();
                }
                if p.subtype.is_empty() {
                    p.subtype = o.subtype.clone();
                }
                if p.note.is_empty() {
                    p.note = o.note.clone();
                }
            }
            p
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rows(v: Value) -> Vec<Vec<Value>> {
        values_of(&v)
    }

    #[test]
    fn the_dividends_table_gives_each_months_total() {
        // The user's "Meu Patrimônio" tab as gog reads it unformatted: the title, months as the sheet's own dates or as
        // text, "#" columns between them, the amounts under, the total of the year elsewhere.
        let t = rows(json!({"values": [
            ["", "", "Total de Proventos =", 82.27],
            [],
            ["", "Proventos pagos"],
            ["", "abr./26", "#", "mai./26", "#", 46174, "#", "jul./26", "#", "ago./26", "#", "set./26", "#", "out./26", "nov./26", "dez./26"],
            ["", "", "", "", "", "", "", "", "", "", "", "", "", 82.27]
        ]}));
        let d = dividends(&t).unwrap();
        assert_eq!(d.months.get("2026-10"), Some(&82.27));
        assert_eq!(d.months.get("2026-06"), Some(&0.0), "a date serial (46174 = 01/06/2026) is a month too");
        assert_eq!(d.months.get("2026-04"), Some(&0.0), "a month with a column and nothing in it: R$ 0");
        assert_eq!(d.months.len(), 9);
        assert!(!d.months.contains_key("2026-03"), "a month the table has no column for: no record");
        assert_eq!(d.total, Some(82.27));
        assert!(d.by_asset.is_empty());
        // Asset by asset, the months' totals added up.
        let t = rows(json!({"values": [
            ["Dividendos recebidos"],
            ["Ativo", "09/2026", "10/2026"],
            ["GARE11", 16.93, 16.4],
            ["GGRC11", "R$ 19,70", 19.7],
        ]}));
        let d = dividends(&t).unwrap();
        assert_eq!(d.by_asset["2026-10"]["GARE11"], 16.4);
        assert!((d.months["2026-09"] - 36.63).abs() < 1e-9);
        assert!(dividends(&rows(json!({"values": [["Ativo", "Quantidade"], ["GARE11", 10]]}))).is_none());
        // "Proventos pagos" as one row of a monthly table, the months in its header above; the patrimônio's amounts
        // (in the tens of thousands, like the sheet's own dates) are not taken for months.
        let t = rows(json!({"values": [
            ["Mês", "set./26", "out./26", "nov./26"],
            ["Patrimônio", 45210.5, 46000, ""],
            ["Proventos pagos", "", 82.27, ""],
            ["Total de Proventos =", "R$ 82,27"]
        ]}));
        let d = dividends(&t).unwrap();
        assert_eq!(d.months.get("2026-10"), Some(&82.27));
        assert_eq!(d.months.get("2026-09"), Some(&0.0));
        assert_eq!(d.months.len(), 3);
        assert_eq!(d.total, Some(82.27));
    }

    #[test]
    fn a_target_column_by_class_gives_each_classs_share() {
        let t = rows(json!({"values": [
            ["Classe", "Atual", "Percentual Ideal"],
            ["Ações", 0.12, 0.25],
            ["FIIs", 0.61, "25%"],
            ["Renda Fixa", 0.27, 0.5],
            ["Total", 1, 1]
        ]}));
        let found = class_targets(&t);
        assert_eq!(found, [(AssetClass::Acao, 25.0), (AssetClass::Fii, 25.0), (AssetClass::RendaFixa, 50.0)]);
        assert!(class_targets(&rows(json!({"values": [["Ativo", "Percentual Ideal"], ["GARE11", 0.075]]}))).is_empty(), "a ticker is not a class");
    }

    #[test]
    fn a_target_column_gives_each_assets_share() {
        let t = rows(json!({"values": [
            ["Ativo", "Quantidade", "Preço médio", "% Ideal"],
            ["GARE11", 204, 8.95, 0.075],
            ["GGRC11", 150, 9.2, "10%"],
            ["KNSC11", 380, 9.1, ""]
        ]}));
        let read = positions(&t).unwrap();
        assert_eq!(read.targets, [("GARE11".to_string(), 7.5), ("GGRC11".to_string(), 10.0)]);
        let t = rows(json!({"values": [["Ticker", "Qtd", "Meta %"], ["GARE11", 204, 7.5]]}));
        assert_eq!(positions(&t).unwrap().targets, [("GARE11".to_string(), 7.5)]);
    }

    #[test]
    fn the_id_comes_from_a_link_or_as_it_is() {
        let id = "1AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-xyz";
        assert_eq!(spreadsheet_id(&format!("https://docs.google.com/spreadsheets/d/{id}/edit?gid=0#gid=0")).as_deref(), Some(id));
        assert_eq!(spreadsheet_id(&format!("  {id} ")).as_deref(), Some(id));
        assert_eq!(spreadsheet_id("not an id"), None);
        assert_eq!(spreadsheet_id("abc;rm -rf"), None);
        assert_eq!(range(""), "A1:Z400");
        assert_eq!(range("Meu Patrimônio"), "'Meu Patrimônio'!A1:Z400");
        assert_eq!(range("Ana's"), "'Ana''s'!A1:Z400");
    }

    #[test]
    fn positions_come_from_the_rows_under_the_header() {
        let sheet = json!({"range": "'Carteira'!A1:Z400", "values": [
            ["Minha carteira"],
            [],
            ["Ativo", "Classe", "Qtd.", "Preço Médio", "Cotação", "Valor atual"],
            ["GGRC11", "FII", 150, 9.2, 8.69, 1303.5],
            ["tims3", "Ação", "30", "18,60"],
            ["BOVA11", "ETF", 10, 120],
            ["GGRC11", "FII", 50, 10],
            ["PORD11", "FII", "", ""],
            ["RDB Nubank", "Renda Fixa", "", "", "", "2.500,00"],
            ["Total", "", "", "", "", 9999],
            ["Bitcoin", "", "", "", "", 1000],
            ["Something", "", 3, 1],
        ]});
        let read = positions(&rows(sheet)).unwrap();
        let tickers: Vec<&str> = read.positions.iter().map(|p| p.ticker.as_str()).collect();
        assert_eq!(tickers, ["GGRC11", "TIMS3", "BOVA11", "RDB Nubank", "Bitcoin"], "PORD11 has no quantity: watched, not held");
        let ggrc = &read.positions[0];
        assert_eq!((ggrc.quantity, ggrc.class), (200.0, AssetClass::Fii));
        assert!((ggrc.average_price - (150.0 * 9.2 + 50.0 * 10.0) / 200.0).abs() < 1e-9, "the average price weighted");
        assert_eq!((read.positions[1].quantity, read.positions[1].average_price, read.positions[1].class), (30.0, 18.6, AssetClass::Acao));
        assert_eq!(read.positions[2].class, AssetClass::Etf, "the sheet's class wins over the guess");
        let rdb = &read.positions[3];
        assert_eq!((rdb.class, rdb.current_value, rdb.quantity), (AssetClass::RendaFixa, Some(2500.0), 1.0));
        assert_eq!(read.positions[4].class, AssetClass::Cripto);
        assert_eq!(read.skipped, ["row 12: \"Something\" is not a B3 ticker"]);
    }

    #[test]
    fn the_users_ceilings_are_read_for_held_and_watched_assets() {
        let read = positions(&rows(json!([
            ["Ativo", "Classe", "Quantidade", "Preço médio", "Meu teto", "Cotação", "Valor", "Teto AXYON"],
            ["GGRC11", "FII", 150, 9.2, 9.5, 8.69, 1303.5, 10.56],
            ["KNRI11", "FII", "", "", 150],
            ["XPLG11", "FII", 3, 100, "", 98, 294, 105],
            ["RDB Nubank", "Renda Fixa", 1, 500, "", "", 512.3],
        ]))).unwrap();
        assert_eq!(read.ceilings, [("GGRC11".to_string(), 9.5), ("KNRI11".to_string(), 150.0)], "AXYON's own column is not the user's");
        assert!(read.has_ceilings);
        assert_eq!(read.order, ["GGRC11", "KNRI11", "XPLG11", "RDB Nubank"]);
        assert_eq!(read.positions.len(), 3, "KNRI11 is only watched");
        assert_eq!(read.positions[2].current_value, Some(512.3), "\"Valor\" is a fixed income's amount");
        assert_eq!(read.positions[2].average_price, 500.0, "what was put in: its average price");
        // A list of watched assets and their ceilings, as the user keeps one: no quantity, no amount.
        let read = positions(&rows(json!([
            ["Ativo", "Classe", "Status", "Cotação base (R$)", "Distribuição anual (R$)", "Yield alvo", "Preço teto (R$)", "Margem até o teto %"],
            ["HGLG11", "FII", "Mantido", 158, 13.2, 0.085, 155.3, -0.017],
            ["XPLG11", "FII", "Mantido", 98, 9.6, 0.09, 106.67, 0.088],
        ]))).unwrap();
        assert_eq!((read.positions.len(), read.ceilings.len()), (0, 2));
        assert_eq!(read.ceilings[1], ("XPLG11".to_string(), 106.67));
        // A row marked sold is left out.
        let read = positions(&rows(json!([["Ativo", "Status", "Preço teto"], ["HGLG11", "Mantido", 155], ["MXRF11", "Vendido", 10.5], ["VISC11", "zerado", 120]]))).unwrap();
        assert_eq!(read.order, ["HGLG11"]);
        // A fixed income typed with its quantity and price, and no amount yet.
        let read = positions(&rows(json!([["Ativo", "Classe", "Quantidade", "Preço médio"], ["CDB Inter 2028", "Renda Fixa", 2, 500]]))).unwrap();
        assert_eq!((read.positions[0].average_price, read.positions[0].current_value), (1000.0, Some(1000.0)));
    }

    #[test]
    fn the_class_is_guessed_without_a_column_and_a_sheet_without_a_header_says_so() {
        let read = positions(&rows(json!([["Ticker", "Quantidade"], ["PETR4", 100], ["XPML11", 12.5], ["AAPL34", 4]]))).unwrap();
        let classes: Vec<AssetClass> = read.positions.iter().map(|p| p.class).collect();
        assert_eq!(classes, [AssetClass::Acao, AssetClass::Fii, AssetClass::Bdr]);
        assert_eq!(read.positions[1].average_price, 0.0, "no average price: zero, the cost unknown");
        assert!(positions(&rows(json!({"values": [["a", "b"], ["GGRC11", 1]]}))).unwrap_err().contains("Ativo"));
        assert!(positions(&rows(json!({"values": []}))).is_err());
        assert_eq!(read.positions[1].note, "guessed", "B3's codes settle it later (refresh::fix_classes)");
        // What AXYON worked out stays: XPML11 known as a fund of the "tijolo" kind, PETR4 as it was.
        let known = vec![Position { ticker: "XPML11".into(), class: AssetClass::Fii, quantity: 3.0, average_price: 100.0, subtype: "tijolo".into(), current_value: None, note: String::new() }];
        let merged = with_known(read.positions.clone(), &known);
        assert_eq!((merged[1].class, merged[1].subtype.as_str(), merged[1].note.as_str(), merged[1].quantity), (AssetClass::Fii, "tijolo", "", 12.5));
        assert_eq!(with_known(merged.clone(), &merged), merged, "read again, nothing changes");
        // Invested amount instead of an average price.
        let read = positions(&rows(json!([["Código", "Cotas", "Valor investido"], ["HGLG11", 10, 1600]]))).unwrap();
        assert_eq!(read.positions[0].average_price, 160.0);
    }
}
