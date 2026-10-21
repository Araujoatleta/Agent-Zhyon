//! The user's own trades, as they tell them ("on 21/10 at 14:30 I bought 10 GGRC11 at R$ 9,20"): each one moves the
//! position (its quantity, and its average price with the costs, the way the Receita counts it) and is kept with the
//! others, for the month's taxes (audit.rs) and the history of contributions. Nothing here buys or sells anything.

use std::collections::BTreeMap;

use crate::date::Date;
use crate::model::{AssetClass, Market, Position, Store, Trade, TradeKind};
use crate::text::{brl, grouped, pct};

/// One trade as the user told it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Told {
    pub ticker: String,
    pub kind: TradeKind,
    pub quantity: f64,
    /// R$ a share; None: B3's last close is used, and said.
    pub price: Option<f64>,
    pub fees: f64,
    /// None: today.
    pub date: Option<Date>,
    pub time: String,
    pub class: Option<AssetClass>,
}

/// "14:30" from "14:30", "14h30", "14h", "14" or "9:05"; "" for anything else.
pub fn time_of(text: &str) -> String {
    let t = text.trim().to_ascii_lowercase().replace(['h', '.'], ":");
    let mut parts = t.split(':').map(str::trim);
    let hour = parts.next().and_then(|h| h.parse::<u32>().ok());
    let minute = parts.next().filter(|m| !m.is_empty()).map(|m| m.parse::<u32>().ok()).unwrap_or(Some(0));
    match (hour, minute) {
        (Some(h), Some(m)) if h < 24 && m < 60 => format!("{h:02}:{m:02}"),
        _ => String::new(),
    }
}

fn class_for(t: &Told, market: &Market) -> (AssetClass, bool) {
    if let Some(c) = t.class {
        return (c, false);
    }
    match market.quotes.get(&t.ticker).map(crate::sources::b3::kind_of) {
        Some("fii") => (AssetClass::Fii, false),
        Some("etf") => (AssetClass::Etf, false),
        Some("bdr") => (AssetClass::Bdr, false),
        Some("acao") => (AssetClass::Acao, false),
        _ => crate::import::guess_class(&t.ticker),
    }
}

fn when(trade: &Trade) -> String {
    if trade.time.is_empty() {
        trade.date.br()
    } else {
        format!("{} {}", trade.date.br(), trade.time)
    }
}

