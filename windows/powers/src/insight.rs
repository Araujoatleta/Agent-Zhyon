//! What helps the user decide, asset by asset and month by month, from what AXYON already reads:
//! - an asset's sheet ("ficha"): its figures now and their history (the fund's VP, P/VP, yield and holders month by
//!   month, a year of dividends a share, the figures its reports gave, document by document), what it pays the user a
//!   month, its magic number, its ceilings, the court's last word, and the signals worth a look, good and bad;
//! - the month's contribution ("aporte"): where an amount goes, share by share, to bring the portfolio nearer its
//!   targets, the assets the court told to avoid and those in poor health left out, cheap ones first;
//! - the year's income tax ("IRPF"): each asset's line of Bens e Direitos at the year's end, the funds' exempt income,
//!   the dividends, the JCP, and the sales month by month;
//! - the portfolio's own return: the yearly rate its money made (buys, sales, proventos and what it is worth now),
//!   against the CDI.
//! Plain data in Portuguese for the app; the brain gets the same through wealth_wallet.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::date::Date;
use crate::library::Library;
use crate::model::{AssetClass, Market, Store, Trade, TradeKind};
use crate::text::{brl, grouped, pct};

// ── The asset's sheet ─────────────────────────────────────────────────────────────────────────────────────────────

/// A signal on an asset: good (a reason to look at it with favour), watch (a thing to follow), bad (a warning).
fn signal(tone: &str, text: String) -> Value {
    json!({ "tone": tone, "text": text })
}

