//! Learning with one's own portfolio: a glossary whose every term shows its value in the user's assets, the
//! apprentice mode (plain words everywhere: the committee, the reader, the weekly summary), flashcards made from the
//! user's own assets in the spaced repetition the exam and English use (srs.rs), a training portfolio with pretend
//! money at the real prices, and the personal score of the decision journal (how often the user is right when sure).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::date::Date;
use crate::library::Library;
use crate::model::{AssetClass, Market, Paper, Position, Store, Trade, TradeKind};
use crate::text::{brl, grouped, pct, plain};
use crate::{company, journal, portfolio, srs, thesis};

/// The flashcards' deck.
pub const DECK: &str = "investimentos";
/// The pretend money a training portfolio starts with by default, R$.
pub const PAPER_START: f64 = 10_000.0;

/// A term of the glossary: its words in Portuguese (the Brazilian market's), and the metric that shows it in the
/// user's own assets.
pub struct Term {
    pub key: &'static str,
    pub term: &'static str,
    pub simple: &'static str,
    pub why: &'static str,
    pub metric: Option<&'static str>,
    /// On every user's cards, whatever they hold.
    pub basic: bool,
}

const fn t(key: &'static str, term: &'static str, simple: &'static str, why: &'static str, metric: Option<&'static str>, basic: bool) -> Term {
    Term { key, term, simple, why, metric, basic }
}

