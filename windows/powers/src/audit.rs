//! The portfolio's auditor: the average price of each holding rebuilt from B3's trades and events (amortizations
//! lower it, bonus shares and subscriptions add to it, splits change the quantity only), the gains of each month and
//! their tax (shares and ETFs 15 % with shares' R$ 20,000 monthly sales exemption, real-estate funds 20 %, day trades
//! 20 %, losses carried forward in their own pool), the DARF and its due date, the payouts that should have arrived
//! and did not, the differences with the positions the user keeps elsewhere, and the year's declaration (Bens e
//! Direitos, exempt income, interest on equity).
//!
//! Rules as of 2026; an estimate to check against the Receita Federal's own program, never a filing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::date::{last_business_day, Date};
use crate::model::{AssetClass, Event, EventKind, Market, Position, Trade, TradeKind};
use crate::text::{brl, grouped};

/// Where a gain is taxed and its losses offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pool {
    /// Shares, ETFs and BDRs held more than a day (15 %).
    Common,
    /// Real-estate funds (20 %).
    Funds,
    /// Bought and sold the same day (20 %).
    DayTrade,
}

impl Pool {
    pub fn rate(self) -> f64 {
        match self {
            Pool::Common => 0.15,
            Pool::Funds | Pool::DayTrade => 0.20,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Pool::Common => "operações comuns (ações, ETFs, BDRs)",
            Pool::Funds => "fundos imobiliários",
            Pool::DayTrade => "day trade",
        }
    }
}

/// Sales of shares up to this amount in a month have their gains exempt.
pub const SHARES_EXEMPTION: f64 = 20_000.0;
/// A DARF under this amount is not paid: it adds to the next one.
pub const DARF_MINIMUM: f64 = 10.0;

/// A holding's state after its history.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    pub quantity: f64,
    pub cost: f64,
}

impl Book {
    pub fn average(&self) -> f64 {
        if self.quantity > 0.0 {
            self.cost / self.quantity
        } else {
            0.0
        }
    }
}

/// One sale and what it made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sale {
    pub date: Date,
    pub ticker: String,
    pub pool: Pool,
    pub class: AssetClass,
    pub quantity: f64,
    pub value: f64,
    pub gain: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ledger {
    pub books: BTreeMap<String, Book>,
    pub sales: Vec<Sale>,
    /// Quantity of each ticker at the end of each day something changed, for the payouts' record dates.
    pub history: BTreeMap<String, Vec<(Date, f64)>>,
    /// What the user should look at: events without what they need (a bonus share's cost).
    pub notes: Vec<String>,
}

impl Ledger {
    /// The quantity held at the end of `day`.
    pub fn held_on(&self, ticker: &str, day: Date) -> f64 {
        self.history.get(ticker).and_then(|h| h.iter().rev().find(|(d, _)| *d <= day)).map(|(_, q)| *q).unwrap_or(0.0)
    }
}

/// The class of a ticker, from the user's positions, the trade itself, or its ending.
pub fn class_of(ticker: &str, positions: &[Position], trade_class: Option<AssetClass>) -> AssetClass {
    positions
        .iter()
        .find(|p| p.ticker == ticker)
        .map(|p| p.class)
        .or(trade_class)
        .unwrap_or_else(|| crate::import::guess_class(ticker).0)
}

fn pool_of(class: AssetClass) -> Pool {
    if class == AssetClass::Fii {
        Pool::Funds
    } else {
        Pool::Common
    }
}

