//! The user's portfolio and history, however they bring them: a table pasted from Investidor10 or a spreadsheet,
//! lines typed in the chat ("150 cotas de GGRC11 a 9,20"), B3's Área do Investidor exports (position, movements,
//! trades; Excel or CSV), or any table with a ticker, a quantity and a price. Classes come from the section a line is
//! under ("FIIs", "Ações"…) or the sheet, else a guess the app corrects with the official data.

use crate::date::Date;
use crate::model::{AssetClass, Event, EventKind, Position, Trade, TradeKind};
use crate::table::Table;
use crate::text::{norm, num, plain, ticker};

/// What came out of an import.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Imported {
    pub positions: Vec<Position>,
    pub trades: Vec<Trade>,
    pub events: Vec<Event>,
    /// "posição B3", "movimentação B3", "negociação B3", "tabela", "texto".
    pub kinds: Vec<String>,
    /// Lines or rows not understood, and what the user should know.
    pub warnings: Vec<String>,
    /// Positions whose class is a guess (the ticker ends in 11: a FII, an ETF or a unit).
    pub guessed: Vec<String>,
}

/// The class a ticker most likely is, with nothing else to go on: 3–8 shares, 11 a real-estate fund (ETFs and units
/// end in 11 too: `guessed`), 31–35 BDRs.
pub fn guess_class(t: &str) -> (AssetClass, bool) {
    let digits: String = t.chars().skip(4).take_while(|c| c.is_ascii_digit()).collect();
    match digits.as_str() {
        "3" | "4" | "5" | "6" | "7" | "8" => (AssetClass::Acao, false),
        "11" => (AssetClass::Fii, true),
        "31" | "32" | "33" | "34" | "35" | "39" => (AssetClass::Bdr, false),
        _ => (AssetClass::Outro, true),
    }
}

fn section(line: &str) -> Option<AssetClass> {
    let p = plain(line);
    let words: Vec<&str> = p.split(' ').filter(|w| !w.is_empty()).collect();
    if words.is_empty() || words.len() > 4 || ticker(line.trim()).is_some() {
        return None;
    }
    // "FIIs (4 ativos)", "Ações", "Tesouro Direto", "Renda Fixa".
    let head: String = words.iter().take_while(|w| !w.chars().all(|c| c.is_ascii_digit())).cloned().collect::<Vec<_>>().join(" ");
    AssetClass::parse(&head).filter(|_| words.iter().all(|w| w.chars().all(|c| c.is_alphanumeric())))
}

/// The numbers of a line in order, each with whether "R$" came right before it.
fn numbers(line: &str) -> Vec<(f64, bool)> {
    let mut out = Vec::new();
    let mut money = false;
    let cleaned = line.replace("R$", " R$ ");
    for word in cleaned.split(|c: char| c.is_whitespace() || c == '|' || c == ';' || c == '\t') {
        let w = word.trim_matches(|c: char| c == ':' || c == '(' || c == ')' || c == '*');
        if w.is_empty() {
            continue;
        }
        if w == "R$" {
            money = true;
            continue;
        }
        if ticker(w).is_some() {
            money = false;
            continue;
        }
        if let Some(v) = num(w).filter(|_| w.chars().any(|c| c.is_ascii_digit()) && !w.contains('/')) {
            out.push((v, money));
        }
        money = false;
    }
    out
}