pub const GLOSSARY: &[Term] = &[
    t("p_vp", "P/VP", "Quanto o mercado paga por cada R$ 1 de patrimônio. Abaixo de 1, a cota custa menos que o patrimônio por cota.", "Ajuda a ver se está caro ou barato perto do que o fundo tem.", Some("p_vp"), false),
    t("dy", "Dividend yield (DY)", "Os proventos de 12 meses divididos pelo preço. DY de 10% = R$ 10 por ano a cada R$ 100 investidos.", "Mostra a renda; DY alto demais pode ser preço caindo ou um pagamento que não se sustenta.", Some("dy"), false),
    t("vp", "Valor patrimonial (VP) por cota", "O patrimônio do fundo dividido pelas cotas: quanto cada cota vale nos livros.", "Um VP caindo mês a mês é sinal de que o fundo perde valor.", Some("vp_change"), false),
    t("vacancia", "Vacância", "A parte dos imóveis sem inquilino. Física: área vazia; financeira: aluguel que deixou de entrar.", "Vacância alta é menos aluguel e menos rendimento.", Some("vacancy"), false),
    t("inadimplencia", "Inadimplência", "Aluguel ou juros que deviam ser pagos e não foram.", "Corta a renda; num fundo de papel pode virar perda.", Some("default"), false),
    t("ltv", "LTV", "Quanto da garantia está comprometido por dívida. LTV de 50%: a dívida é metade do valor do imóvel em garantia.", "Quanto menor, mais folga se o imóvel perder valor.", Some("ltv"), false),
    t("cri", "CRI", "Certificado de Recebíveis Imobiliários: um título de dívida pago por aluguéis ou financiamentos de imóveis. É o que os FIIs de papel compram.", "O risco é quem deve não pagar.", None, false),
    t("tijolo", "FII de tijolo", "Fundo dono de imóveis (galpões, lajes, shoppings) que vive dos aluguéis.", "O risco está nos inquilinos e na vacância.", None, false),
    t("papel", "FII de papel", "Fundo que compra CRIs e vive dos juros, quase sempre ligados ao CDI ou à inflação.", "O risco está no crédito de quem deve.", None, false),
    t("data_com", "Data-com", "O último dia para ter a cota e receber o próximo rendimento.", "Comprar depois dela só dá direito ao seguinte.", None, true),
    t("amortizacao", "Amortização", "Devolução de parte do capital aos cotistas: não é rendimento, e baixa o seu preço médio.", "Não conte como renda.", None, false),
    t("emissao", "Emissão de cotas", "O fundo cria cotas novas para captar dinheiro. Abaixo do VP, dilui quem já é cotista.", "Olhe o preço da emissão e o direito de preferência.", None, true),
    t("preferencia", "Direito de preferência", "A vez de quem já é cotista de comprar as cotas novas, na proporção do que tem.", "Deixar de exercer pode diluir você.", None, false),
    t("taxa_adm", "Taxa de administração", "O que o fundo paga por ano ao administrador e ao gestor, tirado do patrimônio.", "Taxa alta come o rendimento.", None, false),
    t("fato_relevante", "Fato relevante", "Comunicado obrigatório quando algo pode mexer no preço: venda de imóvel, um inquilino que deixou de pagar, uma aquisição.", "É o primeiro documento a ler.", None, true),
    t("relatorio_gerencial", "Relatório gerencial", "O relatório do mês em que o gestor conta o que aconteceu: resultado, vacância, contratos e planos.", "É onde aparecem os problemas antes do preço.", None, true),
    t("resultado_cota", "Resultado por cota", "O que o fundo ganhou no mês dividido pelas cotas. Distribuir mais do que ganha é gastar a reserva.", "Mostra se o rendimento se sustenta.", None, false),
    t("liquidez", "Liquidez", "Quanto se negocia do ativo por dia. Pouca liquidez é difícil de vender sem baixar o preço.", "Pesa quando você precisar do dinheiro.", None, false),
    t("selic", "Selic", "A taxa básica de juros do Brasil, decidida pelo Banco Central.", "Selic alta deixa a renda fixa mais atraente e costuma pesar no preço dos FIIs.", None, false),
    t("cdi", "CDI", "A taxa que os bancos usam entre si, quase igual à Selic: a régua da renda fixa.", "É o mínimo que um investimento com risco deveria superar.", None, false),
    t("ipca", "IPCA", "A inflação oficial do Brasil.", "Aluguéis e muitos CRIs são corrigidos por ela.", None, false),
    t("ntnb", "Tesouro IPCA+", "Título do governo que paga a inflação mais uma taxa fixa.", "A comparação clássica para o rendimento de um FII.", None, false),
    t("preco_teto", "Preço-teto", "O preço máximo que faz sentido pagar para ter o retorno que você quer.", "O AXYON calcula por Bazin, Graham e Lynch e avisa quando o preço passa por baixo.", None, false),
    t("bazin", "Método Bazin", "Preço-teto = proventos de um ano ÷ 6% (ou o retorno que você exige).", "Simples e focado em renda.", None, false),
    t("graham", "Fórmula de Graham", "Preço justo de uma ação pelo lucro e pelo patrimônio por ação: raiz de 22,5 × LPA × VPA.", "Uma régua de valor para ações lucrativas.", None, false),
    t("roe", "ROE", "Lucro dividido pelo patrimônio: quanto a empresa ganha com o dinheiro dos sócios.", "ROE alto e estável é sinal de um bom negócio.", Some("roe"), false),
    t("margem_ebitda", "Margem EBITDA", "Quanto sobra da receita depois dos custos da operação, antes de juros, impostos e depreciação.", "Mostra a força do negócio em si.", Some("ebitda_margin"), false),
    t("divida_ebitda", "Dívida líquida / EBITDA", "Quantos anos da geração de caixa da operação pagariam a dívida.", "Acima de 3 vezes pede atenção.", Some("net_debt_ebitda"), false),
    t("payout", "Payout", "A parte do lucro paga como dividendos.", "Payout acima de 100% não se sustenta por muito tempo.", Some("payout"), false),
    t("diversificacao", "Diversificação", "Não concentrar em poucos ativos: se um vai mal, os outros seguram.", "É a proteção mais barata que existe.", None, true),
    t("reserva_emergencia", "Reserva de emergência", "Dinheiro num lugar seguro e de resgate na hora, antes de qualquer renda variável.", "Evita vender na pior hora.", None, true),
    t("preco_medio", "Preço médio", "O custo médio de cada cota que você tem, somando todas as compras.", "É a base do imposto quando você vende.", None, true),
    t("ir_fii", "Imposto em FII", "Para pessoa física, os rendimentos de FII costumam ser isentos; o lucro na venda paga 20%, por DARF, até o último dia útil do mês seguinte.", "Esquecer o DARF gera multa e juros.", None, true),
    t("darf", "DARF", "A guia para pagar o imposto de renda sobre ganhos na bolsa.", "O AXYON calcula a sua na auditoria dos extratos da B3.", None, false),
];

