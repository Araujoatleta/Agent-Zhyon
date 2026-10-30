//! AXYON's investment committee: five minds and a chair, on the facts this code worked out, each with its source, and
//! on the documents the reader read (library.rs), each page cited. The minds are archetypes, never real people: a
//! valuation analyst, a capital-markets lawyer, a forensic accountant, a risk manager and a macro strategist; the
//! lawyer and the risk manager may veto. The brain writes their opinions; the code keeps the score, the vetoes and the
//! verdicts. Nothing is bought or sold: the chair ends with a question for the user.

use serde_json::Value;

use crate::date::Date;
use crate::library::Library;
use crate::model::{AssetClass, Market, MindVote, Store, Verdict};
use crate::text::{brl, clip, grouped, pct};
use crate::{ceiling, company, fii, journal, portfolio};

/// One mind of the committee.
pub struct Mind {
    pub id: &'static str,
    /// Its archetype, in Portuguese, for the app.
    pub name_pt: &'static str,
    pub name_en: &'static str,
    pub role: &'static str,
    pub veto: bool,
}

pub const MINDS: [Mind; 5] = [
    Mind {
        id: "valuation",
        name_pt: "Analista de valuation",
        name_en: "Valuation analyst",
        role: "A valuation analyst with twenty years pricing Brazilian real-estate funds and shares: is the price fair against the VP, the ceilings and the yield's own history? Cheap or dear, and why.",
        veto: false,
    },
    Mind {
        id: "legal",
        name_pt: "Jurídico e governança",
        name_en: "Legal and governance",
        role: "A capital-markets lawyer and governance specialist: the documents — material facts, assemblies and their proposals, issuances that dilute the holders, lawsuits and contingencies, the manager's conflicts of interest, related parties, the regulation. May VETO when a legal or governance point is serious enough on its own.",
        veto: true,
    },
    Mind {
        id: "forensic",
        name_pt: "Contador forense",
        name_en: "Forensic accountant",
        role: "A forensic accountant: the quality of the result — distributing more than it earns, one-off gains, reserves running down, cash against profit, the auditor's remarks.",
        veto: false,
    },
    Mind {
        id: "risk",
        name_pt: "Gestor de risco",
        name_en: "Risk manager",
        role: "A risk manager: the concentration against the user's own targets, liquidity, vacancy and default, the credit of what it holds, leverage; and a pre-mortem: a year from now this went wrong — the likeliest story. May VETO when one risk alone could seriously harm this user's portfolio.",
        veto: true,
    },
    Mind {
        id: "macro",
        name_pt: "Estrategista macro",
        name_en: "Macro strategist",
        role: "A macro strategist: the Selic, the CDI, inflation and the Focus survey against what this asset pays (a fund's tax-free yield against the CDI after tax), and the cycle of its sector.",
        veto: false,
    },
];

pub const CHAIR: &str = "The chair: the score (for × against), any veto (a veto stops a majority for: say so), the decision as a question for the user — never an order —, the strongest argument of the side that lost (the dissent), what would change the picture, and how it fits the user's own targets.";

pub fn mind(id: &str) -> Option<&'static Mind> {
    MINDS.iter().find(|m| m.id == id)
}

/// A fact the committee works on, and where it comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    pub text: String,
    pub source: String,
}

fn fact(out: &mut Vec<Fact>, text: impl Into<String>, source: &str) {
    let text = text.into();
    if !text.trim().is_empty() {
        out.push(Fact { text, source: source.into() });
    }
}