/// Positions from free text: a pasted table or lines typed by the user. One position per line that names a ticker
/// and has a quantity; the price is the first amount in reais, else the second number.
pub fn positions_from_text(text: &str) -> Imported {
    let mut out = Imported { kinds: vec!["texto".into()], ..Imported::default() };
    // A pasted table with a header is read by its columns.
    let rows: Vec<Vec<String>> = text
        .lines()
        .map(|l| if l.contains('\t') { l.split('\t').map(|c| c.trim().to_string()).collect() } else { l.split("  ").map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect() })
        .collect();
    if let Some(found) = positions_from_rows(rows, None).filter(|i| !i.positions.is_empty()) {
        return found;
    }
    let mut class: Option<AssetClass> = None;
    for line in text.lines() {
        if let Some(c) = section(line) {
            class = Some(c);
            continue;
        }
        let Some(t) = line.split(|c: char| !c.is_ascii_alphanumeric()).find_map(|w| ticker(w).filter(|_| w.chars().take(4).all(|c| c.is_ascii_uppercase())))
        else {
            continue;
        };
        let nums = numbers(line);
        let quantity = nums.iter().find(|(_, money)| !money).map(|(v, _)| *v);
        let price = nums.iter().find(|(_, money)| *money).map(|(v, _)| *v).or_else(|| nums.iter().filter(|(_, m)| !m).nth(1).map(|(v, _)| *v));
        let Some(quantity) = quantity.filter(|q| *q > 0.0) else {
            out.warnings.push(format!("{t}: no quantity in \"{}\"", line.trim()));
            continue;
        };
        let (c, guessed) = match class {
            Some(c) => (c, false),
            None => guess_class(&t),
        };
        if guessed {
            out.guessed.push(t.clone());
        }
        out.positions.push(Position { ticker: t, class: c, quantity, average_price: price.unwrap_or(0.0), subtype: String::new(), current_value: None, note: String::new() });
    }
    merge_duplicates(&mut out.positions);
    out
}

/// The same ticker twice (two brokers): one position, the quantities added and the average price weighted.
pub fn merge_duplicates(positions: &mut Vec<Position>) {
    let mut out: Vec<Position> = Vec::new();
    for p in positions.drain(..) {
        if let Some(have) = out.iter_mut().find(|h| h.ticker == p.ticker && h.class.quoted()) {
            let cost = have.cost() + p.cost();
            have.quantity += p.quantity;
            have.average_price = if have.quantity > 0.0 { cost / have.quantity } else { 0.0 };
        } else {
            out.push(p);
        }
    }
    *positions = out;
}

/// Positions from rows with a header (a pasted table, a CSV, a sheet). `class` is the sheet's own class, if any.
pub fn positions_from_rows(rows: Vec<Vec<String>>, class: Option<AssetClass>) -> Option<Imported> {
    let table = Table::from_rows(
        rows,
        &[&["ativo", "ticker", "codigo_de_negociacao", "codigo", "papel", "produto", "acao", "fundo"], &["quantidade", "quant", "qtd", "qtde", "cotas"]],
    )?;
    let c_ticker = table.find(&["codigo_de_negociacao", "ticker", "ativo", "codigo", "papel", "acao", "fundo", "produto"]);
    let c_qty = table.find(&["quantidade", "quant", "qtd", "qtde", "cotas"]);
    let c_avg = table.find(&["preco_medio", "pm", "custo_medio", "preco_de_compra", "preco_medio_de_compra"]);
    let c_class = table.find(&["classe", "categoria", "tipo_de_ativo", "tipo_ativo"]);
    let c_value = table.find(&["valor_atualizado", "valor_aplicado", "saldo", "valor_total", "valor_bruto"]);
    let c_invested = table.find(&["valor_aplicado", "valor_investido", "total_investido"]);
    let mut out = Imported { kinds: vec!["tabela".into()], ..Imported::default() };
    for row in &table.rows {
        let raw = Table::cell(row, c_ticker);
        let t = raw.split([' ', '-']).find_map(ticker);
        let quantity = num(Table::cell(row, c_qty));
        let row_class = AssetClass::parse(Table::cell(row, c_class)).or(class);
        match (t, quantity) {
            (Some(t), Some(q)) if q > 0.0 => {
                let (c, guessed) = match row_class {
                    Some(c) => (c, false),
                    None => guess_class(&t),
                };
                if guessed {
                    out.guessed.push(t.clone());
                }
                out.positions.push(Position {
                    ticker: t,
                    class: c,
                    quantity: q,
                    average_price: num(Table::cell(row, c_avg)).unwrap_or(0.0),
                    subtype: String::new(),
                    current_value: None,
                    note: String::new(),
                });
            }
            (None, _) if !raw.is_empty() && matches!(row_class, Some(AssetClass::RendaFixa | AssetClass::Tesouro)) => {
                // Fixed income and Tesouro: a name and an amount.
                let value = num(Table::cell(row, c_value));
                let invested = num(Table::cell(row, c_invested)).or(value);
                if let Some(invested) = invested.filter(|v| *v > 0.0) {
                    out.positions.push(Position {
                        ticker: raw.to_string(),
                        class: row_class.unwrap_or(AssetClass::RendaFixa),
                        quantity: 1.0,
                        average_price: invested,
                        subtype: fixed_income_kind(&row.join(" ")).to_string(),
                        current_value: value,
                        note: String::new(),
                    });
                }
            }
            _ => {}
        }
    }
    merge_duplicates(&mut out.positions);
    Some(out)
}