/// Every trade and event in order: the books, the sales with their gains, and each day's quantities.
pub fn ledger(trades: &[Trade], events: &[Event], positions: &[Position]) -> Ledger {
    let mut l = Ledger::default();
    // Day trades first: what was bought and sold the same day by the same ticker.
    let mut days: BTreeMap<(Date, String), (f64, f64, f64, f64)> = BTreeMap::new(); // bought q, bought R$, sold q, sold R$
    for t in trades {
        let e = days.entry((t.date, t.ticker.clone())).or_default();
        match t.kind {
            TradeKind::Buy => {
                e.0 += t.quantity;
                e.1 += t.quantity * t.price + t.fees;
            }
            TradeKind::Sell => {
                e.2 += t.quantity;
                e.3 += t.quantity * t.price - t.fees;
            }
        }
    }
    enum Step {
        Buy(f64, f64),
        Sell(f64, f64),
        Event(Event),
    }
    let mut steps: Vec<(Date, u8, String, Step)> = Vec::new();
    for ((date, ticker), (bq, bv, sq, sv)) in days {
        let dt = bq.min(sq);
        let class = class_of(&ticker, positions, trades.iter().find(|t| t.ticker == ticker).and_then(|t| t.class));
        if dt > 0.0 {
            let gain = dt * (sv / sq - bv / bq);
            l.sales.push(Sale { date, ticker: ticker.clone(), pool: Pool::DayTrade, class, quantity: dt, value: dt * sv / sq, gain });
        }
        if bq - dt > 1e-9 {
            steps.push((date, 0, ticker.clone(), Step::Buy(bq - dt, bv / bq)));
        }
        if sq - dt > 1e-9 {
            steps.push((date, 2, ticker.clone(), Step::Sell(sq - dt, sv / sq)));
        }
    }
    for e in events {
        // Payouts change no book (amortizations do); corporate events before the day's sales.
        steps.push((e.date, 1, e.ticker.clone(), Step::Event(e.clone())));
    }
    steps.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    for (date, _, ticker, step) in steps {
        let class = class_of(&ticker, positions, trades.iter().find(|t| t.ticker == ticker).and_then(|t| t.class));
        let book = l.books.entry(ticker.clone()).or_default();
        match step {
            Step::Buy(q, price) => {
                book.quantity += q;
                book.cost += q * price;
            }
            Step::Sell(q, price) => {
                let sold = q.min(book.quantity);
                if sold + 1e-9 < q {
                    l.notes.push(format!("{ticker}: sold {} on {} but only {} were on record (a trade or a transfer is missing).", grouped(q, 0), date.br(), grouped(book.quantity, 0)));
                }
                let avg = book.average();
                let gain = sold * (price - avg);
                book.cost -= sold * avg;
                book.quantity -= sold;
                if book.quantity < 1e-9 {
                    book.quantity = 0.0;
                    book.cost = 0.0;
                }
                l.sales.push(Sale { date, ticker: ticker.clone(), pool: pool_of(class), class, quantity: sold, value: sold * price, gain });
            }
            Step::Event(e) => match e.kind {
                EventKind::Amortization => book.cost = (book.cost - e.value).max(0.0),
                EventKind::Bonus | EventKind::Subscription => {
                    book.quantity += e.quantity;
                    book.cost += e.quantity * e.value;
                    if e.kind == EventKind::Bonus && e.value <= 0.0 {
                        l.notes.push(format!("{ticker}: bonus shares on {} without their cost: the company's notice gives it (it adds to the average price).", date.br()));
                    }
                }
                EventKind::Split => book.quantity += e.quantity,
                EventKind::Grouping => {
                    if e.quantity > 0.0 && e.quantity < book.quantity {
                        book.quantity = e.quantity;
                    }
                    l.notes.push(format!("{ticker}: a reverse split on {}: check the quantity.", date.br()));
                }
                EventKind::Income | EventKind::Dividend | EventKind::InterestOnEquity => {}
            },
        }
        let q = book.quantity;
        let h = l.history.entry(ticker).or_default();
        match h.last_mut() {
            Some(last) if last.0 == date => last.1 = q,
            _ => h.push((date, q)),
        }
    }
    l
}

/// One month's taxes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Month {
    /// "2026-10".
    pub month: String,
    pub sales_shares: f64,
    /// Gains (losses negative) by pool, before offsets.
    pub gains: BTreeMap<Pool, f64>,
    /// Shares' sales under R$ 20,000: their gains are exempt.
    pub exempt: f64,
    /// Losses carried into the month, by pool.
    pub carried_in: BTreeMap<Pool, f64>,
    pub tax: f64,
    /// The 0.005 % withheld on sales by the broker (deducted).
    pub withheld: f64,
    /// What to pay this month (with what was under R$ 10 before).
    pub darf: f64,
    pub due: Option<Date>,
}