/// The facts on `ticker` for a proposal, each with its source: the user's position and targets, the price, the
/// fund's or company's figures, the ceilings, the thesis, the economy and the user's own past decisions.
pub fn facts(store: &Store, market: &Market, ticker: &str, proposal: &str, today: Date) -> Vec<Fact> {
    let t = ticker;
    let mut out = Vec::new();
    let p = &store.portfolio;
    let v = portfolio::value(p, market);
    let class = p.position(t).map(|x| x.class).unwrap_or_else(|| if market.funds.contains_key(t) { AssetClass::Fii } else { crate::import::guess_class(t).0 });
    const MINE: &str = "the user's portfolio";
    match v.holdings.iter().find(|h| h.ticker == t) {
        Some(h) => fact(
            &mut out,
            format!("The user holds {} of {t} at an average of {} ({} of the portfolio{}).", grouped(h.quantity, 0), brl(h.average_price), pct(h.weight, 1), h.gain_pct.map(|g| format!(", {}{} since bought", if g >= 0.0 { "+" } else { "" }, pct(g, 1))).unwrap_or_default()),
            MINE,
        ),
        None => fact(&mut out, format!("The user does not hold {t} yet."), MINE),
    }
    let amount = {
        let nums: Vec<f64> = proposal.split_whitespace().filter_map(crate::text::num).collect();
        let price = market.price(t).unwrap_or(0.0);
        match nums.as_slice() {
            [q, ..] if price > 0.0 && *q >= 1.0 && q.fract() == 0.0 => q * price,
            _ => 0.0,
        }
    };
    if let Some((before, after, target)) = journal::sizing(&v, class, amount) {
        if amount > 0.0 {
            fact(
                &mut out,
                format!("The proposal costs about {}: the user's {} would go from {} to {} of the portfolio{}.", brl(amount), class.label(), pct(before, 1), pct(after, 1), target.map(|x| format!(" (their target: {})", pct(x, 0))).unwrap_or_default()),
                "the user's portfolio and targets",
            );
        }
    }
    if v.value > 0.0 {
        fact(&mut out, format!("The whole portfolio is worth about {}.", brl(v.value)), MINE);
    }
    match (market.live_now(t), market.close(t)) {
        (Some(l), _) => fact(&mut out, format!("Price now {} (15 minutes late).", brl(l.price)), "Google Finance"),
        (None, Some(c)) => fact(&mut out, format!("Last close {}{}.", brl(c), market.quotes.get(t).map(|q| format!(" on {}", q.date.br())).unwrap_or_default()), "B3"),
        _ => fact(&mut out, format!("No price of {t} could be read."), "B3"),
    }
    if let Some(fund) = market.funds.get(t) {
        let r = fii::report(fund, market.price(t), &market.economy, today);
        let cvm = format!("the CVM's monthly report{}", r.vp_date.map(|d| format!(" ({})", d.month_key())).unwrap_or_default());
        if !r.name.is_empty() || !r.kind.is_empty() {
            fact(&mut out, format!("{t} is {}{}{}.", if r.name.is_empty() { "a fund" } else { &r.name }, if r.kind.is_empty() { String::new() } else { format!(", a {} fund", r.kind) }, if r.segment.is_empty() { String::new() } else { format!(" ({})", r.segment) }), &cvm);
        }
        if let (Some(vp), Some(pvp)) = (r.vp_share, r.p_vp) {
            fact(&mut out, format!("VP per share {} and P/VP {:.2}.", brl(vp), pvp), &cvm);
        }
        if let Some(x) = r.vp_change_12m {
            fact(&mut out, format!("The VP per share moved {}{} in twelve months.", if x >= 0.0 { "+" } else { "" }, pct(x, 1)), &cvm);
        }
        if let Some(x) = r.last_dividend {
            fact(&mut out, format!("Last dividend {} a share{}.", brl(x), r.avg_dividend_12m.map(|a| format!("; twelve-month average {}", brl(a))).unwrap_or_default()), "FundosNet's dividend notices");
        }
        if let Some(x) = r.yield_12m {
            let pace = if r.yield_months >= 12 { "twelve months of dividends".to_string() } else { format!("the pace of the {} months known", r.yield_months) };
            fact(&mut out, format!("Yield at the price {} a year ({pace}), tax-free for individuals.", pct(x, 1)), "FundosNet and the price");
        }
        if let Some(x) = r.holders {
            fact(&mut out, format!("{} holders{}.", grouped(x, 0), r.holders_change_12m.map(|c| format!(" ({}{} in twelve months)", if c >= 0.0 { "+" } else { "" }, pct(c, 1))).unwrap_or_default()), &cvm);
        }
        if let Some(x) = r.liquidity {
            fact(&mut out, format!("Cash and liquid assets are {} of its net assets.", pct(x, 1)), &cvm);
        }
        fact(&mut out, format!("AXYON's reading of its dividend's health: {}.", fii::health_word(r.health)), "AXYON's code on the CVM's reports");
        for s in &r.signals {
            fact(&mut out, s.clone(), "AXYON's code on the CVM's reports");
        }
        for e in fii::events(fund, p.position(t).map(|x| x.quantity).unwrap_or(0.0), today, 60) {
            fact(&mut out, format!("{} {}: {}{}", e.date.br(), e.kind, e.title, e.deadline.map(|d| format!(" (until {})", d.br())).unwrap_or_default()), "FundosNet");
        }
    }
    if let Some(c) = market.companies.get(t) {
        let m = company::metrics(&c.figures);
        let thesis = store.theses.iter().find(|x| x.ticker == t).map(|x| x.conditions.clone()).unwrap_or_default();
        for line in company::for_model(t, &c.name, &m, &company::check(&m, &thesis), &company::red_flags(&m)).lines().skip(1) {
            fact(&mut out, line.trim_start_matches("- ").trim(), "the CVM's statements (DFP, ITR)");
        }
    }
    let a = ceiling::assess(store, market, t, &ceiling::Given::default(), today);
    for line in ceiling::for_model(&a, store.ceilings.margin).lines().filter(|l| !l.trim().is_empty()) {
        fact(&mut out, line.trim_start_matches("- ").trim(), "AXYON's ceilings (Bazin, Graham, Lynch, P/VP)");
    }
    if let Some(th) = store.theses.iter().find(|x| x.ticker == t) {
        if !th.text.trim().is_empty() {
            fact(&mut out, format!("The user's thesis, written on {}: \"{}\".", th.written.br(), clip(&th.text, 300)), "the user's thesis");
        }
    }
    fact(&mut out, crate::engine::economy(market, today), "the Banco Central (rates, IPCA, Focus survey)");
    for d in store.decisions.iter().filter(|d| d.ticker == t).rev().take(4) {
        fact(
            &mut out,
            format!("On {} the user decided to {} {t}{}: \"{}\"{}.", d.date.br(), d.action, d.price.map(|x| format!(" at {}", brl(x))).unwrap_or_default(), clip(&d.reasons, 160), d.confidence.map(|c| format!(", {c}% sure")).unwrap_or_default()),
            "the user's decision journal",
        );
    }
    let cal = journal::calibration(&store.decisions, |x| market.price(x), market.economy.cdi, today);
    if cal.judged >= 3 {
        fact(&mut out, format!("Of the user's {} decisions judged, {} were right.", cal.judged, cal.right), "the user's decision journal");
    }
    out
}

