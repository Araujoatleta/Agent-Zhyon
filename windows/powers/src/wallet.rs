//! The consolidator ("Carteira", the way Investidor10 keeps one): the user's lançamentos, each buy and sale with its
//! date, price and costs, make each asset's position as a broker's statement does: quantity, average price with the
//! costs, what was put in, the result realized in sales and the one still open at the quote of now (Google Finance
//! during the session, about 15 minutes late; B3's close after it), and its share of the portfolio. The proventos
//! each asset paid for the shares held at the end of its data-com: received, provisioned (data-com passed, still to be
//! paid) and announced (data-com ahead), from the funds' notices on FundosNet and the companies' on B3. An asset with
//! no lançamento comes as the user's sheet or paste gives it, its proventos counted on what is held now (said so).
//! Plain data: the app draws it.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::date::Date;
use crate::model::{AssetClass, Market, Position, Store, Trade, TradeKind};
use crate::sources::fnet::Payout;

/// A lançamento made in the consolidator.
pub const APP: &str = "app";
/// What was held before an asset's first lançamento ("saldo inicial"): one buy at the average price then.
pub const OPENING: &str = "opening";
/// Juros sobre capital próprio: 15 % of income tax held at the source.
const JCP_NET: f64 = 0.85;

/// An asset's book after its lançamentos.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Book {
    pub quantity: f64,
    /// What the shares held cost, costs included (the Receita's average price times the quantity).
    pub cost: f64,
    /// The result of every sale: price less costs, against the average price at that day.
    pub realized: f64,
    pub bought: f64,
    pub sold: f64,
}

impl Book {
    pub fn average(&self) -> f64 {
        if self.quantity > 1e-9 { self.cost / self.quantity } else { 0.0 }
    }
}

/// The lançamentos of `ticker`, in the order they happened (their day, then the order they were told).
fn of(store: &Store, ticker: &str) -> Vec<Trade> {
    let mut list: Vec<Trade> = store.trades.iter().filter(|t| t.ticker.eq_ignore_ascii_case(ticker)).cloned().collect();
    list.sort_by(|a, b| a.date.cmp(&b.date).then(a.seq.cmp(&b.seq)));
    list
}

/// The book after `trades` (in order), each sale's result by its seq, and the first sale of more than was held.
pub fn replay(trades: &[Trade]) -> (Book, BTreeMap<u64, f64>, Option<Trade>) {
    let mut b = Book::default();
    let mut results = BTreeMap::new();
    let mut short = None;
    for t in trades {
        match t.kind {
            TradeKind::Buy => {
                b.quantity += t.quantity;
                b.cost += t.quantity * t.price + t.fees;
                b.bought += t.quantity * t.price + t.fees;
            }
            TradeKind::Sell => {
                let q = t.quantity.min(b.quantity);
                if t.quantity > b.quantity + 1e-9 && short.is_none() {
                    short = Some(t.clone());
                }
                let avg = b.average();
                let result = q * t.price - t.fees - q * avg;
                b.realized += result;
                b.sold += q * t.price - t.fees;
                b.cost -= q * avg;
                b.quantity -= q;
                if b.quantity < 1e-9 {
                    b.quantity = 0.0;
                    b.cost = 0.0;
                }
                results.insert(t.seq, result);
            }
        }
    }
    (b, results, short)
}

/// What was held at the end of `day` (a data-com): the lançamentos up to it.
pub fn held_at(trades: &[Trade], day: Date) -> f64 {
    let upto: Vec<Trade> = trades.iter().filter(|t| t.date <= day).cloned().collect();
    replay(&upto).0.quantity
}

/// The asset's lançamentos were made in the consolidator: its position is theirs.
pub fn managed(store: &Store, ticker: &str) -> bool {
    store.trades.iter().any(|t| t.ticker.eq_ignore_ascii_case(ticker) && (t.source == APP || t.source == OPENING))
}