/// "pos" (CDI, Selic), "ipca", "pre", or "" from a fixed-income product's name or index.
pub fn fixed_income_kind(text: &str) -> &'static str {
    let p = plain(text);
    if p.contains("ipca") || p.contains("ntn b") || p.contains("ntnb") {
        "ipca"
    } else if p.contains("cdi") || p.contains("selic") || p.contains("di ") || p.contains("pos") || p.contains("lft") {
        "pos"
    } else if p.contains("prefixado") || p.contains(" pre") || p.contains("ltn") || p.contains("ntn f") {
        "pre"
    } else {
        ""
    }
}

fn sheet_class(name: &str) -> Option<AssetClass> {
    let n = plain(name);
    if n.contains("fundo") || n.contains("fii") {
        Some(AssetClass::Fii)
    } else if n.contains("bdr") {
        Some(AssetClass::Bdr)
    } else if n.contains("etf") {
        Some(AssetClass::Etf)
    } else if n.contains("tesouro") {
        Some(AssetClass::Tesouro)
    } else if n.contains("renda fixa") {
        Some(AssetClass::RendaFixa)
    } else if n.starts_with("acoe") || n.starts_with("acao") || n.contains("acoes") {
        Some(AssetClass::Acao)
    } else {
        None
    }
}

/// The ticker of a B3 "Produto" ("GGRC11 - GGR COVEPI RENDA FDO INV IMOB").
fn product_ticker(product: &str) -> Option<String> {
    product.split(" - ").next().and_then(|first| first.split_whitespace().next()).and_then(ticker)
}

/// B3's movements: payouts and corporate events (trades come from the trades export; a "Transferência -
/// Liquidação" with a price counts as one when there is none).
fn movements(table: &Table, out: &mut Imported) {
    let c_dir = table.find(&["entrada_saida"]);
    let c_date = table.find(&["data"]);
    let c_kind = table.find(&["movimentacao"]);
    let c_product = table.find(&["produto"]);
    let c_qty = table.find(&["quantidade"]);
    let c_price = table.find(&["preco_unitario", "preco"]);
    let c_value = table.find(&["valor_da_operacao", "valor"]);
    for row in &table.rows {
        let Some(date) = Date::parse(Table::cell(row, c_date)) else { continue };
        let Some(t) = product_ticker(Table::cell(row, c_product)) else { continue };
        let kind = plain(Table::cell(row, c_kind));
        let credit = plain(Table::cell(row, c_dir)).starts_with("credito");
        let qty = num(Table::cell(row, c_qty)).unwrap_or(0.0);
        let value = num(Table::cell(row, c_value)).unwrap_or(0.0);
        let price = num(Table::cell(row, c_price)).unwrap_or(0.0);
        let event = |k: EventKind, quantity: f64, value: f64| Event { date, ticker: t.clone(), kind: k, quantity, value };
        let e = if kind.starts_with("rendimento") {
            Some(event(EventKind::Income, qty, value))
        } else if kind.starts_with("dividendo") {
            Some(event(EventKind::Dividend, qty, value))
        } else if kind.contains("juros sobre capital") {
            Some(event(EventKind::InterestOnEquity, qty, value))
        } else if kind.starts_with("amortizacao") {
            Some(event(EventKind::Amortization, qty, value))
        } else if kind.starts_with("bonificacao") {
            Some(event(EventKind::Bonus, qty, price))
        } else if kind.starts_with("desdobro") {
            Some(event(EventKind::Split, qty, 0.0))
        } else if kind.starts_with("grupamento") {
            Some(event(EventKind::Grouping, qty, 0.0))
        } else if kind.contains("subscricao") && kind.contains("exercido") && !kind.contains("nao exercido") {
            Some(event(EventKind::Subscription, qty, price))
        } else if kind.starts_with("transferencia liquidacao") && price > 0.0 && qty > 0.0 {
            out.trades.push(Trade { date, ticker: t.clone(), kind: if credit { TradeKind::Buy } else { TradeKind::Sell }, quantity: qty, price, ..Trade::default() });
            None
        } else {
            None
        };
        if let Some(e) = e {
            out.events.push(e);
        }
    }
}