/// The asset's sheet.
pub fn ficha(store: &Store, market: &Market, lib: Option<&Library>, ticker: &str, today: Date) -> Value {
    let v = crate::portfolio::value(&store.portfolio, market);
    let held = v.holdings.iter().find(|h| h.ticker == ticker);
    let class = held.map(|h| h.class).or_else(|| store.portfolio.position(ticker).map(|p| p.class)).unwrap_or_else(|| crate::trades::class_of(ticker, market).0);
    let price = held.and_then(|h| h.price).or_else(|| market.price(ticker));
    let quantity = held.map(|h| h.quantity).unwrap_or(0.0);
    let average = held.map(|h| h.average_price).unwrap_or(0.0);
    let mut signals: Vec<Value> = Vec::new();
    let mut indicators: Vec<Value> = Vec::new();
    let mut history: Vec<Value> = Vec::new();
    let mut dividends: Vec<Value> = Vec::new();
    let ind = |label: &str, value: String, note: String| json!({ "label": label, "value": value, "note": note });

    // A fund: the CVM's months, the notices' dividends.
    let mut last_dividend = None;
    let mut dividends_year = None;
    if let Some(fund) = market.funds.get(ticker) {
        let r = crate::fii::report(fund, price, &market.economy, today);
        last_dividend = r.last_dividend;
        dividends_year = r.dividends_year;
        if let Some(pvp) = r.p_vp {
            indicators.push(ind("P/VP", grouped(pvp, 2), if pvp < 1.0 { format!("desconto de {}", pct((1.0 - pvp) * 100.0, 0)) } else { format!("ágio de {}", pct((pvp - 1.0) * 100.0, 0)) }));
            if pvp < 0.95 {
                signals.push(signal("good", format!("Negocia a P/VP {}: cada R$ 1,00 de patrimônio custa R$ {} na bolsa.", grouped(pvp, 2), grouped(pvp, 2))));
            } else if pvp > 1.08 {
                signals.push(signal("bad", format!("P/VP {}: você paga {} acima do patrimônio por cota.", grouped(pvp, 2), pct((pvp - 1.0) * 100.0, 0))));
            }
        }
        if let Some(vp) = r.vp_share {
            indicators.push(ind("VP por cota", brl(vp), r.vp_date.map(|d| format!("{}/{}", d.m, d.y)).unwrap_or_default()));
        }
        if let Some(c) = r.vp_change_12m {
            indicators.push(ind("VP em 12 meses", format!("{}{}", if c >= 0.0 { "+" } else { "" }, pct(c, 1)), String::new()));
            if c < -3.0 {
                signals.push(signal("bad", format!("O VP por cota caiu {} em 12 meses: o patrimônio do fundo encolheu por cota.", pct(-c, 1))));
            } else if c > 2.0 {
                signals.push(signal("good", format!("O VP por cota subiu {} em 12 meses.", pct(c, 1))));
            }
        }
        if let Some(y) = r.yield_12m {
            indicators.push(ind("Dividend yield 12m", pct(y, 2), if r.yield_months < 12 { format!("{} meses conhecidos", r.yield_months) } else { "pela cotação".into() }));
        }
        if let Some(l) = r.last_dividend {
            indicators.push(ind("Último rendimento", brl_fine(l), r.avg_dividend_12m.map(|a| format!("média 12m {}", brl_fine(a))).unwrap_or_default()));
            if let Some(a) = r.avg_dividend_12m.filter(|a| *a > 0.0) {
                let cut = (1.0 - l / a) * 100.0;
                if cut > 8.0 {
                    signals.push(signal("bad", format!("O último rendimento ({}) veio {} abaixo da média de 12 meses ({}).", brl_fine(l), pct(cut, 0), brl_fine(a))));
                } else if cut < -5.0 {
                    signals.push(signal("good", format!("O último rendimento ({}) veio acima da média de 12 meses ({}).", brl_fine(l), brl_fine(a))));
                }
            }
        }
        if let Some(h) = r.holders {
            indicators.push(ind("Cotistas", grouped(h, 0), r.holders_change_12m.map(|c| format!("{}{} em 12m", if c >= 0.0 { "+" } else { "" }, pct(c, 1))).unwrap_or_default()));
        }
        if let Some(l) = r.liquidity {
            indicators.push(ind("Caixa / patrimônio", pct(l, 1), String::new()));
        }
        if !r.segment.is_empty() {
            indicators.push(ind("Segmento", r.segment.clone(), r.kind.clone()));
        }
        match r.health {
            crate::fii::Health::Risk => signals.push(signal("bad", "A saúde do provento pede atenção: o fundo distribui no limite do que gera ou o patrimônio vem caindo.".into())),
            crate::fii::Health::Watch => signals.push(signal("watch", "A saúde do provento pede acompanhamento nos próximos relatórios.".into())),
            _ => {}
        }
        // The last twelve months of the CVM's reports.
        let months: Vec<_> = fund.months.iter().rev().take(12).collect();
        for m in months.into_iter().rev() {
            history.push(json!({
                "month": m.date.month_key(),
                "vp": m.vp_share,
                "pvp": m.vp_share.zip(price).filter(|(vp, _)| *vp > 0.0).map(|(vp, p)| p / vp),
                "dy": m.dividend_yield,
                "holders": m.holders,
                "netAssets": m.net_assets,
            }));
        }
        // A year of dividends a share, one per data-com.
        let mut pays: Vec<_> = fund.payouts.iter().filter(|p| !p.amortization && p.of_share(ticker) && p.record <= today && p.record > today.add_months(-13)).collect();
        pays.sort_by_key(|p| p.record);
        pays.dedup_by_key(|p| p.record);
        for p in pays {
            dividends.push(json!({ "record": p.record, "payment": p.payment, "value": p.value }));
        }
    } else if let Some(c) = market.company_payouts.get(ticker) {
        let mut pays: Vec<_> = c.payouts.iter().filter(|p| p.record <= today && p.record > today.add_months(-13)).collect();
        pays.sort_by_key(|p| p.record);
        let year: f64 = pays.iter().map(|p| if p.tax_free == Some(false) { p.value * 0.85 } else { p.value }).sum();
        if year > 0.0 {
            dividends_year = Some(year);
            if let Some(p) = price.filter(|p| *p > 0.0) {
                indicators.push(ind("Dividend yield 12m", pct(year / p * 100.0, 2), "líquido de IR no JCP".into()));
            }
        }
        last_dividend = pays.last().map(|p| p.value);
        for p in pays {
            dividends.push(json!({ "record": p.record, "payment": p.payment, "value": p.value, "kind": p.period }));
        }
    }

    // What it pays the user, and its magic number.
    let mut income = json!(null);
    if let Some(l) = last_dividend.filter(|l| *l > 0.0) {
        let monthly = if class == AssetClass::Fii { l * quantity } else { dividends_year.unwrap_or(0.0) * quantity / 12.0 };
        let magic = price.filter(|p| *p > 0.0).map(|p| (p / l).ceil());
        income = json!({ "monthly": monthly, "magic": magic, "yieldOnCost": dividends_year.filter(|_| average > 0.0).map(|y| y / average * 100.0) });
        if let (Some(m), true) = (magic, class == AssetClass::Fii && quantity > 0.0) {
            if quantity >= m {
                signals.push(signal("good", format!("Você já passou do número mágico ({} cotas): os rendimentos de um mês compram uma cota nova.", grouped(m, 0))));
            } else {
                signals.push(signal("watch", format!("Faltam {} cotas para o número mágico ({}): o ponto em que um mês de rendimentos compra uma cota.", grouped(m - quantity, 0), grouped(m, 0))));
            }
        }
        if let (Some(y), true) = (dividends_year, average > 0.0) {
            indicators.push(ind("Yield on cost", pct(y / average * 100.0, 2), format!("sobre o seu preço médio {}", brl(average))));
        }
    }

    // Against the user's own price and target.
    if let (Some(p), true) = (price, average > 0.0) {
        if p < average * 0.97 {
            signals.push(signal("watch", format!("A cotação ({}) está {} abaixo do seu preço médio ({}): comprar agora baixa o seu preço médio.", brl(p), pct((1.0 - p / average) * 100.0, 1), brl(average))));
        }
    }
    let target = store.portfolio.asset_targets.get(ticker).copied();
    if let (Some(t), Some(h)) = (target, held) {
        if h.weight > t * 1.15 {
            signals.push(signal("bad", format!("Pesa {} da carteira, acima da sua meta de {}.", pct(h.weight, 1), pct(t, 1))));
        } else if h.weight < t * 0.85 {
            signals.push(signal("watch", format!("Pesa {} da carteira, abaixo da sua meta de {}: candidato a aporte.", pct(h.weight, 1), pct(t, 1))));
        }
    }

    // The ceilings.
    let a = crate::ceiling::assess(store, market, ticker, &crate::ceiling::Given::default(), today);
    let ceilings: Vec<Value> = a
        .ceilings
        .iter()
        .map(|c| json!({ "method": crate::ceiling::label(c.method), "value": c.value, "below": price.map(|p| crate::ceiling::below(p, c.value)) }))
        .collect();
    if let (Some(p), Some(best)) = (price, a.ceilings.iter().filter(|c| a.watched.contains(&c.method)).map(|c| c.value).fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))))) {
        if p <= best {
            signals.push(signal("good", format!("Abaixo do preço-teto ({}): {} de margem.", brl(best), pct(crate::ceiling::below(p, best), 1))));
        } else {
            signals.push(signal("watch", format!("Acima do preço-teto ({}).", brl(best))));
        }
    }

    // The court's last word.
    let court = store.verdicts.iter().rev().find(|v| v.ticker == ticker).map(|v| {
        let view = v.portfolio_fit.view.clone();
        if !v.vetoed_by().is_empty() || view == "evitar_novo_aporte" || view == "reduzir" {
            signals.push(signal("bad", format!("O tribunal, em {}, pediu para {}.", v.date.map(|d| d.br()).unwrap_or_default(), if view == "reduzir" { "reduzir a posição" } else { "evitar novo aporte" })));
        } else if view == "aumentar" {
            signals.push(signal("good", format!("O tribunal, em {}, viu espaço para aumentar a posição.", v.date.map(|d| d.br()).unwrap_or_default())));
        }
        json!({ "date": v.date, "view": view, "asset": v.asset.view, "vetoed": !v.vetoed_by().is_empty(), "question": v.question })
    });

    // The figures its reports gave, report by report.
    let mut reported: BTreeMap<String, (String, Vec<Value>)> = BTreeMap::new();
    if let Some(lib) = lib {
        let mut docs: Vec<_> = lib.docs.iter().filter(|d| d.ticker == ticker && d.is_read() && !d.figures.is_empty()).collect();
        docs.sort_by_key(|d| d.delivered);
        for d in docs.iter().rev().take(8).rev() {
            for f in &d.figures {
                let e = reported.entry(f.key.clone()).or_insert_with(|| (f.label.clone(), Vec::new()));
                e.1.push(json!({ "date": d.delivered, "value": f.value, "shown": f.shown(), "page": f.page, "document": d.key }));
            }
        }
    }
    let reports: Vec<Value> = reported
        .into_iter()
        .map(|(key, (label, points))| {
            let trend = match (points.first().and_then(|p| p["value"].as_f64()), points.last().and_then(|p| p["value"].as_f64())) {
                (Some(a), Some(b)) if points.len() > 1 && (b - a).abs() > 1e-9 => if b > a { "up" } else { "down" },
                _ => "flat",
            };
            json!({ "key": key, "label": label, "points": points, "trend": trend })
        })
        .collect();
    for r in &reports {
        let key = r["key"].as_str().unwrap_or("");
        let pts = r["points"].as_array().map(|a| a.len()).unwrap_or(0);
        if pts > 1 && (key.contains("vacan") || key.contains("inadimpl")) && r["trend"] == "up" {
            let first = &r["points"][0]["shown"];
            let last = &r["points"][pts - 1]["shown"];
            signals.push(signal("bad", format!("{} subiu nos relatórios: de {} para {}.", r["label"].as_str().unwrap_or(""), first.as_str().unwrap_or(""), last.as_str().unwrap_or(""))));
        }
    }
    // The bad first, then what to follow, then the good.
    signals.sort_by_key(|s| match s["tone"].as_str() { Some("bad") => 0, Some("watch") => 1, _ => 2 });

    json!({
        "ticker": ticker,
        "class": class.id(),
        "price": price,
        "quantity": quantity,
        "average": average,
        "indicators": indicators,
        "history": history,
        "dividends": dividends,
        "income": income,
        "ceilings": ceilings,
        "court": court,
        "reports": reports,
        "signals": signals,
    })
}