pub fn term(key: &str) -> Option<&'static Term> {
    let squash = |x: &str| plain(x).replace(' ', "");
    let k = squash(key);
    if k.is_empty() {
        return None;
    }
    GLOSSARY.iter().find(|t| t.key == key || squash(t.term) == k || squash(t.key) == k).or_else(|| GLOSSARY.iter().find(|t| k.len() >= 3 && squash(t.term).contains(&k)))
}

/// The value of a term's metric in each of the user's assets that has it: "GGRC11 0,93 · GARE11 1,02".
pub fn mine(store: &Store, market: &Market, lib: Option<&Library>, metric: &str, today: Date) -> Option<String> {
    let mut parts = Vec::new();
    for p in store.portfolio.positions.iter().filter(|p| p.quantity > 0.0) {
        let value = match p.class {
            AssetClass::Fii => thesis::fund_metrics(market, lib, &p.ticker, today).get(metric).map(|x| x.0),
            AssetClass::Acao => market.companies.get(&p.ticker).and_then(|c| company::metrics(&c.figures).now.get(metric).copied()),
            _ => None,
        };
        if let Some(v) = value {
            let shown = match metric {
                "p_vp" | "net_debt_ebitda" => grouped(v, 2),
                "dividend" => brl(v),
                _ => pct(v, 1),
            };
            parts.push(format!("{} {shown}", p.ticker));
        }
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// A term in the user's language (Portuguese here) with its value in their assets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermView {
    pub key: String,
    pub term: String,
    pub simple: String,
    pub why: String,
    pub mine: Option<String>,
}

pub fn glossary(store: &Store, market: &Market, lib: Option<&Library>, today: Date) -> Vec<TermView> {
    GLOSSARY
        .iter()
        .map(|t| TermView {
            key: t.key.into(),
            term: t.term.into(),
            simple: t.simple.into(),
            why: t.why.into(),
            mine: t.metric.and_then(|m| mine(store, market, lib, m, today)),
        })
        .collect()
}

/// The flashcards of the user's own assets in the deck `DECK`: every basic term, each term with a value in their
/// assets (its card's back says it, updated as it changes without losing its schedule), and their portfolio's shape.
/// Returns how many were added.
pub fn make_cards(store: &mut Store, market: &Market, lib: Option<&Library>, today: Date) -> usize {
    let mut wanted: Vec<(String, String, String)> = Vec::new();
    for t in GLOSSARY {
        let mine = t.metric.and_then(|m| mine(store, market, lib, m, today));
        if t.basic || mine.is_some() {
            let back = match &mine {
                Some(m) => format!("{}\nNa sua carteira: {m}", t.simple),
                None => t.simple.to_string(),
            };
            wanted.push((format!("inv:{}", t.key), format!("O que é {}?", t.term), back));
        }
    }
    let v = portfolio::value(&store.portfolio, market);
    if let Some(top) = v.holdings.iter().filter(|h| h.value > 0.0).max_by(|a, b| a.weight.total_cmp(&b.weight)) {
        wanted.push(("inv:top".into(), "Qual ativo pesa mais na sua carteira?".into(), format!("{}, com {} do total.", top.ticker, pct(top.weight, 1))));
    }
    for c in v.classes.iter().filter(|c| c.value > 0.0) {
        wanted.push((format!("inv:class:{}", c.class.id()), format!("Quanto da sua carteira está em {}?", c.label), format!("{}{}.", pct(c.weight, 1), c.target.map(|x| format!(" (a sua meta: {})", pct(x, 0))).unwrap_or_default())));
    }
    let mut added = 0;
    for (id, front, back) in wanted {
        match store.cards.iter_mut().find(|c| c.id == id) {
            Some(card) => {
                card.back = back;
            }
            None => {
                store.cards.push(srs::new_card(id, DECK, &front, &back, vec!["axyon".into()], today));
                added += 1;
            }
        }
    }
    added
}

// ── The training portfolio ───────────────────────────────────────────────────

/// A training portfolio started again with `amount` of pretend money.
pub fn paper_reset(store: &mut Store, amount: Option<f64>, today: Date) -> String {
    let start = amount.filter(|a| *a >= 100.0).unwrap_or(PAPER_START);
    store.learn.paper = Paper { started: Some(today), start, cash: start, positions: Vec::new(), trades: Vec::new() };
    format!("Training portfolio started with {} of pretend money: buy and sell at the real prices, nothing real moves.", brl(start))
}

/// A pretend trade at the real price: a buy within the cash, a sale within what is held.
pub fn paper_trade(store: &mut Store, market: &Market, ticker: &str, sell: bool, quantity: f64, today: Date) -> Result<String, String> {
    if store.learn.paper.started.is_none() {
        paper_reset(store, None, today);
    }
    let t = crate::text::ticker(ticker).ok_or("Which asset? Its ticker (\"GGRC11\").")?;
    if quantity <= 0.0 || quantity.fract() != 0.0 {
        return Err("A whole number of shares.".into());
    }
    let price = market.price(&t).ok_or_else(|| format!("AXYON has no price of {t} yet: ask about it first, so its quote is read."))?;
    let paper = &mut store.learn.paper;
    let amount = price * quantity;
    let class = market.funds.contains_key(&t).then_some(AssetClass::Fii).unwrap_or_else(|| crate::import::guess_class(&t).0);
    if sell {
        let held = paper.positions.iter_mut().find(|p| p.ticker == t).filter(|p| p.quantity >= quantity).ok_or_else(|| format!("The training portfolio has not {} of {t}.", grouped(quantity, 0)))?;
        held.quantity -= quantity;
        paper.cash += amount;
        paper.positions.retain(|p| p.quantity > 0.0);
    } else {
        if amount > paper.cash + 1e-6 {
            return Err(format!("Not enough pretend money: {} left, {} needed.", brl(paper.cash), brl(amount)));
        }
        paper.cash -= amount;
        match paper.positions.iter_mut().find(|p| p.ticker == t) {
            Some(p) => {
                p.average_price = (p.average_price * p.quantity + amount) / (p.quantity + quantity);
                p.quantity += quantity;
            }
            None => paper.positions.push(Position { ticker: t.clone(), class, quantity, average_price: price, subtype: String::new(), current_value: None, note: "training".into() }),
        }
    }
    paper.trades.push(Trade {
        date: today,
        ticker: t.clone(),
        kind: if sell { TradeKind::Sell } else { TradeKind::Buy },
        quantity,
        price,
        fees: 0.0,
        class: Some(class),
        time: String::new(),
        source: "training".into(),
        seq: paper.trades.len() as u64 + 1,
        average: None,
    });
    Ok(format!("Training: {} {} of {t} at {} ({}). Pretend cash left {}.", if sell { "sold" } else { "bought" }, grouped(quantity, 0), brl(price), brl(amount), brl(paper.cash)))
}

/// The training portfolio now, against the real one and the CDI over the same days.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperView {
    pub started: Option<Date>,
    pub start: f64,
    pub cash: f64,
    pub value: f64,
    pub return_pct: f64,
    /// The real portfolio's return over the same days (money added taken out), percent.
    pub real_pct: Option<f64>,
    pub cdi_pct: Option<f64>,
    pub positions: Vec<Value>,
}