/// Records the trades and moves their positions. Each trade gets a line saying what changed; a trade that cannot be
/// (a sale of more than is held, a ticker that is not one) gets a line saying why, and the others go on.
pub fn record(store: &mut Store, market: &Market, told: &[Told], today: Date) -> Result<Vec<String>, String> {
    if told.is_empty() {
        return Err("No trade to record: give each one's ticker, buy or sell, quantity and price (and its date and time).".into());
    }
    let mut lines = Vec::new();
    let mut seq = store.trades.iter().map(|t| t.seq).max().unwrap_or(0);
    let mut changed = false;
    for t in told {
        let date = t.date.unwrap_or(today);
        if date > today {
            lines.push(format!("{}: {} is in the future; nothing recorded.", t.ticker, date.br()));
            continue;
        }
        if !(t.quantity > 0.0) {
            lines.push(format!("{}: the quantity must be more than zero.", t.ticker));
            continue;
        }
        let (price, guessed_price) = match t.price.filter(|p| *p > 0.0) {
            Some(p) => (p, false),
            None => match market.price(&t.ticker) {
                Some(p) => (p, true),
                None => {
                    lines.push(format!("{}: no price given and no B3 close known: ask the user the price they paid.", t.ticker));
                    continue;
                }
            },
        };
        let fees = t.fees.max(0.0);
        // Told twice, or already in a B3 file (which has no time): two orders of the same size on the same day at
        // different times are two trades.
        let same = |x: &Trade| {
            x.date == date
                && x.ticker == t.ticker
                && x.kind == t.kind
                && (x.quantity - t.quantity).abs() < 1e-9
                && (x.price - price).abs() < 0.005
                && (x.time.is_empty() || t.time.is_empty() || x.time == t.time)
        };
        if store.trades.iter().any(same) {
            lines.push(format!("{}: this trade ({} on {}) is already in the record; not counted twice.", t.ticker, grouped(t.quantity, 0), date.br()));
            continue;
        }
        let at = store.portfolio.positions.iter().position(|p| p.ticker == t.ticker);
        let mut trade = Trade { date, ticker: t.ticker.clone(), kind: t.kind, quantity: t.quantity, price, fees, class: None, time: t.time.clone(), source: "chat".into(), seq: 0, average: None };
        let line = match t.kind {
            TradeKind::Buy => {
                let cost = t.quantity * price + fees;
                let (before_q, before_avg) = at.map(|i| (store.portfolio.positions[i].quantity, store.portfolio.positions[i].average_price)).unwrap_or((0.0, 0.0));
                let q = before_q + t.quantity;
                let avg = (before_q * before_avg + cost) / q;
                match at {
                    Some(i) => {
                        let p = &mut store.portfolio.positions[i];
                        p.quantity = q;
                        p.average_price = avg;
                        trade.class = Some(p.class);
                    }
                    None => {
                        let (class, guessed) = class_for(t, market);
                        trade.class = Some(class);
                        store.portfolio.positions.push(Position {
                            ticker: t.ticker.clone(),
                            class,
                            quantity: q,
                            average_price: avg,
                            subtype: String::new(),
                            current_value: None,
                            note: if guessed { "guessed".into() } else { String::new() },
                        });
                    }
                }
                trade.average = Some(avg);
                let was = if before_q > 0.0 && before_avg <= 0.0 {
                    format!(" (the {} held before had no average price in Coucou, so this one is too low: ask the user their average and save it with wealth_update)", grouped(before_q, 0))
                } else if before_q > 0.0 {
                    format!(" (was {} at {})", grouped(before_q, 0), brl(before_avg))
                } else {
                    " (a new position)".to_string()
                };
                format!("Bought {} {} at {} on {}{}: now {} shares, average price {}{}.", grouped(t.quantity, 0), t.ticker, brl(price), when(&trade), if fees > 0.0 { format!(", costs {}", brl(fees)) } else { String::new() }, grouped(q, 0), brl(avg), was)
            }
            TradeKind::Sell => {
                let Some(i) = at else {
                    lines.push(format!("{}: the user holds none in Coucou, so a sale cannot be recorded (record the buy first, or save the portfolio).", t.ticker));
                    continue;
                };
                let (held, avg) = (store.portfolio.positions[i].quantity, store.portfolio.positions[i].average_price);
                if t.quantity > held + 1e-9 {
                    lines.push(format!("{}: the user holds {}, so {} cannot be sold; nothing recorded.", t.ticker, grouped(held, 0), grouped(t.quantity, 0)));
                    continue;
                }
                trade.class = Some(store.portfolio.positions[i].class);
                trade.average = Some(avg);
                let left = held - t.quantity;
                if left < 1e-9 {
                    store.portfolio.positions.remove(i);
                } else {
                    store.portfolio.positions[i].quantity = left;
                }
                let gain = t.quantity * (price - avg) - fees;
                format!(
                    "Sold {} {} at {} on {}{}: {} left at the same average price {}; {} {} on this sale (wealth_audit counts it in the month's taxes).",
                    grouped(t.quantity, 0),
                    t.ticker,
                    brl(price),
                    when(&trade),
                    if fees > 0.0 { format!(", costs {}", brl(fees)) } else { String::new() },
                    grouped(left, 0),
                    brl(avg),
                    if gain >= 0.0 { "a gain of" } else { "a loss of" },
                    brl(gain.abs())
                )
            }
        };
        seq += 1;
        trade.seq = seq;
        store.trades.push(trade);
        lines.push(if guessed_price { format!("{line} The price was not given: B3's last close was used; correct it if the user paid another.") } else { line });
        changed = true;
    }
    if changed {
        store.trades.sort_by(|a, b| a.date.cmp(&b.date).then(a.seq.cmp(&b.seq)));
        store.portfolio.updated = Some(today);
        store.portfolio.source = "chat".into();
        lines.push(month_line(store, today));
    }
    Ok(lines)
}

