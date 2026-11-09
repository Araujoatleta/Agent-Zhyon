//! The Carteira outside the app, and back: an Excel workbook (a summary, the positions, the lançamentos, the proventos,
//! and an out-of-sight sheet holding everything AXYON keeps, so that importing it brings all back as it was: the
//! lançamentos, the proventos recorded, the committee's analyses in each asset's folder, the decisions), a CSV of the
//! same records (the positions, the lançamentos, the proventos, the days of purchase and the targets), and their import.
//! The import also takes the lançamentos and proventos of B3's Área do Investidor files (Negociação, Movimentação).
//! The PDF report is drawn by the app.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::date::Date;
use crate::model::{AssetClass, EventKind, Market, Position, ProventoEntry, Store, Trade, TradeKind};
use crate::xlsx::{Cell, Tab};

/// The out-of-sight sheet with everything AXYON keeps.
pub const BACKUP_SHEET: &str = "Backup AXYON";
/// Its first cell: what tells AXYON's own backup from any sheet with that name.
const MARK: &str = "AXYON backup 1";
/// Excel takes 32 767 characters in a cell.
const CHUNK: usize = 30_000;

// ── base64, for the backup's bytes in the workbook and the files the app passes ──────────────────────────────────

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn unbase64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut n = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' | b'\t' => continue,
            _ => return None,
        } as u32;
        n = n << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits & 0xff) as u8);
        }
    }
    Some(out)
}

fn deflate(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(bytes).expect("in memory");
    enc.finish().expect("in memory")
}

fn inflate(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::DeflateDecoder::new(bytes).read_to_end(&mut out).map_err(|_| "o backup do AXYON neste arquivo está danificado".to_string())?;
    Ok(out)
}

// ── What is exported ──────────────────────────────────────────────────────────────────────────────────────────────

fn n(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

fn day_br(v: &Value) -> String {
    v.as_str().and_then(Date::parse).map(|d| d.br()).unwrap_or_default()
}

fn class_label(id: &str) -> &'static str {
    match id {
        "acao" => "Ações",
        "fii" => "FIIs",
        "etf" => "ETFs",
        "bdr" => "BDRs",
        "renda_fixa" => "Renda fixa",
        "tesouro" => "Tesouro",
        "cripto" => "Cripto",
        _ => "Outros",
    }
}

fn basis_label(id: &str) -> &'static str {
    match id {
        "lancamentos" => "pelos lançamentos",
        "planilha" => "pela sua planilha",
        "posicao" => "pela posição atual",
        "estimado" => "estimado",
        "lancado" => "lançado por você",
        _ => "",
    }
}

/// Every provento of the screen, with where it stands: received, to receive, announced.
fn all_proventos(screen: &Value) -> Vec<(&'static str, &Value)> {
    let mut out = Vec::new();
    for (key, label) in [("ahead", "anunciado"), ("pending", "a receber"), ("received", "recebido")] {
        for p in screen["income"][key].as_array().into_iter().flatten() {
            out.push((label, p));
        }
    }
    out
}

