//! The facts the committee works on, checked before any mind sees them. Each says where it comes from, its reference
//! date (a price's moment, a fund report's month, a dividend's record date), when AXYON read it, how this code computed
//! it, and its status: confirmed (as its source says), computed (by this code, from confirmed facts, formula given), an
//! estimate (an extrapolation, said so), missing (what could not be read: never a fact against the asset), or
//! divergent (two sources disagree). Values keep their source's precision (R$ 0,083 stays R$ 0,083); every indicator
//! is computed here from them. In Portuguese: the user reads them in the app when a point cites one.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::library::Library;
use crate::model::{AssetClass, Market, Store};
use crate::text::{brl, brl_big, brl_exact, clip, grouped, pct};
use crate::{ceiling, fii, journal, portfolio, thesis};

pub const CONFIRMED: &str = "confirmado";
pub const COMPUTED: &str = "calculado";
pub const ESTIMATE: &str = "estimativa";
pub const MISSING: &str = "ausente";
pub const DIVERGENT: &str = "divergente";

/// A price and the moment it is of: further apart than this from a fund report's month, a P/VP mixes dates.
const DATES_APART: i64 = 75;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Fact {
    /// "F1"…, given when the facts are numbered for a meeting.
    pub id: String,
    /// carteira, preco, patrimonio, proventos, teto, empresa, macro, tese, decisao, documento.
    pub topic: String,
    pub text: String,
    pub source: String,
    /// Its reference date: the price's moment, the report's month, a dividend's record date.
    pub reference: String,
    /// When AXYON read it.
    pub consulted: String,
    /// How this code computed it.
    pub formula: String,
    pub status: String,
    /// A caution: dates far apart, too few months known, a figure without its date.
    pub warning: String,
    /// Its main figure, with its source's precision.
    pub value: Option<f64>,
}

impl Fact {
    fn new(topic: &str, status: &str, text: impl Into<String>, source: &str) -> Fact {
        Fact { topic: topic.into(), status: status.into(), text: text.into(), source: source.into(), ..Fact::default() }
    }

    fn at(mut self, reference: impl Into<String>) -> Fact {
        self.reference = reference.into();
        self
    }

    fn read(mut self, consulted: Option<Date>) -> Fact {
        self.consulted = consulted.map(|d| d.br()).unwrap_or_default();
        self
    }

    fn by(mut self, formula: impl Into<String>) -> Fact {
        self.formula = formula.into();
        self
    }

    fn worth(mut self, value: f64) -> Fact {
        self.value = Some(value);
        self
    }

    fn warn(mut self, warning: impl Into<String>) -> Fact {
        self.warning = warning.into();
        self
    }

    /// The fact as the brain reads it: "F7 [calculado · Google Finance e CVM · ref. 30/10/2026] …".
    pub fn line(&self) -> String {
        let mut meta = vec![self.status.clone(), self.source.clone()];
        if !self.reference.is_empty() {
            meta.push(format!("ref. {}", self.reference));
        }
        let mut out = format!("{} [{}] {}", self.id, meta.join(" · "), self.text);
        if !self.formula.is_empty() {
            out.push_str(&format!(" (conta: {})", self.formula));
        }
        if !self.warning.is_empty() {
            out.push_str(&format!(" ATENÇÃO: {}", self.warning));
        }
        out
    }
}

/// "09/2026": a fund report's month.
fn month_br(d: Date) -> String {
    format!("{:02}/{}", d.m, d.y)
}

/// A moment in Brasília (B3's own time): "30/10/2026 13:40".
fn moment(secs: i64) -> (Date, String) {
    let local = secs - 3 * 3600;
    let day = Date::from_days(local.div_euclid(86_400));
    let minute = local.rem_euclid(86_400) / 60;
    (day, format!("{} {:02}:{:02}", day.br(), minute / 60, minute % 60))
}

fn signed(v: f64, decimals: usize) -> String {
    format!("{}{}", if v >= 0.0 { "+" } else { "" }, grouped(v, decimals))
}

/// "0,904": a ratio with three places.
fn ratio(v: f64) -> String {
    grouped(v, 3)
}

/// The price the committee uses, its source and its moment: the session's (Google Finance) while it is newer than
/// B3's last close, else that close.
pub struct Price {
    pub value: f64,
    pub source: &'static str,
    pub day: Date,
    pub when: String,
}

pub fn price(market: &Market, ticker: &str) -> Option<Price> {
    if let Some(l) = market.live_now(ticker) {
        let (day, when) = moment(l.at);
        return Some(Price { value: l.price, source: "Google Finance (pregão, cerca de 15 min de atraso)", day, when });
    }
    let q = market.quotes.get(&ticker.to_uppercase()).filter(|q| q.close > 0.0)?;
    Some(Price { value: q.close, source: "B3 (fechamento)", day: q.date, when: q.date.br() })
}

/// The month a fund's report closes: its last day.
fn month_end(d: Date) -> Date {
    d.last_of_month()
}

/// The dividends of the twelve months to `today`, one per record date, newest first.
fn year_of_payouts(fund: &crate::model::FundData, today: Date) -> Vec<crate::sources::fnet::Payout> {
    let year_ago = today.add_months(-12);
    let mut list: Vec<crate::sources::fnet::Payout> = fund.payouts.iter().filter(|p| !p.amortization && p.record > year_ago && p.record <= today).cloned().collect();
    list.sort_by_key(|p| std::cmp::Reverse(p.record));
    list.dedup_by_key(|p| p.record);
    list
}