/// R$ with the places a dividend a share needs (R$ 0,083).
fn brl_fine(x: f64) -> String {
    format!("R$ {}", grouped(x, if x < 1.0 { 3 } else { 2 }))
}

// ── The month's contribution ──────────────────────────────────────────────────────────────────────────────────────

/// Where `amount` goes, share by share: the targets (the user's per asset; else their class's, shared among its
/// assets; else equal among the assets held) decide what is most below its share once the money is in; a fund at a
/// discount (P/VP under 1) or an asset under its ceiling comes a little first; the court's "avoid" or "reduce" and a
/// fund in poor health leave an asset out. Nothing is bought: a suggestion with its reasons.
pub fn aporte(store: &Store, market: &Market, amount: f64, today: Date) -> Value {
    let v = crate::portfolio::value(&store.portfolio, market);
    let class_targets = crate::portfolio::targets(&store.portfolio);
    let quoted: Vec<&crate::portfolio::Holding> = v.holdings.iter().filter(|h| h.class.quoted() && h.price.is_some_and(|p| p > 0.0) && h.quantity > 0.0).collect();
    let mut left_out: Vec<Value> = Vec::new();
    struct Pick {
        ticker: String,
        price: f64,
        value: f64,
        share: f64,
        bonus: f64,
        reasons: Vec<String>,
        bought: f64,
        dividend: Option<f64>,
    }
    let mut picks: Vec<Pick> = Vec::new();
    let count_in = |c: AssetClass| quoted.iter().filter(|h| h.class == c).count().max(1) as f64;
    for h in &quoted {
        let price = h.price.unwrap_or(0.0);
        // Left out: what the court told to avoid, a fund in poor health.
        if let Some(verdict) = store.verdicts.iter().rev().find(|x| x.ticker == h.ticker && x.date.is_some_and(|d| d.until(today) <= 120)) {
            let view = verdict.portfolio_fit.view.as_str();
            if !verdict.vetoed_by().is_empty() || view == "evitar_novo_aporte" || view == "reduzir" {
                left_out.push(json!({ "ticker": h.ticker, "reason": format!("o tribunal de {} pediu para evitar novo aporte", verdict.date.map(|d| d.br()).unwrap_or_default()) }));
                continue;
            }
        }
        let mut reasons = Vec::new();
        let mut bonus = 1.0;
        let mut dividend = None;
        if let Some(fund) = market.funds.get(&h.ticker) {
            let r = crate::fii::report(fund, Some(price), &market.economy, today);
            if r.health == crate::fii::Health::Risk {
                left_out.push(json!({ "ticker": h.ticker, "reason": "a saúde do provento pede atenção (veja a ficha)" }));
                continue;
            }
            dividend = r.last_dividend;
            if let Some(p) = r.p_vp.filter(|p| *p < 1.0) {
                bonus += (1.0 - p).min(0.3);
                reasons.push(format!("P/VP {}", grouped(p, 2)));
            }
        }
        let a = crate::ceiling::assess(store, market, &h.ticker, &crate::ceiling::Given::default(), today);
        if let Some(best) = a.ceilings.iter().filter(|c| a.watched.contains(&c.method)).map(|c| c.value).reduce(f64::max) {
            if price <= best {
                bonus += 0.15;
                reasons.push(format!("abaixo do teto {}", brl(best)));
            }
        }
        if price < h.average_price * 0.97 {
            reasons.push("abaixo do seu preço médio".into());
        }
        let share = match store.portfolio.asset_targets.get(&h.ticker) {
            Some(t) => *t / 100.0,
            None => match class_targets.get(&h.class) {
                Some(c) => c / count_in(h.class),
                None => 1.0 / quoted.len().max(1) as f64,
            },
        };
        picks.push(Pick { ticker: h.ticker.clone(), price, value: h.value, share, bonus, reasons, bought: 0.0, dividend });
    }
    // Share by share: the one furthest below its share of the portfolio once the money is in, weighted by its bonus.
    let total_after = v.value + amount;
    let mut cash = amount;
    let mut rounds = 0;
    loop {
        rounds += 1;
        if rounds > 100_000 {
            break;
        }
        let best = picks
            .iter()
            .enumerate()
            .filter(|(_, p)| p.price <= cash + 1e-9)
            .map(|(i, p)| (i, (p.share * total_after - p.value) / (p.share * total_after).max(1.0) * p.bonus))
            .filter(|(_, gap)| *gap > -0.25)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let Some((i, _)) = best else { break };
        let p = &mut picks[i];
        p.bought += 1.0;
        p.value += p.price;
        cash -= p.price;
    }
    let mut list: Vec<Value> = picks
        .iter()
        .filter(|p| p.bought > 0.0)
        .map(|p| {
            let mut why = p.reasons.clone();
            let weight_after = p.value / total_after * 100.0;
            why.insert(0, format!("fica com {} da carteira (alvo {})", pct(weight_after, 1), pct(p.share * 100.0, 1)));
            json!({
                "ticker": p.ticker,
                "shares": p.bought,
                "price": p.price,
                "total": p.bought * p.price,
                "income": p.dividend.map(|d| d * p.bought),
                "why": why,
            })
        })
        .collect();
    list.sort_by(|a, b| b["total"].as_f64().unwrap_or(0.0).total_cmp(&a["total"].as_f64().unwrap_or(0.0)));
    let added_income: f64 = list.iter().filter_map(|x| x["income"].as_f64()).sum();
    json!({
        "amount": amount,
        "used": amount - cash,
        "left": cash,
        "buys": list,
        "leftOut": left_out,
        "income": added_income,
        "basis": if !store.portfolio.asset_targets.is_empty() { "suas metas por ativo" } else if !class_targets.is_empty() { "suas metas por classe, divididas entre os ativos de cada uma" } else { "partes iguais entre os ativos" },
    })
}