/// Every lançamento with an id of its own (a file's have none): what the app removes one by. Whether any changed.
pub fn ensure_seqs(store: &mut Store) -> bool {
    let mut next = store.trades.iter().map(|t| t.seq).max().unwrap_or(0);
    let mut changed = false;
    let mut seen = std::collections::BTreeSet::new();
    for t in &mut store.trades {
        if t.seq == 0 || !seen.insert(t.seq) {
            next += 1;
            t.seq = next;
            seen.insert(next);
            changed = true;
        }
    }
    changed
}

/// The position of `ticker` from its lançamentos (gone when none is left), its class kept.
fn set_position(store: &mut Store, market: &Market, ticker: &str) {
    let trades = of(store, ticker);
    let (book, _, _) = replay(&trades);
    let class = store.portfolio.position(ticker).map(|p| p.class).or_else(|| trades.iter().find_map(|t| t.class)).unwrap_or_else(|| crate::trades::class_of(ticker, market).0);
    let at = store.portfolio.positions.iter().position(|p| p.ticker.eq_ignore_ascii_case(ticker));
    match (at, book.quantity > 1e-9) {
        (Some(i), true) => {
            let p = &mut store.portfolio.positions[i];
            p.quantity = book.quantity;
            p.average_price = book.average();
        }
        (Some(i), false) => {
            store.portfolio.positions.remove(i);
        }
        (None, true) => store.portfolio.positions.push(Position {
            ticker: ticker.to_uppercase(),
            class,
            quantity: book.quantity,
            average_price: book.average(),
            subtype: String::new(),
            current_value: None,
            note: String::new(),
        }),
        (None, false) => {}
    }
}

/// The positions of the assets whose lançamentos were made here, from them: after a copy of the computer's portfolio
/// replaced the positions, the consolidator's still stand.
pub fn apply_ledger(store: &mut Store, market: &Market) {
    let tickers: std::collections::BTreeSet<String> = store.trades.iter().filter(|t| t.source == APP || t.source == OPENING).map(|t| t.ticker.to_uppercase()).collect();
    for t in tickers {
        set_position(store, market, &t);
    }
}

/// One lançamento as the app gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub ticker: String,
    pub kind: TradeKind,
    pub date: Date,
    pub quantity: f64,
    pub price: f64,
    pub fees: f64,
    pub class: Option<AssetClass>,
    /// The asset is held (the sheet's position) and has no lançamento yet: the shares held now come first as a "saldo
    /// inicial", and this one is added to them. False: this is part of the history of the shares held, and the
    /// position becomes what the lançamentos make.
    pub opening: bool,
}

/// The entry read from the app's JSON.
pub fn entry(input: &Value, today: Date) -> Result<Entry, String> {
    let s = |k: &str| input[k].as_str().map(str::trim).unwrap_or("").to_string();
    let n = |k: &str| input[k].as_f64().or_else(|| input[k].as_str().and_then(crate::text::num));
    let ticker = crate::text::ticker(&s("ticker")).ok_or("Informe o código do ativo (como GARE11 ou BBAS3).")?;
    let kind = match crate::text::plain(&s("kind")).as_str() {
        "sell" | "venda" | "vender" => TradeKind::Sell,
        _ => TradeKind::Buy,
    };
    let date = if s("date").is_empty() { today } else { Date::parse(&s("date")).ok_or("A data não é válida.")? };
    if date > today {
        return Err("A data do lançamento não pode estar no futuro.".into());
    }
    let quantity = n("quantity").filter(|q| *q > 0.0).ok_or("A quantidade precisa ser maior que zero.")?;
    let price = n("price").filter(|p| *p > 0.0).ok_or("O preço precisa ser maior que zero.")?;
    let fees = n("fees").unwrap_or(0.0).max(0.0);
    let class = AssetClass::parse(&s("class"));
    Ok(Entry { ticker, kind, date, quantity, price, fees, class, opening: input["opening"].as_bool().unwrap_or(true) })
}