/// The months with sales, in order, with losses carried from one to the next.
pub fn months(sales: &[Sale]) -> Vec<Month> {
    let mut by_month: BTreeMap<String, Vec<&Sale>> = BTreeMap::new();
    for s in sales {
        by_month.entry(s.date.month_key()).or_default().push(s);
    }
    let mut carry: BTreeMap<Pool, f64> = BTreeMap::new();
    let mut pending = 0.0;
    let mut out = Vec::new();
    for (month, list) in by_month {
        let mut m = Month { month: month.clone(), carried_in: carry.clone(), ..Month::default() };
        m.sales_shares = list.iter().filter(|s| s.pool == Pool::Common && s.class == AssetClass::Acao).map(|s| s.value).sum();
        for s in &list {
            *m.gains.entry(s.pool).or_default() += s.gain;
        }
        // Shares' gains are exempt in a month of small sales (the ETFs' and BDRs' are not).
        let shares_gain: f64 = list.iter().filter(|s| s.pool == Pool::Common && s.class == AssetClass::Acao).map(|s| s.gain).sum();
        let exempt = m.sales_shares <= SHARES_EXEMPTION && shares_gain > 0.0;
        if exempt {
            m.exempt = shares_gain;
        }
        let mut tax = 0.0;
        for pool in [Pool::Common, Pool::Funds, Pool::DayTrade] {
            let mut gain = m.gains.get(&pool).copied().unwrap_or(0.0);
            if pool == Pool::Common && exempt {
                gain -= shares_gain;
            }
            let c = carry.entry(pool).or_default();
            let net = gain - *c;
            if net > 0.0 {
                tax += net * pool.rate();
                *c = 0.0;
            } else {
                *c = -net;
            }
        }
        m.withheld = list.iter().filter(|s| s.pool != Pool::DayTrade).map(|s| s.value * 0.00005).sum::<f64>()
            + list.iter().filter(|s| s.pool == Pool::DayTrade && s.gain > 0.0).map(|s| s.gain * 0.01).sum::<f64>();
        m.tax = tax;
        let to_pay = (tax - m.withheld).max(0.0) + pending;
        if to_pay >= DARF_MINIMUM {
            m.darf = to_pay;
            pending = 0.0;
            let d = Date::parse(&format!("{month}-01")).unwrap_or_default().add_months(1);
            m.due = Some(last_business_day(d.y, d.m));
        } else {
            pending = to_pay;
        }
        out.push(m);
    }
    out
}

/// A payout the books say should have come, and what came.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingPayout {
    pub ticker: String,
    pub record: Date,
    pub shares: f64,
    pub expected: f64,
    pub received: f64,
}

/// The funds' announced dividends of the last year, against the payouts B3 recorded (only when B3's movements were
/// imported for that time).
pub fn missing_payouts(l: &Ledger, events: &[Event], market: &Market, today: Date) -> Vec<MissingPayout> {
    let Some(first_event) = events.iter().filter(|e| matches!(e.kind, EventKind::Income | EventKind::Dividend)).map(|e| e.date).min() else {
        return Vec::new();
    };
    let last_event = events.iter().map(|e| e.date).max().unwrap_or(today);
    let mut out = Vec::new();
    for (ticker, fund) in &market.funds {
        for p in fund.payouts.iter().filter(|p| !p.amortization) {
            let paid_by = p.payment.unwrap_or(p.record.add_days(30));
            // Only payouts that would have been paid within the time B3's movements cover.
            if paid_by < first_event || paid_by > last_event || p.record < today.add_months(-12) {
                continue;
            }
            let shares = l.held_on(ticker, p.record);
            if shares <= 0.0 {
                continue;
            }
            let expected = shares * p.value;
            let received: f64 = events
                .iter()
                .filter(|e| &e.ticker == ticker && matches!(e.kind, EventKind::Income | EventKind::Dividend) && e.date >= p.record && e.date <= paid_by.add_days(5))
                .map(|e| e.value)
                .sum();
            if expected > 0.5 && received < expected * 0.9 {
                out.push(MissingPayout { ticker: ticker.clone(), record: p.record, shares, expected, received });
            }
        }
    }
    out.sort_by_key(|m| m.record);
    out
}