// ── The year's income tax ─────────────────────────────────────────────────────────────────────────────────────────

fn lancamentos(store: &Store, ticker: &str) -> Vec<Trade> {
    let mut list: Vec<Trade> = store.trades.iter().filter(|t| t.ticker.eq_ignore_ascii_case(ticker)).cloned().collect();
    list.sort_by(|a, b| a.date.cmp(&b.date).then(a.seq.cmp(&b.seq)));
    list
}

/// The income tax of `year`: Bens e Direitos at 31/12 (and the year before), the funds' exempt income, the dividends,
/// the JCP, and the sales month by month with the tax each month owes as AXYON reckons it. To check against the broker's
/// and B3's own reports before declaring.
pub fn irpf(store: &Store, market: &Market, year: i32, today: Date) -> Value {
    let end = Date::new(year, 12, 31).unwrap_or(today);
    let before = Date::new(year - 1, 12, 31).unwrap_or(today);
    let mut tickers: Vec<String> = store.trades.iter().map(|t| t.ticker.to_uppercase()).collect();
    for p in &store.portfolio.positions {
        if p.class.quoted() && !tickers.contains(&p.ticker) {
            tickers.push(p.ticker.clone());
        }
    }
    tickers.sort();
    tickers.dedup();
    let cnpj_of = |t: &str| -> (String, String) {
        if let Some(f) = market.funds.get(t) {
            let name = f.months.last().map(|m| m.name.clone()).unwrap_or_default();
            return (f.cnpj.clone(), name);
        }
        if let Some(c) = market.companies.get(t) {
            return (c.cnpj.clone(), c.name.clone());
        }
        market.company_cnpjs.get(&t.chars().take(4).collect::<String>()).cloned().unwrap_or_default()
    };
    let mut goods = Vec::new();
    for t in &tickers {
        let trades = lancamentos(store, t);
        let class = trades.iter().find_map(|x| x.class).or_else(|| store.portfolio.position(t).map(|p| p.class)).unwrap_or_else(|| crate::trades::class_of(t, market).0);
        let at = |day: Date| -> (f64, f64, bool) {
            if trades.is_empty() {
                // From the sheet, with no lançamento: held now, at its average price; the year is not known.
                return store.portfolio.position(t).map(|p| (p.quantity, p.quantity * p.average_price, false)).unwrap_or((0.0, 0.0, false));
            }
            let upto: Vec<Trade> = trades.iter().filter(|x| x.date <= day).cloned().collect();
            let book = crate::wallet::replay(&upto).0;
            (book.quantity, book.cost, true)
        };
        let (q, cost, known) = at(end);
        let (q0, cost0, _) = at(before);
        if q <= 1e-9 && q0 <= 1e-9 {
            continue;
        }
        let (cnpj, name) = cnpj_of(t);
        let (group, code, kind) = match class {
            AssetClass::Fii => ("07", "03", "cotas do fundo imobiliário"),
            AssetClass::Acao => ("03", "01", "ações"),
            _ => ("", "", "cotas"),
        };
        let unit = if q > 0.0 { cost / q } else { 0.0 };
        let mut text = format!("{} {} {} ({})", grouped(q, 0), kind, t, if name.is_empty() { "nome conforme a corretora".to_string() } else { name.clone() });
        if !cnpj.is_empty() {
            text.push_str(&format!(", CNPJ {}", cnpj_fmt(&cnpj)));
        }
        text.push_str(&format!(", custo médio {} por cota/ação, custódia na B3.", brl(unit)));
        goods.push(json!({
            "ticker": t,
            "class": class.id(),
            "group": group,
            "code": code,
            "cnpj": cnpj_fmt(&cnpj),
            "name": name,
            "quantity": q,
            "before": cost0,
            "after": cost,
            "text": text,
            "fromLancamentos": known,
        }));
    }
    // Income: the proventos received in the year, by asset, from the Carteira's own count.
    let screen = crate::wallet::view(store, market, today);
    let mut exempt: BTreeMap<String, f64> = BTreeMap::new();
    let mut dividends: BTreeMap<String, f64> = BTreeMap::new();
    let mut jcp: BTreeMap<String, f64> = BTreeMap::new();
    for p in screen["income"]["received"].as_array().into_iter().flatten() {
        let Some(day) = p["payment"].as_str().and_then(Date::parse) else { continue };
        if day.y != year {
            continue;
        }
        let t = p["ticker"].as_str().unwrap_or("").to_string();
        let net = p["net"].as_f64().unwrap_or(0.0);
        let kind = p["kind"].as_str().unwrap_or("");
        if kind.contains("Juros") {
            *jcp.entry(t).or_default() += net;
        } else if market.funds.contains_key(&t) || kind == "Rendimento" {
            *exempt.entry(t).or_default() += net;
        } else if kind != "Amortização" {
            *dividends.entry(t).or_default() += net;
        }
    }
    let lines = |m: &BTreeMap<String, f64>| -> Vec<Value> {
        m.iter().map(|(t, v)| { let (c, n) = cnpj_of(t); json!({ "ticker": t, "cnpj": cnpj_fmt(&c), "name": n, "value": (v * 100.0).round() / 100.0 }) }).collect()
    };
    // Sales, month by month: shares (exempt up to R$ 20.000 sold in the month) and funds (20% on the gain, losses
    // carried forward within the class).
    let mut months: BTreeMap<String, (f64, f64, f64)> = BTreeMap::new(); // share sales, share result, fund result
    for t in &tickers {
        let trades = lancamentos(store, t);
        let (_, results, _) = crate::wallet::replay(&trades);
        let class = trades.iter().find_map(|x| x.class).unwrap_or_else(|| crate::trades::class_of(t, market).0);
        for x in trades.iter().filter(|x| x.kind == TradeKind::Sell && x.date.y == year) {
            let e = months.entry(x.date.month_key()).or_default();
            let result = results.get(&x.seq).copied().unwrap_or(0.0);
            if class == AssetClass::Fii {
                e.2 += result;
            } else {
                e.0 += x.quantity * x.price;
                e.1 += result;
            }
        }
    }
    let mut fund_loss = 0.0;
    let mut share_loss = 0.0;
    let sales: Vec<Value> = months
        .iter()
        .map(|(m, (sold, shares, funds))| {
            let fund_base = funds + fund_loss;
            let fund_tax = if fund_base > 0.0 { fund_base * 0.20 } else { 0.0 };
            fund_loss = fund_base.min(0.0);
            let share_tax = if *sold > 20_000.0 {
                let base = shares + share_loss;
                share_loss = base.min(0.0);
                if base > 0.0 { base * 0.15 } else { 0.0 }
            } else {
                if *shares < 0.0 { share_loss += shares; }
                0.0
            };
            json!({ "month": m, "sharesSold": sold, "sharesResult": shares, "fundsResult": funds, "darf": ((fund_tax + share_tax) * 100.0).round() / 100.0 })
        })
        .collect();
    json!({
        "year": year,
        "goods": goods,
        "exempt": lines(&exempt),
        "dividends": lines(&dividends),
        "jcp": lines(&jcp),
        "sales": sales,
        "notes": [
            "Bens e Direitos: o custo de compra (com as taxas), não o valor de mercado.",
            "Rendimentos de FII e dividendos de ações são isentos; o JCP é tributado na fonte (exclusiva).",
            "Confira com os informes da corretora e da B3 antes de declarar.",
        ],
    })
}