/// Undoes the last trade the user told (not one from a file): the trade goes, and its position is moved back.
pub fn undo_last(store: &mut Store, today: Date) -> Result<String, String> {
    let Some(i) = store.trades.iter().enumerate().filter(|(_, t)| t.source == "chat" && t.seq > 0).max_by_key(|(_, t)| t.seq).map(|(i, _)| i) else {
        return Err("No trade told in the chat to undo.".into());
    };
    let trade = store.trades.remove(i);
    let at = store.portfolio.positions.iter().position(|p| p.ticker == trade.ticker);
    let position = match (trade.kind, at) {
        (TradeKind::Buy, Some(i)) => {
            let p = &mut store.portfolio.positions[i];
            let q = p.quantity - trade.quantity;
            if q < 1e-9 {
                store.portfolio.positions.remove(i);
                "the position is gone".to_string()
            } else {
                let cost = (p.quantity * p.average_price - (trade.quantity * trade.price + trade.fees)).max(0.0);
                p.quantity = q;
                p.average_price = cost / q;
                format!("now {} at {}", grouped(q, 0), brl(p.average_price))
            }
        }
        (TradeKind::Sell, Some(i)) => {
            let p = &mut store.portfolio.positions[i];
            p.quantity += trade.quantity;
            format!("now {} at {}", grouped(p.quantity, 0), brl(p.average_price))
        }
        (TradeKind::Sell, None) => {
            let avg = trade.average.unwrap_or(trade.price);
            store.portfolio.positions.push(Position {
                ticker: trade.ticker.clone(),
                class: trade.class.unwrap_or_else(|| crate::import::guess_class(&trade.ticker).0),
                quantity: trade.quantity,
                average_price: avg,
                subtype: String::new(),
                current_value: None,
                note: String::new(),
            });
            format!("back to {} at {}", grouped(trade.quantity, 0), brl(avg))
        }
        (TradeKind::Buy, None) => "its position had already changed, so only the trade is gone".to_string(),
    };
    store.portfolio.updated = Some(today);
    let what = if trade.kind == TradeKind::Buy { "buy" } else { "sale" };
    Ok(format!("Undone: the {what} of {} {} at {} on {}; {}.", grouped(trade.quantity, 0), trade.ticker, brl(trade.price), when(&trade), position))
}

/// The buys and sales of a month (YYYY-MM), and what they put in.
fn month_totals<'a>(store: &'a Store, key: &str) -> (Vec<&'a Trade>, f64, f64) {
    let list: Vec<&Trade> = store.trades.iter().filter(|t| t.date.month_key() == key).collect();
    let bought: f64 = list.iter().filter(|t| t.kind == TradeKind::Buy).map(|t| t.quantity * t.price + t.fees).sum();
    let sold: f64 = list.iter().filter(|t| t.kind == TradeKind::Sell).map(|t| t.quantity * t.price - t.fees).sum();
    (list, bought, sold)
}

/// This month's contributions against the monthly contribution the user set.
fn month_line(store: &Store, today: Date) -> String {
    let (list, bought, sold) = month_totals(store, &today.month_key());
    let buys = list.iter().filter(|t| t.kind == TradeKind::Buy).count();
    let mut out = format!("This month: {} put in over {buys} buy(s)", brl(bought));
    if sold > 0.0 {
        out.push_str(&format!(", {} taken out in sales", brl(sold)));
    }
    if let Some(goal) = store.portfolio.monthly_contribution.filter(|g| *g > 0.0) {
        out.push_str(&format!(" — {} of the {} a month the user set", pct(bought / goal * 100.0, 0), brl(goal)));
    }
    out.push('.');
    out
}