pub fn paper_view(store: &Store, market: &Market, today: Date) -> PaperView {
    let p = &store.learn.paper;
    let Some(started) = p.started else { return PaperView::default() };
    let mut value = p.cash;
    let mut positions = Vec::new();
    for x in &p.positions {
        let price = market.price(&x.ticker);
        let now = price.map(|pr| pr * x.quantity).unwrap_or(x.average_price * x.quantity);
        value += now;
        positions.push(json!({ "ticker": x.ticker, "quantity": x.quantity, "average": x.average_price, "price": price, "value": now }));
    }
    let days = started.until(today).max(0) as f64;
    let cdi_pct = market.economy.cdi.map(|r| ((1.0 + r / 100.0).powf(days / 365.0) - 1.0) * 100.0);
    let then = store.snapshots.iter().filter(|s| s.date >= started).min_by_key(|s| s.date);
    let now = store.snapshots.iter().max_by_key(|s| s.date);
    let real_pct = match (then, now) {
        (Some(a), Some(b)) if a.value > 0.0 && b.date > a.date => Some((b.value - a.value - (b.cost - a.cost)) / a.value * 100.0),
        _ => None,
    };
    PaperView { started: Some(started), start: p.start, cash: p.cash, value, return_pct: if p.start > 0.0 { (value / p.start - 1.0) * 100.0 } else { 0.0 }, real_pct, cdi_pct, positions }
}