/// The headers of a column saying buy or sell.
const SIDE: &[&str] = &["tipo_de_movimentacao", "tipo", "operacao", "c_v", "compra_venda"];

/// B3's trades, or a spreadsheet of the user's with a column saying buy or sell: each one with its price.
fn trades(table: &Table, out: &mut Imported) {
    let c_date = table.find(&["data_do_negocio", "data"]);
    let c_time = table.find(&["hora", "horario"]);
    let c_kind = table.find(SIDE);
    let c_ticker = table.find(&["codigo_de_negociacao", "ticker", "ativo", "codigo", "papel"]);
    let c_qty = table.find(&["quantidade", "qtd", "qtde", "cotas"]);
    let c_price = table.find(&["preco", "preco_unitario", "valor_unitario", "valor_da_cota"]);
    let c_fees = table.find(&["taxas", "custos", "corretagem"]);
    for row in &table.rows {
        let Some(date) = Date::parse(Table::cell(row, c_date)) else { continue };
        let Some(t) = ticker(Table::cell(row, c_ticker)) else { continue };
        let k = plain(Table::cell(row, c_kind));
        let kind = if k.starts_with("compra") || k == "c" || k.starts_with("buy") {
            TradeKind::Buy
        } else if k.starts_with("venda") || k == "v" || k.starts_with("sell") {
            TradeKind::Sell
        } else {
            continue;
        };
        let (Some(q), Some(p)) = (num(Table::cell(row, c_qty)), num(Table::cell(row, c_price))) else { continue };
        let time = crate::trades::time_of(Table::cell(row, c_time));
        out.trades.push(Trade { date, ticker: t, kind, quantity: q.abs(), price: p, fees: num(Table::cell(row, c_fees)).unwrap_or(0.0), time, ..Trade::default() });
    }
}

/// A spreadsheet of the user's own contributions: a date, a ticker, a quantity and a price (or the total paid), with
/// no column saying buy or sell — each row a buy.
fn contributions(table: &Table, out: &mut Imported) {
    let c_date = table.find(&["data_do_aporte", "data_da_compra", "data_do_negocio", "data"]);
    let c_time = table.find(&["hora", "horario"]);
    let c_ticker = table.find(&["codigo_de_negociacao", "ticker", "ativo", "codigo", "papel"]);
    let c_qty = table.find(&["quantidade", "qtd", "qtde", "cotas"]);
    let c_price = table.find(&["preco_unitario", "preco_pago", "preco_de_compra", "valor_unitario", "valor_da_cota", "preco"]);
    let c_total = table.find(&["valor_total", "total_investido", "valor_investido", "total"]);
    let c_fees = table.find(&["taxas", "custos", "corretagem"]);
    for row in &table.rows {
        let Some(date) = Date::parse(Table::cell(row, c_date)) else { continue };
        let Some(t) = ticker(Table::cell(row, c_ticker)) else { continue };
        let Some(q) = num(Table::cell(row, c_qty)).map(f64::abs).filter(|q| *q > 0.0) else { continue };
        let fees = num(Table::cell(row, c_fees)).unwrap_or(0.0).max(0.0);
        let price = num(Table::cell(row, c_price)).filter(|p| *p > 0.0).or_else(|| num(Table::cell(row, c_total)).map(|v| (v - fees) / q).filter(|p| *p > 0.0));
        let Some(price) = price else { continue };
        let time = crate::trades::time_of(Table::cell(row, c_time));
        out.trades.push(Trade { date, ticker: t, kind: TradeKind::Buy, quantity: q, price, fees, time, ..Trade::default() });
    }
}