/// A position that does not match its history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Difference {
    pub ticker: String,
    /// From the trades.
    pub books: Book,
    /// What the user keeps (Investidor10, typed).
    pub kept_quantity: f64,
    pub kept_average: f64,
}

pub fn reconcile(l: &Ledger, positions: &[Position]) -> Vec<Difference> {
    let mut out = Vec::new();
    for p in positions.iter().filter(|p| p.class.quoted()) {
        let Some(b) = l.books.get(&p.ticker) else { continue };
        let qty_off = (b.quantity - p.quantity).abs() > 0.5;
        let avg_off = p.average_price > 0.0 && b.average() > 0.0 && ((b.average() / p.average_price) - 1.0).abs() > 0.02;
        if qty_off || avg_off {
            out.push(Difference { ticker: p.ticker.clone(), books: b.clone(), kept_quantity: p.quantity, kept_average: p.average_price });
        }
    }
    out
}

/// One line of the declaration's Bens e Direitos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub group: String,
    pub code: String,
    pub cnpj: String,
    pub description: String,
    pub previous: f64,
    pub current: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Declaration {
    pub year: i32,
    pub assets: Vec<Asset>,
    /// Exempt: dividends of shares (code 09), FII income (other exempt income).
    pub dividends: f64,
    pub fund_income: f64,
    /// Taxed at source: interest on equity (code 10).
    pub interest_on_equity: f64,
    pub months: Vec<Month>,
}

fn group_code(class: AssetClass) -> (&'static str, &'static str) {
    match class {
        AssetClass::Acao => ("03", "01"),
        AssetClass::Fii => ("07", "03"),
        AssetClass::RendaFixa | AssetClass::Tesouro => ("04", "01 or 02"),
        _ => ("07", "check in the program"),
    }
}

/// The year's declaration, from the books at the end of the year before and of the year.
pub fn declaration(year: i32, trades: &[Trade], events: &[Event], positions: &[Position], market: &Market) -> Declaration {
    let end = Date { y: year, m: 12, d: 31 };
    let before_end = Date { y: year - 1, m: 12, d: 31 };
    let books_at = |day: Date| {
        let t: Vec<Trade> = trades.iter().filter(|t| t.date <= day).cloned().collect();
        let e: Vec<Event> = events.iter().filter(|e| e.date <= day).cloned().collect();
        ledger(&t, &e, positions).books
    };
    let now = books_at(end);
    let before = books_at(before_end);
    let mut assets = Vec::new();
    for (ticker, b) in &now {
        let prev = before.get(ticker).map(|b| b.cost).unwrap_or(0.0);
        if b.quantity <= 0.0 && prev <= 0.0 {
            continue;
        }
        let class = class_of(ticker, positions, None);
        let (group, code) = group_code(class);
        let cnpj = market
            .funds
            .get(ticker)
            .map(|f| f.cnpj.clone())
            .or_else(|| market.company_cnpjs.get(ticker).map(|(c, _)| c.clone()))
            .unwrap_or_default();
        let name = market.company_cnpjs.get(ticker).map(|(_, n)| n.clone()).filter(|n| !n.is_empty());
        assets.push(Asset {
            group: group.into(),
            code: code.into(),
            cnpj,
            description: format!(
                "{} {} de {}{}, custo médio {} (corretora: informe a sua)",
                grouped(b.quantity, if b.quantity.fract() == 0.0 { 0 } else { 2 }),
                if class == AssetClass::Fii { "cotas" } else { "ações" },
                ticker,
                name.map(|n| format!(" ({n})")).unwrap_or_default(),
                brl(b.average())
            ),
            previous: prev,
            current: b.cost,
        });
    }
    let in_year = |e: &&Event| e.date.y == year;
    let sum = |k: EventKind| events.iter().filter(in_year).filter(|e| e.kind == k).map(|e| e.value).sum::<f64>();
    let l = ledger(trades, events, positions);
    Declaration {
        year,
        assets,
        dividends: sum(EventKind::Dividend),
        fund_income: sum(EventKind::Income),
        interest_on_equity: sum(EventKind::InterestOnEquity),
        months: months(&l.sales).into_iter().filter(|m| m.month.starts_with(&year.to_string())).collect(),
    }
}