/// "12.345.678/0001-90" from its digits.
fn cnpj_fmt(c: &str) -> String {
    let d: String = c.chars().filter(|x| x.is_ascii_digit()).collect();
    if d.len() != 14 {
        return c.to_string();
    }
    format!("{}.{}.{}/{}-{}", &d[0..2], &d[2..5], &d[5..8], &d[8..12], &d[12..14])
}

// ── The portfolio's own return ────────────────────────────────────────────────────────────────────────────────────

/// The yearly rate (percent) that makes the flows' present value zero; None when there is no answer.
pub fn xirr(flows: &[(Date, f64)]) -> Option<f64> {
    if flows.len() < 2 || !flows.iter().any(|f| f.1 > 0.0) || !flows.iter().any(|f| f.1 < 0.0) {
        return None;
    }
    let first = flows.iter().map(|f| f.0).min()?;
    let npv = |rate: f64| -> f64 { flows.iter().map(|(d, v)| v / (1.0 + rate).powf(first.until(*d) as f64 / 365.0)).sum() };
    let (mut lo, mut hi) = (-0.99, 10.0);
    if npv(lo).signum() == npv(hi).signum() {
        return None;
    }
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        if npv(mid).signum() == npv(lo).signum() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some((lo + hi) / 2.0 * 100.0)
}

