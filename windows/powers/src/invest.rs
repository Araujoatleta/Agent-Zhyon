//! What the app's investing screen shows, in one piece of JSON: the portfolio by class and by asset (the ring), the
//! dividends month by month and asset by asset (the ring of a month), what the reader read with what changed, the
//! theses, the committee's verdicts and the meetings it is due, the learning screen's numbers, and the week's summary.
//! Plain data: the app draws it.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::committee;
use crate::date::Date;
use crate::library::Library;
use crate::model::{AssetClass, Market, Store, TradeKind, Verdict};
use crate::{learn, portfolio, srs, thesis, weekly};

/// The quantity of `ticker` held at the end of `day`: from the trades when the user's history has them, else what is
/// held now.
fn held_on(store: &Store, ticker: &str, day: Date) -> f64 {
    let trades: Vec<_> = store.trades.iter().filter(|t| t.ticker == ticker).collect();
    if trades.is_empty() {
        return store.portfolio.position(ticker).map(|p| p.quantity).unwrap_or(0.0);
    }
    trades.iter().filter(|t| t.date <= day).map(|t| if t.kind == TradeKind::Sell { -t.quantity } else { t.quantity }).sum::<f64>().max(0.0)
}

/// The day the user's records show `ticker` first held: its first purchase, or the first snapshot to list it once the
/// snapshots list prices (a fund bought after AXYON began keeping them). None: unknown, every payout counts.
pub fn held_since(store: &Store, ticker: &str) -> Option<Date> {
    let bought = store.trades.iter().filter(|t| t.ticker == ticker && t.kind != TradeKind::Sell).map(|t| t.date).min();
    let mut priced: Vec<&crate::model::Snapshot> = store.snapshots.iter().filter(|s| !s.prices.is_empty()).collect();
    priced.sort_by_key(|s| s.date);
    let seen = match priced.first() {
        Some(first) if !first.prices.contains_key(ticker) => priced.iter().find(|s| s.prices.contains_key(ticker)).map(|s| s.date),
        _ => None,
    };
    match (bought, seen) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// The dividends (not the amortizations) the user's funds announced, by the month they were paid, asset by asset:
/// each payout times what was held on its record date, not before the user held the fund.
fn announced(store: &Store, market: &Market, first: Date, last: Date) -> BTreeMap<String, BTreeMap<String, f64>> {
    let mut by_month: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    let mut tickers: Vec<String> = store.portfolio.tickers(Some(AssetClass::Fii));
    for t in store.trades.iter().map(|t| &t.ticker) {
        if market.funds.contains_key(t) && !tickers.contains(t) {
            tickers.push(t.clone());
        }
    }
    for t in tickers {
        let Some(fund) = market.funds.get(&t) else { continue };
        let since = held_since(store, &t);
        for p in fund.payouts.iter().filter(|p| !p.amortization) {
            let paid = p.payment.unwrap_or(p.record);
            if paid < first || paid > last || since.is_some_and(|s| p.record < s) {
                continue;
            }
            let amount = p.value * held_on(store, &t, p.record);
            if amount > 0.0 {
                *by_month.entry(paid.month_key()).or_default().entry(t.clone()).or_insert(0.0) += amount;
            }
        }
    }
    by_month
}

/// The dividends month by month, the last `months` months up to this one. The user's sheet is the record when it has
/// one ("Proventos pagos"): its months' totals (R$ 0 for a month it has no amount for), its assets when it lists them;
/// the funds' announcements give the assets otherwise, and their sum is kept beside the record to show a divergence.
/// Without a sheet, the announcements.
pub fn dividends(store: &Store, market: &Market, today: Date, months: i32) -> Vec<Value> {
    let first = today.first_of_month().add_months(-(months - 1));
    let mut by_month = announced(store, market, first, today.last_of_month());
    let record = &store.recorded_dividends;
    let has_record = !record.months.is_empty();
    (0..months)
        .map(|i| {
            let key = first.add_months(i).month_key();
            let mut list: Vec<(String, f64)> = by_month.remove(&key).unwrap_or_default().into_iter().collect();
            list.sort_by(|a, b| b.1.total_cmp(&a.1));
            let announced_total: f64 = list.iter().map(|x| x.1).sum();
            let recorded_assets = record.by_asset.get(&key).filter(|x| !x.is_empty());
            let items: Vec<Value> = match recorded_assets {
                Some(assets) => {
                    let mut own: Vec<(&String, &f64)> = assets.iter().collect();
                    own.sort_by(|a, b| b.1.total_cmp(a.1));
                    own.iter().map(|(t, v)| json!({ "key": t, "label": t, "value": v })).collect()
                }
                None => list.iter().map(|(t, v)| json!({ "key": t, "label": t, "value": v })).collect(),
            };
            let total = if has_record { record.months.get(&key).copied().unwrap_or(0.0) } else { announced_total };
            json!({
                "month": key,
                "total": total,
                "items": items,
                "source": if has_record { "planilha" } else { "anuncios" },
                "covered": !has_record || record.months.contains_key(&key),
                "announced": announced_total,
                "itemsFrom": if recorded_assets.is_some() { "planilha" } else { "anuncios" },
            })
        })
        .collect()
}

fn verdict_json(v: &Verdict) -> Value {
    let (f, a, n) = v.score();
    json!({
        "id": v.id,
        "ticker": v.ticker,
        "proposal": v.proposal,
        "date": v.date,
        "version": v.version,
        "score": { "for": f, "against": a, "neutral": n },
        "vetoedBy": v.vetoed_by(),
        "minds": v.minds.iter().map(|m| json!({
            "id": m.id,
            "name": committee::mind(&m.id).map(|x| x.name_pt).unwrap_or(""),
            "vote": m.vote,
            "was": m.was,
            "confidence": m.confidence,
            "why": m.why,
            "veto": m.veto,
            "vetoReason": m.veto_reason,
            "revised": m.revised,
            "missing": m.missing,
            "points": m.points,
        })).collect::<Vec<_>>(),
        "asset": v.asset,
        "portfolio": v.portfolio_fit,
        "audit": v.audit,
        "toVerify": v.to_verify,
        "facts": v.facts,
        "documents": v.documents,
        "question": v.question,
        "dissent": v.dissent,
        "wouldChange": v.would_change,
        "fit": v.fit,
        "reason": v.reason,
        "document": v.document,
    })
}

pub fn verdict(v: &Verdict) -> Value {
    verdict_json(v)
}

/// A document of the library for the app: what it is, its digest, what changed, where to open it.
pub fn document(lib: &Library, key: &str) -> Value {
    let Some(d) = lib.get(key) else { return Value::Null };
    json!({
        "key": d.key,
        "ticker": d.ticker,
        "kind": d.kind.id(),
        "title": d.title,
        "headline": d.headline(),
        "delivered": d.delivered,
        "read": d.read,
        "url": d.url,
        "pages": d.pages.len(),
        "figures": d.figures.iter().map(|f| json!({ "label": f.label, "shown": f.shown(), "page": f.page })).collect::<Vec<_>>(),
        "changes": lib.changes(key).iter().map(|c| c.shown()).collect::<Vec<_>>(),
        "digest": d.digest,
        "problem": d.problem,
    })
}

/// wealth_documents, for the brain: `do` = feed (what the reader read, with what changed; `ticker` for one asset),
/// read (a document's text by `key`, from `offset`), theses (the user's theses checked now), thesis (write the user's
/// thesis on `ticker` from their `text`).
pub fn documents_tool(store: &mut Store, market: &Market, lib: Option<&Library>, args: &Value, today: Date) -> Result<String, String> {
    let s = |k: &str| args[k].as_str().map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    let what = s("do").map(|x| crate::text::plain(&x)).unwrap_or_else(|| "feed".into());
    let empty = Library::default();
    let lib = lib.unwrap_or(&empty);
    match what.as_str() {
        "read" => {
            let key = s("key").ok_or("read needs the document's key (as feed gives it: \"fnet:904\")")?;
            let offset = args["offset"].as_u64().unwrap_or(0) as usize;
            lib.read(&key, offset, 12_000).ok_or_else(|| format!("No document {key} was read by AXYON."))
        }
        "theses" => Ok(thesis::for_model(&thesis::states(store, market, Some(lib), today))),
        "thesis" => {
            let ticker = s("ticker").and_then(|t| crate::text::ticker(&t)).ok_or("thesis needs the ticker")?;
            let text = s("text").or_else(|| s("thesis")).ok_or("thesis needs the user's words")?;
            let th = thesis::write(store, market, &ticker, &text, today);
            let states = thesis::states(store, market, Some(lib), today);
            let state = states.iter().find(|x| x.ticker == ticker).cloned();
            Ok(format!(
                "Thesis on {ticker} saved ({} measurable condition(s); the rest kept as the user's words).\n{}",
                th.conditions.len(),
                state.map(|x| thesis::for_model(&[x])).unwrap_or_default()
            ))
        }
        _ => {
            let only = s("ticker").and_then(|t| crate::text::ticker(&t));
            let docs: Vec<_> = lib.feed(40).into_iter().filter(|d| only.as_ref().is_none_or(|t| &d.ticker == t)).take(10).collect();
            let pending = lib.pending(store, market, today, 50).len();
            if docs.is_empty() {
                return Ok(format!(
                    "AXYON's reader has read no document{} yet.{}",
                    only.map(|t| format!(" of {t}")).unwrap_or_default(),
                    if pending > 0 { format!(" {pending} wait to be read: the iPhone app reads them (Investimentos) when it is open.") } else { String::new() }
                ));
            }
            let mut out = String::from("What AXYON's reader read, newest first (cite the pages):");
            for d in docs {
                out.push_str(&format!("\n\n[{}] {} — {} ({}{})", d.key, d.ticker, d.title, d.kind.label(), d.delivered.map(|x| format!(", {}", x.br())).unwrap_or_default()));
                match &d.digest {
                    Some(g) => {
                        out.push_str(&format!("\n{}", g.headline));
                        for pt in &g.points {
                            out.push_str(&format!("\n- {}{}", pt.text, pt.page.map(|n| format!(" (p. {n})")).unwrap_or_default()));
                        }
                    }
                    None => out.push_str("\nNot summarized yet: read it (do read, key) and tell the user what matters."),
                }
                for c in lib.changes(&d.key) {
                    out.push_str(&format!("\n- {}", c.shown()));
                }
            }
            if pending > 0 {
                out.push_str(&format!("\n\n{pending} more wait to be read."));
            }
            Ok(out)
        }
    }
}

/// The JSON object in a brain's answer: alone, in a ```json fence, or with words around it (braces among them too),
/// a trailing comma forgiven.
pub fn json_in(text: &str) -> Option<Value> {
    let end = text.rfind('}')?;
    for (start, _) in text.match_indices('{').take(20) {
        if start >= end {
            break;
        }
        let slice = &text[start..=end];
        for candidate in [slice.to_string(), without_trailing_commas(slice)] {
            if let Ok(v) = serde_json::from_str::<Value>(&candidate) {
                if v.is_object() {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// A comma just before a closing brace or bracket taken out (outside strings), as models sometimes leave one.
fn without_trailing_commas(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let (mut in_string, mut escaped) = (false, false);
    for (i, &c) in chars.iter().enumerate() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
        } else if c == ',' && matches!(chars[i + 1..].iter().find(|x| !x.is_whitespace()), Some('}') | Some(']')) {
            continue;
        }
        out.push(c);
    }
    out
}

/// A brain's answer given as its text or as JSON already.
fn answer_in(v: &Value) -> Option<Value> {
    match v {
        Value::String(t) => json_in(t),
        x if x.is_object() => Some(x.clone()),
        _ => None,
    }
}

/// The facts and documents a meeting's brief gave, as the host hands them back.
fn meeting_of(input: &Value) -> (Vec<crate::evidence::Fact>, Vec<crate::model::DocRef>) {
    (serde_json::from_value(input["facts"].clone()).unwrap_or_default(), serde_json::from_value(input["docs"].clone()).unwrap_or_default())
}

/// What an app call changed, for the caller to save.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Saved {
    pub store: bool,
    pub library: bool,
}

const STORE: Saved = Saved { store: true, library: false };
const LIBRARY: Saved = Saved { store: false, library: true };

fn theses_json(store: &Store, market: &Market, lib: Option<&Library>, today: Date) -> Vec<Value> {
    thesis::states(store, market, lib, today)
        .into_iter()
        .map(|s| {
            json!({
                "ticker": s.ticker,
                "text": s.text,
                "written": s.written,
                "ok": s.ok(),
                "broken": s.broken,
                "checks": s.checks,
                "risks": s.risks.iter().map(|r| r.1.clone()).collect::<Vec<_>>(),
            })
        })
        .collect()
}

/// The iPhone app's own calls, never the brain's tools (the brain's answers come through them, from the app's host):
/// `invest_view`; `invest_docs` (do: due, ingest, failed, brief, digest, undigested, document, page); `invest_committee`
/// (do: brief, audit, verdict, meetings); `invest_thesis` (do: write, remove); `invest_target` (ticker, target);
/// `invest_learn` (do: view, apprentice, cards, grade, paper, reset); `invest_weekly` (do: script, told). The answer,
/// and what it changed.
pub fn app(store: &mut Store, market: &Market, lib: &mut Library, tool: &str, input: &Value, today: Date) -> Result<(Value, Saved), String> {
    let s = |k: &str| input[k].as_str().map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    let what = s("do").map(|x| crate::text::plain(&x)).unwrap_or_default();
    let language = s("language").unwrap_or_else(|| "Portuguese".into());
    let most = input["most"].as_u64().map(|n| n.clamp(1, 50) as usize);
    let pending = || serde_json::from_value::<crate::library::Pending>(input["pending"].clone()).map_err(|_| "the document to read (pending) is missing".to_string());
    let key = || s("key").ok_or_else(|| "the document's key is missing".to_string());
    match tool {
        "invest_view" => Ok((view(store, market, Some(lib), today), Saved::default())),
        "invest_docs" => match what.as_str() {
            "due" => Ok((json!(lib.pending(store, market, today, most.unwrap_or(6))), Saved::default())),
            "ingest" => {
                let p = pending()?;
                let pages: Vec<String> = input["pages"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                lib.ingest(&p, pages, today);
                Ok((document(lib, &p.key), LIBRARY))
            }
            "failed" => {
                let p = pending()?;
                lib.failed(&p, &s("problem").unwrap_or_else(|| "It could not be read.".into()), today);
                Ok((json!({ "ok": true }), LIBRARY))
            }
            "brief" => Ok((json!({ "prompt": lib.digest_brief(&key()?, store.learn.apprentice, &language)? }), Saved::default())),
            "digest" => {
                let k = key()?;
                let given = match &input["answer"] {
                    Value::String(t) => json_in(t).ok_or("The reader's answer has no JSON.")?,
                    v if v.is_object() => v.clone(),
                    _ => return Err("The reader's answer is missing.".into()),
                };
                lib.set_digest(&k, &given, today)?;
                Ok((document(lib, &k), LIBRARY))
            }
            "undigested" => Ok((
                json!(lib.undigested(most.unwrap_or(4)).iter().map(|d| json!({ "key": d.key, "ticker": d.ticker, "title": d.title })).collect::<Vec<_>>()),
                Saved::default(),
            )),
            "document" => Ok((document(lib, &key()?), Saved::default())),
            // A page the committee cited, as the reader read it: the excerpt behind a point.
            "page" => {
                let k = key()?;
                let d = lib.get(&k).ok_or("No such document.")?;
                let number = input["page"].as_u64().unwrap_or(1).max(1) as usize;
                let text = d.pages.get(number - 1).ok_or_else(|| format!("{k} has no page {number}."))?;
                Ok((json!({ "key": k, "page": number, "pages": d.pages.len(), "text": text, "url": d.url }), Saved::default()))
            }
            other => Err(format!("invest_docs has no \"{other}\".")),
        },
        "invest_committee" => match what.as_str() {
            "brief" => {
                let b = committee::brief_full(store, market, Some(lib), input, today, true)?;
                let keys: Vec<String> = b.documents.iter().map(|d| d.key.clone()).collect();
                Ok((json!({ "prompt": b.prompt, "ticker": b.ticker, "proposal": b.proposal, "documents": keys, "facts": b.facts, "docs": b.documents }), Saved::default()))
            }
            // The audit's brief: the minds' answer checked by this code, for the brain to audit.
            "audit" => {
                let minds = answer_in(&input["answer"]).ok_or("The committee's answer has no JSON.")?;
                let (facts, docs) = meeting_of(input);
                let ticker = s("ticker").and_then(|t| crate::text::ticker(&t)).ok_or("the audit needs the ticker")?;
                let findings = committee::precheck(&minds, &facts, &docs, Some(lib))?;
                let (pages, _) = lib.for_committee(&ticker, 9_000);
                let prompt = committee::audit_brief(&minds, &facts, &pages, &findings, &ticker, &s("proposal").unwrap_or_default(), store.learn.apprentice);
                Ok((json!({ "prompt": prompt, "findings": findings }), Saved::default()))
            }
            "verdict" => {
                let given = answer_in(&input["answer"]).ok_or("The committee's answer has no JSON.")?;
                let audit = answer_in(&input["audit"]);
                let (facts, docs) = meeting_of(input);
                let ticker = s("ticker").and_then(|t| crate::text::ticker(&t)).ok_or("the verdict needs the ticker")?;
                let mut verdict = committee::finalize(&given, audit.as_ref(), facts, docs, Some(lib), &ticker, &s("proposal").unwrap_or_default(), &s("reason").unwrap_or_else(|| "asked".into()), &s("document").unwrap_or_default(), today)?;
                // The host says why the auditor gave no answer: the verdict says it first.
                if let (None, Some(why)) = (&audit, s("auditFailed")) {
                    verdict.audit.insert(0, committee::unaudited(&why));
                }
                let out = verdict_json(&verdict);
                committee::keep(store, verdict);
                Ok((out, STORE))
            }
            "meetings" => Ok((json!(thesis::to_meet(store, market, lib, today, most.unwrap_or(2))), Saved::default())),
            other => Err(format!("invest_committee has no \"{other}\".")),
        },
        "invest_thesis" => {
            let ticker = s("ticker").and_then(|t| crate::text::ticker(&t)).ok_or("the thesis needs the ticker")?;
            match what.as_str() {
                "remove" => store.theses.retain(|t| t.ticker != ticker),
                _ => {
                    let text = s("text").ok_or("the thesis needs the user's words")?;
                    thesis::write(store, market, &ticker, &text, today);
                }
            }
            Ok((json!(theses_json(store, market, Some(lib), today)), STORE))
        }
        "invest_learn" => {
            let saved = match what.as_str() {
                "" | "view" => Saved::default(),
                "apprentice" => {
                    store.learn.apprentice = input["on"].as_bool().unwrap_or(!store.learn.apprentice);
                    STORE
                }
                "cards" => {
                    learn::make_cards(store, market, Some(lib), today);
                    STORE
                }
                "grade" => {
                    let id = s("id").ok_or("which card?")?;
                    let q = input["grade"].as_u64().ok_or("the grade (0–5) is missing")?.min(5) as u8;
                    let card = store.cards.iter_mut().find(|c| c.id == id).ok_or("No such card.")?;
                    srs::grade(card, q, today);
                    STORE
                }
                "paper" => {
                    let ticker = s("ticker").ok_or("which asset?")?;
                    let quantity = input["quantity"].as_f64().ok_or("how many?")?;
                    learn::paper_trade(store, market, &ticker, s("action").as_deref() == Some("sell"), quantity, today)?;
                    STORE
                }
                "reset" => {
                    learn::paper_reset(store, input["amount"].as_f64(), today);
                    STORE
                }
                other => return Err(format!("invest_learn has no \"{other}\".")),
            };
            Ok((learn::view(store, market, Some(lib), today), saved))
        }
        // The share of the portfolio the user wants in one asset, percent; none (or 0) takes it away.
        "invest_target" => {
            let ticker = s("ticker").and_then(|t| crate::text::ticker(&t)).ok_or("the target needs the ticker")?;
            match input["target"].as_f64().filter(|x| *x > 0.0) {
                Some(x) if x <= 100.0 => store.portfolio.asset_targets.insert(ticker, x),
                Some(_) => return Err("A target is a share of the portfolio, up to 100 %.".into()),
                None => store.portfolio.asset_targets.remove(&ticker),
            };
            Ok((view(store, market, Some(lib), today), STORE))
        }
        "invest_weekly" => match what.as_str() {
            "told" => {
                store.learn.weekly_told = Some(weekly::monday(today));
                Ok((json!({ "ok": true }), STORE))
            }
            _ => {
                // "pt-BR", "Portuguese", "Brazilian Portuguese": the script in Portuguese.
                let pt = { let l = crate::text::plain(&language); l.starts_with("pt") || l.contains("portug") };
                Ok((json!(weekly::script(store, market, Some(lib), today, pt)), Saved::default()))
            }
        },
        other => Err(format!("The app has no call named \"{other}\".")),
    }
}

/// The investing screen.
pub fn view(store: &Store, market: &Market, lib: Option<&Library>, today: Date) -> Value {
    let v = portfolio::value(&store.portfolio, market);
    let classes: Vec<Value> = v.classes.iter().filter(|c| c.value > 0.0).map(|c| json!({ "key": c.class.id(), "label": c.label, "value": c.value, "weight": c.weight, "target": c.target })).collect();
    let mut assets: Vec<Value> = v
        .holdings
        .iter()
        .filter(|h| h.value > 0.0)
        .map(|h| json!({ "key": h.ticker, "label": h.ticker, "value": h.value, "weight": h.weight, "class": h.class.id(), "price": h.price, "gainPct": h.gain_pct, "quantity": h.quantity, "target": store.portfolio.asset_targets.get(&h.ticker) }))
        .collect();
    assets.sort_by(|a, b| b["value"].as_f64().unwrap_or(0.0).total_cmp(&a["value"].as_f64().unwrap_or(0.0)));
    let feed: Vec<Value> = lib.map(|l| l.feed(12).into_iter().map(|d| document(l, &d.key)).collect()).unwrap_or_default();
    let theses = theses_json(store, market, lib, today);
    let meetings = lib.map(|l| thesis::to_meet(store, market, l, today, 2)).unwrap_or_default();
    let w = weekly::monday(today);
    json!({
        "total": v.value,
        "cost": v.cost,
        "gainPct": v.gain_pct,
        "pricedOn": v.priced_on,
        "byClass": classes,
        "byAsset": assets,
        "dividends": dividends(store, market, today, 12),
        // Where the dividends come from when the user's sheet records them, and when it was read.
        "dividendsRecord": if store.recorded_dividends.months.is_empty() { Value::Null } else { json!({ "source": store.recorded_dividends.source, "read": store.recorded_dividends.read }) },
        "targets": store.portfolio.asset_targets,
        "feed": feed,
        "pending": lib.map(|l| l.pending(store, market, today, 50).len()).unwrap_or(0),
        "undigested": lib.map(|l| l.undigested(50).len()).unwrap_or(0),
        "theses": theses,
        "verdicts": store.verdicts.iter().rev().take(10).map(verdict_json).collect::<Vec<_>>(),
        "meetings": meetings,
        "learn": {
            "apprentice": store.learn.apprentice,
            "due": srs::due(&store.cards, learn::DECK, today, 100).len(),
            "paper": learn::paper_view(store, market, today),
            "judged": crate::journal::calibration(&store.decisions, |t| market.price(t), market.economy.cdi, today).judged,
        },
        "weekly": { "week": w, "told": store.learn.weekly_told == Some(w) },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::fixtures;

    #[test]
    fn the_screen_has_the_rings_the_dividends_and_the_rest() {
        let today = Date::new(2026, 10, 21).unwrap();
        let market = fixtures::market();
        let store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let view = view(&store, &market, Some(&Library::default()), today);
        let classes = view["byClass"].as_array().unwrap();
        assert!(classes.len() >= 2);
        let sum: f64 = classes.iter().map(|c| c["value"].as_f64().unwrap()).sum();
        assert!((sum - view["total"].as_f64().unwrap()).abs() < 0.01, "the ring's slices add up to the total");
        assert!(view["byAsset"][0]["value"].as_f64() >= view["byAsset"][1]["value"].as_f64(), "largest first");
        let months = view["dividends"].as_array().unwrap();
        assert_eq!(months.len(), 12);
        assert_eq!(months[11]["month"], "2026-10");
        let october = &months[11];
        assert!(october["total"].as_f64().unwrap() > 0.0, "the fixtures' funds paid in October: {october}");
        assert!(view["feed"].as_array().unwrap().is_empty() && view["theses"].as_array().unwrap().is_empty());
        assert_eq!(view["weekly"]["week"], "2026-10-19");
    }

    #[test]
    fn the_sheets_record_is_the_dividends_and_the_announcements_show_beside_it() {
        let today = Date::new(2026, 10, 30).unwrap();
        let market = fixtures::market();
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        // Without the sheet: the funds' announcements times what is held, every month of the year.
        let months = dividends(&store, &market, today, 12);
        assert!(months[0]["total"].as_f64().unwrap() > 0.0, "a year of announcements, as before");
        assert_eq!(months[11]["source"], "anuncios");
        // With it: its months are the record, R$ 82,27 in October and nothing before, as the user's sheet says.
        store.recorded_dividends.source = "Sua planilha — aba Meu Patrimônio".into();
        for (m, v) in [("2026-05", 0.0), ("2026-06", 0.0), ("2026-07", 0.0), ("2026-08", 0.0), ("2026-09", 0.0), ("2026-10", 82.27), ("2026-11", 0.0), ("2026-12", 0.0)] {
            store.recorded_dividends.months.insert(m.into(), v);
        }
        let months = dividends(&store, &market, today, 12);
        let october = &months[11];
        assert_eq!((october["total"].as_f64(), october["source"].as_str()), (Some(82.27), Some("planilha")));
        assert!(october["announced"].as_f64().unwrap() > 0.0, "the announcements kept beside, to show a divergence");
        assert_eq!(october["itemsFrom"], "anuncios");
        assert_eq!((months[0]["total"].as_f64(), months[0]["covered"].as_bool()), (Some(0.0), Some(false)), "November 2025: no record in the sheet");
        let year: f64 = months.iter().map(|m| m["total"].as_f64().unwrap()).sum();
        assert!((year - 82.27).abs() < 1e-9, "the year's total is the sheet's: {year}");
    }

    #[test]
    fn a_fund_bought_lately_is_not_paid_for_the_months_before() {
        let today = Date::new(2026, 10, 30).unwrap();
        let market = fixtures::market();
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        // The snapshots began listing prices in August, and GGRC11 shows up only in October.
        let snapshot = |d: u32, m: u32, with: bool| {
            let mut prices = BTreeMap::new();
            prices.insert("GARE11".to_string(), 8.4);
            if with {
                prices.insert("GGRC11".to_string(), 9.1);
            }
            crate::model::Snapshot { date: Date::new(2026, m, d).unwrap(), value: 1000.0, cost: 1000.0, by_class: BTreeMap::new(), cost_by_class: BTreeMap::new(), income: 0.0, prices }
        };
        store.snapshots = vec![snapshot(1, 8, false), snapshot(1, 9, false), snapshot(2, 10, true)];
        assert_eq!(held_since(&store, "GGRC11"), Date::new(2026, 10, 2));
        assert_eq!(held_since(&store, "GARE11"), None, "held since before the snapshots knew prices: unknown");
        let months = dividends(&store, &market, today, 12);
        let september = &months[10];
        assert!(september["items"].as_array().unwrap().iter().all(|i| i["key"] != "GGRC11"), "{september}");
    }

    #[test]
    fn a_brains_json_is_found_in_its_words() {
        assert_eq!(json_in("```json\n{\"a\": 1}\n```"), Some(json!({"a": 1})));
        assert_eq!(json_in("Aqui está: {\"a\": {\"b\": 2}} pronto."), Some(json!({"a": {"b": 2}})));
        assert_eq!(json_in("nada"), None);
        assert_eq!(json_in("} {"), None);
        assert_eq!(json_in("O comitê {resumo} decidiu: {\"a\": [1, 2,], \"b\": \"x, }\",}"), Some(json!({"a": [1, 2], "b": "x, }"})));
    }

    #[test]
    fn the_apps_calls_read_a_report_meet_the_committee_and_keep_the_thesis() {
        use crate::sources::fnet::{self, Document};
        let today = Date::new(2026, 10, 28).unwrap();
        let mut market = fixtures::market();
        let report = |id: u64, delivered: Date, reference: &str| Document {
            id,
            category: "Relatórios".into(),
            kind: "Relatório Gerencial".into(),
            species: String::new(),
            reference: reference.into(),
            delivered: Some(delivered),
            info: String::new(),
            topic: fnet::topic("Relatórios", "Relatório Gerencial", "", ""),
            structured: false,
            fund: String::new(),
            trading: String::new(),
        };
        market.funds.get_mut("GGRC11").unwrap().documents = vec![report(904, Date::new(2026, 10, 10).unwrap(), "09/2026"), report(903, Date::new(2026, 9, 10).unwrap(), "08/2026")];
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let mut lib = Library::default();
        let call = |store: &mut Store, lib: &mut Library, tool: &str, input: Value| app(store, &market, lib, tool, &input, today);

        // The reader: what is due, read as the app read its pages, digested by the brain.
        let (due, _) = call(&mut store, &mut lib, "invest_docs", json!({"do": "due"})).unwrap();
        let due = due.as_array().unwrap().clone();
        assert_eq!(due.len(), 2, "{due:?}");
        assert_eq!(due[0]["key"], "fnet:904");
        call(&mut store, &mut lib, "invest_docs", json!({"do": "ingest", "pending": due[1], "pages": crate::library::fixtures::report_pages("6,0", "0,085")})).unwrap();
        let (doc, saved) = call(&mut store, &mut lib, "invest_docs", json!({"do": "ingest", "pending": due[0], "pages": crate::library::fixtures::report_pages("8,5", "0,080")})).unwrap();
        assert_eq!(saved, LIBRARY);
        assert!(doc["changes"].as_array().unwrap().iter().any(|c| c.as_str().unwrap().starts_with("Vacância física: 6,0% → 8,5%")), "{doc}");
        let (brief, _) = call(&mut store, &mut lib, "invest_docs", json!({"do": "brief", "key": "fnet:904"})).unwrap();
        assert!(brief["prompt"].as_str().unwrap().contains("[Page 2]"));
        let answer = "Claro:\n```json\n{\"headline\": \"Vacância subiu para 8,5%\", \"points\": [{\"text\": \"A vacância física foi de 6% para 8,5%\", \"page\": 2}], \"tone\": \"worse\", \"thesisRisk\": true}\n```";
        let (doc, _) = call(&mut store, &mut lib, "invest_docs", json!({"do": "digest", "key": "fnet:904", "answer": answer})).unwrap();
        assert_eq!(doc["headline"], "Vacância subiu para 8,5%");
        let (page, _) = call(&mut store, &mut lib, "invest_docs", json!({"do": "page", "key": "fnet:904", "page": 2})).unwrap();
        assert!(page["text"].as_str().unwrap().contains("8,5"), "the page the committee cites, as read: {page}");
        assert!(call(&mut store, &mut lib, "invest_docs", json!({"do": "page", "key": "fnet:904", "page": 99})).is_err());
        assert_eq!(call(&mut store, &mut lib, "invest_docs", json!({"do": "undigested"})).unwrap().0.as_array().unwrap().len(), 1);

        // The thesis breaks on the new report: the committee meets by itself.
        let (theses, _) = call(&mut store, &mut lib, "invest_thesis", json!({"ticker": "GGRC11", "text": "vacância até 7%; DY acima de 9%"})).unwrap();
        assert_eq!(theses[0]["ok"], false, "{theses}");
        let (meetings, _) = call(&mut store, &mut lib, "invest_committee", json!({"do": "meetings"})).unwrap();
        assert_eq!(meetings[0]["ticker"], "GGRC11");
        let (brief, _) = call(&mut store, &mut lib, "invest_committee", json!({"do": "brief", "proposal": meetings[0]["about"], "ticker": "GGRC11", "language": "Portuguese"})).unwrap();
        assert_eq!(brief["documents"][0], "fnet:904");
        assert_eq!(brief["docs"][0]["id"], "D1");
        assert!(brief["prompt"].as_str().unwrap().contains("D1 —") && brief["prompt"].as_str().unwrap().contains("\"minds\""));
        assert!(brief["facts"].as_array().unwrap().len() > 5, "the facts go back with the audit and the verdict");
        let minds: Vec<Value> = ["valuation", "legal", "forensic", "risk", "macro"]
            .iter()
            .map(|id| {
                let against = !(*id == "valuation" || *id == "macro");
                let point = if *id == "risk" { json!({"text": "Inadimplência de 0,8% da receita", "source": "D1 p.2", "kind": "fato"}) } else { json!({"text": "A vacância física foi a 8,5%", "source": "D1 p.2", "kind": "fato"}) };
                json!({"id": id, "vote": if against { "against" } else { "for" }, "confidence": 70, "why": "…", "veto": *id == "risk", "vetoReason": "inadimplencia_relevante", "points": [point]})
            })
            .collect();
        let answer = json!({"minds": minds, "asset": {"view": "neutro"}, "portfolio": {"view": "manter"}, "chair": {"question": "Manter GGRC11 com a vacância subindo?", "dissent": "O P/VP está baixo.", "wouldChange": "Vacância de volta a 6%.", "fit": "FIIs acima da meta."}}).to_string();
        let (audit, _) = call(&mut store, &mut lib, "invest_committee", json!({"do": "audit", "answer": answer, "ticker": "GGRC11", "proposal": "manter", "facts": brief["facts"], "docs": brief["docs"]})).unwrap();
        assert!(audit["prompt"].as_str().unwrap().contains("auditor") && audit["prompt"].as_str().unwrap().contains("O que o código do AXYON já encontrou"));
        let (verdict, saved) = call(&mut store, &mut lib, "invest_committee", json!({"do": "verdict", "answer": answer, "audit": "{\"audit\": [], \"asset\": {\"view\": \"neutro\"}, \"portfolio\": {\"view\": \"manter\"}, \"chair\": {\"question\": \"Manter?\"}}", "ticker": "GGRC11", "proposal": "manter", "reason": "document", "document": "fnet:904", "facts": brief["facts"], "docs": brief["docs"]})).unwrap();
        assert_eq!(saved, STORE);
        assert_eq!((verdict["score"]["for"].as_u64(), verdict["score"]["against"].as_u64()), (Some(2), Some(3)), "{verdict}");
        assert_eq!(verdict["vetoedBy"], json!(["risk"]), "a criterion and a page that holds it: {}", verdict["audit"]);
        assert_eq!(verdict["minds"][3]["name"], "Gestor de risco");
        assert_eq!(verdict["minds"][0]["points"][0]["verified"], json!(true), "8,5 is on D1's page 2");
        assert_eq!((verdict["asset"]["view"].as_str(), verdict["portfolio"]["view"].as_str(), verdict["question"].as_str()), (Some("neutro"), Some("manter"), Some("Manter?")));
        assert_eq!(verdict["documents"][0]["key"], "fnet:904");
        assert!(call(&mut store, &mut lib, "invest_committee", json!({"do": "meetings"})).unwrap().0.as_array().unwrap().is_empty(), "met");

        // Learning: apprentice, cards graded, a pretend trade.
        let (learn, _) = call(&mut store, &mut lib, "invest_learn", json!({"do": "apprentice", "on": true})).unwrap();
        assert_eq!(learn["apprentice"], true);
        let (learn, _) = call(&mut store, &mut lib, "invest_learn", json!({"do": "cards"})).unwrap();
        let card = learn["cards"]["due"][0]["id"].as_str().unwrap().to_string();
        let before = learn["cards"]["due"].as_array().unwrap().len();
        let (learn, _) = call(&mut store, &mut lib, "invest_learn", json!({"do": "grade", "id": card, "grade": 5})).unwrap();
        assert_eq!(learn["cards"]["due"].as_array().unwrap().len(), before - 1);
        let (learn, _) = call(&mut store, &mut lib, "invest_learn", json!({"do": "paper", "action": "buy", "ticker": "GGRC11", "quantity": 100.0})).unwrap();
        assert!((learn["paper"]["cash"].as_f64().unwrap() - (10_000.0 - 912.0)).abs() < 1e-6, "{}", learn["paper"]);
        assert!(call(&mut store, &mut lib, "invest_learn", json!({"do": "paper", "action": "sell", "ticker": "GGRC11", "quantity": 500.0})).is_err());

        // The week's script, then told.
        let (week, _) = call(&mut store, &mut lib, "invest_weekly", json!({"language": "pt-BR"})).unwrap();
        assert!(week["text"].as_str().unwrap().contains("Isto não é recomendação de investimento"), "{week}");
        let (week, _) = call(&mut store, &mut lib, "invest_weekly", json!({"language": "Brazilian Portuguese"})).unwrap();
        assert!(week["text"].as_str().unwrap().starts_with("Seu resumo da semana"), "{week}");
        let (week, _) = call(&mut store, &mut lib, "invest_weekly", json!({"language": "en-US"})).unwrap();
        assert!(week["text"].as_str().unwrap().starts_with("Your week"), "{week}");
        call(&mut store, &mut lib, "invest_weekly", json!({"do": "told"})).unwrap();
        assert_eq!(view(&store, &market, Some(&lib), today)["weekly"]["told"], true);

        // A target for one asset, set in the app: on the screen and in the committee's facts; then taken away.
        let (screen, saved) = call(&mut store, &mut lib, "invest_target", json!({"ticker": "ggrc11", "target": 7.5})).unwrap();
        assert_eq!(saved, STORE);
        assert_eq!(screen["targets"]["GGRC11"], 7.5);
        assert_eq!(screen["byAsset"].as_array().unwrap().iter().find(|a| a["key"] == "GGRC11").unwrap()["target"], 7.5);
        let (brief, _) = call(&mut store, &mut lib, "invest_committee", json!({"do": "brief", "proposal": "comprar 10 cotas", "ticker": "GGRC11"})).unwrap();
        assert!(brief["prompt"].as_str().unwrap().contains("Sua meta para GGRC11: 7,5% da carteira"), "{}", brief["prompt"]);
        assert!(call(&mut store, &mut lib, "invest_target", json!({"ticker": "GGRC11", "target": 140})).is_err());
        let (screen, _) = call(&mut store, &mut lib, "invest_target", json!({"ticker": "GGRC11"})).unwrap();
        assert!(screen["targets"].as_object().unwrap().is_empty());
        assert!(call(&mut store, &mut lib, "invest_nothing", json!({})).is_err());
    }
}