/// Everything in a set of sheets (an Excel file's, or one CSV as one sheet).
pub fn from_sheets(sheets: Vec<(String, Vec<Vec<String>>)>) -> Imported {
    let mut out = Imported::default();
    for (name, rows) in sheets {
        let class = sheet_class(&name);
        if let Some(t) = Table::from_rows(rows.clone(), &[&["entrada_saida"], &["movimentacao"]]) {
            movements(&t, &mut out);
            out.kinds.push("movimentação B3".into());
            continue;
        }
        // A sheet whose side column holds something else ("Tipo": FII, Ação) gives no trade, and is read on below.
        if let Some(t) = Table::from_rows(rows.clone(), &[&["data_do_negocio", "data"], SIDE, &["preco", "valor_unitario", "valor_da_cota"]]) {
            if t.find(&["codigo_de_negociacao", "ticker", "ativo", "codigo", "papel"]).is_some() {
                let before = out.trades.len();
                trades(&t, &mut out);
                if out.trades.len() > before {
                    out.kinds.push("negociação B3".into());
                    continue;
                }
            }
        }
        if let Some(t) = Table::from_rows(rows.clone(), &[&["data_do_aporte", "data_da_compra", "data"], &["preco", "valor_unitario", "valor_da_cota", "valor_total", "total_investido"]]) {
            let no_side = t.find(SIDE).is_none();
            if no_side && t.find(&["codigo_de_negociacao", "ticker", "ativo", "codigo", "papel"]).is_some() && t.find(&["quantidade", "qtd", "qtde", "cotas"]).is_some() {
                let before = out.trades.len();
                contributions(&t, &mut out);
                if out.trades.len() > before {
                    out.kinds.push("planilha de aportes".into());
                    continue;
                }
            }
        }
        if let Some(found) = positions_from_rows(rows, class) {
            if !found.positions.is_empty() {
                out.kinds.push(if name.is_empty() { "tabela".into() } else { format!("posição ({name})") });
                out.positions.extend(found.positions);
                out.guessed.extend(found.guessed);
                out.warnings.extend(found.warnings);
            }
        }
    }
    merge_duplicates(&mut out.positions);
    // Trades seen in both exports (the movements' settlement and the trades list) are kept once.
    out.trades.sort_by(|a, b| a.date.cmp(&b.date).then(a.ticker.cmp(&b.ticker)));
    out.trades.dedup_by(|a, b| a.date == b.date && a.ticker == b.ticker && a.kind == b.kind && (a.quantity - b.quantity).abs() < 1e-9 && (a.price - b.price).abs() < 0.005);
    out.events.sort_by(|a, b| a.date.cmp(&b.date));
    out
}

/// A file the user gave: .xlsx, .csv or .txt. `name` is its file name (for its kind).
pub fn from_file(name: &str, bytes: &[u8]) -> Result<Imported, String> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".xlsx") || bytes.starts_with(b"PK\x03\x04") {
        let sheets = crate::xlsx::read(bytes)?;
        let imported = from_sheets(sheets.into_iter().map(|s| (s.name, s.rows)).collect());
        return if imported.positions.is_empty() && imported.trades.is_empty() && imported.events.is_empty() {
            Err("nothing in this spreadsheet looks like positions, trades or payouts".into())
        } else {
            Ok(imported)
        };
    }
    if lower.ends_with(".xls") {
        return Err("old Excel files (.xls) cannot be read: save it as .xlsx or .csv".into());
    }
    let text = crate::text::decode(bytes);
    let first = text.lines().next().unwrap_or("");
    let sep = crate::table::separator(first);
    let rows = crate::table::records(&text, sep);
    let imported = from_sheets(vec![(String::new(), rows)]);
    if !imported.positions.is_empty() || !imported.trades.is_empty() || !imported.events.is_empty() {
        return Ok(imported);
    }
    let typed = positions_from_text(&text);
    if typed.positions.is_empty() {
        return Err("nothing in this file looks like positions, trades or payouts".into());
    }
    Ok(typed)
}