/// What the money in the assets with lançamentos made a year, against the CDI (the Carteira's own figure).
pub fn returns(store: &Store, market: &Market, today: Date) -> Option<Value> {
    let screen = crate::wallet::view(store, market, today);
    (!screen["returns"].is_null()).then(|| screen["returns"].clone())
}

// ── For the brain ─────────────────────────────────────────────────────────────────────────────────────────────────

pub fn ficha_for_model(f: &Value) -> String {
    let mut out = format!("{} ({}):", f["ticker"].as_str().unwrap_or(""), f["class"].as_str().unwrap_or(""));
    for i in f["indicators"].as_array().into_iter().flatten() {
        out.push_str(&format!(" {} {}{};", i["label"].as_str().unwrap_or(""), i["value"].as_str().unwrap_or(""), i["note"].as_str().filter(|n| !n.is_empty()).map(|n| format!(" ({n})")).unwrap_or_default()));
    }
    if let Some(m) = f["income"]["monthly"].as_f64() {
        out.push_str(&format!(" pays the user about {} a month;", brl(m)));
    }
    let signals: Vec<String> = f["signals"].as_array().into_iter().flatten().map(|s| format!("[{}] {}", s["tone"].as_str().unwrap_or(""), s["text"].as_str().unwrap_or(""))).collect();
    if !signals.is_empty() {
        out.push_str(&format!("\nSignals (in Portuguese, for the user):\n{}", signals.join("\n")));
    }
    for r in f["reports"].as_array().into_iter().flatten() {
        let pts: Vec<String> = r["points"].as_array().into_iter().flatten().map(|p| p["shown"].as_str().unwrap_or("").to_string()).collect();
        out.push_str(&format!("\nIn its reports, {}: {}", r["label"].as_str().unwrap_or(""), pts.join(" → ")));
    }
    out
}