/// Adds a lançamento: the asset's position becomes what its lançamentos make. A sale of more than was held that day,
/// or the same lançamento twice, is refused, and nothing changes. Its id.
pub fn add(store: &mut Store, market: &Market, e: Entry, today: Date) -> Result<u64, String> {
    ensure_seqs(store);
    let mut trades = of(store, &e.ticker);
    let same = trades.iter().any(|t| t.date == e.date && t.kind == e.kind && (t.quantity - e.quantity).abs() < 1e-9 && (t.price - e.price).abs() < 0.005 && t.source != OPENING);
    if same {
        return Err(format!("Este lançamento ({} {} em {}) já está na carteira.", crate::text::grouped(e.quantity, 0), e.ticker, e.date.br()));
    }
    let mut next = store.trades.iter().map(|t| t.seq).max().unwrap_or(0);
    let mut opening = None;
    if trades.is_empty() && e.opening {
        if let Some(p) = store.portfolio.position(&e.ticker).filter(|p| p.quantity > 1e-9 && p.class.quoted()) {
            next += 1;
            let day = e.date.min(today).add_days(-1);
            let t = Trade { date: day, ticker: e.ticker.clone(), kind: TradeKind::Buy, quantity: p.quantity, price: p.average_price, fees: 0.0, class: Some(p.class), time: String::new(), source: OPENING.into(), seq: next, average: Some(p.average_price) };
            trades.push(t.clone());
            opening = Some(t);
        }
    }
    next += 1;
    let class = e.class.or_else(|| store.portfolio.position(&e.ticker).map(|p| p.class)).unwrap_or_else(|| crate::trades::class_of(&e.ticker, market).0);
    let new = Trade { date: e.date, ticker: e.ticker.clone(), kind: e.kind, quantity: e.quantity, price: e.price, fees: e.fees, class: Some(class), time: String::new(), source: APP.into(), seq: next, average: None };
    trades.push(new.clone());
    trades.sort_by(|a, b| a.date.cmp(&b.date).then(a.seq.cmp(&b.seq)));
    if let (_, _, Some(short)) = replay(&trades) {
        let held = held_at(&trades.iter().filter(|t| t.seq != short.seq).cloned().collect::<Vec<_>>(), short.date);
        return Err(format!("Em {} você tinha {} {}: não dá para vender {}.", short.date.br(), crate::text::grouped(held, 0), e.ticker, crate::text::grouped(short.quantity, 0)));
    }
    if let Some(t) = opening {
        store.trades.push(t);
    }
    store.trades.push(new);
    store.trades.sort_by(|a, b| a.date.cmp(&b.date).then(a.seq.cmp(&b.seq)));
    set_position(store, market, &e.ticker);
    store.portfolio.updated = Some(today);
    Ok(next)
}

/// Removes a lançamento by its id; refused when a later sale would then sell more than was held.
pub fn remove(store: &mut Store, market: &Market, seq: u64, today: Date) -> Result<(), String> {
    let at = store.trades.iter().position(|t| t.seq == seq).ok_or("Lançamento não encontrado.")?;
    let ticker = store.trades[at].ticker.clone();
    let rest: Vec<Trade> = of(store, &ticker).into_iter().filter(|t| t.seq != seq).collect();
    if let (_, _, Some(short)) = replay(&rest) {
        return Err(format!("A venda de {} em {} usa estas cotas: remova a venda antes.", crate::text::grouped(short.quantity, 0), short.date.br()));
    }
    store.trades.remove(at);
    set_position(store, market, &ticker);
    store.portfolio.updated = Some(today);
    Ok(())
}

/// One provento as the consolidator shows it.
#[derive(Debug, Clone, PartialEq)]
struct Paid {
    ticker: String,
    kind: String,
    record: Date,
    payment: Option<Date>,
    per_share: f64,
    quantity: f64,
    gross: f64,
    net: f64,
    /// Counted on the shares held now (no lançamento tells what was held at the data-com).
    estimated: bool,
}