/// Whether a column header names a ticker (for the app's own checks).
pub fn is_ticker_header(h: &str) -> bool {
    matches!(norm(h).as_str(), "ativo" | "ticker" | "codigo" | "codigo_de_negociacao" | "papel")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_pasted_from_investidor10_with_its_sections() {
        let text = "Ações (1 ativo)\nTIMS3\t30\tR$ 18,60\tR$ 19,20\t3,23%\n\nFIIs (4 ativos)\nGGRC11 150 R$ 9,20 R$ 9,12\nGARE11 180 R$ 8,55 R$ 8,40\nBTCI11\t140\tR$ 8,90\nPORD11 120 8,45\n\nETFs\nSPXB11 40 R$ 16,75 R$ 17,12\n";
        let got = positions_from_text(text);
        assert_eq!(got.positions.len(), 6, "{:?}", got.warnings);
        let tims = &got.positions[0];
        assert_eq!((tims.ticker.as_str(), tims.class, tims.quantity, tims.average_price), ("TIMS3", AssetClass::Acao, 30.0, 18.60));
        let pord = got.positions.iter().find(|p| p.ticker == "PORD11").unwrap();
        assert_eq!((pord.class, pord.quantity, pord.average_price), (AssetClass::Fii, 120.0, 8.45));
        let spxb = got.positions.iter().find(|p| p.ticker == "SPXB11").unwrap();
        assert_eq!(spxb.class, AssetClass::Etf, "the section says ETF");
        assert!(got.guessed.is_empty());
    }

    #[test]
    fn lines_typed_in_the_chat() {
        let got = positions_from_text("tenho 150 cotas de GGRC11 a R$ 9,20\ne 30 TIMS3 com PM 18,60\nVALE3 sem nada");
        assert_eq!(got.positions.len(), 2);
        assert_eq!(got.positions[0].average_price, 9.20);
        assert_eq!(got.positions[1].average_price, 18.60);
        assert_eq!(got.guessed, vec!["GGRC11"], "a ticker ending in 11 with no section is a guess");
        assert_eq!(got.warnings.len(), 1, "VALE3 has no quantity");
    }

    #[test]
    fn a_table_with_a_header_and_the_same_ticker_twice() {
        let text = "Ativo\tQuantidade\tPreço Médio\tClasse\nGARE11\t100\t8,00\tFII\nGARE11\t100\t9,00\tFII\nTIMS3\t30\t18,60\tAção\n";
        let got = positions_from_text(text);
        assert_eq!(got.positions.len(), 2);
        let gare = &got.positions[0];
        assert_eq!(gare.quantity, 200.0);
        assert!((gare.average_price - 8.5).abs() < 1e-9);
        assert_eq!(got.positions[1].class, AssetClass::Acao);
    }

    #[test]
    fn b3_position_export_with_fixed_income() {
        let bytes = crate::xlsx::build(&[
            ("Acoes", vec![vec!["Produto", "Instituição", "Código de Negociação", "Quantidade", "Preço de Fechamento", "Valor Atualizado"], vec!["TIMS3 - TIM S.A.", "XP", "TIMS3", "30", "19.2", "576"]]),
            ("Fundo de Investimento", vec![vec!["Produto", "Código de Negociação", "Quantidade", "Valor Atualizado"], vec!["GGRC11 - GGR COVEPI", "GGRC11", "150", "1368"]]),
            ("ETF", vec![vec!["Produto", "Código de Negociação", "Quantidade"], vec!["SPXB11 - ETF", "SPXB11", "40"]]),
            ("Tesouro Direto", vec![vec!["Produto", "Indexador", "Quantidade", "Valor Aplicado", "Valor Atualizado"], vec!["Tesouro IPCA+ 2035", "IPCA", "0.1", "45", "49.8"]]),
        ]);
        let got = from_file("posicao-2026-10-20.xlsx", &bytes).unwrap();
        assert_eq!(got.positions.len(), 4);
        assert_eq!(got.positions.iter().find(|p| p.ticker == "GGRC11").unwrap().class, AssetClass::Fii);
        assert_eq!(got.positions.iter().find(|p| p.ticker == "SPXB11").unwrap().class, AssetClass::Etf);
        let tesouro = got.positions.iter().find(|p| p.class == AssetClass::Tesouro).unwrap();
        assert_eq!(tesouro.ticker, "Tesouro IPCA+ 2035");
        assert_eq!((tesouro.average_price, tesouro.current_value), (45.0, Some(49.8)));
        assert_eq!(tesouro.subtype, "ipca");
        assert_eq!(got.positions.iter().find(|p| p.ticker == "TIMS3").unwrap().average_price, 0.0, "B3's position has no average price");
    }

    #[test]
    fn a_spreadsheet_of_contributions_is_read_as_buys() {
        let csv = "Data;Hora;Ativo;Quantidade;Preço;Taxas\n\
                   10/09/2026;14h30;GGRC11;100;9,00;0,50\n\
                   21/10/2026;10:05;GGRC11;50;9,60;\n\
                   21/10/2026;;TIMS3;10;19,00;0\n";
        let got = from_file("aportes.csv", csv.as_bytes()).unwrap();
        assert_eq!(got.kinds, vec!["planilha de aportes"]);
        assert!(got.positions.is_empty(), "a history, not a position list");
        assert_eq!(got.trades.len(), 3);
        let first = &got.trades[0];
        assert_eq!((first.ticker.as_str(), first.kind, first.quantity, first.price, first.fees, first.time.as_str()), ("GGRC11", TradeKind::Buy, 100.0, 9.0, 0.5, "14:30"));
        // A total paid instead of the price.
        let totals = "Data do aporte;Ticker;Cotas;Valor total\n05/10/2026;PORD11;10;86,00\n";
        let got = from_file("aportes.csv", totals.as_bytes()).unwrap();
        assert!((got.trades[0].price - 8.6).abs() < 1e-9);
        // With a column saying buy or sell: trades, not positions.
        for side in ["Tipo", "C/V", "Operação", "Compra/Venda"] {
            let (buy, sell) = if side == "C/V" { ("C", "V") } else { ("Compra", "Venda") };
            let csv = format!("Data;Hora;Ativo;{side};Qtde;Preço unitário;Corretagem\n10/09/2026;14:30;GGRC11;{buy};100;9,00;0,50\n21/10/2026;;GGRC11;{sell};20;9,60;0\n");
            let got = from_file("carteira.csv", csv.as_bytes()).unwrap();
            assert!(got.positions.is_empty(), "{side}: {:?}", got.positions);
            assert_eq!(got.trades.iter().map(|t| (t.kind, t.quantity)).collect::<Vec<_>>(), vec![(TradeKind::Buy, 100.0), (TradeKind::Sell, 20.0)], "{side}");
            assert_eq!((got.trades[0].time.as_str(), got.trades[0].fees), ("14:30", 0.5), "{side}");
        }
    }

    #[test]
    fn b3_movements_and_trades() {
        let csv = "Entrada/Saída;Data;Movimentação;Produto;Instituição;Quantidade;Preço unitário;Valor da Operação\n\
                   Credito;15/10/2026;Rendimento;GGRC11 - GGR COVEPI RENDA FDO INV IMOB;XP;150;0,088;13,20\n\
                   Credito;10/10/2026;Amortização;BTCI11 - BTG PACTUAL CRED IMOB;XP;140;0,05;7,00\n\
                   Credito;01/09/2026;Transferência - Liquidação;GARE11 - GUARDIAN;XP;180;8,55;1539,00\n\
                   Debito;02/09/2026;Transferência - Liquidação;TIMS3 - TIM S.A.;XP;10;20,00;200,00\n\
                   Credito;05/09/2026;Juros Sobre Capital Próprio;TIMS3 - TIM S.A.;XP;30;0,25;7,50\n\
                   Credito;06/09/2026;Desdobro;XPTO3 - X;XP;100;-;-\n";
        let got = from_file("movimentacao.csv", csv.as_bytes()).unwrap();
        assert_eq!(got.events.len(), 4);
        assert_eq!(got.events.iter().find(|e| e.kind == EventKind::Income).unwrap().value, 13.2);
        assert_eq!(got.events.iter().find(|e| e.kind == EventKind::Amortization).unwrap().ticker, "BTCI11");
        assert_eq!(got.trades.len(), 2);
        assert_eq!(got.trades[0].kind, TradeKind::Buy);
        assert_eq!(got.trades[1].kind, TradeKind::Sell);
        let neg = "Data do Negócio;Tipo de Movimentação;Mercado;Prazo/Vencimento;Instituição;Código de Negociação;Quantidade;Preço;Valor\n\
                   01/09/2026;Compra;Mercado à Vista;-;XP;GARE11;180;8,55;1539,00\n\
                   02/09/2026;Venda;Mercado Fracionário;-;XP;TIMS3F;10;20,00;200,00\n";
        let t = from_file("negociacao.csv", neg.as_bytes()).unwrap();
        assert_eq!(t.trades.len(), 2);
        assert_eq!(t.trades[1].ticker, "TIMS3", "the odd lot is the same share");
        assert!(from_file("x.xls", b"").is_err());
        assert!(from_file("x.txt", b"nada aqui").is_err());
    }
}