/// The FIIs of the portfolio by segment, as the CVM's reports name it: segment to (weight, tickers).
fn segments(store: &Store, market: &Market) -> BTreeMap<String, (f64, Vec<String>)> {
    let v = portfolio::value(&store.portfolio, market);
    let mut out: BTreeMap<String, (f64, Vec<String>)> = BTreeMap::new();
    for h in v.holdings.iter().filter(|h| h.class == AssetClass::Fii && h.value > 0.0) {
        let segment = market.funds.get(&h.ticker).and_then(|f| f.months.iter().max_by_key(|m| m.date)).map(|m| m.segment.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "segmento não informado".into());
        let entry = out.entry(segment).or_default();
        entry.0 += h.weight;
        entry.1.push(h.ticker.clone());
    }
    out
}

/// The facts on `ticker` for a proposal, in order: the user's portfolio (position, weight, targets, the proposal's
/// effect, segments, the user's own ceiling), the price, the fund's or company's figures, the dividends and yields,
/// the ceilings, the economy, the thesis and the user's decisions; a divergence between sources where there is one.
/// Not numbered yet (`number`).
pub fn gather(store: &Store, market: &Market, lib: Option<&Library>, ticker: &str, proposal: &str, today: Date) -> Vec<Fact> {
    let t = ticker;
    let mut out: Vec<Fact> = Vec::new();
    let p = &store.portfolio;
    let v = portfolio::value(p, market);
    let class = p.position(t).map(|x| x.class).unwrap_or_else(|| if market.funds.contains_key(t) { AssetClass::Fii } else { crate::import::guess_class(t).0 });
    let mine = if p.source == "computer" { "Sua carteira (cópia do AXYON do computador, da sua planilha)" } else { "Sua carteira no AXYON" };
    let now = price(market, t);

    // The portfolio.
    let holding = v.holdings.iter().find(|h| h.ticker == t);
    match holding {
        Some(h) => {
            out.push(
                Fact::new("carteira", CONFIRMED, format!("Você tem {} de {t} a preço médio de {} (custo {}).", grouped(h.quantity, 0), brl_exact(h.average_price), brl(h.quantity * h.average_price)), mine)
                    .at(p.updated.map(|d| d.br()).unwrap_or_default())
                    .worth(h.quantity),
            );
            out.push(
                Fact::new("carteira", COMPUTED, format!("{t} vale {} e pesa {} da carteira (total {}).", brl(h.value), pct(h.weight, 1), brl(v.value)), mine)
                    .by(format!("{} ÷ {} = {}", brl(h.value), brl(v.value), pct(h.weight, 2)))
                    .at(now.as_ref().map(|x| x.when.clone()).unwrap_or_default())
                    .worth(h.weight),
            );
        }
        None => out.push(Fact::new("carteira", CONFIRMED, format!("Você ainda não tem {t}."), mine)),
    }
    let weight = holding.map(|h| h.weight).unwrap_or(0.0);
    match p.asset_targets.get(t) {
        Some(target) => {
            out.push(Fact::new("carteira", CONFIRMED, format!("Sua meta para {t}: {} da carteira.", pct(*target, 1)), "Sua meta por ativo (planilha ou app)").worth(*target));
            out.push(
                Fact::new("carteira", COMPUTED, format!("{t} está {} ponto(s) {} da meta ({} contra {}).", grouped((weight - target).abs(), 1), if weight >= *target { "acima" } else { "abaixo" }, pct(weight, 1), pct(*target, 1)), mine)
                    .by(format!("{} − {} = {} p.p.", grouped(weight, 2), grouped(*target, 2), signed(weight - target, 2)))
                    .worth(weight - target),
            );
        }
        None => out.push(Fact::new(
            "carteira",
            MISSING,
            format!("Meta por ativo para {t}: não encontrada — nem na sua planilha (coluna \"Percentual Ideal\" ou \"Meta\") nem cadastrada no app."),
            mine,
        )),
    }
    if let Some(target) = p.targets.get(&class) {
        let now_class = v.classes.iter().find(|c| c.class == class).map(|c| c.weight).unwrap_or(0.0);
        out.push(
            Fact::new("carteira", COMPUTED, format!("{} somam {} da carteira; sua meta para a classe é {}.", class.label(), pct(now_class, 1), pct(*target, 1)), mine)
                .by(format!("{} − {} = {} p.p.", grouped(now_class, 2), grouped(*target, 2), signed(now_class - target, 2))),
        );
    }
    // The proposal's effect: "comprar 100 cotas" at the price used.
    let quantity = proposal.split_whitespace().filter_map(crate::text::num).find(|q| *q >= 1.0 && q.fract() == 0.0 && *q < 1_000_000.0);
    if let (Some(q), Some(pr)) = (quantity, now.as_ref()) {
        let amount = q * pr.value;
        let selling = crate::text::plain(proposal).contains("vend") || crate::text::plain(proposal).contains("sell");
        let held_value = holding.map(|h| h.value).unwrap_or(0.0);
        let (after_value, total_after) = if selling { ((held_value - amount).max(0.0), v.value - amount.min(held_value)) } else { (held_value + amount, v.value + amount) };
        if total_after > 0.0 {
            let after = after_value / total_after * 100.0;
            out.push(
                Fact::new("carteira", COMPUTED, format!("{} {} de {t} a {} ({}) levaria {t} de {} para {} da carteira{}.", if selling { "Vender" } else { "Comprar" }, grouped(q, 0), brl_exact(pr.value), brl(amount), pct(weight, 1), pct(after, 1), p.asset_targets.get(t).map(|x| format!(" (meta {})", pct(*x, 1))).unwrap_or_default()), mine)
                    .by(format!("({} {} {}) ÷ ({} {} {}) = {}", brl(held_value), if selling { "−" } else { "+" }, brl(amount), brl(v.value), if selling { "−" } else { "+" }, brl(amount), pct(after, 2)))
                    .at(pr.when.clone())
                    .worth(after),
            );
        }
    }
    if class == AssetClass::Fii {
        let segs = segments(store, market);
        let own = market.funds.get(t).and_then(|f| f.months.iter().max_by_key(|m| m.date)).map(|m| m.segment.trim().to_string()).filter(|s| !s.is_empty());
        if let Some(seg) = &own {
            match segs.get(seg) {
                Some((w, list)) => out.push(Fact::new("carteira", COMPUTED, format!("Segmento de {t} ({seg}): {} da carteira, em {}.", pct(*w, 1), list.join(", ")), "CVM — Informe Mensal (segmento) e sua carteira").by(format!("soma dos pesos de {}", list.join(" + "))).worth(*w)),
                None => out.push(Fact::new("carteira", CONFIRMED, format!("Segmento de {t}: {seg}; nenhum outro FII seu está nele."), "CVM — Informe Mensal (segmento)")),
            }
        }
        let others: Vec<String> = segs.iter().filter(|(s, _)| Some(*s) != own.as_ref()).map(|(s, (w, _))| format!("{s} {}", pct(*w, 1))).collect();
        if !others.is_empty() {
            out.push(Fact::new("carteira", COMPUTED, format!("Seus outros segmentos de FII: {}.", others.join("; ")), "CVM — Informe Mensal (segmento) e sua carteira"));
        }
    }
    if let Some(c) = store.ceilings.manual.get(t) {
        out.push(Fact::new("teto", CONFIRMED, format!("Seu teto para {t}: {}.", brl_exact(*c)), "Seu preço-teto (planilha ou app)").worth(*c));
        if let Some(pr) = &now {
            let gap = (pr.value / c - 1.0) * 100.0;
            out.push(
                Fact::new("teto", COMPUTED, format!("O preço ({}) está {} {} do seu teto.", brl_exact(pr.value), pct(gap.abs(), 2), if gap >= 0.0 { "acima" } else { "abaixo" }), pr.source)
                    .by(format!("({} ÷ {} − 1) × 100 = {}", brl_exact(pr.value), brl_exact(*c), signed(gap, 2)))
                    .at(pr.when.clone())
                    .worth(gap),
            );
        }
    }

    // The price.
    match &now {
        Some(pr) => {
            out.push(Fact::new("preco", CONFIRMED, format!("Preço de {t}: {}.", brl_exact(pr.value)), pr.source).at(pr.when.clone()).read(Some(pr.day)).worth(pr.value));
            if market.live_now(t).is_some() {
                if let Some(q) = market.quotes.get(&t.to_uppercase()).filter(|q| q.close > 0.0) {
                    out.push(Fact::new("preco", CONFIRMED, format!("Último fechamento na B3: {}.", brl_exact(q.close)), "B3 (fechamento)").at(q.date.br()).worth(q.close));
                }
            }
        }
        None => out.push(Fact::new("preco", MISSING, format!("Preço de {t}: não foi possível ler (nem pregão nem fechamento da B3)."), "B3 e Google Finance")),
    }

    // A fund: its report, its dividends, its yields.
    if let Some(fund) = market.funds.get(t) {
        let mut months = fund.months.clone();
        months.sort_by_key(|m| m.date);
        let consulted = fund.checked.or(market.funds_read);
        let cvm = "CVM — Informe Mensal do FII";
        let latest = months.iter().rev().find(|m| m.vp_share.is_some());
        if let Some(info) = months.last() {
            let kind = fii::report(fund, now.as_ref().map(|x| x.value), &market.economy, today).kind;
            out.push(
                Fact::new("patrimonio", CONFIRMED, format!("{t} é {}{}{}.", if info.name.is_empty() { "um FII".to_string() } else { info.name.clone() }, if kind.is_empty() { String::new() } else { format!(", fundo de {kind}") }, if info.segment.is_empty() { String::new() } else { format!(", segmento {}", info.segment) }), cvm)
                    .at(month_br(info.date))
                    .read(consulted),
            );
        }
        match latest {
            Some(m) => {
                let vp = m.vp_share.unwrap_or(0.0);
                out.push(Fact::new("patrimonio", CONFIRMED, format!("VP por cota: {}.", brl_exact(vp)), cvm).at(month_br(m.date)).read(consulted).worth(vp));
                if let Some(pr) = &now {
                    let pvp = pr.value / vp;
                    let apart = month_end(m.date).until(pr.day);
                    let mut f = Fact::new("patrimonio", COMPUTED, format!("P/VP de {t}: {}.", ratio(pvp)), "Preço e CVM — Informe Mensal")
                        .by(format!("preço {} ({}) ÷ VP {} ({}) = {}", brl_exact(pr.value), pr.when, brl_exact(vp), month_br(m.date), ratio(pvp)))
                        .at(format!("preço {}; VP {}", pr.when, month_br(m.date)))
                        .worth(pvp);
                    if apart > DATES_APART {
                        f = f.warn(format!("o VP é de {}, {} dias antes do preço: o P/VP mistura datas distantes", month_br(m.date), apart));
                    }
                    out.push(f);
                    let discount = (1.0 - pvp) * 100.0;
                    out.push(
                        Fact::new("patrimonio", COMPUTED, if discount >= 0.0 { format!("Desconto patrimonial: {}.", pct(discount, 2)) } else { format!("Ágio sobre o patrimônio: {}.", pct(-discount, 2)) }, "Preço e CVM — Informe Mensal")
                            .by(format!("(1 − {}) × 100 = {}", ratio(pvp), signed(discount, 2)))
                            .worth(discount),
                    );
                }
                let year_back = months.iter().filter(|x| x.date <= m.date.add_months(-12) && x.vp_share.is_some()).max_by_key(|x| x.date);
                if let Some(old) = year_back {
                    let old_vp = old.vp_share.unwrap_or(0.0);
                    if old_vp > 0.0 {
                        let change = (vp / old_vp - 1.0) * 100.0;
                        out.push(
                            Fact::new("patrimonio", COMPUTED, format!("O VP por cota variou {} em 12 meses.", pct(change, 2)), cvm)
                                .by(format!("({} em {} ÷ {} em {} − 1) × 100 = {}", brl_exact(vp), month_br(m.date), brl_exact(old_vp), month_br(old.date), signed(change, 2)))
                                .worth(change),
                        );
                    }
                }
                if let (Some(net), Some(cash)) = (m.net_assets, m.cash) {
                    if net > 0.0 {
                        out.push(Fact::new("patrimonio", CONFIRMED, format!("Patrimônio líquido {} e caixa e ativos líquidos {}.", brl_big(net), brl_big(cash)), cvm).at(month_br(m.date)).worth(net));
                        out.push(
                            Fact::new("patrimonio", COMPUTED, format!("Caixa e ativos líquidos são {} do patrimônio (caixa imediato; não inclui recursos futuros previstos).", pct(cash / net * 100.0, 2)), cvm)
                                .by(format!("{} ÷ {} = {}", brl_big(cash), brl_big(net), pct(cash / net * 100.0, 2)))
                                .at(month_br(m.date))
                                .worth(cash / net * 100.0),
                        );
                    }
                }
                if let Some(h) = m.holders {
                    out.push(Fact::new("patrimonio", CONFIRMED, format!("{} cotistas.", grouped(h, 0)), cvm).at(month_br(m.date)).worth(h));
                }
                if let Some(dy) = m.dividend_yield {
                    out.push(Fact::new("proventos", CONFIRMED, format!("DY do mês informado pelo fundo: {} (sobre o patrimônio, não sobre o preço).", pct(dy, 2)), cvm).at(month_br(m.date)).worth(dy));
                }
            }
            None => out.push(Fact::new("patrimonio", MISSING, format!("VP por cota de {t}: o Informe Mensal da CVM ainda não foi lido."), cvm)),
        }
        // The dividends, each with its record and payment dates.
        let payouts = year_of_payouts(fund, today);
        for (i, pay) in payouts.iter().take(3).enumerate() {
            out.push(
                Fact::new(
                    "proventos",
                    CONFIRMED,
                    format!(
                        "{} rendimento: {} por cota (data-com {}{}){}.",
                        if i == 0 { "Último" } else { "Anterior" },
                        brl_exact(pay.value),
                        pay.record.br(),
                        pay.payment.map(|d| format!(", pago em {}", d.br())).unwrap_or_default(),
                        if pay.period.is_empty() { String::new() } else { format!(", referente a {}", pay.period) }
                    ),
                    "FundosNet — aviso de rendimentos",
                )
                .at(pay.record.br())
                .read(consulted)
                .worth(pay.value),
            );
        }
        if payouts.is_empty() {
            out.push(Fact::new("proventos", MISSING, format!("Rendimentos de {t} nos últimos 12 meses: nenhum aviso lido no FundosNet."), "FundosNet"));
        }
        if let Some(pr) = &now {
            let mut months_known: Vec<(i32, u32)> = payouts.iter().map(|x| (x.record.y, x.record.m)).collect();
            months_known.dedup();
            let n = months_known.len();
            if n > 0 {
                let sum: f64 = payouts.iter().map(|x| x.value).sum();
                let dy = sum / pr.value * 100.0;
                let first = payouts.last().map(|x| x.record.br()).unwrap_or_default();
                let last = payouts.first().map(|x| x.record.br()).unwrap_or_default();
                let mut f = Fact::new("proventos", COMPUTED, format!("DY dos últimos 12 meses: {}.", pct(dy, 2)), "FundosNet e preço")
                    .by(format!("{} rendimentos ({first} a {last}) somam {} ÷ preço {} = {}", payouts.len(), brl_exact(sum), brl_exact(pr.value), pct(dy, 2)))
                    .at(format!("rendimentos até {last}; preço {}", pr.when))
                    .worth(dy);
                if n < 12 {
                    f = f.warn(format!("só {n} dos 12 meses têm rendimento conhecido: o DY de 12 meses fica subestimado"));
                }
                out.push(f);
            }
            if let Some(last) = payouts.first() {
                let dy = last.value * 12.0 / pr.value * 100.0;
                out.push(
                    Fact::new("proventos", ESTIMATE, format!("DY anualizado pelo último rendimento: {} (projeção, não é o DY realizado).", pct(dy, 2)), "FundosNet e preço")
                        .by(format!("{} × 12 ÷ {} = {}", brl_exact(last.value), brl_exact(pr.value), pct(dy, 2)))
                        .at(format!("rendimento de {}; preço {}", last.record.br(), pr.when))
                        .worth(dy),
                );
            }
        }
        // Its yield required by Bazin, and the ceilings worked out from the same figures.
        let report = fii::report(fund, now.as_ref().map(|x| x.value), &market.economy, today);
        let (y, why) = ceiling::fund_yield(&store.ceilings, &market.economy);
        let why_pt = if store.ceilings.fund_yield.is_some_and(|x| x > 0.0) { "o retorno que você pediu".to_string() } else { market.economy.cdi.map(|c| format!("CDI {} líquido de 15% de IR", pct(c, 2))).unwrap_or_else(|| "10% (CDI ainda não lido)".into()) };
        let _ = why;
        if let Some(year) = report.dividends_year.filter(|d| *d > 0.0) {
            let value = year / (y / 100.0);
            let mut f = Fact::new("teto", COMPUTED, format!("Teto Bazin: {}.", brl_exact((value * 100.0).round() / 100.0)), "AXYON (FundosNet e Banco Central)")
                .by(format!("{} de rendimentos por cota em 12 meses ÷ {} ({why_pt}) = {}", brl_exact(year), pct(y, 2), brl_exact((value * 100.0).round() / 100.0)))
                .worth(value);
            if report.yield_months < 12 {
                f = f.warn(format!("só {} meses conhecidos, no ritmo deles", report.yield_months));
            }
            out.push(f);
        }
        if let Some(vp) = report.vp_share.filter(|x| *x > 0.0) {
            let value = vp * store.ceilings.max_pvp.max(0.1);
            out.push(Fact::new("teto", COMPUTED, format!("Teto pelo P/VP: {}.", brl_exact((value * 100.0).round() / 100.0)), "AXYON (CVM — Informe Mensal)").by(format!("VP {} × P/VP máximo {} = {}", brl_exact(vp), grouped(store.ceilings.max_pvp, 2), brl_exact((value * 100.0).round() / 100.0))).worth(value));
        }
        out.push(Fact::new("proventos", COMPUTED, format!("Leitura do AXYON sobre a saúde dos rendimentos: {}.", health_pt(report.health)), "AXYON (CVM e FundosNet)"));
        for e in fii::events(fund, p.position(t).map(|x| x.quantity).unwrap_or(0.0), today, 60) {
            out.push(Fact::new("documento", CONFIRMED, format!("{}: {}{}", e.kind, e.title, e.deadline.map(|d| format!(" (prazo {})", d.br())).unwrap_or_default()), "FundosNet").at(e.date.br()));
        }
        // A divergence: the dividend the notice gives against the one the manager's report states.
        if let (Some(lib), Some(last)) = (lib, payouts.first()) {
            let reported = lib
                .docs
                .iter()
                .filter(|d| d.ticker == t && d.is_read() && d.delivered.is_some_and(|x| x.until(today) <= 60))
                .flat_map(|d| d.figures.iter().filter(|f| f.key == "dividend_per_share").map(move |f| (d, f)))
                .max_by_key(|(d, _)| d.delivered);
            if let Some((d, f)) = reported {
                let gap = (f.value - last.value).abs();
                if gap > 0.0005 && gap / last.value > 0.01 {
                    out.push(
                        Fact::new("proventos", DIVERGENT, format!("Divergência no rendimento por cota: aviso da FundosNet {} (data-com {}) × relatório gerencial {} (\"{}\", p. {}).", brl_exact(last.value), last.record.br(), brl_exact(f.value), clip(&d.title, 60), f.page), "FundosNet e relatório gerencial")
                            .warn("as fontes discordam: confira o mês de referência de cada uma antes de usar o número"),
                    );
                }
            }
        }
    }

    // A company.
    if let Some(c) = market.companies.get(t) {
        let m = crate::company::metrics(&c.figures);
        let end = m.end.map(|d| d.br()).unwrap_or_default();
        let source = "CVM — demonstrações (DFP/ITR)";
        let partial = (m.months > 0 && m.months < 12).then(|| format!("só {} meses de demonstrações: os fluxos estão no ritmo de um ano", m.months));
        for (key, label, percent) in [
            ("revenue", "Receita dos últimos 12 meses", false),
            ("ebitda", "EBITDA dos últimos 12 meses", false),
            ("net_income", "Lucro líquido dos últimos 12 meses", false),
            ("operating_cash", "Fluxo de caixa operacional dos últimos 12 meses", false),
            ("fcf", "Fluxo de caixa livre dos últimos 12 meses", false),
            ("dividends_paid", "Dividendos pagos nos últimos 12 meses", false),
            ("ebitda_margin", "Margem EBITDA", true),
            ("net_margin", "Margem líquida", true),
            ("payout", "Payout (dividendos ÷ lucro)", true),
            ("net_debt", "Dívida líquida", false),
            ("net_debt_ebitda", "Dívida líquida ÷ EBITDA", false),
        ] {
            if let Some(x) = m.now.get(key) {
                let text = if percent { pct(*x, 1) } else if key == "net_debt_ebitda" { format!("{}x", grouped(*x, 2)) } else { brl_big(*x) };
                let mut f = Fact::new("empresa", if percent || key == "net_debt_ebitda" { COMPUTED } else { CONFIRMED }, format!("{label}: {text}."), source).at(end.clone()).worth(*x);
                if let Some(w) = &partial {
                    f = f.warn(w.clone());
                }
                out.push(f);
            }
        }
        if m.now.is_empty() {
            out.push(Fact::new("empresa", MISSING, format!("Demonstrações de {t}: não lidas na CVM ainda."), source));
        }
        let a = ceiling::assess(store, market, t, &ceiling::Given::default(), today);
        for x in a.ceilings.iter().filter(|x| x.method != "manual") {
            out.push(Fact::new("teto", COMPUTED, format!("Teto {}: {}.", method_pt(x.method), brl_exact((x.value * 100.0).round() / 100.0)), "AXYON (CVM e preço)").by(basis_pt(&x.basis)).worth(x.value));
        }
    }

    // The economy.
    let e = &market.economy;
    let bc = "Banco Central (SGS e Focus)";
    let read = market.economy_read;
    let when = e.date.map(|d| d.br()).unwrap_or_default();
    if let Some(x) = e.selic {
        out.push(Fact::new("macro", CONFIRMED, format!("Selic meta: {} ao ano.", pct(x, 2)), bc).at(when.clone()).read(read).worth(x));
    }
    match e.cdi {
        Some(x) => {
            out.push(Fact::new("macro", CONFIRMED, format!("CDI: {} ao ano.", pct(x, 2)), bc).at(when.clone()).read(read).worth(x));
            out.push(Fact::new("macro", COMPUTED, format!("CDI líquido de IR: {} (alíquota de 15%, mais de 2 anos) a {} (22,5%, até 6 meses).", pct(x * 0.85, 2), pct(x * 0.775, 2)), bc).by(format!("{} × 0,85 = {}; {} × 0,775 = {}", pct(x, 2), pct(x * 0.85, 2), pct(x, 2), pct(x * 0.775, 2))).worth(x * 0.85));
        }
        None => out.push(Fact::new("macro", MISSING, "CDI: não lido.", bc)),
    }
    if let Some(x) = e.ipca_12m {
        out.push(Fact::new("macro", CONFIRMED, format!("IPCA dos últimos 12 meses: {}.", pct(x, 2)), bc).at(when.clone()).read(read).worth(x));
    }
    for (label, list) in [("Selic", &e.selic_expected), ("IPCA", &e.ipca_expected)] {
        let years: Vec<String> = list.iter().take(3).map(|x| format!("{} em {}", pct(x.median, 2), x.year)).collect();
        if !years.is_empty() {
            out.push(Fact::new("macro", ESTIMATE, format!("Focus (mediana do mercado) para a {label}: {}.", years.join("; ")), bc).at(when.clone()));
        }
    }
    out.push(Fact::new("macro", MISSING, "Taxa do Tesouro IPCA+: não consultada (o AXYON ainda não lê o Tesouro Direto).", "Tesouro Direto"));

    // The thesis, and the user's own decisions.
    if let Some(th) = store.theses.iter().find(|x| x.ticker == t) {
        if !th.text.trim().is_empty() {
            out.push(Fact::new("tese", CONFIRMED, format!("Sua tese, escrita em {}: \"{}\".", th.written.br(), clip(&th.text, 300)), "Sua tese no AXYON").at(th.written.br()));
        }
        if let Some(state) = thesis::states(store, market, lib, today).into_iter().find(|s| s.ticker == t) {
            for c in state.checks {
                let held = match c.holds {
                    Some(true) => "atendida",
                    Some(false) => "QUEBRADA",
                    None => "sem dado para conferir",
                };
                out.push(Fact::new("tese", if c.holds.is_some() { COMPUTED } else { MISSING }, format!("Condição \"{}\": {held}{}.", c.label, c.now.map(|n| format!(" (agora {})", grouped(n, 2))).unwrap_or_default()), &c.source));
            }
        }
    }
    for d in store.decisions.iter().filter(|d| d.ticker == t).rev().take(3) {
        out.push(
            Fact::new("decisao", CONFIRMED, format!("Em {} você decidiu {} {t}{}: \"{}\"{}.", d.date.br(), action_pt(&d.action), d.price.map(|x| format!(" a {}", brl_exact(x))).unwrap_or_default(), clip(&d.reasons, 160), d.confidence.map(|c| format!(", {c}% de certeza")).unwrap_or_default()), "Seu diário de decisões")
                .at(d.date.br()),
        );
    }
    let cal = journal::calibration(&store.decisions, |x| market.price(x), market.economy.cdi, today);
    if cal.judged >= 3 {
        out.push(Fact::new("decisao", COMPUTED, format!("Das suas {} decisões já julgadas, {} acertaram.", cal.judged, cal.right), "Seu diário de decisões"));
    }
    out
}