/// The committee's procedure: the minds, the chair, the rules, and the answer's form ("chat": read by the user in the
/// chat; "json": for the app's committee screen).
pub fn protocol(json: bool, apprentice: bool, language: &str) -> String {
    let mut out = String::from(
        "Run AXYON's investment committee on the proposal below: five minds, then the chair. Use only the facts (F…) and the documents' pages (D…) given; after each point cite one, as [F3] or [D1 p.4]. A number that is not in them is not said; a mind short of data says what it lacks. Each mind is an archetype, never a real person. Nothing is bought or sold.",
    );
    for m in &MINDS {
        out.push_str(&format!("\n[{}] {}", m.id, m.role));
    }
    out.push_str("\nEach mind: its vote (for, against or neutral), how sure it is (0–100), at most three points with their sources, and whether it vetoes (legal and risk only, and only for a serious reason).");
    out.push_str(&format!("\n[chair] {CHAIR}"));
    if apprentice {
        out.push_str("\nThe user is learning to invest: plain words, short sentences, and each term explained the first time it comes (P/VP: the price against the fund's book value per share).");
    }
    if json {
        out.push_str(&format!(
            "\nAnswer with only this JSON, its text in {language}: {{\"minds\": [{{\"id\": \"valuation\", \"vote\": \"for|against|neutral\", \"confidence\": 70, \"veto\": false, \"points\": [{{\"text\": \"…\", \"source\": \"F3\"}}]}}, … the five, in this order: valuation, legal, forensic, risk, macro], \"chair\": {{\"question\": \"…\", \"dissent\": \"…\", \"wouldChange\": \"…\", \"fit\": \"…\"}}}}"
        ));
    } else {
        out.push_str(&format!("\nWrite it in {language} for the chat: each mind in two or three lines with its vote, then the chair's synthesis with the score. Then offer to record the decision in the journal (wealth_decision) once the user decides, with their reasons and how sure they are."));
    }
    out
}

/// The committee's brief: its procedure, the proposal, the facts numbered with their sources, and the documents.
/// Also returns the ticker and the documents' keys.
pub fn brief(store: &Store, market: &Market, lib: Option<&Library>, args: &Value, today: Date, json: bool) -> Result<(String, String, Vec<String>), String> {
    let s = |k: &str| args[k].as_str().map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    let proposal = s("proposal").ok_or("the committee needs the proposal, in the user's words")?;
    let ticker = s("ticker").and_then(|t| crate::text::ticker(&t)).or_else(|| crate::text::tickers_in(&proposal).into_iter().next()).ok_or("the committee needs the asset's ticker")?;
    let language = s("language").unwrap_or_else(|| "the user's language".into());
    let mut out = format!("{}\n\nProposal: {proposal}\nAsset: {ticker}\n\nFacts:", protocol(json, store.learn.apprentice, &language));
    for (i, f) in facts(store, market, &ticker, &proposal, today).iter().enumerate() {
        out.push_str(&format!("\nF{} [{}] {}", i + 1, f.source, f.text));
    }
    let mut keys = Vec::new();
    if let Some(lib) = lib {
        let (docs, k) = lib.for_committee(&ticker, 9_000);
        if !docs.is_empty() {
            out.push_str("\n\nDocuments (read by AXYON's reader):");
            out.push_str(&docs);
        }
        keys = k;
    }
    Ok((out, ticker, keys))
}