/// The audit in words, for the brain.
pub fn for_model(l: &Ledger, months_list: &[Month], missing: &[MissingPayout], diffs: &[Difference], today: Date) -> String {
    let mut out = String::new();
    if l.books.is_empty() {
        return "No trade or B3 movement has been imported yet: the user can drop B3's 'Movimentação' and 'Negociação' exports (Área do Investidor → Extratos) on the island, or give the file's path.".into();
    }
    out.push_str("Average prices rebuilt from the trades and events:");
    for (t, b) in l.books.iter().filter(|(_, b)| b.quantity > 0.0) {
        out.push_str(&format!("\n- {t}: {} at {}", grouped(b.quantity, 0), brl(b.average())));
    }
    if !diffs.is_empty() {
        out.push_str("\nDifferences with the positions kept elsewhere (Investidor10 or typed):");
        for d in diffs {
            out.push_str(&format!(
                "\n- {}: the history gives {} at {}, the portfolio says {} at {}",
                d.ticker,
                grouped(d.books.quantity, 0),
                brl(d.books.average()),
                grouped(d.kept_quantity, 0),
                brl(d.kept_average)
            ));
        }
    }
    let recent: Vec<&Month> = months_list.iter().filter(|m| m.month.as_str() >= today.add_months(-12).month_key().as_str()).collect();
    if recent.is_empty() {
        out.push_str("\nNo sale in the last twelve months: no tax on gains.");
    } else {
        out.push_str("\nMonths with sales (estimate, Receita's rules of 2026):");
        for m in recent {
            let gains: Vec<String> = m.gains.iter().map(|(p, g)| format!("{}: {}", p.label(), brl(*g))).collect();
            out.push_str(&format!("\n- {}: {}", m.month, gains.join(", ")));
            if m.exempt > 0.0 {
                out.push_str(&format!("; shares' sales {} ≤ R$ 20.000: {} exempt", brl(m.sales_shares), brl(m.exempt)));
            }
            if m.darf > 0.0 {
                out.push_str(&format!("; DARF code 6015 of {} due {}", brl(m.darf), m.due.map(|d| d.br()).unwrap_or_default()));
            } else if m.tax > 0.0 {
                out.push_str("; tax under R$ 10: it adds to the next DARF");
            }
            let carry: f64 = m.carried_in.values().sum();
            if carry > 0.0 {
                out.push_str(&format!("; losses carried in {}", brl(carry)));
            }
        }
    }
    if !missing.is_empty() {
        out.push_str("\nPayouts that should have arrived and are not in B3's movements:");
        for m in missing {
            out.push_str(&format!(
                "\n- {} record {}: {} shares × announced dividend = {}, received {}",
                m.ticker,
                m.record.br(),
                grouped(m.shares, 0),
                brl(m.expected),
                brl(m.received)
            ));
        }
    }
    for n in &l.notes {
        out.push_str(&format!("\nNote: {n}"));
    }
    out
}