/// The facts numbered F1, F2…, as the minds cite them.
pub fn number(mut facts: Vec<Fact>) -> Vec<Fact> {
    for (i, f) in facts.iter_mut().enumerate() {
        f.id = format!("F{}", i + 1);
    }
    facts
}

fn health_pt(h: fii::Health) -> &'static str {
    match h {
        fii::Health::Good => "saudável",
        fii::Health::Watch => "vale acompanhar",
        fii::Health::Risk => "em risco",
        fii::Health::Unknown => "dados insuficientes",
    }
}

fn action_pt(a: &str) -> &str {
    match a {
        "buy" => "comprar",
        "sell" => "vender",
        "hold" => "manter",
        "avoid" => "evitar",
        other => other,
    }
}

pub fn method_pt(m: &str) -> &'static str {
    match m {
        "bazin" => "Bazin",
        "bazin_projected" => "Bazin projetivo",
        "graham" => "Graham",
        "lynch" => "Peter Lynch",
        "pvp" => "pelo P/VP",
        _ => "seu",
    }
}

/// A ceiling's basis, written in English by ceiling.rs, in Portuguese.
fn basis_pt(b: &str) -> String {
    let mut s = b.to_string();
    for (en, pt) in [
        (" of dividends a share over twelve months, over ", " de proventos por ação em 12 meses ÷ "),
        (" of dividends a share in a year", " de proventos por cota em 12 meses"),
        ("earnings a share", "lucro por ação"),
        ("book value a share", "valor patrimonial por ação"),
        (" grown ", " crescendo "),
        (", paid out at ", ", payout de "),
        ("growth", "crescimento"),
        ("a fair P/E of", "P/L justo de"),
        (" a year over ", " por ano ÷ "),
        (" over ", " ÷ "),
        (" to ", " para "),
    ] {
        s = s.replace(en, pt);
    }
    s
}