impl Paid {
    fn json(&self) -> Value {
        json!({
            "ticker": self.ticker,
            "kind": self.kind,
            "record": self.record,
            "payment": self.payment,
            "perShare": self.per_share,
            "quantity": self.quantity,
            "gross": self.gross,
            "net": self.net,
            "estimated": self.estimated,
        })
    }
}

/// The payouts an asset announced: a fund's on FundosNet, a company's on B3.
fn payouts_of<'a>(market: &'a Market, ticker: &str) -> Vec<(&'a Payout, String)> {
    if let Some(f) = market.funds.get(ticker) {
        return f.payouts.iter().map(|p| (p, if p.amortization { "Amortização".to_string() } else { "Rendimento".to_string() })).collect();
    }
    market
        .company_payouts
        .get(ticker)
        .map(|c| c.payouts.iter().map(|p| (p, if p.period.is_empty() { "Dividendo".to_string() } else { p.period.clone() })).collect())
        .unwrap_or_default()
}

/// Every provento of what is or was held, with the quantity at its data-com.
fn proventos(store: &Store, market: &Market, today: Date) -> Vec<Paid> {
    let mut tickers: Vec<String> = store.portfolio.tickers(None);
    for t in store.trades.iter().map(|t| t.ticker.to_uppercase()) {
        if !tickers.contains(&t) {
            tickers.push(t);
        }
    }
    let mut out = Vec::new();
    for t in tickers {
        let trades = of(store, &t);
        let since = crate::invest::held_since(store, &t);
        let now = store.portfolio.position(&t).map(|p| p.quantity).unwrap_or(0.0);
        for (p, kind) in payouts_of(market, &t) {
            if p.value <= 0.0 {
                continue;
            }
            // Ahead of its data-com, what is held now; after it, what was held at its end.
            let (quantity, estimated) = if p.record > today {
                (now, false)
            } else if !trades.is_empty() {
                (held_at(&trades, p.record), false)
            } else if since.is_none_or(|s| s <= p.record) {
                (now, true)
            } else {
                (0.0, true)
            };
            if quantity <= 1e-9 {
                continue;
            }
            let gross = p.value * quantity;
            let net = if p.tax_free == Some(false) && !p.amortization && !market.funds.contains_key(&t) { gross * JCP_NET } else { gross };
            out.push(Paid { ticker: t.clone(), kind, record: p.record, payment: p.payment, per_share: p.value, quantity, gross, net, estimated });
        }
    }
    out.sort_by(|a, b| b.payment.unwrap_or(b.record).cmp(&a.payment.unwrap_or(a.record)).then(a.ticker.cmp(&b.ticker)));
    out
}

/// A sum that is 0, not -0, when there is nothing to add (f64's own starts at -0.0, which a screen shows as "-R$ 0,00").
fn sum(values: impl Iterator<Item = f64>) -> f64 {
    values.fold(0.0, |a, b| a + b)
}