fn paper_for_model(v: &PaperView) -> String {
    if v.started.is_none() {
        return "No training portfolio yet: start one (do paper, action reset) with pretend money, then buy and sell at the real prices.".into();
    }
    let mut out = format!(
        "Training portfolio (pretend money, since {}): worth {} from {} ({}{}), cash {}.",
        v.started.map(|d| d.br()).unwrap_or_default(),
        brl(v.value),
        brl(v.start),
        if v.return_pct >= 0.0 { "+" } else { "" },
        pct(v.return_pct, 2),
        brl(v.cash)
    );
    if let Some(r) = v.real_pct {
        out.push_str(&format!(" The real portfolio over the same days: {}{}.", if r >= 0.0 { "+" } else { "" }, pct(r, 2)));
    }
    if let Some(c) = v.cdi_pct {
        out.push_str(&format!(" The CDI: +{}.", pct(c, 2)));
    }
    for p in &v.positions {
        out.push_str(&format!("\n- {} {} at an average of {}", p["ticker"].as_str().unwrap_or(""), grouped(p["quantity"].as_f64().unwrap_or(0.0), 0), brl(p["average"].as_f64().unwrap_or(0.0))));
    }
    out
}

// ── The personal score ───────────────────────────────────────────────────────

/// The decision journal's score: by how sure the user said they were, how often they were right.
pub fn score(store: &Store, market: &Market, today: Date) -> Value {
    let cal = journal::calibration(&store.decisions, |t| market.price(t), market.economy.cdi, today);
    json!({
        "decisions": store.decisions.len(),
        "judged": cal.judged,
        "right": cal.right,
        "bands": cal.bands.iter().map(|(label, n, right)| json!({ "label": label, "judged": n, "rightPct": right })).collect::<Vec<_>>(),
        "averageReturn": cal.average_return,
        "averageCdi": cal.average_cdi,
    })
}

fn score_for_model(v: &Value) -> String {
    let judged = v["judged"].as_u64().unwrap_or(0);
    if judged == 0 {
        return format!("The personal score needs judged decisions: {} recorded, none reviewed yet. Record decisions with how sure you are (wealth_decision), and review them later.", v["decisions"].as_u64().unwrap_or(0));
    }
    let mut out = format!("Personal score: {} of {judged} decisions judged right.", v["right"].as_u64().unwrap_or(0));
    for b in v["bands"].as_array().into_iter().flatten().filter(|b| b["judged"].as_u64().unwrap_or(0) > 0) {
        out.push_str(&format!("\n- when {} sure: {} decisions, right {}", b["label"].as_str().unwrap_or(""), b["judged"], pct(b["rightPct"].as_f64().unwrap_or(0.0), 0)));
    }
    out
}