pub fn aporte_for_model(a: &Value) -> String {
    let mut out = format!("For {} ({}): ", brl(a["amount"].as_f64().unwrap_or(0.0)), a["basis"].as_str().unwrap_or(""));
    let buys: Vec<String> = a["buys"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|b| format!("{} {} at {} = {} ({})", grouped(b["shares"].as_f64().unwrap_or(0.0), 0), b["ticker"].as_str().unwrap_or(""), brl(b["price"].as_f64().unwrap_or(0.0)), brl(b["total"].as_f64().unwrap_or(0.0)), b["why"].as_array().map(|w| w.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("; ")).unwrap_or_default()))
        .collect();
    out.push_str(&if buys.is_empty() { "nothing fits the amount.".to_string() } else { buys.join("; ") });
    out.push_str(&format!(". Left over: {}. About {} more a month in income.", brl(a["left"].as_f64().unwrap_or(0.0)), brl(a["income"].as_f64().unwrap_or(0.0))));
    for l in a["leftOut"].as_array().into_iter().flatten() {
        out.push_str(&format!(" Left out: {} ({}).", l["ticker"].as_str().unwrap_or(""), l["reason"].as_str().unwrap_or("")));
    }
    out.push_str(" A suggestion, not an order: nothing is bought here.");
    out
}