/// The consolidator's screen.
pub fn view(store: &Store, market: &Market, today: Date) -> Value {
    let v = crate::portfolio::value(&store.portfolio, market);
    let all = proventos(store, market, today);
    let received: Vec<&Paid> = all.iter().filter(|p| p.payment.is_some_and(|d| d <= today) && p.kind != "Amortização").collect();
    let provisioned: Vec<&Paid> = all.iter().filter(|p| p.record <= today && p.payment.is_none_or(|d| d > today) && p.record.until(today) <= 180).collect();
    let announced: Vec<&Paid> = all.iter().filter(|p| p.record > today).collect();
    let year_ago = today.add_days(-365);
    let twelve = sum(received.iter().filter(|p| p.payment.is_some_and(|d| d > year_ago)).map(|p| p.net));
    let this_year = sum(received.iter().filter(|p| p.payment.is_some_and(|d| d.y == today.y)).map(|p| p.net));
    let ahead = sum(provisioned.iter().map(|p| p.net));
    // The months, the last twelve, by the day each was paid.
    let first = today.first_of_month().add_months(-11);
    let months: Vec<Value> = (0..12)
        .map(|i| {
            let key = first.add_months(i).month_key();
            let mut items: BTreeMap<String, f64> = BTreeMap::new();
            for p in received.iter().filter(|p| p.payment.is_some_and(|d| d.month_key() == key)) {
                *items.entry(p.ticker.clone()).or_default() += p.net;
            }
            let mut list: Vec<(String, f64)> = items.into_iter().collect();
            list.sort_by(|a, b| b.1.total_cmp(&a.1));
            json!({ "month": key, "total": sum(list.iter().map(|x| x.1)), "items": list.iter().map(|(t, v)| json!({ "ticker": t, "value": v })).collect::<Vec<_>>() })
        })
        .collect();
    // Each asset: its numbers, the quote and where it is from, its proventos.
    let mut realized = 0.0;
    let mut realized_year = 0.0;
    let mut sold_out: Vec<Value> = Vec::new();
    let mut tickers: Vec<String> = store.trades.iter().map(|t| t.ticker.to_uppercase()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    tickers.retain(|t| !t.is_empty());
    for t in &tickers {
        let trades = of(store, t);
        let (book, results, _) = replay(&trades);
        realized += book.realized;
        realized_year += sum(trades.iter().filter(|x| x.date.y == today.y).filter_map(|x| results.get(&x.seq).copied()));
        if book.quantity <= 1e-9 && store.portfolio.position(t).is_none() {
            sold_out.push(json!({ "ticker": t, "realized": book.realized, "bought": book.bought, "sold": book.sold }));
        }
    }
    let asset = |h: &crate::portfolio::Holding| -> Value {
        let trades = of(store, &h.ticker);
        let live = market.live_now(&h.ticker);
        let close = market.quotes.get(&h.ticker).filter(|q| q.close > 0.0);
        let (day_change, day_pct) = match live.and_then(|l| l.previous.filter(|p| *p > 0.0).map(|p| (l.price, p))) {
            Some((price, previous)) => (Some(h.quantity * (price - previous)), Some((price / previous - 1.0) * 100.0)),
            None => (None, None),
        };
        let mine: Vec<&Paid> = all.iter().filter(|p| p.ticker == h.ticker).collect();
        let twelve = sum(mine.iter().filter(|p| p.kind != "Amortização" && p.payment.is_some_and(|d| d <= today && d > year_ago)).map(|p| p.net));
        let ahead = sum(mine.iter().filter(|p| p.record <= today && p.payment.is_none_or(|d| d > today) && p.record.until(today) <= 180).map(|p| p.net));
        let (book, _, _) = replay(&trades);
        json!({
            "ticker": h.ticker,
            "class": h.class.id(),
            "quantity": h.quantity,
            "average": h.average_price,
            "price": h.price,
            "live": h.live,
            "priceAt": live.map(|l| l.at),
            "closeDay": close.map(|q| q.date),
            "dayChange": day_change,
            "dayPct": day_pct,
            "value": h.value,
            "cost": h.cost,
            "gain": h.gain,
            "gainPct": h.gain_pct,
            "weight": h.weight,
            "target": store.portfolio.asset_targets.get(&h.ticker),
            "dividends12": twelve,
            "yieldOnCost": (h.cost > 0.0 && twelve > 0.0).then(|| twelve / h.cost * 100.0),
            "provisioned": ahead,
            "trades": trades.len(),
            "managed": managed(store, &h.ticker),
            "realized": book.realized,
            "unpriced": h.unpriced,
        })
    };
    let classes: Vec<Value> = v
        .classes
        .iter()
        .filter(|c| c.value > 0.0)
        .map(|c| {
            let mut assets: Vec<&crate::portfolio::Holding> = v.holdings.iter().filter(|h| h.class == c.class && h.value > 0.0).collect();
            assets.sort_by(|a, b| b.value.total_cmp(&a.value));
            let gain = c.value - c.cost;
            json!({
                "key": c.class.id(),
                "label": c.label,
                "value": c.value,
                "cost": c.cost,
                "gain": gain,
                "gainPct": (c.cost > 0.0).then(|| (c.value / c.cost - 1.0) * 100.0),
                "weight": c.weight,
                "target": c.target,
                "assets": assets.into_iter().map(asset).collect::<Vec<_>>(),
            })
        })
        .collect();
    let day: Option<f64> = {
        let changes: Vec<f64> = v.holdings.iter().filter_map(|h| market.live_now(&h.ticker).and_then(|l| l.previous.filter(|p| *p > 0.0).map(|p| h.quantity * (l.price - p)))).collect();
        (!changes.is_empty()).then(|| changes.iter().sum())
    };
    let mut list: Vec<&Trade> = store.trades.iter().collect();
    list.sort_by(|a, b| b.date.cmp(&a.date).then(b.seq.cmp(&a.seq)));
    let results: BTreeMap<u64, f64> = tickers.iter().flat_map(|t| replay(&of(store, t)).1).collect();
    let trades: Vec<Value> = list
        .iter()
        .take(300)
        .map(|t| {
            let total = match t.kind {
                TradeKind::Buy => t.quantity * t.price + t.fees,
                TradeKind::Sell => t.quantity * t.price - t.fees,
            };
            json!({
                "id": t.seq,
                "date": t.date,
                "ticker": t.ticker,
                "kind": if t.source == OPENING { "opening" } else if t.kind == TradeKind::Sell { "sell" } else { "buy" },
                "quantity": t.quantity,
                "price": t.price,
                "fees": t.fees,
                "total": total,
                "result": results.get(&t.seq),
                "source": t.source,
            })
        })
        .collect();
    json!({
        "total": v.value,
        "cost": v.cost,
        "gain": v.gain,
        "gainPct": v.gain_pct,
        "realized": realized,
        "realizedYear": realized_year,
        "day": day,
        "dayPct": day.and_then(|d| (v.value - d > 0.0).then(|| d / (v.value - d) * 100.0)),
        "liveAt": v.live_at,
        "pricedOn": v.priced_on,
        "income": {
            "twelve": twelve,
            "year": this_year,
            "provisioned": ahead,
            "announced": sum(announced.iter().map(|p| p.net)),
            "yieldOnCost": (v.cost > 0.0 && twelve > 0.0).then(|| twelve / v.cost * 100.0),
            "months": months,
            "received": received.iter().take(60).map(|p| p.json()).collect::<Vec<_>>(),
            "pending": provisioned.iter().map(|p| p.json()).collect::<Vec<_>>(),
            "ahead": announced.iter().map(|p| p.json()).collect::<Vec<_>>(),
        },
        "classes": classes,
        "trades": trades,
        "soldOut": sold_out,
        "managedAll": !store.portfolio.positions.is_empty() && store.portfolio.positions.iter().filter(|p| p.class.quoted()).all(|p| managed(store, &p.ticker)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::fixtures;

    fn day(y: i32, m: u32, d: u32) -> Date {
        Date::new(y, m, d).unwrap()
    }

    fn input(ticker: &str, kind: &str, date: &str, q: f64, p: f64) -> Value {
        json!({ "ticker": ticker, "kind": kind, "date": date, "quantity": q, "price": p })
    }

    #[test]
    fn lancamentos_make_the_position_as_a_statement_does() {
        let today = day(2026, 11, 4);
        let market = fixtures::market();
        let mut store = Store::default();
        add(&mut store, &market, entry(&input("GARE11", "compra", "2026-03-10", 100.0, 9.0), today).unwrap(), today).unwrap();
        let mut second = entry(&input("GARE11", "compra", "2026-06-10", 100.0, 8.0), today).unwrap();
        second.fees = 2.0;
        add(&mut store, &market, second, today).unwrap();
        let p = store.portfolio.position("GARE11").unwrap();
        assert_eq!(p.quantity, 200.0);
        assert!((p.average_price - (900.0 + 802.0) / 200.0).abs() < 1e-9, "the costs go into the average: {}", p.average_price);
        // A sale keeps the average and realizes the result.
        let id = add(&mut store, &market, entry(&input("GARE11", "venda", "2026-09-01", 50.0, 9.5), today).unwrap(), today).unwrap();
        let p = store.portfolio.position("GARE11").unwrap();
        assert_eq!(p.quantity, 150.0);
        assert!((p.average_price - 8.51).abs() < 1e-9);
        let (book, results, _) = replay(&of(&store, "GARE11"));
        assert!((results[&id] - 50.0 * (9.5 - 8.51)).abs() < 1e-9, "{results:?}");
        assert!((book.realized - 49.5).abs() < 1e-9);
        // Nothing to receive is 0, never -0 (a screen would show "-R$ 0,00").
        let screen = view(&store, &market, today).to_string();
        assert!(!screen.contains("-0.0"), "{screen}");
        // Selling more than was held that day is refused; so is the same lançamento twice.
        let err = add(&mut store, &market, entry(&input("GARE11", "venda", "2026-04-01", 150.0, 9.0), today).unwrap(), today).unwrap_err();
        assert!(err.contains("você tinha 100 GARE11"), "{err}");
        assert!(add(&mut store, &market, entry(&input("GARE11", "compra", "2026-03-10", 100.0, 9.0), today).unwrap(), today).is_err());
        // Removing a buy a later sale needs is refused; the sale first, then it.
        let first = store.trades.iter().find(|t| t.date == day(2026, 3, 10)).unwrap().seq;
        let second = store.trades.iter().find(|t| t.date == day(2026, 6, 10)).unwrap().seq;
        remove(&mut store, &market, second, today).unwrap();
        assert!(remove(&mut store, &market, first, today).unwrap_err().contains("remova a venda antes"));
        remove(&mut store, &market, id, today).unwrap();
        remove(&mut store, &market, first, today).unwrap();
        assert!(store.portfolio.position("GARE11").is_none(), "no lançamento left: no position");
        // A future date, a bad ticker, no quantity: refused before anything changes.
        assert!(entry(&input("GARE11", "compra", "2026-12-01", 1.0, 9.0), today).is_err());
        assert!(entry(&input("??", "compra", "", 1.0, 9.0), today).is_err());
        assert!(entry(&input("GARE11", "compra", "", 0.0, 9.0), today).is_err());
    }

    #[test]
    fn the_sheets_position_comes_first_as_an_opening_or_is_rebuilt_from_its_history() {
        let today = day(2026, 11, 4);
        let market = fixtures::market();
        // GGRC11: 150 at R$ 9,20 in the fixture's portfolio, with no lançamento.
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        add(&mut store, &market, entry(&input("GGRC11", "compra", "2026-11-03", 50.0, 9.0), today).unwrap(), today).unwrap();
        let p = store.portfolio.position("GGRC11").unwrap();
        assert_eq!(p.quantity, 200.0, "the 150 held, then the 50 bought");
        assert!((p.average_price - (150.0 * 9.2 + 450.0) / 200.0).abs() < 1e-9);
        let opening = store.trades.iter().find(|t| t.source == OPENING).unwrap();
        assert_eq!((opening.quantity, opening.date), (150.0, day(2026, 11, 2)));
        // The history of what is held instead: the position becomes what the lançamentos make.
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let mut history = entry(&input("GGRC11", "compra", "2025-05-02", 100.0, 9.5), today).unwrap();
        history.opening = false;
        add(&mut store, &market, history, today).unwrap();
        assert_eq!(store.portfolio.position("GGRC11").unwrap().quantity, 100.0);
        // A copy of the computer's portfolio replaces the positions; the consolidator's stand.
        store.portfolio.positions = fixtures::portfolio().positions;
        apply_ledger(&mut store, &market);
        assert_eq!(store.portfolio.position("GGRC11").unwrap().quantity, 100.0);
    }

    #[test]
    fn proventos_count_what_was_held_at_the_data_com_received_and_provisioned() {
        let today = day(2026, 11, 4);
        let mut market = fixtures::market();
        let fund = market.funds.get_mut("GGRC11").unwrap();
        let payout = |record: Date, payment: Option<Date>, value: f64| Payout { record, payment, value, amortization: false, tax_free: Some(true), period: String::new(), document: 1 };
        fund.payouts = vec![
            payout(day(2026, 9, 30), Some(day(2026, 10, 7)), 0.10),
            payout(day(2026, 10, 30), Some(day(2026, 11, 7)), 0.11),
            payout(day(2026, 11, 28), Some(day(2026, 12, 5)), 0.12),
        ];
        // A company's JCP on B3: 15 % held at the source.
        market.company_payouts.insert("TIMS3".into(), crate::model::CompanyPayouts { checked: Some(today), payouts: vec![Payout { record: day(2026, 10, 1), payment: Some(day(2026, 10, 20)), value: 1.0, amortization: false, tax_free: Some(false), period: "Juros sobre capital próprio".into(), document: 0 }] });
        let mut store = Store::default();
        add(&mut store, &market, entry(&input("GGRC11", "compra", "2026-09-01", 100.0, 9.0), today).unwrap(), today).unwrap();
        // Bought after September's data-com: those 50 do not get September's.
        add(&mut store, &market, entry(&input("GGRC11", "compra", "2026-10-01", 50.0, 9.0), today).unwrap(), today).unwrap();
        add(&mut store, &market, entry(&input("TIMS3", "compra", "2026-09-15", 10.0, 18.0), today).unwrap(), today).unwrap();
        let screen = view(&store, &market, today);
        let received = screen["income"]["received"].as_array().unwrap();
        let september = received.iter().find(|p| p["ticker"] == "GGRC11").unwrap();
        assert_eq!(september["quantity"], 100.0, "what was held at the end of the data-com");
        assert!((september["net"].as_f64().unwrap() - 10.0).abs() < 1e-9);
        let jcp = received.iter().find(|p| p["ticker"] == "TIMS3").unwrap();
        assert!((jcp["net"].as_f64().unwrap() - 8.5).abs() < 1e-9, "JCP net of 15 %: {jcp}");
        let pending = screen["income"]["pending"].as_array().unwrap();
        assert_eq!(pending.len(), 1, "October's, data-com passed, paid on 07/11");
        assert!((pending[0]["net"].as_f64().unwrap() - 150.0 * 0.11).abs() < 1e-9);
        assert!((screen["income"]["provisioned"].as_f64().unwrap() - 16.5).abs() < 1e-9);
        assert_eq!(screen["income"]["ahead"][0]["record"], "2026-11-28", "data-com ahead: announced");
        assert!((screen["income"]["twelve"].as_f64().unwrap() - 18.5).abs() < 1e-9);
        let months = screen["income"]["months"].as_array().unwrap();
        assert_eq!(months[11]["month"], "2026-11");
        assert!((months[10]["total"].as_f64().unwrap() - 18.5).abs() < 1e-9, "October: R$ 10 + R$ 8,50");
        // The assets by class, each with its quote, result, weight and proventos.
        let fiis = screen["classes"].as_array().unwrap().iter().find(|c| c["key"] == "fii").unwrap();
        let g = &fiis["assets"][0];
        assert_eq!((g["ticker"].as_str(), g["quantity"].as_f64()), (Some("GGRC11"), Some(150.0)));
        assert!(g["managed"].as_bool().unwrap());
        assert!((g["provisioned"].as_f64().unwrap() - 16.5).abs() < 1e-9);
        assert_eq!(screen["trades"].as_array().unwrap().len(), 3);
        assert_eq!(screen["trades"][0]["ticker"], "GGRC11", "newest first");
    }
}