/// The Excel workbook: what the user reads, and the backup that restores it all.
pub fn workbook(store: &Store, market: &Market, today: Date) -> Vec<u8> {
    let screen = crate::wallet::view(store, market, today);
    let head = |t: &str| Cell::Head(t.to_string());
    let text = |t: &str| Cell::Text(t.to_string());
    let mut tabs = Vec::new();

    // The summary.
    let income = &screen["income"];
    let line = |label: &str, value: Cell| vec![text(label), value];
    let mut summary = vec![
        vec![head("AXYON · Carteira"), head("")],
        line("Gerado em", text(&today.br())),
        line("Patrimônio", Cell::Money(n(&screen["total"]))),
        line("Investido", Cell::Money(n(&screen["cost"]))),
        line("Resultado", Cell::Money(n(&screen["gain"]))),
        line("Realizado em vendas", Cell::Money(n(&screen["realized"]))),
        line("Proventos em 12 meses", Cell::Money(n(&income["twelve"]))),
        line("Proventos no ano", Cell::Money(n(&income["year"]))),
        line("A receber", Cell::Money(n(&income["provisioned"]))),
    ];
    if let Some(y) = income["yieldOnCost"].as_f64() {
        summary.push(line("Yield on cost (%)", Cell::Number((y * 100.0).round() / 100.0)));
    }
    summary.push(vec![]);
    summary.push(vec![text("Para restaurar: AXYON → Carteira → Backup → Importar este arquivo. A aba oculta \"Backup AXYON\" guarda tudo; não a altere.")]);
    tabs.push(Tab { name: "Resumo".into(), widths: vec![28.0, 18.0], rows: summary, hidden: false });

    // The positions.
    let mut rows = vec![["Ativo", "Classe", "Quantidade", "Preço médio", "Cotação", "Valor", "Custo", "Resultado", "Resultado (%)", "Peso (%)", "Meta (%)", "Proventos 12 meses", "Desde"].map(head).to_vec()];
    for class in screen["classes"].as_array().into_iter().flatten() {
        for a in class["assets"].as_array().into_iter().flatten() {
            rows.push(vec![
                text(a["ticker"].as_str().unwrap_or("")),
                text(class_label(a["class"].as_str().unwrap_or(""))),
                Cell::Number(n(&a["quantity"])),
                Cell::Money(n(&a["average"])),
                a["price"].as_f64().map(Cell::Money).unwrap_or(text("")),
                Cell::Money(n(&a["value"])),
                Cell::Money(n(&a["cost"])),
                Cell::Money(n(&a["gain"])),
                a["gainPct"].as_f64().map(|x| Cell::Number((x * 100.0).round() / 100.0)).unwrap_or(text("")),
                Cell::Number((n(&a["weight"]) * 100.0).round() / 100.0),
                a["target"].as_f64().map(Cell::Number).unwrap_or(text("")),
                Cell::Money(n(&a["dividends12"])),
                text(&day_br(&a["heldFrom"])),
            ]);
        }
    }
    tabs.push(Tab { name: "Posições".into(), widths: vec![12.0, 12.0, 12.0, 14.0, 12.0, 14.0, 14.0, 14.0, 14.0, 10.0, 10.0, 18.0, 12.0], rows, hidden: false });

    // The lançamentos, all of them.
    let mut rows = vec![["Data", "Ativo", "Operação", "Quantidade", "Preço", "Taxas", "Total", "Resultado da venda", "Classe", "Id"].map(head).to_vec()];
    let mut trades: Vec<&Trade> = store.trades.iter().collect();
    trades.sort_by(|a, b| a.date.cmp(&b.date).then(a.seq.cmp(&b.seq)));
    let results: BTreeMap<u64, f64> = screen["trades"].as_array().into_iter().flatten().filter_map(|t| Some((t["id"].as_u64()?, t["result"].as_f64()?))).collect();
    for t in trades {
        let kind = if t.source == crate::wallet::OPENING { "saldo inicial" } else if t.kind == TradeKind::Sell { "venda" } else { "compra" };
        let total = if t.kind == TradeKind::Sell { t.quantity * t.price - t.fees } else { t.quantity * t.price + t.fees };
        rows.push(vec![
            text(&t.date.br()),
            text(&t.ticker),
            text(kind),
            Cell::Number(t.quantity),
            Cell::Money(t.price),
            Cell::Money(t.fees),
            Cell::Money(total),
            results.get(&t.seq).map(|r| Cell::Money(*r)).unwrap_or(text("")),
            text(t.class.map(|c| class_label(c.id())).unwrap_or("")),
            Cell::Number(t.seq as f64),
        ]);
    }
    tabs.push(Tab { name: "Lançamentos".into(), widths: vec![12.0, 10.0, 14.0, 12.0, 12.0, 10.0, 14.0, 18.0, 12.0, 6.0], rows, hidden: false });

    // The proventos: received, to receive, announced, each with how it was counted.
    let mut rows = vec![["Ativo", "Tipo", "Data-com", "Pagamento", "Quantidade", "Valor por cota", "Líquido", "Situação", "Como foi contado", "Lançado (id)"].map(head).to_vec()];
    for (state, p) in all_proventos(&screen) {
        rows.push(vec![
            text(p["ticker"].as_str().unwrap_or("")),
            text(p["kind"].as_str().unwrap_or("")),
            text(&day_br(&p["record"])),
            text(&day_br(&p["payment"])),
            Cell::Number(n(&p["quantity"])),
            Cell::Number(n(&p["perShare"])),
            Cell::Money(n(&p["net"])),
            text(state),
            text(basis_label(p["basis"].as_str().unwrap_or(""))),
            p["id"].as_u64().map(|i| Cell::Number(i as f64)).unwrap_or(text("")),
        ]);
    }
    tabs.push(Tab { name: "Proventos".into(), widths: vec![10.0, 16.0, 12.0, 12.0, 12.0, 14.0, 12.0, 12.0, 18.0, 12.0], rows, hidden: false });

    // The income tax of last year, to declare.
    let tax = crate::insight::irpf(store, market, today.y - 1, today);
    let mut rows = vec![["Grupo", "Código", "Ativo", "CNPJ", "Discriminação", "Situação em 31/12 anterior", "Situação em 31/12"].map(head).to_vec()];
    for g in tax["goods"].as_array().into_iter().flatten() {
        let s = |k: &str| g[k].as_str().unwrap_or("").to_string();
        rows.push(vec![text(&s("group")), text(&s("code")), text(&s("ticker")), text(&s("cnpj")), text(&s("text")), Cell::Money(n(&g["before"])), Cell::Money(n(&g["after"]))]);
    }
    rows.push(vec![]);
    for (title, key) in [("Rendimentos isentos — FII", "exempt"), ("Rendimentos isentos — dividendos", "dividends"), ("Tributação exclusiva — JCP", "jcp")] {
        for x in tax[key].as_array().into_iter().flatten() {
            rows.push(vec![text(title), text(""), text(x["ticker"].as_str().unwrap_or("")), text(x["cnpj"].as_str().unwrap_or("")), text(x["name"].as_str().unwrap_or("")), text(""), Cell::Money(n(&x["value"]))]);
        }
    }
    rows.push(vec![]);
    rows.push(vec![text("Confira com os informes da corretora e da B3 antes de declarar.")]);
    tabs.push(Tab { name: format!("IRPF {}", today.y - 1), widths: vec![8.0, 8.0, 10.0, 22.0, 70.0, 22.0, 18.0], rows, hidden: false });

    // Everything AXYON keeps, out of sight.
    let kept = serde_json::to_vec(store).unwrap_or_default();
    let packed = base64(&deflate(&kept));
    let mut rows = vec![vec![text(MARK)], vec![text("Não altere esta aba: é o que o AXYON usa para restaurar a carteira inteira.")]];
    let bytes = packed.as_bytes();
    for part in bytes.chunks(CHUNK) {
        rows.push(vec![text(std::str::from_utf8(part).unwrap_or_default())]);
    }
    tabs.push(Tab { name: BACKUP_SHEET.into(), widths: vec![60.0], rows, hidden: true });
    crate::xlsx::write(&tabs)
}