pub fn irpf_for_model(r: &Value) -> String {
    let mut out = format!("IRPF {} — Bens e Direitos at 31/12:\n", r["year"]);
    for g in r["goods"].as_array().into_iter().flatten() {
        out.push_str(&format!("- [{}/{}] {} — 31/12/{}: {} ; 31/12/{}: {}\n", g["group"].as_str().unwrap_or(""), g["code"].as_str().unwrap_or(""), g["text"].as_str().unwrap_or(""), r["year"].as_i64().unwrap_or(0) - 1, brl(g["before"].as_f64().unwrap_or(0.0)), r["year"], brl(g["after"].as_f64().unwrap_or(0.0))));
    }
    for (title, key) in [("Exempt (FII income)", "exempt"), ("Exempt (dividends)", "dividends"), ("Exclusive taxation (JCP)", "jcp")] {
        let l: Vec<String> = r[key].as_array().into_iter().flatten().map(|x| format!("{} CNPJ {} {}", x["ticker"].as_str().unwrap_or(""), x["cnpj"].as_str().unwrap_or(""), brl(x["value"].as_f64().unwrap_or(0.0)))).collect();
        if !l.is_empty() {
            out.push_str(&format!("{title}: {}\n", l.join("; ")));
        }
    }
    for s in r["sales"].as_array().into_iter().flatten() {
        out.push_str(&format!("Sales {}: shares sold {}, shares result {}, funds result {}, DARF {}\n", s["month"].as_str().unwrap_or(""), brl(s["sharesSold"].as_f64().unwrap_or(0.0)), brl(s["sharesResult"].as_f64().unwrap_or(0.0)), brl(s["fundsResult"].as_f64().unwrap_or(0.0)), brl(s["darf"].as_f64().unwrap_or(0.0))));
    }
    out.push_str("To be checked against the broker's and B3's reports before declaring.");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::fixtures;
    use crate::wallet::{add, entry};

    fn day(y: i32, m: u32, d: u32) -> Date {
        Date::new(y, m, d).unwrap()
    }

    #[test]
    fn xirr_finds_the_rate() {
        // R$ 1.000 that became R$ 1.100 in a year: 10% a year.
        let r = xirr(&[(day(2025, 1, 1), -1000.0), (day(2026, 1, 1), 1100.0)]).unwrap();
        assert!((r - 10.0).abs() < 0.05, "{r}");
        assert!(xirr(&[(day(2025, 1, 1), -1000.0)]).is_none());
    }

    #[test]
    fn the_months_contribution_goes_where_the_portfolio_lacks_and_leaves_out_what_the_court_avoided() {
        let today = day(2026, 11, 5);
        let market = fixtures::market();
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let a = aporte(&store, &market, 500.0, today);
        let used = a["used"].as_f64().unwrap();
        assert!(used > 400.0 && used <= 500.0, "{a}");
        assert!((used + a["left"].as_f64().unwrap() - 500.0).abs() < 1e-6);
        let first = a["buys"][0]["ticker"].as_str().unwrap().to_string();
        // The court told to avoid it: it is left out, the money goes elsewhere.
        store.verdicts.push(serde_json::from_value(json!({"ticker": first, "date": "2026-11-01", "portfolioFit": {"view": "evitar_novo_aporte"}})).unwrap());
        let b = aporte(&store, &market, 500.0, today);
        assert!(!b["buys"].as_array().unwrap().iter().any(|x| x["ticker"] == first.as_str()), "{b}");
        assert!(b["leftOut"].as_array().unwrap().iter().any(|x| x["ticker"] == first.as_str()));
        assert!(aporte_for_model(&b).contains("nothing is bought"));
    }

    #[test]
    fn the_year_s_income_tax_from_the_lancamentos_and_proventos() {
        let today = day(2026, 11, 5);
        let market = fixtures::market();
        let mut store = Store::default();
        add(&mut store, &market, entry(&json!({"ticker": "GARE11", "kind": "buy", "date": "2025-06-01", "quantity": 100, "price": 9, "fees": 1}), today).unwrap(), today).unwrap();
        add(&mut store, &market, entry(&json!({"ticker": "GARE11", "kind": "buy", "date": "2026-03-01", "quantity": 100, "price": 8}), today).unwrap(), today).unwrap();
        add(&mut store, &market, entry(&json!({"ticker": "GARE11", "kind": "sell", "date": "2026-09-01", "quantity": 50, "price": 9.5}), today).unwrap(), today).unwrap();
        let r = irpf(&store, &market, 2026, today);
        let g = &r["goods"][0];
        assert_eq!((g["ticker"].as_str(), g["group"].as_str(), g["code"].as_str()), (Some("GARE11"), Some("07"), Some("03")));
        assert!((g["before"].as_f64().unwrap() - 901.0).abs() < 1e-6, "31/12/2025: 100 at R$ 9 plus R$ 1: {g}");
        assert_eq!(g["quantity"], 150.0);
        let sale = &r["sales"][0];
        assert_eq!(sale["month"], "2026-09");
        assert!(sale["fundsResult"].as_f64().unwrap() > 0.0 && sale["darf"].as_f64().unwrap() > 0.0, "a fund's gain owes 20%: {sale}");
        assert!(irpf_for_model(&r).contains("Bens e Direitos"));
        assert!(returns(&store, &market, today).is_some());
    }

    #[test]
    fn an_asset_s_sheet_tells_its_figures_and_signals() {
        let today = day(2026, 11, 5);
        let market = fixtures::market();
        let store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let t = store.portfolio.positions.iter().find(|p| market.funds.contains_key(&p.ticker)).map(|p| p.ticker.clone()).unwrap();
        let f = ficha(&store, &market, None, &t, today);
        assert_eq!(f["ticker"], t.as_str());
        assert!(f["indicators"].as_array().is_some_and(|a| !a.is_empty()), "{f}");
        assert!(ficha_for_model(&f).starts_with(&t));
    }
}