// ── The tool ─────────────────────────────────────────────────────────────────

/// wealth_learn: `do` = glossary (one `term`, or the list), apprentice (`on`), cards (make them; or the ones due),
/// paper (show, buy, sell, reset), score.
pub fn tool(store: &mut Store, market: &Market, lib: Option<&Library>, args: &Value, today: Date) -> Result<String, String> {
    let s = |k: &str| args[k].as_str().map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    let what = s("do").map(|x| plain(&x)).unwrap_or_else(|| "glossary".into());
    match what.as_str() {
        "glossary" | "term" => match s("term") {
            Some(k) => {
                let t = term(&k).ok_or_else(|| format!("\"{k}\" is not in AXYON's glossary yet; explain it in plain words yourself."))?;
                let mine = t.metric.and_then(|m| mine(store, market, lib, m, today));
                Ok(format!("{}: {} {}{}", t.term, t.simple, t.why, mine.map(|m| format!(" In the user's assets now: {m}.")).unwrap_or_default()))
            }
            None => Ok(format!("AXYON's glossary (Portuguese): {}.", GLOSSARY.iter().map(|t| t.term).collect::<Vec<_>>().join(", "))),
        },
        "apprentice" => {
            let on = args["on"].as_bool().or_else(|| s("on").map(|x| matches!(plain(&x).as_str(), "true" | "on" | "sim" | "ligar" | "1"))).unwrap_or(!store.learn.apprentice);
            store.learn.apprentice = on;
            Ok(if on { "Apprentice mode on: the committee, the reader and the weekly summary explain in plain words, each term the first time.".into() } else { "Apprentice mode off.".into() })
        }
        "cards" => {
            let added = make_cards(store, market, lib, today);
            let due = srs::due(&store.cards, DECK, today, 10);
            let mut out = format!("{added} new card(s) in the deck \"{DECK}\"; {} due now.", due.len());
            for c in due {
                out.push_str(&format!("\n- [{}] {} → {}", c.id, c.front, c.back.replace('\n', " ")));
            }
            out.push_str("\nGrade them with life_cards (action grade, deck investimentos).");
            Ok(out)
        }
        "paper" => match s("action").map(|a| plain(&a)).unwrap_or_else(|| "show".into()).as_str() {
            "reset" | "start" => Ok(paper_reset(store, args["amount"].as_f64().or_else(|| s("amount").and_then(|a| crate::text::num(&a))), today)),
            a @ ("buy" | "sell" | "comprar" | "vender") => {
                let ticker = s("ticker").ok_or("paper needs the ticker")?;
                let q = args["quantity"].as_f64().or_else(|| s("quantity").and_then(|x| crate::text::num(&x))).ok_or("paper needs the quantity")?;
                paper_trade(store, market, &ticker, matches!(a, "sell" | "vender"), q, today)
            }
            _ => Ok(paper_for_model(&paper_view(store, market, today))),
        },
        "score" => Ok(score_for_model(&score(store, market, today))),
        other => Err(format!("wealth_learn has no \"{other}\": glossary, apprentice, cards, paper or score.")),
    }
}