/// The brain's JSON answer (protocol `json`) as a verdict: the five minds in their order (an unknown one dropped, a
/// missing one neutral), a veto only where a mind may veto, the chair's words.
pub fn parse_verdict(v: &Value, ticker: &str, proposal: &str, reason: &str, document: &str, today: Date) -> Result<Verdict, String> {
    let given = v["minds"].as_array().ok_or("The committee's answer has no minds.")?;
    let mut minds = Vec::new();
    for m in &MINDS {
        let found = given.iter().find(|x| x["id"].as_str().is_some_and(|id| id.trim().eq_ignore_ascii_case(m.id)));
        let vote = |x: &Value| match x["vote"].as_str().unwrap_or("").trim().to_lowercase().as_str() {
            "for" | "a favor" | "favor" | "pour" => "for".to_string(),
            "against" | "contra" | "contre" => "against".to_string(),
            _ => "neutral".to_string(),
        };
        minds.push(match found {
            Some(x) => MindVote {
                id: m.id.into(),
                vote: vote(x),
                confidence: x["confidence"].as_f64().map(|c| c.clamp(0.0, 100.0) as u8).unwrap_or(50),
                veto: m.veto && x["veto"].as_bool().unwrap_or(false),
                points: x["points"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|p| {
                                let text = p["text"].as_str().or_else(|| p.as_str())?.trim();
                                (!text.is_empty()).then(|| (clip(text, 360), p["source"].as_str().unwrap_or("").trim().to_string()))
                            })
                            .take(3)
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            None => MindVote { id: m.id.into(), vote: "neutral".into(), confidence: 0, veto: false, points: Vec::new() },
        });
    }
    if minds.iter().all(|m| m.points.is_empty()) {
        return Err("The committee's minds said nothing.".into());
    }
    let chair = &v["chair"];
    let said = |k: &str| clip(chair[k].as_str().unwrap_or("").trim(), 500);
    Ok(Verdict {
        id: format!("{}-{}-{}", ticker, today, (proposal.len() + minds.iter().map(|m| m.points.len()).sum::<usize>()) % 1000),
        ticker: ticker.into(),
        proposal: clip(proposal, 300),
        date: Some(today),
        minds,
        question: said("question"),
        dissent: said("dissent"),
        would_change: said("wouldChange"),
        fit: said("fit"),
        reason: reason.into(),
        document: document.into(),
    })
}

/// The most verdicts kept.
pub const KEPT: usize = 40;

/// A verdict kept (one per ticker and day and reason replaces the one before it).
pub fn keep(store: &mut Store, verdict: Verdict) {
    store.verdicts.retain(|x| !(x.ticker == verdict.ticker && x.date == verdict.date && x.reason == verdict.reason));
    store.verdicts.push(verdict);
    let extra = store.verdicts.len().saturating_sub(KEPT);
    store.verdicts.drain(..extra);
}

/// The latest verdict on `ticker`.
pub fn latest<'a>(store: &'a Store, ticker: &str) -> Option<&'a Verdict> {
    store.verdicts.iter().rev().find(|v| v.ticker == ticker)
}