/// The contributions month by month, newest first (`months` of them), with each trade.
pub fn history(store: &Store, months: usize, today: Date) -> String {
    if store.trades.is_empty() {
        return "No trade recorded yet: the user can tell their buys and sales (\"comprei 10 GGRC11 a 9,20 hoje às 14h\"), or import B3's 'Negociação' or a spreadsheet of their contributions.".into();
    }
    let mut keys: Vec<String> = store.trades.iter().map(|t| t.date.month_key()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    keys.reverse();
    let mut out = Vec::new();
    for key in keys.iter().take(months.max(1)) {
        let (list, bought, sold) = month_totals(store, key);
        let mut block = format!("{key}: {} put in", brl(bought));
        if sold > 0.0 {
            block.push_str(&format!(", {} from sales", brl(sold)));
        }
        let mut by: BTreeMap<&str, f64> = BTreeMap::new();
        for t in list.iter().filter(|t| t.kind == TradeKind::Buy) {
            *by.entry(t.ticker.as_str()).or_default() += t.quantity * t.price + t.fees;
        }
        if !by.is_empty() {
            block.push_str(&format!(" ({})", by.iter().map(|(t, v)| format!("{t} {}", brl(*v))).collect::<Vec<_>>().join(", ")));
        }
        for t in &list {
            let side = if t.kind == TradeKind::Buy { "bought" } else { "sold" };
            block.push_str(&format!("\n- {} {side} {} {} at {}{}", when(t), grouped(t.quantity, 0), t.ticker, brl(t.price), if t.source == "chat" { "" } else { " (from a file)" }));
        }
        out.push(block);
    }
    let total: f64 = store.trades.iter().filter(|t| t.kind == TradeKind::Buy && t.date.add_months(12) > today).map(|t| t.quantity * t.price + t.fees).sum();
    out.push(format!("Last twelve months: {} put in.", brl(total)));
    out.join("\n\n")
}

/// Positions from a history of trades alone (a spreadsheet of contributions, with no position list): each ticker's
/// quantity and average price after its buys and sales.
pub fn positions_from(trades: &[Trade]) -> Vec<Position> {
    let ledger = crate::audit::ledger(trades, &[], &[]);
    ledger
        .books
        .iter()
        .filter(|(_, b)| b.quantity > 1e-9)
        .map(|(t, b)| {
            let (class, guessed) = trades.iter().find(|x| &x.ticker == t).and_then(|x| x.class).map_or_else(|| crate::import::guess_class(t), |c| (c, false));
            Position { ticker: t.clone(), class, quantity: b.quantity, average_price: b.average(), subtype: String::new(), current_value: None, note: if guessed { "guessed".into() } else { String::new() } }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::fixtures;

    fn today() -> Date {
        Date::new(2026, 10, 21).unwrap()
    }

    fn buy(t: &str, q: f64, p: f64) -> Told {
        Told { ticker: t.into(), kind: TradeKind::Buy, quantity: q, price: Some(p), time: "14:30".into(), ..Told::default() }
    }

    #[test]
    fn a_time_is_read_the_way_it_is_said() {
        assert_eq!(time_of("14:30"), "14:30");
        assert_eq!(time_of("14h30"), "14:30");
        assert_eq!(time_of("14h"), "14:00");
        assert_eq!(time_of("9"), "09:00");
        assert_eq!(time_of("25:00"), "");
        assert_eq!(time_of("de manhã"), "");
    }

    #[test]
    fn a_buy_moves_the_average_price_with_its_costs_and_a_sale_does_not() {
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        store.portfolio.monthly_contribution = Some(500.0);
        let market = fixtures::market();
        // GGRC11: 150 at R$ 9,20 in the fixture.
        let mut b = buy("GGRC11", 50.0, 9.0);
        b.fees = 1.0;
        let lines = record(&mut store, &market, &[b.clone()], today()).unwrap();
        let p = store.portfolio.position("GGRC11").unwrap();
        assert_eq!(p.quantity, 200.0);
        assert!((p.average_price - (150.0 * 9.20 + 50.0 * 9.0 + 1.0) / 200.0).abs() < 1e-9, "{}", p.average_price);
        assert!(lines[0].contains("now 200 shares") && lines[0].contains("21/10/2026 14:30"), "{}", lines[0]);
        assert!(lines.last().unwrap().contains("R$ 451,00 put in") && lines.last().unwrap().contains("90%"), "{lines:?}");
        // The same trade told again is not counted twice.
        let again = record(&mut store, &market, &[b.clone()], today()).unwrap();
        assert!(again[0].contains("already in the record"));
        assert_eq!(store.portfolio.position("GGRC11").unwrap().quantity, 200.0);
        // The same order at another time of the day is another trade.
        let later = Told { time: "16:10".into(), ..b };
        record(&mut store, &market, &[later], today()).unwrap();
        assert_eq!(store.portfolio.position("GGRC11").unwrap().quantity, 250.0);
        undo_last(&mut store, today()).unwrap();
        // A sale keeps the average and says what it made.
        let avg = store.portfolio.position("GGRC11").unwrap().average_price;
        let sell = Told { kind: TradeKind::Sell, ..buy("GGRC11", 20.0, 10.0) };
        let lines = record(&mut store, &market, &[sell], today()).unwrap();
        let p = store.portfolio.position("GGRC11").unwrap();
        assert_eq!(p.quantity, 180.0);
        assert!((p.average_price - avg).abs() < 1e-9);
        assert!(lines[0].contains("a gain of"), "{}", lines[0]);
        // More than is held cannot be sold.
        let too_much = Told { kind: TradeKind::Sell, ..buy("GGRC11", 1000.0, 10.0) };
        assert!(record(&mut store, &market, &[too_much], today()).unwrap()[0].contains("cannot be sold"));
        // A new ticker opens a position, its class from B3's quote when known.
        record(&mut store, &market, &[buy("VALE3", 10.0, 60.0)], today()).unwrap();
        assert_eq!(store.portfolio.position("VALE3").unwrap().quantity, 10.0);
        assert_eq!(store.trades.iter().filter(|t| t.source == "chat").count(), 3);
    }

    #[test]
    fn the_last_trade_told_is_undone_and_its_position_moved_back() {
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let market = fixtures::market();
        let before = store.portfolio.position("GGRC11").unwrap().clone();
        record(&mut store, &market, &[buy("GGRC11", 50.0, 9.0)], today()).unwrap();
        let note = undo_last(&mut store, today()).unwrap();
        assert!(note.contains("Undone: the buy of 50 GGRC11"), "{note}");
        let after = store.portfolio.position("GGRC11").unwrap();
        assert_eq!(after.quantity, before.quantity);
        assert!((after.average_price - before.average_price).abs() < 1e-9);
        // A sale of everything, undone, brings the position back at its average.
        let all = Told { kind: TradeKind::Sell, ..buy("PORD11", 120.0, 9.0) };
        record(&mut store, &market, &[all], today()).unwrap();
        assert!(store.portfolio.position("PORD11").is_none());
        undo_last(&mut store, today()).unwrap();
        assert_eq!(store.portfolio.position("PORD11").unwrap().quantity, 120.0);
        assert!(undo_last(&mut store, today()).is_err(), "nothing told is left");
    }

    #[test]
    fn the_history_month_by_month_and_positions_from_trades_alone() {
        let mut store = Store::default();
        let market = fixtures::market();
        let mut september = buy("GGRC11", 100.0, 9.0);
        september.date = Date::new(2026, 9, 10);
        record(&mut store, &market, &[september, buy("GGRC11", 50.0, 9.6), buy("TIMS3", 10.0, 19.0)], today()).unwrap();
        let text = history(&store, 6, today());
        assert!(text.starts_with("2026-10: R$ 670,00 put in"), "{text}");
        assert!(text.contains("2026-09: R$ 900,00 put in") && text.contains("Last twelve months: R$ 1.570,00"), "{text}");
        let positions = positions_from(&store.trades);
        let g = positions.iter().find(|p| p.ticker == "GGRC11").unwrap();
        assert_eq!(g.quantity, 150.0);
        assert!((g.average_price - 9.2).abs() < 1e-9);
    }
}