/// What the app's learning screen shows.
pub fn view(store: &Store, market: &Market, lib: Option<&Library>, today: Date) -> Value {
    let due: Vec<Value> = srs::due(&store.cards, DECK, today, 30).into_iter().map(|c| json!({ "id": c.id, "front": c.front, "back": c.back })).collect();
    json!({
        "apprentice": store.learn.apprentice,
        "glossary": glossary(store, market, lib, today),
        "cards": { "due": due, "total": store.cards.iter().filter(|c| c.deck == DECK).count() },
        "paper": paper_view(store, market, today),
        "score": score(store, market, today),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::fixtures;

    fn today() -> Date {
        Date::new(2026, 10, 21).unwrap()
    }

    #[test]
    fn the_glossary_shows_each_term_in_the_users_own_assets_and_makes_cards() {
        let market = fixtures::market();
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        let dy = glossary(&store, &market, None, today()).into_iter().find(|t| t.key == "dy").unwrap();
        let mine = dy.mine.expect("the user's funds have a yield");
        assert!(mine.contains("GARE11"), "{mine}");
        assert!(term("pvp").is_some() && term("vacância").is_some() && term("xyz").is_none());
        let added = make_cards(&mut store, &market, None, today());
        assert!(added >= 10, "{added}");
        assert_eq!(make_cards(&mut store, &market, None, today()), 0, "made once");
        let card = store.cards.iter().find(|c| c.id == "inv:dy").unwrap();
        assert!(card.back.contains("Na sua carteira:") && card.deck == DECK);
        assert!(store.cards.iter().any(|c| c.id == "inv:data_com"), "a basic term, whatever is held");
        let text = tool(&mut store, &market, None, &json!({"do": "cards"}), today()).unwrap();
        assert!(text.contains("due now") && text.contains("[inv:"), "{text}");
        let one = tool(&mut store, &market, None, &json!({"do": "glossary", "term": "dividend yield"}), today()).unwrap();
        assert!(one.starts_with("Dividend yield (DY):") && one.contains("In the user's assets now"), "{one}");
        assert!(tool(&mut store, &market, None, &json!({"do": "apprentice", "on": true}), today()).unwrap().contains("on"));
        assert!(store.learn.apprentice);
        assert!(tool(&mut store, &market, None, &json!({"do": "nope"}), today()).is_err());
    }

    #[test]
    fn a_training_portfolio_trades_pretend_money_at_real_prices() {
        let market = fixtures::market();
        let mut store = Store { portfolio: fixtures::portfolio(), ..Store::default() };
        assert!(tool(&mut store, &market, None, &json!({"do": "paper"}), today()).unwrap().contains("No training portfolio"));
        tool(&mut store, &market, None, &json!({"do": "paper", "action": "reset", "amount": 5000}), today()).unwrap();
        let price = market.price("GARE11").unwrap();
        let out = tool(&mut store, &market, None, &json!({"do": "paper", "action": "buy", "ticker": "GARE11", "quantity": 100}), today()).unwrap();
        assert!(out.contains("bought 100 of GARE11"), "{out}");
        assert!((store.learn.paper.cash - (5000.0 - 100.0 * price)).abs() < 1e-6);
        assert!(paper_trade(&mut store, &market, "GARE11", false, 1000.0, today()).is_err(), "not enough pretend money");
        assert!(paper_trade(&mut store, &market, "GARE11", true, 200.0, today()).is_err(), "not held");
        assert!(paper_trade(&mut store, &market, "ZZZZ11", false, 1.0, today()).is_err(), "no price");
        paper_trade(&mut store, &market, "GARE11", true, 40.0, today()).unwrap();
        let v = paper_view(&store, &market, today());
        assert!((v.value - 5000.0).abs() < 1e-6, "at the same price, the same money: {v:?}");
        assert_eq!(v.positions.len(), 1);
        assert_eq!(store.portfolio, fixtures::portfolio(), "the real portfolio untouched");
        let text = tool(&mut store, &market, None, &json!({"do": "paper"}), today()).unwrap();
        assert!(text.contains("Training portfolio (pretend money") && text.contains("GARE11 60"), "{text}");
        assert!(tool(&mut store, &market, None, &json!({"do": "score"}), today()).unwrap().contains("needs judged decisions"));
        let app = view(&store, &market, None, today());
        assert_eq!(app["paper"]["positions"][0]["ticker"], "GARE11");
        assert!(app["glossary"].as_array().unwrap().len() >= 30);
    }
}