/// A CSV of the records (";" between columns, the decimal comma, as Excel in Brazil opens it): what importing it brings
/// back. Its proventos "calculado" are for reading: they come again from the sources.
pub fn csv(store: &Store, market: &Market, today: Date) -> String {
    let screen = crate::wallet::view(store, market, today);
    let num = |x: f64| {
        let s = format!("{}", (x * 1e8).round() / 1e8);
        s.replace('.', ",")
    };
    let cell = |s: &str| if s.contains([';', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() };
    let mut lines = vec!["Tipo;Ativo;Data;Operação;Quantidade;Preço;Taxas;Data-com;Pagamento;Valor;Classe;Id;Subtipo".to_string()];
    let mut row = |cols: [String; 13]| lines.push(cols.iter().map(|c| cell(c)).collect::<Vec<_>>().join(";"));
    for p in &store.portfolio.positions {
        row(["posição".into(), p.ticker.clone(), String::new(), String::new(), num(p.quantity), num(p.average_price), String::new(), String::new(), String::new(), p.current_value.map(num).unwrap_or_default(), p.class.id().into(), String::new(), p.subtype.clone()]);
    }
    let mut trades: Vec<&Trade> = store.trades.iter().collect();
    trades.sort_by(|a, b| a.date.cmp(&b.date).then(a.seq.cmp(&b.seq)));
    for t in trades {
        let kind = if t.source == crate::wallet::OPENING { "saldo inicial" } else if t.kind == TradeKind::Sell { "venda" } else { "compra" };
        row(["lançamento".into(), t.ticker.clone(), t.date.br(), kind.into(), num(t.quantity), num(t.price), num(t.fees), String::new(), String::new(), String::new(), t.class.map(|c| c.id().to_string()).unwrap_or_default(), t.seq.to_string(), String::new()]);
    }
    for m in &store.proventos {
        row(["provento lançado".into(), m.ticker.clone(), String::new(), m.kind.clone(), m.quantity.map(num).unwrap_or_default(), m.per_share.map(num).unwrap_or_default(), String::new(), m.record.map(|d| d.br()).unwrap_or_default(), m.payment.map(|d| d.br()).unwrap_or_default(), num(m.value), String::new(), m.id.to_string(), String::new()]);
    }
    for (state, p) in all_proventos(&screen).into_iter().filter(|(_, p)| p["basis"] != "lancado") {
        row([format!("provento calculado ({state})"), p["ticker"].as_str().unwrap_or("").into(), String::new(), p["kind"].as_str().unwrap_or("").into(), num(n(&p["quantity"])), num(n(&p["perShare"])), String::new(), day_br(&p["record"]), day_br(&p["payment"]), num(n(&p["net"])), String::new(), String::new(), String::new()]);
    }
    for (t, d) in &store.held_from {
        row(["desde".into(), t.clone(), d.br(), String::new(), String::new(), String::new(), String::new(), String::new(), String::new(), String::new(), String::new(), String::new(), String::new()]);
    }
    for (t, x) in &store.portfolio.asset_targets {
        row(["meta".into(), t.clone(), String::new(), String::new(), String::new(), String::new(), String::new(), String::new(), String::new(), num(*x), String::new(), String::new(), String::new()]);
    }
    // The BOM: Excel reads the accents right.
    format!("\u{feff}{}\r\n", lines.join("\r\n"))
}

// ── What comes back ───────────────────────────────────────────────────────────────────────────────────────────────

/// The records a CSV or a workbook's sheets hold.
#[derive(Debug, Default)]
struct Records {
    positions: Vec<Position>,
    trades: Vec<Trade>,
    proventos: Vec<ProventoEntry>,
    since: BTreeMap<String, Date>,
    targets: BTreeMap<String, f64>,
}

impl Records {
    fn is_empty(&self) -> bool {
        self.positions.is_empty() && self.trades.is_empty() && self.proventos.is_empty()
    }
}

fn col(header: &[String], names: &[&str]) -> Option<usize> {
    header.iter().position(|h| names.contains(&crate::text::plain(h).as_str()))
}

/// The records of AXYON's own CSV (typed rows), or of the workbook's Posições, Lançamentos and Proventos sheets.
fn records_of(sheets: &[(String, Vec<Vec<String>>)], market: &Market, today: Date) -> Records {
    let mut r = Records::default();
    let num = |s: &str| crate::text::num(s);
    for (name, rows) in sheets {
        let Some(header) = rows.first() else { continue };
        let plain_name = crate::text::plain(name);
        let get = |row: &Vec<String>, names: &[&str]| col(header, names).and_then(|i| row.get(i)).map(|s| s.trim().to_string()).unwrap_or_default();
        // AXYON's CSV: a "Tipo" of row and an "Operação" (the workbook's Proventos sheet has a "Tipo" of provento only).
        let typed = col(header, &["tipo"]).is_some() && col(header, &["operacao"]).is_some();
        for row in rows.iter().skip(1) {
            let kind = if typed { crate::text::plain(&get(row, &["tipo"])) } else if plain_name.starts_with("lancamento") { "lancamento".into() } else if plain_name.starts_with("posic") { "posicao".into() } else if plain_name.starts_with("provento") { if get(row, &["lancado id"]).is_empty() { String::new() } else { "provento lancado".into() } } else { String::new() };
            let Some(ticker) = crate::text::ticker(&get(row, &["ativo"])).or_else(|| (kind == "posicao").then(|| get(row, &["ativo"])).filter(|t| !t.is_empty())) else { continue };
            let class = AssetClass::parse(&get(row, &["classe"]));
            match kind.as_str() {
                "posicao" => {
                    let quantity = num(&get(row, &["quantidade"])).unwrap_or(0.0);
                    let average = num(&get(row, &["preco medio", "preco"])).unwrap_or(0.0);
                    let class = class.unwrap_or_else(|| crate::trades::class_of(&ticker, market).0);
                    r.positions.push(Position { ticker, class, quantity, average_price: average, subtype: get(row, &["subtipo"]), current_value: num(&get(row, &["valor"])).filter(|_| !class.quoted()), note: String::new() });
                }
                "lancamento" => {
                    let Some(date) = Date::parse(&get(row, &["data"])) else { continue };
                    let op = crate::text::plain(&get(row, &["operacao"]));
                    let (Some(quantity), Some(price)) = (num(&get(row, &["quantidade"])), num(&get(row, &["preco"]))) else { continue };
                    let source = if op.starts_with("saldo") { crate::wallet::OPENING } else { crate::wallet::APP };
                    let seq = num(&get(row, &["id"])).map(|x| x as u64).unwrap_or(0);
                    r.trades.push(Trade { date, ticker, kind: if op.starts_with("vend") { TradeKind::Sell } else { TradeKind::Buy }, quantity, price, fees: num(&get(row, &["taxas"])).unwrap_or(0.0), class, time: String::new(), source: source.into(), seq, average: (source == crate::wallet::OPENING).then_some(price) });
                }
                "provento lancado" => {
                    let input = json!({
                        "ticker": ticker,
                        "kind": if typed { get(row, &["operacao"]) } else { get(row, &["tipo"]) },
                        "payment": get(row, &["pagamento"]),
                        "record": get(row, &["data com"]),
                        "value": num(&get(row, &["valor", "liquido"])),
                        "per_share": num(&get(row, &["preco", "valor por cota"])),
                        "quantity": num(&get(row, &["quantidade"])),
                        "source": "import",
                    });
                    if let Ok(mut e) = crate::wallet::provento_entry(&input, market, today) {
                        e.id = num(&get(row, &["id", "lancado id"])).map(|x| x as u64).unwrap_or(0);
                        r.proventos.push(e);
                    }
                }
                "desde" => {
                    if let Some(d) = Date::parse(&get(row, &["data"])) {
                        r.since.insert(ticker, d);
                    }
                }
                "meta" => {
                    if let Some(x) = num(&get(row, &["valor"])).filter(|x| *x > 0.0 && *x <= 100.0) {
                        r.targets.insert(ticker, x);
                    }
                }
                _ => {}
            }
        }
    }
    r
}

/// The records of B3's Área do Investidor files: the trades of Negociação, the proventos received of Movimentação.
fn records_of_b3(name: &str, bytes: &[u8]) -> Option<Records> {
    let imported = crate::import::from_file(name, bytes).ok()?;
    let mut r = Records::default();
    for mut t in imported.trades {
        t.source = crate::wallet::APP.into();
        t.seq = 0;
        r.trades.push(t);
    }
    for e in imported.events.iter().filter(|e| e.value > 0.0) {
        let kind = match e.kind {
            EventKind::Income => "Rendimento",
            EventKind::Dividend => "Dividendo",
            EventKind::InterestOnEquity => "Juros sobre capital próprio",
            EventKind::Amortization => "Amortização",
            _ => continue,
        };
        r.proventos.push(ProventoEntry { id: 0, ticker: e.ticker.clone(), kind: kind.into(), record: None, payment: Some(e.date), per_share: (e.quantity > 0.0).then(|| e.value / e.quantity), quantity: (e.quantity > 0.0).then_some(e.quantity), value: (e.value * 100.0).round() / 100.0, source: "import".into() });
    }
    (!r.trades.is_empty() || !r.proventos.is_empty()).then_some(r)
}

/// Brings a file back: AXYON's workbook restores everything it kept (the whole record, as it was when exported);
/// AXYON's CSV, or a workbook whose backup sheet is gone, the Carteira's records (they replace the ones here); a B3
/// file adds its lançamentos and proventos to the ones here. What was done, in Portuguese.
pub fn restore(store: &mut Store, market: &Market, name: &str, bytes: &[u8], today: Date) -> Result<String, String> {
    let is_book = bytes.starts_with(b"PK\x03\x04");
    let sheets: Vec<(String, Vec<Vec<String>>)> = if is_book {
        crate::xlsx::read(bytes)?.into_iter().map(|s| (s.name, s.rows)).collect()
    } else {
        let text = crate::text::decode(bytes);
        let text = text.trim_start_matches('\u{feff}');
        let first = text.lines().next().unwrap_or("");
        vec![(String::new(), crate::table::records(text, crate::table::separator(first)))]
    };
    // The whole record.
    if let Some((_, rows)) = sheets.iter().find(|(n, rows)| n == BACKUP_SHEET && rows.first().and_then(|r| r.first()).is_some_and(|c| c == MARK)) {
        let packed: String = rows.iter().skip(2).filter_map(|r| r.first()).map(|c| c.trim()).collect();
        let bytes = unbase64(&packed).ok_or("o backup do AXYON neste arquivo está danificado")?;
        let restored: Store = serde_json::from_slice(&inflate(&bytes)?).map_err(|_| "o backup do AXYON neste arquivo não pôde ser lido".to_string())?;
        *store = restored;
        crate::wallet::ensure_seqs(store);
        crate::wallet::apply_ledger(store, market);
        return Ok(format!(
            "Carteira restaurada por inteiro: {} posições, {} lançamentos, {} proventos lançados, {} análises do comitê e {} decisões.",
            store.portfolio.positions.len(), store.trades.len(), store.proventos.len(), store.verdicts.len(), store.decisions.len()
        ));
    }
    let records = records_of(&sheets, market, today);
    if !records.is_empty() {
        let (p, t, v) = (records.positions.len(), records.trades.len(), records.proventos.len());
        if !records.positions.is_empty() {
            store.portfolio.positions = records.positions;
        }
        store.trades = records.trades;
        store.proventos = records.proventos;
        store.held_from = records.since;
        if !records.targets.is_empty() {
            store.portfolio.asset_targets = records.targets;
        }
        renumber(store);
        crate::wallet::apply_ledger(store, market);
        store.portfolio.updated = Some(today);
        return Ok(format!("Carteira restaurada: {p} posições, {t} lançamentos e {v} proventos lançados."));
    }
    // B3's files: added to what is here, what is here already left alone.
    let b3 = records_of_b3(name, bytes).ok_or("Este arquivo não é um backup do AXYON nem um arquivo de lançamentos ou proventos da B3.")?;
    let mut added = (0, 0);
    for t in b3.trades {
        let dup = store.trades.iter().any(|x| x.ticker.eq_ignore_ascii_case(&t.ticker) && x.date == t.date && x.kind == t.kind && (x.quantity - t.quantity).abs() < 1e-9 && (x.price - t.price).abs() < 0.005);
        if !dup {
            store.trades.push(t);
            added.0 += 1;
        }
    }
    for e in b3.proventos {
        if crate::wallet::add_provento(store, e).is_ok() {
            added.1 += 1;
        }
    }
    renumber(store);
    crate::wallet::apply_ledger(store, market);
    store.portfolio.updated = Some(today);
    Ok(format!("Do arquivo da B3: {} lançamentos e {} proventos novos (o que já estava aqui ficou como estava).", added.0, added.1))
}

/// Every lançamento and recorded provento with an id of its own, in their order.
fn renumber(store: &mut Store) {
    store.trades.sort_by(|a, b| a.date.cmp(&b.date).then(a.seq.cmp(&b.seq)));
    let mut seen = std::collections::BTreeSet::new();
    for t in &mut store.trades {
        if t.seq == 0 || !seen.insert(t.seq) {
            t.seq = 0;
        }
    }
    crate::wallet::ensure_seqs(store);
    let mut next = store.proventos.iter().map(|m| m.id).max().unwrap_or(0);
    let mut seen = std::collections::BTreeSet::new();
    for m in &mut store.proventos {
        if m.id == 0 || !seen.insert(m.id) {
            next += 1;
            m.id = next;
        }
    }
}

/// The file's name: "AXYON-carteira-2026-11-05.xlsx".
pub fn file_name(today: Date, extension: &str) -> String {
    format!("AXYON-carteira-{}.{extension}", today)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::fixtures;
    use crate::wallet::{add, add_provento, entry, provento_entry};

    fn day(y: i32, m: u32, d: u32) -> Date {
        Date::new(y, m, d).unwrap()
    }

    fn sample() -> (Store, Market, Date) {
        let today = day(2026, 11, 5);
        let market = fixtures::market();
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        add(&mut store, &market, entry(&json!({"ticker": "GARE11", "kind": "buy", "date": "2026-09-01", "quantity": 204, "price": "8,50", "fees": 1.2}), today).unwrap(), today).unwrap();
        add(&mut store, &market, entry(&json!({"ticker": "GARE11", "kind": "sell", "date": "2026-10-20", "quantity": 4, "price": 8.9}), today).unwrap(), today).unwrap();
        add_provento(&mut store, provento_entry(&json!({"ticker": "BTCI11", "payment": "15/10/2026", "value": 16.82, "per_share": 0.095, "quantity": 177}), &market, today).unwrap()).unwrap();
        store.held_from.insert("GGRC11".into(), day(2026, 6, 1));
        store.portfolio.asset_targets.insert("GARE11".into(), 7.5);
        store.verdicts.push(Default::default());
        (store, market, today)
    }

    /// The sample files, for opening them in Excel or LibreOffice by hand: AXYON_SAMPLES=<folder> cargo test -- --ignored.
    #[test]
    #[ignore]
    fn write_samples() {
        let (store, market, today) = sample();
        let dir = std::env::var("AXYON_SAMPLES").unwrap_or_else(|_| ".".into());
        std::fs::write(format!("{dir}/{}", file_name(today, "xlsx")), workbook(&store, &market, today)).unwrap();
        std::fs::write(format!("{dir}/{}", file_name(today, "csv")), csv(&store, &market, today)).unwrap();
    }

    #[test]
    fn base64_both_ways() {
        for bytes in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0, 255, 128, 7, 9]] {
            assert_eq!(unbase64(&base64(bytes)).unwrap(), bytes);
        }
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert!(unbase64("??").is_none());
    }

    #[test]
    fn the_workbook_brings_everything_back_as_it_was() {
        let (store, market, today) = sample();
        let book = workbook(&store, &market, today);
        let sheets = crate::xlsx::read(&book).unwrap();
        let names: Vec<&str> = sheets.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Resumo", "Posições", "Lançamentos", "Proventos", "IRPF 2025", BACKUP_SHEET]);
        let trades = &sheets[2].rows;
        assert_eq!(trades[0][0], "Data");
        assert!(trades.iter().any(|r| r[1] == "GARE11" && r[2] == "venda"), "{trades:?}");
        assert!(sheets[3].rows.iter().any(|r| r[0] == "BTCI11" && r[8] == "lançado por você"), "{:?}", sheets[3].rows);
        // A crash wiped the app: the file brings it all back.
        let mut empty = Store::default();
        let note = restore(&mut empty, &market, "AXYON-carteira.xlsx", &book, today).unwrap();
        assert!(note.contains("por inteiro") && note.contains("1 análises"), "{note}");
        assert_eq!(empty, store);
    }

    #[test]
    fn the_csv_brings_the_carteira_back_and_a_workbook_without_its_backup_too() {
        let (store, market, today) = sample();
        let text = csv(&store, &market, today);
        assert!(text.starts_with('\u{feff}'));
        assert!(text.contains("lançamento;GARE11;01/09/2026;compra;204;8,5;1,2"), "{text}");
        assert!(text.contains("provento lançado;BTCI11;;Rendimento;177;0,095;;;15/10/2026;16,82"), "{text}");
        let mut other = Store::default();
        let note = restore(&mut other, &market, "carteira.csv", text.as_bytes(), today).unwrap();
        // The fixture's GARE11 came first as a "saldo inicial": three lançamentos.
        assert!(note.contains("3 lançamentos e 1 proventos lançados"), "{note}");
        assert_eq!(other.trades, store.trades);
        let held = store.portfolio.position("GARE11").unwrap().clone();
        assert_eq!(other.portfolio.position("GARE11"), Some(&held));
        assert_eq!(other.proventos[0].value, 16.82);
        assert_eq!(other.held_from.get("GGRC11"), Some(&day(2026, 6, 1)));
        assert_eq!(other.portfolio.asset_targets.get("GARE11"), Some(&7.5));
        let a = crate::wallet::view(&store, &market, today);
        let b = crate::wallet::view(&other, &market, today);
        assert_eq!(a["income"]["received"], b["income"]["received"], "the same proventos");
        assert_eq!(a["realized"], b["realized"]);
        // The workbook with its backup sheet deleted: its sheets bring the records back.
        let book = workbook(&store, &market, today);
        let mut sheets: Vec<Tab> = crate::xlsx::read(&book).unwrap().into_iter().filter(|s| s.name != BACKUP_SHEET).map(|s| Tab { name: s.name, widths: vec![], rows: s.rows.into_iter().map(|r| r.into_iter().map(Cell::Text).collect()).collect(), hidden: false }).collect();
        sheets.truncate(5);
        let mut again = Store::default();
        let note = restore(&mut again, &market, "x.xlsx", &crate::xlsx::write(&sheets), today).unwrap();
        assert!(note.contains("3 lançamentos e 1 proventos"), "{note}");
        assert_eq!(again.portfolio.position("GARE11").map(|p| p.quantity), Some(held.quantity));
        assert_eq!(again.proventos[0].value, 16.82);
        // Not ours, not B3's: refused, nothing changed.
        let mut kept = store.clone();
        assert!(restore(&mut kept, &market, "notas.txt", b"nada a ver", today).is_err());
        assert_eq!(kept, store);
    }
}