/// A verdict in a few lines, for the brain.
pub fn for_model(v: &Verdict) -> String {
    let (f, a, n) = v.score();
    let mut out = format!(
        "Committee on {} ({}{}): {f} for × {a} against{}.",
        v.ticker,
        v.date.map(|d| d.br()).unwrap_or_default(),
        if v.proposal.is_empty() { String::new() } else { format!(", \"{}\"", clip(&v.proposal, 80)) },
        if n > 0 { format!(", {n} neutral") } else { String::new() }
    );
    let vetoes = v.vetoed_by();
    if !vetoes.is_empty() {
        out.push_str(&format!(" Vetoed by: {}.", vetoes.join(", ")));
    }
    for m in &v.minds {
        if let Some((p, src)) = m.points.first() {
            out.push_str(&format!("\n- {} ({}, {}%): {p}{}", m.id, m.vote, m.confidence, if src.is_empty() { String::new() } else { format!(" [{src}]") }));
        }
    }
    if !v.question.is_empty() {
        out.push_str(&format!("\nThe chair asks: {}", v.question));
    }
    if !v.dissent.is_empty() {
        out.push_str(&format!("\nDissent: {}", v.dissent));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::fixtures;
    use serde_json::json;

    fn today() -> Date {
        Date::new(2026, 10, 21).unwrap()
    }

    fn store_and_market() -> (Store, Market) {
        let mut store = Store::default();
        let market = fixtures::market();
        crate::engine::update(&mut store, &json!({"text": "GGRC11 150 R$ 9,20\nTIMS3 30 R$ 18,60", "targets": {"FIIs": 60, "Ações": 40}}), None, today()).unwrap();
        (store, market)
    }

    #[test]
    fn the_brief_numbers_its_facts_with_their_sources_and_names_the_five_minds() {
        let (mut store, market) = store_and_market();
        store.learn.apprentice = true;
        let (text, ticker, keys) = brief(&store, &market, None, &json!({"proposal": "comprar 50 cotas de GGRC11", "language": "Portuguese"}), today(), true).unwrap();
        assert_eq!(ticker, "GGRC11");
        assert!(keys.is_empty());
        for m in &MINDS {
            assert!(text.contains(&format!("[{}]", m.id)), "{} is in the brief", m.id);
        }
        assert!(text.contains("May VETO") && text.contains("[chair]") && text.contains("never a real person"));
        assert!(text.contains("F1 [the user's portfolio] The user holds 150 of GGRC11"), "{text}");
        assert!(text.contains("[B3] Last close") || text.contains("[Google Finance]"), "{text}");
        assert!(text.contains("[the Banco Central (rates, IPCA, Focus survey)]"));
        assert!(text.contains("plain words") && text.contains("its text in Portuguese") && text.contains("\"wouldChange\""));
        assert!(brief(&store, &market, None, &json!({"proposal": "comprar algo"}), today(), true).is_err(), "no ticker, no committee");
        let (chat, _, _) = brief(&store, &market, None, &json!({"proposal": "vender TIMS3"}), today(), false).unwrap();
        assert!(chat.contains("for the chat") && chat.contains("wealth_decision") && !chat.contains("\"minds\""));
    }

    #[test]
    fn the_answer_becomes_a_verdict_with_its_score_and_vetoes() {
        let (mut store, _) = store_and_market();
        let answer = json!({
            "minds": [
                {"id": "valuation", "vote": "for", "confidence": 75, "points": [{"text": "P/VP 0,93", "source": "F4"}]},
                {"id": "legal", "vote": "against", "confidence": 80, "veto": true, "points": [{"text": "Emissão dilui", "source": "D1 p.2"}]},
                {"id": "forensic", "vote": "a favor", "confidence": 60, "veto": true, "points": ["Resultado cobre a distribuição"]},
                {"id": "risk", "vote": "contra", "confidence": 140, "points": [{"text": "Concentração", "source": "F2"}]},
                {"id": "someone", "vote": "for", "points": [{"text": "x"}]}
            ],
            "chair": {"question": "Vale esperar a assembleia?", "dissent": "O preço está bom", "wouldChange": "A emissão acima do VP", "fit": "Acima da meta de FIIs"}
        });
        let v = parse_verdict(&answer, "GGRC11", "comprar 50 cotas", "user", "", today()).unwrap();
        assert_eq!(v.minds.len(), 5, "the five, in order; the unknown one dropped");
        assert_eq!(v.score(), (2, 2, 1), "macro, not given, is neutral");
        assert_eq!(v.vetoed_by(), ["legal"], "only a mind that may veto does");
        assert_eq!(v.minds[3].confidence, 100);
        assert_eq!(v.minds[2].points[0].0, "Resultado cobre a distribuição");
        assert_eq!(v.question, "Vale esperar a assembleia?");
        keep(&mut store, v.clone());
        keep(&mut store, v);
        assert_eq!(store.verdicts.len(), 1, "the same day's verdict replaces itself");
        let text = for_model(latest(&store, "GGRC11").unwrap());
        assert!(text.contains("2 for × 2 against, 1 neutral") && text.contains("Vetoed by: legal") && text.contains("The chair asks: Vale esperar"), "{text}");
        assert!(parse_verdict(&json!({"minds": []}), "GGRC11", "", "user", "", today()).is_err());
        assert!(parse_verdict(&json!({}), "GGRC11", "", "user", "", today()).is_err());
    }
}