#[cfg(test)]
pub mod fixtures {
    use super::*;
    use crate::model::{FundData, Position};
    use crate::sources::cvm_fii::FiiMonth;
    use crate::sources::fnet::Payout;

    pub fn day(y: i32, m: u32, d: u32) -> Date {
        Date::new(y, m, d).unwrap()
    }

    fn payout(record: Date, value: f64) -> Payout {
        Payout { record, payment: Some(record.add_days(14)), value, amortization: false, tax_free: Some(true), period: String::new(), document: 1 }
    }

    /// GARE11 as the committee saw it on 30/10/2026 (its shape, not its real figures): held at 19,5 % of the
    /// portfolio, the user's own ceiling R$ 8,00 and target 7,5 %, a VP of September, dividends of R$ 0,083.
    pub fn gare11() -> (Store, Market) {
        let mut store = Store::default();
        let pos = |t: &str, class: AssetClass, q: f64, pm: f64| Position { ticker: t.into(), class, quantity: q, average_price: pm, subtype: String::new(), current_value: None, note: String::new() };
        store.portfolio.positions = vec![
            pos("GARE11", AssetClass::Fii, 204.0, 8.95),
            pos("GGRC11", AssetClass::Fii, 150.0, 9.2),
            pos("BTLG11", AssetClass::Fii, 7.0, 101.0),
            pos("KNSC11", AssetClass::Fii, 380.0, 9.1),
            pos("BBAS3", AssetClass::Acao, 60.0, 21.0),
        ];
        store.portfolio.asset_targets.insert("GARE11".into(), 7.5);
        store.ceilings.manual.insert("GARE11".into(), 8.0);
        store.ceilings.max_pvp = 1.0;
        let mut market = Market::default();
        for (t, close) in [("GARE11", 8.14), ("GGRC11", 9.12), ("BTLG11", 101.5), ("KNSC11", 9.08), ("BBAS3", 22.1)] {
            market.quotes.insert(
                t.into(),
                crate::sources::b3::Quote { ticker: t.into(), date: day(2026, 10, 29), open: close, high: close, low: close, average: close, close, trades: 1, volume: 1.0, bdi: "02".into(), kind: String::new(), name: String::new(), isin: String::new(), market: 10 },
            );
        }
        let month = |m: u32, vp: f64, segment: &str| FiiMonth { date: day(2026, m, 1), vp_share: Some(vp), net_assets: Some(1_900_000_000.0), cash: Some(15_200_000.0), holders: Some(160_000.0), segment: segment.into(), name: "Guardian Real Estate".into(), ..FiiMonth::default() };
        let mut gare = FundData { ticker: "GARE11".into(), months: vec![month(9, 9.2318, "Logística"), month(9, 9.0012, "Logística")], ..FundData::default() };
        gare.months[0].date = day(2025, 9, 1);
        for i in 0..12 {
            gare.payouts.push(payout(day(2026, 10, 15).add_months(-i), if i == 0 { 0.083 } else { 0.0825 }));
        }
        market.funds.insert("GARE11".into(), gare);
        market.funds.insert("BTLG11".into(), FundData { ticker: "BTLG11".into(), months: vec![month(9, 100.0, "Logística")], ..FundData::default() });
        market.funds.insert("GGRC11".into(), FundData { ticker: "GGRC11".into(), months: vec![month(9, 9.9, "Híbrido")], ..FundData::default() });
        market.funds.insert("KNSC11".into(), FundData { ticker: "KNSC11".into(), months: vec![month(9, 9.2, "Títulos e Val. Mob.")], ..FundData::default() });
        market.economy.cdi = Some(13.65);
        market.economy.selic = Some(13.75);
        (store, market)
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{day, gare11};
    use super::*;

    fn find<'a>(facts: &'a [Fact], words: &str) -> &'a Fact {
        facts.iter().find(|f| f.text.contains(words)).unwrap_or_else(|| panic!("no fact with {words:?} in {facts:#?}"))
    }

    #[test]
    fn a_dividend_keeps_its_precision_into_the_yield() {
        let (store, market) = gare11();
        let today = day(2026, 10, 30);
        let facts = number(gather(&store, &market, None, "GARE11", "comprar 100 cotas de GARE11", today));
        let last = find(&facts, "Último rendimento");
        assert!(last.text.contains("R$ 0,083 por cota"), "not rounded to R$ 0,08: {}", last.text);
        assert_eq!(last.value, Some(0.083));
        // The yield annualized from it is computed from R$ 0,083, not from R$ 0,08.
        let annual = find(&facts, "DY anualizado pelo último rendimento");
        assert_eq!(annual.status, ESTIMATE);
        assert!((annual.value.unwrap() - 0.083 * 12.0 / 8.14 * 100.0).abs() < 1e-9, "{annual:?}");
        assert!(annual.formula.contains("R$ 0,083 × 12 ÷ R$ 8,14"), "{}", annual.formula);
        // Twelve months realized and the annualized estimate are two facts, said apart.
        let realized = find(&facts, "DY dos últimos 12 meses");
        assert_eq!(realized.status, COMPUTED);
        assert!((realized.value.unwrap() - (0.083 + 11.0 * 0.0825) / 8.14 * 100.0).abs() < 1e-9);
        assert!(realized.warning.is_empty(), "twelve months known");
    }

    #[test]
    fn the_pvp_names_the_dates_of_its_price_and_its_vp() {
        let (store, market) = gare11();
        let facts = number(gather(&store, &market, None, "GARE11", "", day(2026, 10, 30)));
        let pvp = find(&facts, "P/VP de GARE11");
        assert!(pvp.formula.contains("preço R$ 8,14 (29/10/2026)") && pvp.formula.contains("VP R$ 9,0012 (09/2026)"), "{}", pvp.formula);
        assert_eq!(pvp.reference, "preço 29/10/2026; VP 09/2026");
        assert!((pvp.value.unwrap() - 8.14 / 9.0012).abs() < 1e-12);
        assert!(pvp.warning.is_empty(), "September's VP and October's price are close enough");
        // A VP five months older than the price: said.
        let (store, mut market) = gare11();
        market.funds.get_mut("GARE11").unwrap().months.retain(|m| m.date.y == 2025);
        let facts = number(gather(&store, &market, None, "GARE11", "", day(2026, 10, 30)));
        assert!(find(&facts, "P/VP de GARE11").warning.contains("datas distantes"));
    }

    #[test]
    fn the_target_comes_from_the_portfolio_and_its_absence_is_said() {
        let (store, market) = gare11();
        let facts = number(gather(&store, &market, None, "GARE11", "comprar 100 cotas de GARE11", day(2026, 10, 30)));
        let weight = find(&facts, "GARE11 vale");
        assert!(weight.text.contains("pesa 19,5% da carteira"), "as on the user's screen: {}", weight.text);
        let target = find(&facts, "Sua meta para GARE11");
        assert_eq!((target.status.as_str(), target.value), (CONFIRMED, Some(7.5)));
        assert!(find(&facts, "acima da meta").text.contains("contra 7,5%"));
        assert!(find(&facts, "levaria GARE11 de").text.contains("(meta 7,5%)"));
        assert!(find(&facts, "Segmento de GARE11 (Logística)").text.contains("GARE11, BTLG11"), "the segment's other fund is named");
        // Without it: missing, said so, never guessed.
        let (mut store, market) = gare11();
        store.portfolio.asset_targets.clear();
        let facts = number(gather(&store, &market, None, "GARE11", "", day(2026, 10, 30)));
        let none = find(&facts, "Meta por ativo para GARE11");
        assert_eq!(none.status, MISSING);
        assert!(!facts.iter().any(|f| f.text.contains("Sua meta para GARE11")));
    }

    #[test]
    fn a_dividend_the_notice_and_the_report_disagree_on_is_a_divergence() {
        let (store, market) = gare11();
        let mut lib = Library::default();
        let pending = crate::library::Pending {
            key: "fnet:9".into(),
            ticker: "GARE11".into(),
            kind: crate::library::DocKind::Report,
            title: "Relatório Gerencial 09/2026".into(),
            reference: "09/2026".into(),
            delivered: Some(day(2026, 10, 20)),
            url: String::new(),
            download: String::new(),
        };
        lib.ingest(&pending, crate::library::fixtures::report_pages("7,5", "0,080"), day(2026, 10, 21));
        let facts = number(gather(&store, &market, Some(&lib), "GARE11", "", day(2026, 10, 30)));
        let d = find(&facts, "Divergência no rendimento");
        assert_eq!(d.status, DIVERGENT);
        assert!(d.text.contains("R$ 0,083") && d.text.contains("R$ 0,08") && d.text.contains("p. 2"), "{}", d.text);
    }

    #[test]
    fn missing_figures_are_missing_never_against_the_asset() {
        let (store, mut market) = gare11();
        market.funds.get_mut("GARE11").unwrap().payouts.clear();
        market.economy.cdi = None;
        let facts = number(gather(&store, &market, None, "GARE11", "", day(2026, 10, 30)));
        assert_eq!(find(&facts, "Rendimentos de GARE11 nos últimos 12 meses").status, MISSING);
        assert_eq!(find(&facts, "CDI: não lido").status, MISSING);
        assert_eq!(find(&facts, "Tesouro IPCA+").status, MISSING);
        assert!(facts.iter().all(|f| f.status != MISSING || f.value.is_none()), "a missing fact carries no figure");
        assert!(facts.iter().enumerate().all(|(i, f)| f.id == format!("F{}", i + 1)));
    }
}
