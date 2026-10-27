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
}

/// What a header cell names, by its words (any case, accents or not).
fn column(header: &str) -> Option<Column> {
    let p = plain(header);
    let p = p.trim().trim_end_matches([':', '.']);
    let is = |words: &[&str]| words.iter().any(|w| p == *w);
    let starts = |words: &[&str]| words.iter().any(|w| p.starts_with(w));
    Some(if is(&["ativo", "ativos", "ticker", "papel", "acao", "simbolo", "cod", "codigo", "asset", "symbol"]) || starts(&["codigo do", "ticker ", "ativo "]) {
        Column::Asset
    } else if starts(&["quantidade", "qtd", "qtde", "quant", "n de cotas", "numero de cotas", "num de cotas", "cotas", "quantity", "shares"]) || is(&["qt"]) {
        Column::Quantity
    } else if p.contains("preco medio") || p.contains("custo medio") || p.contains("preco de compra") || p.contains("valor medio") || is(&["pm", "p.m", "p. medio", "average price", "avg price", "medio"]) {
        Column::AveragePrice
    } else if is(&["classe", "tipo", "categoria", "classe do ativo", "tipo de ativo", "class", "type"]) {
        Column::Class
    } else if starts(&["valor atual", "saldo", "valor de mercado", "patrimonio atual"]) {
        Column::Value
    } else if starts(&["valor investido", "total investido", "valor aplicado", "investido", "aplicado"]) {
        Column::Invested
    } else {
        return None;
    })
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

/// The positions of a sheet's rows: the first row (of the first 30) that names an asset column and a quantity or
/// amount column is the header; each row under it with a B3 ticker and a quantity above zero is a position, and a row
/// with a fixed-income name and an amount is one too. The same ticker twice is one position (quantities added, the
/// average price weighted). Err when no such header is there.
pub fn positions(rows: &[Vec<Value>]) -> Result<Read, String> {
    let header = rows.iter().take(30).enumerate().find_map(|(i, row)| {
        let columns: Vec<Option<Column>> = row.iter().map(|c| column(&cell_text(c))).collect();
        let has = |k: Column| columns.iter().any(|c| *c == Some(k));
        (has(Column::Asset) && (has(Column::Quantity) || has(Column::Value))).then_some((i, columns))
    });
    let Some((at, columns)) = header else {
        return Err(NO_HEADER.into());
    };
    let index = |k: Column| columns.iter().position(|c| *c == Some(k));
    let (asset, quantity, average, class, value, invested) = (
        index(Column::Asset).unwrap_or(0),
        index(Column::Quantity),
        index(Column::AveragePrice),
        index(Column::Class),
        index(Column::Value),
        index(Column::Invested),
    );
    let mut read = Read::default();
    for (i, row) in rows.iter().enumerate().skip(at + 1) {
        let get = |k: Option<usize>| k.and_then(|k| row.get(k));
        let name = get(Some(asset)).map(cell_text).unwrap_or_default();
        if name.is_empty() || is_total(&name) {
            continue;
        }
        let line = i + 1;
        let given_class = get(class).map(cell_text).and_then(|c| AssetClass::parse(&c));
        let amount = get(value).and_then(cell_number).filter(|v| *v > 0.0);
        match ticker(&name).filter(|_| given_class.is_none_or(AssetClass::quoted)) {
            Some(t) => {
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
                let invested_amount = get(invested).and_then(cell_number).filter(|v| *v > 0.0);
                match (class, amount.or(invested_amount)) {
                    (Some(class), Some(now)) => read.positions.push(Position {
                        ticker: name.chars().take(60).collect(),
                        class,
                        quantity: 1.0,
                        average_price: invested_amount.unwrap_or(now),
                        subtype: String::new(),
                        current_value: Some(now),
                        note: String::new(),
                    }),
                    _ => read.skipped.push(format!("row {line}: \"{}\" is not a B3 ticker", name.chars().take(40).collect::<String>())),
                }
            }
        }
    }
    Ok(read)
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