pub fn declaration_for_model(d: &Declaration) -> String {
    let mut out = format!("Income tax declaration {} (base year {}), an estimate to check in the Receita's program:", d.year + 1, d.year);
    out.push_str("\nBens e Direitos (cost, never market value):");
    for a in &d.assets {
        out.push_str(&format!(
            "\n- group {} code {}{}: {} — 31/12/{} {} → 31/12/{} {}",
            a.group,
            a.code,
            if a.cnpj.is_empty() { String::new() } else { format!(", CNPJ {}", a.cnpj) },
            a.description,
            d.year - 1,
            brl(a.previous),
            d.year,
            brl(a.current)
        ));
    }
    out.push_str(&format!(
        "\nRendimentos isentos: dividends {} (code 09), FII income {} (other exempt income, by fund CNPJ).\nTributação exclusiva: interest on equity {} (code 10).",
        brl(d.dividends),
        brl(d.fund_income),
        brl(d.interest_on_equity)
    ));
    if !d.months.is_empty() {
        out.push_str("\nRenda variável: the months with sales are in the audit (gains per month, DARFs paid).");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FundData;
    use crate::sources::fnet::Payout;

    fn d(s: &str) -> Date {
        Date::parse(s).unwrap()
    }

    fn trade(date: &str, t: &str, kind: TradeKind, q: f64, p: f64) -> Trade {
        Trade { date: d(date), ticker: t.into(), kind, quantity: q, price: p, ..Trade::default() }
    }

    fn event(date: &str, t: &str, kind: EventKind, q: f64, v: f64) -> Event {
        Event { date: d(date), ticker: t.into(), kind, quantity: q, value: v }
    }

    #[test]
    fn average_prices_follow_trades_and_events() {
        let trades = vec![
            trade("2026-01-10", "BTCI11", TradeKind::Buy, 100.0, 9.0),
            trade("2026-02-10", "BTCI11", TradeKind::Buy, 100.0, 8.0),
            trade("2026-03-10", "BTCI11", TradeKind::Sell, 50.0, 9.5),
        ];
        let events = vec![event("2026-02-20", "BTCI11", EventKind::Amortization, 200.0, 10.0), event("2026-04-01", "BTCI11", EventKind::Split, 150.0, 0.0)];
        let l = ledger(&trades, &events, &[]);
        let b = &l.books["BTCI11"];
        // 200 at 8.50 = 1700, amortized 10 → 1690 (8.45 each); 50 sold; split doubles the 150 left.
        assert!((b.quantity - 300.0).abs() < 1e-9);
        assert!((b.cost - 150.0 * 8.45).abs() < 1e-6);
        let sale = &l.sales[0];
        assert_eq!(sale.pool, Pool::Funds);
        assert!((sale.gain - 50.0 * (9.5 - 8.45)).abs() < 1e-9);
        assert_eq!(l.held_on("BTCI11", d("2026-02-15")), 200.0);
        assert_eq!(l.held_on("BTCI11", d("2026-01-01")), 0.0);
    }

    #[test]
    fn monthly_taxes_exemption_losses_and_the_darf() {
        let pos = vec![Position { ticker: "SPXB11".into(), class: AssetClass::Etf, quantity: 0.0, average_price: 0.0, subtype: String::new(), current_value: None, note: String::new() }];
        let trades = vec![
            trade("2026-01-05", "TIMS3", TradeKind::Buy, 1000.0, 10.0),
            trade("2026-01-05", "GARE11", TradeKind::Buy, 1000.0, 10.0),
            trade("2026-01-05", "SPXB11", TradeKind::Buy, 100.0, 10.0),
            // February: shares sold for 12,000 (exempt gain 2,000), a FII sold at a 500 loss.
            trade("2026-02-10", "TIMS3", TradeKind::Sell, 1000.0, 12.0),
            trade("2026-02-10", "GARE11", TradeKind::Sell, 500.0, 9.0),
            // March: the FII gains 1,500, less the 500 carried: 20 % of 1,000.
            trade("2026-03-10", "GARE11", TradeKind::Sell, 500.0, 13.0),
            // April: an ETF gain of 50 (no exemption): 15 % = 7.50, under the minimum.
            trade("2026-04-10", "SPXB11", TradeKind::Sell, 50.0, 11.0),
            // A day trade in May: bought and sold the same day.
            trade("2026-05-04", "TIMS3", TradeKind::Buy, 100.0, 10.0),
            trade("2026-05-04", "TIMS3", TradeKind::Sell, 100.0, 11.0),
        ];
        let l = ledger(&trades, &[], &pos);
        let ms = months(&l.sales);
        assert_eq!(ms.len(), 4);
        let feb = &ms[0];
        assert!((feb.exempt - 2000.0).abs() < 1e-6);
        assert_eq!(feb.darf, 0.0);
        let mar = &ms[1];
        assert!((mar.carried_in[&Pool::Funds] - 500.0).abs() < 1e-6);
        assert!((mar.tax - 200.0).abs() < 1e-6);
        assert!(mar.darf > 199.0 && mar.darf < 200.0, "the 0.005 % withheld is deducted");
        assert_eq!(mar.due, Some(d("2026-04-30")));
        let apr = &ms[2];
        assert!((apr.tax - 7.5).abs() < 1e-6 && apr.darf == 0.0, "under R$ 10: carried to the next DARF");
        let may = &ms[3];
        assert!((may.gains[&Pool::DayTrade] - 100.0).abs() < 1e-6);
        assert!(may.darf > 26.0 && may.darf < 27.0, "the day trade's 20 % (less 1 % withheld) plus April's pending");
        let text = for_model(&l, &ms, &[], &[], d("2026-10-21"));
        assert!(text.contains("DARF code 6015") && text.contains("exempt"), "{text}");
    }

    #[test]
    fn missing_payouts_and_differences_with_the_portfolio() {
        let trades = vec![trade("2026-01-05", "GGRC11", TradeKind::Buy, 150.0, 9.20)];
        let events = vec![event("2026-08-21", "GGRC11", EventKind::Income, 150.0, 13.2), event("2026-10-01", "GGRC11", EventKind::Income, 150.0, 1.0)];
        let mut market = Market::default();
        let payout = |rec: &str, pay: &str| Payout { record: d(rec), payment: Some(d(pay)), value: 0.088, amortization: false, tax_free: Some(true), period: String::new(), document: 1 };
        market.funds.insert(
            "GGRC11".into(),
            FundData { ticker: "GGRC11".into(), cnpj: "1".into(), payouts: vec![payout("2026-08-14", "2026-08-21"), payout("2026-09-14", "2026-09-21")], ..FundData::default() },
        );
        let l = ledger(&trades, &events, &[]);
        let missing = missing_payouts(&l, &events, &market, d("2026-10-21"));
        assert_eq!(missing.len(), 1, "{missing:?}");
        assert_eq!(missing[0].record, d("2026-09-14"));
        assert!((missing[0].expected - 150.0 * 0.088).abs() < 1e-9);
        let kept = vec![Position { ticker: "GGRC11".into(), class: AssetClass::Fii, quantity: 200.0, average_price: 9.20, subtype: String::new(), current_value: None, note: String::new() }];
        let diffs = reconcile(&l, &kept);
        assert_eq!(diffs.len(), 1);
        assert!(missing_payouts(&l, &[], &market, d("2026-10-21")).is_empty(), "nothing to compare without B3's movements");
    }

    #[test]
    fn the_yearly_declaration() {
        let trades = vec![trade("2025-06-01", "TIMS3", TradeKind::Buy, 10.0, 18.0), trade("2026-03-01", "TIMS3", TradeKind::Buy, 20.0, 18.90), trade("2026-02-01", "GGRC11", TradeKind::Buy, 150.0, 9.20)];
        let events = vec![event("2026-05-01", "TIMS3", EventKind::InterestOnEquity, 30.0, 7.5), event("2026-08-21", "GGRC11", EventKind::Income, 150.0, 13.2)];
        let mut market = Market::default();
        market.company_cnpjs.insert("TIMS3".into(), ("02558115000121".into(), "TIM S.A.".into()));
        let pos = vec![Position { ticker: "GGRC11".into(), class: AssetClass::Fii, quantity: 150.0, average_price: 9.20, subtype: String::new(), current_value: None, note: String::new() }];
        let decl = declaration(2026, &trades, &events, &pos, &market);
        let tims = decl.assets.iter().find(|a| a.description.contains("TIMS3")).unwrap();
        assert_eq!((tims.group.as_str(), tims.code.as_str(), tims.cnpj.as_str()), ("03", "01", "02558115000121"));
        assert!((tims.previous - 180.0).abs() < 1e-9 && (tims.current - (180.0 + 20.0 * 18.90)).abs() < 1e-9);
        let ggrc = decl.assets.iter().find(|a| a.description.contains("GGRC11")).unwrap();
        assert_eq!(ggrc.group, "07");
        assert_eq!(decl.interest_on_equity, 7.5);
        assert_eq!(decl.fund_income, 13.2);
        assert!(declaration_for_model(&decl).contains("code 09"));
    }
}
