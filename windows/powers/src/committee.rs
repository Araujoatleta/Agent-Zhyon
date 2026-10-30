//! AXYON's investment committee: five minds and a chair, on the facts this code checked (evidence.rs: each with its
//! source, dates, formula and status) and on the pages of the documents the reader read (library.rs). The minds are
//! archetypes, never real people: a valuation analyst, a legal and governance specialist, a forensic accountant, a
//! risk manager and a macro strategist. Each says how sure it is by the quality of its evidence, marks each point as a
//! fact, a calculation, a hypothesis, an estimate or a missing datum, cites its source, and names the data that could
//! change its view. The committee gives two opinions apart: the asset's quality (A) and its fit in the user's
//! portfolio (B), so a good asset can still be one not to add more of.
//!
//! Before the verdict is kept, an audit (the brain again, as an auditor) looks for contradictions between the minds,
//! claims their sources do not hold, dates that do not match, absence taken for evidence and data ignored, and revises
//! the minds it must; and this code checks what it can by itself: each cited source exists, each figure is in its
//! source, a vote against rests on evidence and not only on what is missing, a veto meets an objective criterion, and
//! sources that disagree are flagged. The brain writes; the code keeps the score, the vetoes and the verdicts. Nothing
//! is bought or sold: the chair ends with a question for the user.

use serde_json::{json, Value};

use crate::date::Date;
use crate::evidence::{self, Fact};
use crate::library::Library;
use crate::model::{AuditItem, DocRef, Market, MindPoint, MindVote, Opinion, Store, Verdict};
use crate::text::{clip, grouped, plain};

/// One mind of the committee.
pub struct Mind {
    pub id: &'static str,
    /// Its archetype, in Portuguese, for the app.
    pub name_pt: &'static str,
    pub name_en: &'static str,
    /// What it looks at, as the protocol tells it (Portuguese).
    pub role: &'static str,
    pub veto: bool,
}

pub const MINDS: [Mind; 5] = [
    Mind {
        id: "valuation",
        name_pt: "Analista de valuation",
        name_en: "Valuation analyst",
        role: "Analista de valuation: P/VP e desconto patrimonial (com as datas do preço e do VP), dividend yield dos últimos 12 meses e anualizado pelo último rendimento (sem confundir os dois), histórico do VP, o preço-teto do usuário e os dos métodos, e se o retorno se sustenta.",
        veto: false,
    },
    Mind {
        id: "legal",
        name_pt: "Jurídico e governança",
        name_en: "Legal and governance",
        role: "Jurídico e governança: fatos relevantes, emissões e diluição, transações com partes relacionadas, conflitos de interesse, assembleias e suas propostas — só o que os documentos efetivamente trazem. Pode VETAR apenas por um destes critérios, com fonte confirmada: irregularidade_confirmada, litigio_material, emissao_diluitiva, conflito_de_interesses, violacao_do_regulamento.",
        veto: true,
    },
    Mind {
        id: "forensic",
        name_pt: "Contador forense",
        name_en: "Forensic accountant",
        role: "Contador forense: resultado recorrente × ganhos extraordinários, cobertura dos dividendos pelo resultado, fluxo de caixa, reservas e observações dos auditores. Sem esses dados, diga que faltam: ausência de dado contábil não é prejuízo nem fraude.",
        veto: false,
    },
    Mind {
        id: "risk",
        name_pt: "Gestor de risco",
        name_en: "Risk manager",
        role: "Gestor de riscos: concentração contra a META DO USUÁRIO (use os fatos de peso, meta e segmento; se a meta não estiver nos fatos, diga que não foi possível recuperá-la), liquidez, endividamento, obrigações futuras e o cronograma e as fontes para pagá-las (caixa imediato não é o mesmo que recursos futuros previstos), inquilinos e um cenário adverso. Pode VETAR apenas por um destes critérios, com fonte confirmada: concentracao_excessiva (o ativo passaria do dobro da meta ou de 25% da carteira), liquidez_insuficiente, alavancagem_sem_cobertura, inadimplencia_relevante.",
        veto: true,
    },
    Mind {
        id: "macro",
        name_pt: "Estrategista macro",
        name_en: "Macro strategist",
        role: "Estrategista macro: o rendimento contra o CDI líquido de IR, o Tesouro IPCA+ (se estiver nos fatos) e a inflação, em cenários de juros (Focus), considerando impostos e prazos.",
        veto: false,
    },
];

/// The objective criteria a veto may stand on, by mind.
pub const VETO_REASONS: &[(&str, &str)] = &[
    ("legal", "irregularidade_confirmada"),
    ("legal", "litigio_material"),
    ("legal", "emissao_diluitiva"),
    ("legal", "conflito_de_interesses"),
    ("legal", "violacao_do_regulamento"),
    ("risk", "concentracao_excessiva"),
    ("risk", "liquidez_insuficiente"),
    ("risk", "alavancagem_sem_cobertura"),
    ("risk", "inadimplencia_relevante"),
];

pub const ASSET_VIEWS: [&str; 3] = ["positivo", "neutro", "negativo"];
pub const FIT_VIEWS: [&str; 4] = ["aumentar", "manter", "reduzir", "evitar_novo_aporte"];
pub const KINDS: [&str; 5] = ["fato", "calculo", "hipotese", "estimativa", "ausente"];

pub const CHAIR: &str = "O presidente: emite DOIS pareceres independentes — A, a qualidade do ativo (positivo, neutro ou negativo), e B, a adequação à carteira do usuário (aumentar, manter, reduzir ou evitar_novo_aporte), olhando o peso atual, a meta e a concentração; um ativo bom pode não caber na carteira, e votar contra uma compra não é recomendar venda. A contagem de votos não substitui a análise da carteira. Depois, a decisão como uma pergunta ao usuário (nunca uma ordem), o melhor argumento do lado vencido e o que mudaria o quadro.";

pub fn mind(id: &str) -> Option<&'static Mind> {
    MINDS.iter().find(|m| m.id == id)
}

/// The committee's procedure ("json": for the app's committee screen; else for the chat).
pub fn protocol(json: bool, apprentice: bool) -> String {
    let mut out = String::from(
        "Você é o comitê de investimentos do AXYON. Analise a proposta abaixo com cinco mentes e, depois, o presidente. Regras:\n\
         1. Use somente os fatos (F…) e as páginas dos documentos (D1 p.4) fornecidos. Depois de cada ponto, cite a fonte que contém o dado. Um número que não está na fonte citada não pode ser dito; não arredonde valores antes de calcular (R$ 0,083 é R$ 0,083).\n\
         2. Marque cada ponto: \"fato\" (está na fonte), \"calculo\" (conta feita com fontes citadas — mostre a conta), \"hipotese\" (interpretação), \"estimativa\" (projeção) ou \"ausente\" (dado que falta).\n\
         3. Ausência de informação não é evidência negativa: dado ausente não é prejuízo, fraude nem risco confirmado. Liste em \"missing\" o que falta e poderia mudar o parecer.\n\
         4. Respeite as datas e os status dos fatos: \"estimativa\" é projeção, \"divergente\" é incerto (as fontes discordam), \"ausente\" não existe. Não trate caixa imediato como toda a disponibilidade futura.\n\
         5. Confiança de 0 a 100 conforme a qualidade das evidências: fatos confirmados pesam mais que cálculos, e estes mais que estimativas; poucos dados, confiança baixa.\n\
         6. As mentes são arquétipos, nunca pessoas reais. Nada é comprado nem vendido. Escreva tudo em português do Brasil.",
    );
    for m in &MINDS {
        out.push_str(&format!("\n[{}] {}", m.id, m.role));
    }
    out.push_str("\nCada mente: voto sobre a proposta (for, against ou neutral), a justificativa do voto (\"why\"), a confiança, até quatro pontos com fonte e tipo, os dados ausentes e o veto (só jurídico e risco, com \"vetoReason\").");
    out.push_str(&format!("\n[chair] {CHAIR}"));
    if apprentice {
        out.push_str("\nO usuário está aprendendo a investir: palavras simples, frases curtas e cada termo explicado na primeira vez (P/VP: o preço contra o valor patrimonial por cota).");
    }
    if json {
        out.push_str(
            "\nResponda só com este JSON: {\"minds\": [{\"id\": \"valuation\", \"vote\": \"for|against|neutral\", \"confidence\": 70, \"why\": \"…\", \"veto\": false, \"vetoReason\": \"\", \"points\": [{\"text\": \"…\", \"source\": \"F3\", \"kind\": \"fato|calculo|hipotese|estimativa|ausente\"}], \"missing\": [\"…\"]}, … as cinco, nesta ordem: valuation, legal, forensic, risk, macro], \"asset\": {\"view\": \"positivo|neutro|negativo\", \"confidence\": 60, \"why\": \"…\", \"sources\": [\"F5\"]}, \"portfolio\": {\"view\": \"aumentar|manter|reduzir|evitar_novo_aporte\", \"why\": \"…\", \"sources\": [\"F2\"]}, \"chair\": {\"question\": \"…\", \"dissent\": \"…\", \"wouldChange\": \"…\", \"fit\": \"…\"}}",
        );
    } else {
        out.push_str("\nEscreva para o chat: cada mente em duas ou três linhas com o voto, a justificativa e as fontes; depois os pareceres A e B e a síntese do presidente. Depois ofereça registrar a decisão no diário (wealth_decision) quando o usuário decidir, com os motivos e quanta certeza tem.");
    }
    out
}

/// A meeting's brief: the procedure, the proposal, the facts numbered, the documents with their pages; and the facts
/// and documents themselves, which the audit and the verdict get back.
#[derive(Debug, Clone)]
pub struct Brief {
    pub prompt: String,
    pub ticker: String,
    pub proposal: String,
    pub facts: Vec<Fact>,
    pub documents: Vec<DocRef>,
    /// The documents' text as the brief gave it (the audit reads the same pages).
    pub pages: String,
}

pub fn brief_full(store: &Store, market: &Market, lib: Option<&Library>, args: &Value, today: Date, json: bool) -> Result<Brief, String> {
    let s = |k: &str| args[k].as_str().map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    let proposal = s("proposal").ok_or("the committee needs the proposal, in the user's words")?;
    let ticker = s("ticker").and_then(|t| crate::text::ticker(&t)).or_else(|| crate::text::tickers_in(&proposal).into_iter().next()).ok_or("the committee needs the asset's ticker")?;
    let facts = evidence::number(evidence::gather(store, market, lib, &ticker, &proposal, today));
    let (pages, keys) = lib.map(|l| l.for_committee(&ticker, 9_000)).unwrap_or_default();
    let documents: Vec<DocRef> = keys
        .iter()
        .enumerate()
        .filter_map(|(i, k)| {
            let d = lib?.get(k)?;
            Some(DocRef { id: format!("D{}", i + 1), key: d.key.clone(), title: d.title.clone(), kind: d.kind.id().into(), delivered: d.delivered, url: d.url.clone(), pages: d.pages.len() })
        })
        .collect();
    let mut prompt = format!("{}\n\nProposta: {proposal}\nAtivo: {ticker}\nData da reunião: {}\n\nFatos (status · fonte · data de referência):", protocol(json, store.learn.apprentice), today.br());
    for f in &facts {
        prompt.push_str(&format!("\n{}", f.line()));
    }
    if !pages.is_empty() {
        prompt.push_str("\n\nDocumentos lidos pelo leitor do AXYON (cite a página como D1 p.4):");
        prompt.push_str(&pages);
    }
    Ok(Brief { prompt, ticker, proposal, facts, documents, pages })
}

/// The brief's text, the ticker and the documents' keys (the chat's committee and the tests).
pub fn brief(store: &Store, market: &Market, lib: Option<&Library>, args: &Value, today: Date, json: bool) -> Result<(String, String, Vec<String>), String> {
    let b = brief_full(store, market, lib, args, today, json)?;
    Ok((b.prompt, b.ticker, b.documents.into_iter().map(|d| d.key).collect()))
}

// ── Reading the brain's answers ──────────────────────────────────────────────

fn vote_of(x: &Value) -> String {
    match plain(x["vote"].as_str().unwrap_or("")).trim() {
        "for" | "a favor" | "favor" | "favoravel" | "pour" => "for".into(),
        "against" | "contra" | "contrario" | "contre" => "against".into(),
        _ => "neutral".into(),
    }
}

fn kind_of(x: &Value) -> String {
    let k = plain(x["kind"].as_str().unwrap_or(""));
    let k = k.trim();
    if k.starts_with("fato") || k.starts_with("fact") {
        "fato".into()
    } else if k.starts_with("calc") || k.starts_with("conta") {
        "calculo".into()
    } else if k.starts_with("hip") || k.starts_with("interpret") {
        "hipotese".into()
    } else if k.starts_with("estim") || k.starts_with("proje") {
        "estimativa".into()
    } else if k.starts_with("ausen") || k.starts_with("falt") || k.starts_with("missing") {
        "ausente".into()
    } else {
        "hipotese".into()
    }
}

fn strings(v: &Value, most: usize, chars: usize) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|t| clip(t.trim(), chars))).filter(|t| !t.is_empty()).take(most).collect())
        .unwrap_or_default()
}

fn points_of(v: &Value) -> Vec<MindPoint> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let text = p["text"].as_str().or_else(|| p.as_str())?.trim();
                    (!text.is_empty()).then(|| MindPoint { text: clip(text, 400), source: p["source"].as_str().unwrap_or("").trim().to_string(), kind: kind_of(p), verified: None, check: String::new() })
                })
                .take(4)
                .collect()
        })
        .unwrap_or_default()
}

fn mind_of(m: &Mind, x: &Value) -> MindVote {
    MindVote {
        id: m.id.into(),
        vote: vote_of(x),
        confidence: x["confidence"].as_f64().map(|c| c.clamp(0.0, 100.0) as u8).unwrap_or(50),
        veto: m.veto && x["veto"].as_bool().unwrap_or(false),
        veto_reason: if m.veto { x["vetoReason"].as_str().or_else(|| x["veto_reason"].as_str()).map(|r| plain(r).replace(' ', "_")).unwrap_or_default() } else { String::new() },
        why: clip(x["why"].as_str().unwrap_or("").trim(), 400),
        points: points_of(&x["points"]),
        missing: strings(&x["missing"], 6, 200),
        revised: false,
        was: String::new(),
    }
}

/// The five minds of the brain's JSON, in their order: an unknown one dropped, a missing one neutral with nothing said.
pub fn parse_minds(v: &Value) -> Result<Vec<MindVote>, String> {
    let given = v["minds"].as_array().ok_or("The committee's answer has no minds.")?;
    let minds: Vec<MindVote> = MINDS
        .iter()
        .map(|m| match given.iter().find(|x| x["id"].as_str().is_some_and(|id| id.trim().eq_ignore_ascii_case(m.id))) {
            Some(x) => mind_of(m, x),
            None => MindVote { id: m.id.into(), vote: "neutral".into(), confidence: 0, ..MindVote::default() },
        })
        .collect();
    if minds.iter().all(|m| m.points.is_empty()) {
        return Err("The committee's minds said nothing.".into());
    }
    Ok(minds)
}

fn opinion(v: &Value, allowed: &[&str]) -> Opinion {
    let raw = v["view"].as_str().or_else(|| v["action"].as_str()).or_else(|| v["verdict"].as_str()).unwrap_or("");
    let view = plain(raw).trim().replace(' ', "_");
    let view = allowed.iter().find(|a| view == **a || view.starts_with(*a)).map(|a| a.to_string()).unwrap_or_default();
    Opinion {
        view,
        why: clip(v["why"].as_str().unwrap_or("").trim(), 600),
        confidence: v["confidence"].as_f64().map(|c| c.clamp(0.0, 100.0) as u8),
        sources: strings(&v["sources"], 8, 20),
    }
}

// ── What this code checks by itself ─────────────────────────────────────────

/// A figure in a text: its value, how many decimal places it was written with, and whether it was money or a percent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Figure {
    pub value: f64,
    pub places: u32,
    pub money: bool,
    pub percent: bool,
}

/// The figures a text states that are worth checking: money, percents, numbers with decimals, and amounts of a
/// thousand or more; not dates, years, tickers ("GARE11"), sources ("F18", "D1 p.2") or small counts.
pub fn figures_in(text: &str) -> Vec<Figure> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || ((chars[i] == '.' || chars[i] == ',') && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()))) {
            i += 1;
        }
        let token: String = chars[start..i].iter().collect();
        let before = if start > 0 { chars[start - 1] } else { ' ' };
        let after = chars.get(i).copied().unwrap_or(' ');
        // A ticker's digits, a source's number, a date's part.
        if before.is_alphabetic() || before == '/' || after == '/' || before == '-' && start > 1 && chars[start - 2].is_ascii_digit() {
            continue;
        }
        let rest: String = chars[i..chars.len().min(i + 12)].iter().collect::<String>().to_lowercase();
        let lead: String = chars[start.saturating_sub(4)..start].iter().collect::<String>().to_lowercase();
        let percent = rest.trim_start().starts_with('%');
        let money = lead.contains("r$");
        if lead.ends_with("p.") || lead.ends_with("p. ") || lead.ends_with("pág") {
            continue;
        }
        let (value, places) = number(&token);
        let mut value = value;
        let r = rest.trim_start();
        let scale = if r.starts_with("bilh") || r.starts_with("billion") || r.starts_with("bi ") {
            1e9
        } else if r.starts_with("milh") || r.starts_with("million") || r.starts_with("mi ") || r.starts_with("mi)") || r.starts_with("mi.") {
            1e6
        } else if r.starts_with("mil ") || r.starts_with("mil)") || r.starts_with("mil.") || r.starts_with("thousand") {
            1e3
        } else {
            1.0
        };
        value *= scale;
        let year = places == 0 && !percent && !money && scale == 1.0 && (1990.0..=2100.0).contains(&value);
        let significant = percent || money || places > 0 || value >= 1000.0;
        if significant && !year {
            out.push(Figure { value, places: if scale > 1.0 { places + scale.log10() as u32 } else { places }, money, percent });
        }
    }
    out
}

/// "1.234,56" or "155.9" or "12,2": the value and its decimal places, the separators read as a Brazilian writes them
/// unless the text says otherwise.
fn number(token: &str) -> (f64, u32) {
    let (dot, comma) = (token.rfind('.'), token.rfind(','));
    let decimal = match (dot, comma) {
        (Some(d), Some(c)) => Some(d.max(c)),
        (None, Some(c)) => Some(c),
        (Some(d), None) => {
            // "1.234" or "1.234.567": thousands; "155.9", "0.083": a decimal point.
            let groups: Vec<&str> = token.split('.').collect();
            let thousands = groups.len() > 1 && groups[0].len() <= 3 && groups[1..].iter().all(|g| g.len() == 3);
            (!thousands).then_some(d)
        }
        (None, None) => None,
    };
    match decimal {
        Some(at) => {
            let int: String = token[..at].chars().filter(|c| c.is_ascii_digit()).collect();
            let frac: String = token[at + 1..].chars().filter(|c| c.is_ascii_digit()).collect();
            (format!("{}.{}", if int.is_empty() { "0" } else { &int }, frac).parse().unwrap_or(0.0), frac.len() as u32)
        }
        None => (token.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0.0), 0),
    }
}

/// Whether `x`, written with its places, is `y` (rounded to those places, or within half a percent).
fn same(x: &Figure, y: f64) -> bool {
    let scale = 10f64.powi(x.places.min(9) as i32);
    let rounded = (y * scale).round() / scale;
    (x.value - rounded).abs() < 0.5 / scale + 1e-9 || (y != 0.0 && ((x.value - y) / y).abs() <= 0.005)
}

/// A cited source: a fact's number, or a document's number and page.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Cited {
    Fact(usize),
    Doc(usize, Option<usize>),
}

/// "F3", "D1 p.4", "F2; F7", "D2 pág. 3": what a point cites.
fn cited(source: &str) -> Vec<Cited> {
    let s = source.to_uppercase().replace("PÁG", "P").replace("PAG", "P");
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let read = |i: &mut usize| -> Option<usize> {
        let start = *i;
        while *i < chars.len() && chars[*i].is_ascii_digit() {
            *i += 1;
        }
        chars[start..*i].iter().collect::<String>().parse().ok()
    };
    while i < chars.len() {
        let c = chars[i];
        let standalone = i == 0 || !chars[i - 1].is_alphanumeric();
        if (c == 'F' || c == 'D') && standalone && chars.get(i + 1).is_some_and(|x| x.is_ascii_digit()) {
            i += 1;
            let Some(n) = read(&mut i) else { continue };
            if c == 'F' {
                out.push(Cited::Fact(n));
            } else {
                // "p.4", "p 4", ", p. 4" after it.
                let mut j = i;
                while j < chars.len() && (chars[j] == ' ' || chars[j] == ',') {
                    j += 1;
                }
                let mut page = None;
                if chars.get(j) == Some(&'P') {
                    j += 1;
                    while j < chars.len() && (chars[j] == '.' || chars[j] == ' ') {
                        j += 1;
                    }
                    let mut k = j;
                    page = read(&mut k);
                    if page.is_some() {
                        i = k;
                    }
                }
                out.push(Cited::Doc(n, page));
            }
            continue;
        }
        i += 1;
    }
    out
}

fn name(id: &str) -> &str {
    mind(id).map(|m| m.name_pt).unwrap_or(id)
}

fn item(severity: &str, minds: Vec<String>, text: String, sources: Vec<String>) -> AuditItem {
    AuditItem { severity: severity.into(), minds, text, sources, by: "codigo".into() }
}

/// The brain's audit could not be done: said first, with why; this code's checks still stand.
pub fn unaudited(why: &str) -> AuditItem {
    item("material", Vec::new(), format!("A revisão cruzada do auditor não foi feita ({}); valeram só as checagens do código do AXYON. Refaça o comitê para a auditoria completa.", clip(why.trim(), 160)), Vec::new())
}

/// The text a source holds: a fact (its text, formula and value) or a document's page (or all of it).
fn source_text(c: Cited, facts: &[Fact], docs: &[DocRef], lib: Option<&Library>) -> Option<(String, Vec<f64>)> {
    match c {
        Cited::Fact(n) => facts.get(n.checked_sub(1)?).map(|f| (format!("{} {} {}", f.text, f.formula, f.reference), f.value.into_iter().collect())),
        Cited::Doc(n, page) => {
            let d = docs.get(n.checked_sub(1)?)?;
            let doc = lib?.get(&d.key)?;
            match page {
                Some(p) => doc.pages.get(p.checked_sub(1)?).map(|t| (t.clone(), Vec::new())),
                None => Some((doc.pages.join("\n"), Vec::new())),
            }
        }
    }
}

fn holds(figure: &Figure, text: &str, values: &[f64]) -> bool {
    figures_in(text).iter().any(|y| same(figure, y.value)) || values.iter().any(|v| same(figure, *v))
}

/// Each point checked against its sources: the source exists, and each figure it states is in it. Marks the points
/// and returns what was found.
pub fn check_points(minds: &mut [MindVote], facts: &[Fact], docs: &[DocRef], lib: Option<&Library>) -> Vec<AuditItem> {
    let mut found = Vec::new();
    for m in minds.iter_mut() {
        for p in m.points.iter_mut() {
            let refs = cited(&p.source);
            if refs.is_empty() {
                p.verified = Some(p.kind == "ausente" || p.kind == "hipotese");
                if p.kind != "ausente" && p.kind != "hipotese" {
                    p.check = "sem fonte citada".into();
                    found.push(item("observacao", vec![m.id.clone()], format!("{}: \"{}\" não cita fonte.", name(&m.id), clip(&p.text, 90)), Vec::new()));
                }
                continue;
            }
            let texts: Vec<Option<(String, Vec<f64>)>> = refs.iter().map(|c| source_text(*c, facts, docs, lib)).collect();
            if texts.iter().any(Option::is_none) {
                p.verified = Some(false);
                p.check = format!("a fonte {} não existe", p.source);
                found.push(item("material", vec![m.id.clone()], format!("{}: \"{}\" cita {}, que não existe entre as fontes da reunião.", name(&m.id), clip(&p.text, 90), p.source), vec![p.source.clone()]));
                continue;
            }
            let texts: Vec<(String, Vec<f64>)> = texts.into_iter().flatten().collect();
            let mut missing = Vec::new();
            for fig in figures_in(&p.text) {
                if !texts.iter().any(|(t, v)| holds(&fig, t, v)) {
                    missing.push(fig);
                }
            }
            if missing.is_empty() {
                p.verified = Some(true);
                continue;
            }
            p.verified = Some(false);
            let shown: Vec<String> = missing.iter().map(|f| grouped(f.value, f.places.min(4) as usize)).collect();
            // The figure in another fact: a citation to fix; in none: a figure without a source.
            let elsewhere: Vec<String> = missing
                .iter()
                .filter_map(|fig| facts.iter().find(|f| holds(fig, &format!("{} {}", f.text, f.formula), &f.value.into_iter().collect::<Vec<_>>())).map(|f| f.id.clone()))
                .collect();
            if elsewhere.len() == missing.len() {
                p.check = format!("{} está em {}, não em {}", shown.join(", "), elsewhere.join(", "), p.source);
                found.push(item("observacao", vec![m.id.clone()], format!("{}: {} — citação a corrigir.", name(&m.id), p.check), vec![p.source.clone()]));
            } else {
                p.check = format!("{} não aparece em {}", shown.join(", "), p.source);
                let severity = if p.kind == "fato" { "material" } else { "observacao" };
                found.push(item(severity, vec![m.id.clone()], format!("{}: \"{}\" — {}.", name(&m.id), clip(&p.text, 90), p.check), vec![p.source.clone()]));
            }
        }
    }
    found
}

/// A vote against that rests only on what is missing or on hypotheses is counted as neutral: the absence of a datum is
/// not evidence against the asset.
pub fn absence_votes(minds: &mut [MindVote]) -> Vec<AuditItem> {
    let mut found = Vec::new();
    for m in minds.iter_mut().filter(|m| m.vote == "against") {
        let evidence = m.points.iter().any(|p| (p.kind == "fato" || p.kind == "calculo" || p.kind == "estimativa") && p.verified != Some(false));
        if !evidence {
            m.was = "against".into();
            m.vote = "neutral".into();
            found.push(item("material", vec![m.id.clone()], format!("{} votou contra só com dados ausentes, hipóteses ou pontos sem fonte: ausência de informação não é evidência negativa — o voto conta como neutro.", name(&m.id)), Vec::new()));
        }
    }
    found
}

/// A veto stands only on an objective criterion, with a fact or a page that holds: concentration only when the
/// facts show the asset going over twice the user's target or a quarter of the portfolio.
pub fn check_vetoes(minds: &mut [MindVote], facts: &[Fact]) -> Vec<AuditItem> {
    let mut found = Vec::new();
    let after = facts.iter().find(|f| f.topic == "carteira" && f.text.contains(" levaria ")).and_then(|f| f.value);
    let now = facts.iter().find(|f| f.topic == "carteira" && f.text.contains(" pesa ")).and_then(|f| f.value);
    let target = facts.iter().find(|f| f.text.starts_with("Sua meta para")).and_then(|f| f.value);
    for m in minds.iter_mut().filter(|m| m.veto) {
        let allowed = VETO_REASONS.iter().any(|(id, r)| *id == m.id && *r == m.veto_reason);
        let backed = m.points.iter().any(|p| (p.kind == "fato" || p.kind == "calculo") && p.verified == Some(true));
        let why = if !allowed {
            Some(format!("sem um critério objetivo de veto (\"{}\")", m.veto_reason))
        } else if !backed {
            Some("sem fonte confirmada que o sustente".to_string())
        } else if m.veto_reason == "concentracao_excessiva" {
            let weight = after.or(now).unwrap_or(0.0);
            let limit = target.map(|t| (2.0 * t).min(25.0)).unwrap_or(25.0);
            (weight <= limit).then(|| format!("a concentração ({}) não passa do limite ({})", crate::text::pct(weight, 1), crate::text::pct(limit, 1)))
        } else {
            None
        };
        if let Some(why) = why {
            m.veto = false;
            found.push(item("material", vec![m.id.clone()], format!("Veto de {} desconsiderado: {why}.", name(&m.id)), Vec::new()));
        }
    }
    found
}

/// What the facts themselves call for: sources that disagree, dates far apart in what the minds cited, a target that
/// could not be found, or one a mind says it does not know while the facts give it.
fn fact_findings(facts: &[Fact], minds: &[MindVote]) -> (Vec<AuditItem>, Vec<String>) {
    let mut found = Vec::new();
    let mut verify = Vec::new();
    let cited_ids: Vec<usize> = minds.iter().flat_map(|m| m.points.iter().flat_map(|p| cited(&p.source))).filter_map(|c| if let Cited::Fact(n) = c { Some(n) } else { None }).collect();
    for (i, f) in facts.iter().enumerate() {
        if f.status == evidence::DIVERGENT {
            found.push(item("material", Vec::new(), format!("Fontes divergentes em {}: {}", f.id, f.text), vec![f.id.clone()]));
            verify.push(format!("Conferir a divergência de {}: {}", f.id, clip(&f.text, 140)));
        }
        if !f.warning.is_empty() && cited_ids.contains(&(i + 1)) {
            found.push(item("observacao", Vec::new(), format!("{} foi citado, mas atenção: {}.", f.id, f.warning), vec![f.id.clone()]));
        }
        if f.status == evidence::MISSING && f.text.starts_with("Meta por ativo") {
            verify.push("Cadastrar a meta deste ativo (coluna \"Percentual Ideal\" ou \"Meta\" na planilha, ou no app) para o parecer da carteira.".into());
        }
    }
    if let Some(target) = facts.iter().find(|f| f.topic == "carteira" && f.text.starts_with("Sua meta para")) {
        for m in minds {
            let unknown = m.points.iter().any(|p| p.kind == "ausente" && plain(&p.text).contains("meta")) || m.missing.iter().any(|x| plain(x).contains("meta"));
            if unknown {
                found.push(item("material", vec![m.id.clone()], format!("{} diz que a meta do ativo não está disponível, mas ela está em {}: {}", name(&m.id), target.id, target.text), vec![target.id.clone()]));
            }
        }
    }
    (found, verify)
}

fn audit_items(v: &Value) -> Vec<AuditItem> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let text = clip(x["text"].as_str()?.trim(), 400);
                    let severity = match plain(x["severity"].as_str().unwrap_or("")).trim() {
                        s if s.starts_with("crit") => "critico",
                        s if s.starts_with("mat") => "material",
                        _ => "observacao",
                    };
                    (!text.is_empty()).then(|| AuditItem { severity: severity.into(), minds: strings(&x["minds"], 5, 20), text, sources: strings(&x["sources"], 6, 20), by: "auditor".into() })
                })
                .take(12)
                .collect()
        })
        .unwrap_or_default()
}

/// The audit's brief: the proposal, the facts, the documents' pages, what each mind said and what this code already
/// found; the auditor looks for what is wrong, revises the minds it must, and writes the final opinions.
pub fn audit_brief(minds_answer: &Value, facts: &[Fact], pages: &str, findings: &[AuditItem], ticker: &str, proposal: &str, apprentice: bool) -> String {
    let mut out = String::from(
        "Você é o auditor do comitê de investimentos do AXYON. Revise os pareceres abaixo antes de serem publicados:\n\
         1. Contradições entre as mentes (uma diz que falta caixa, outra cita os recursos previstos; uma conta o DY anualizado como realizado…).\n\
         2. Se cada afirmação corresponde à fonte citada (o código já conferiu os números: veja os achados).\n\
         3. Indicadores calculados com datas incompatíveis.\n\
         4. Ausência de informação tratada como evidência negativa.\n\
         5. Conclusões que ignoram dados relevantes dos fatos ou dos documentos (por exemplo, uma obrigação de pagamento sem considerar as fontes de recursos previstas, ou caixa imediato confundido com disponibilidade futura).\n\
         6. Fontes com status \"divergente\".\n\
         Para cada problema: severidade \"critico\" (muda a conclusão), \"material\" (exige revisão) ou \"observacao\". Revise as mentes afetadas por problemas críticos ou materiais (voto, confiança, justificativa, pontos com fonte e tipo, dados ausentes), sempre com as mesmas regras de fontes. Depois escreva os pareceres A (ativo) e B (carteira) e a síntese do presidente FINAIS, já considerando a auditoria, e a lista do que o usuário precisa verificar. A contagem de votos não substitui a análise da carteira; votar contra uma compra não é recomendar venda. Tudo em português do Brasil.",
    );
    if apprentice {
        out.push_str("\nO usuário está aprendendo: palavras simples.");
    }
    out.push_str(&format!("\n\nProposta: {proposal}\nAtivo: {ticker}\n\nFatos:"));
    for f in facts {
        out.push_str(&format!("\n{}", f.line()));
    }
    if !pages.is_empty() {
        out.push_str("\n\nDocumentos:");
        out.push_str(pages);
    }
    out.push_str(&format!("\n\nPareceres das cinco mentes:\n{}", serde_json::to_string(&minds_answer["minds"]).unwrap_or_default()));
    if let Some(chair) = minds_answer.get("chair") {
        out.push_str(&format!("\nRascunho do presidente: {}", serde_json::to_string(&json!({ "asset": minds_answer["asset"], "portfolio": minds_answer["portfolio"], "chair": chair })).unwrap_or_default()));
    }
    out.push_str("\n\nO que o código do AXYON já encontrou:");
    if findings.is_empty() {
        out.push_str("\n- nada");
    }
    for f in findings {
        out.push_str(&format!("\n- [{}] {}", f.severity, f.text));
    }
    out.push_str(
        "\n\nResponda só com este JSON: {\"audit\": [{\"severity\": \"critico|material|observacao\", \"minds\": [\"risk\"], \"text\": \"…\", \"sources\": [\"F12\"]}], \"revisions\": [{\"id\": \"risk\", \"vote\": \"for|against|neutral\", \"confidence\": 55, \"why\": \"…\", \"veto\": false, \"vetoReason\": \"\", \"points\": [{\"text\": \"…\", \"source\": \"F3\", \"kind\": \"fato|calculo|hipotese|estimativa|ausente\"}], \"missing\": [\"…\"]}], \"asset\": {\"view\": \"positivo|neutro|negativo\", \"confidence\": 60, \"why\": \"…\", \"sources\": [\"F5\"]}, \"portfolio\": {\"view\": \"aumentar|manter|reduzir|evitar_novo_aporte\", \"why\": \"…\", \"sources\": [\"F2\"]}, \"chair\": {\"question\": \"…\", \"dissent\": \"…\", \"wouldChange\": \"…\", \"fit\": \"…\"}, \"toVerify\": [\"…\"]}",
    );
    out
}

/// What this code finds on the minds' first answer, before the audit (the auditor reads it).
pub fn precheck(minds_answer: &Value, facts: &[Fact], docs: &[DocRef], lib: Option<&Library>) -> Result<Vec<AuditItem>, String> {
    let mut minds = parse_minds(minds_answer)?;
    let mut found = check_points(&mut minds, facts, docs, lib);
    found.extend(absence_votes(&mut minds));
    found.extend(check_vetoes(&mut minds, facts));
    found.extend(fact_findings(facts, &minds).0);
    Ok(found)
}

/// The verdict: the minds (revised by the audit where it did), checked by this code, the two opinions, the chair, the
/// audit's findings and this code's, what to verify, and the facts and documents as they were cited.
#[allow(clippy::too_many_arguments)]
pub fn finalize(minds_answer: &Value, audit_answer: Option<&Value>, facts: Vec<Fact>, documents: Vec<DocRef>, lib: Option<&Library>, ticker: &str, proposal: &str, reason: &str, document: &str, today: Date) -> Result<Verdict, String> {
    let mut minds = parse_minds(minds_answer)?;
    if let Some(a) = audit_answer {
        for r in a["revisions"].as_array().into_iter().flatten() {
            let Some(id) = r["id"].as_str() else { continue };
            let Some(spec) = mind(id.trim()) else { continue };
            if let Some(m) = minds.iter_mut().find(|m| m.id == spec.id) {
                let before = m.vote.clone();
                let mut revised = mind_of(spec, r);
                if revised.points.is_empty() {
                    revised.points = m.points.clone();
                }
                revised.revised = true;
                if revised.vote != before {
                    revised.was = before;
                }
                *m = revised;
            }
        }
    }
    let checked = !facts.is_empty() || !documents.is_empty();
    let mut audit = audit_answer.map(|a| audit_items(&a["audit"])).unwrap_or_default();
    if checked {
        audit.extend(check_points(&mut minds, &facts, &documents, lib));
    }
    audit.extend(absence_votes(&mut minds));
    audit.extend(check_vetoes(&mut minds, &facts));
    let (from_facts, mut to_verify) = fact_findings(&facts, &minds);
    audit.extend(from_facts);
    let source = audit_answer.filter(|a| a.get("chair").is_some()).unwrap_or(minds_answer);
    let asset = opinion(&source["asset"], &ASSET_VIEWS);
    let portfolio_fit = opinion(&source["portfolio"], &FIT_VIEWS);
    // Opinion B against the facts: adding more of an asset already above its target is said.
    let above = facts.iter().find(|f| f.text.contains("da meta (") && f.topic == "carteira").and_then(|f| f.value.map(|v| (v, f.id.clone())));
    if let Some((gap, id)) = above {
        if gap > 0.0 && portfolio_fit.view == "aumentar" {
            audit.push(item("material", Vec::new(), format!("O parecer B propõe aumentar {ticker}, que já está {} p.p. acima da sua meta ({id}).", grouped(gap, 1)), vec![id]));
        }
    }
    for v in audit_answer.map(|a| strings(&a["toVerify"], 8, 240)).unwrap_or_default() {
        if !to_verify.contains(&v) {
            to_verify.push(v);
        }
    }
    let chair = &source["chair"];
    let said = |k: &str| clip(chair[k].as_str().unwrap_or("").trim(), 600);
    let points = minds.iter().map(|m| m.points.len()).sum::<usize>();
    Ok(Verdict {
        id: format!("{}-{}-{}", ticker, today, (proposal.len() + points) % 1000),
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
        version: 2,
        asset,
        portfolio_fit,
        audit,
        to_verify,
        facts,
        documents,
    })
}

/// The brain's single answer as a verdict, without the facts to check it against (the first committee's form).
pub fn parse_verdict(v: &Value, ticker: &str, proposal: &str, reason: &str, document: &str, today: Date) -> Result<Verdict, String> {
    finalize(v, None, Vec::new(), Vec::new(), None, ticker, proposal, reason, document, today)
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
    if !v.asset.view.is_empty() {
        out.push_str(&format!("\nOpinion A (the asset): {} — {}", v.asset.view, clip(&v.asset.why, 160)));
    }
    if !v.portfolio_fit.view.is_empty() {
        out.push_str(&format!("\nOpinion B (the portfolio): {} — {}", v.portfolio_fit.view, clip(&v.portfolio_fit.why, 160)));
    }
    for m in &v.minds {
        if let Some(p) = m.points.first() {
            out.push_str(&format!("\n- {} ({}, {}%): {}{}", m.id, m.vote, m.confidence, p.text, if p.source.is_empty() { String::new() } else { format!(" [{}]", p.source) }));
        }
    }
    let serious = v.audit.iter().filter(|x| x.severity != "observacao").count();
    if serious > 0 {
        out.push_str(&format!("\nThe audit found {serious} point(s) to correct or check."));
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
    use crate::evidence::fixtures::{day, gare11};

    fn minds_with(points: Value) -> Value {
        let mut minds: Vec<Value> = MINDS.iter().map(|m| json!({"id": m.id, "vote": "neutral", "confidence": 50, "why": "…", "points": [{"text": "Sem dados contábeis no material.", "source": "", "kind": "ausente"}]})).collect();
        if let Some(obj) = points.as_object() {
            for (id, v) in obj {
                if let Some(m) = minds.iter_mut().find(|m| m["id"] == json!(id)) {
                    *m = v.clone();
                    m["id"] = json!(id);
                }
            }
        }
        json!({"minds": minds, "asset": {"view": "positivo", "why": "Desconto e rendimento", "sources": ["F12"]}, "portfolio": {"view": "evitar novo aporte", "why": "Acima da meta"}, "chair": {"question": "Manter sem aportar?", "dissent": "Desconto", "wouldChange": "Peso abaixo da meta", "fit": "19,5% contra 7,5%"}})
    }

    fn meeting() -> (Store, Market, Brief) {
        let (store, market) = gare11();
        let b = brief_full(&store, &market, None, &json!({"proposal": "comprar 100 cotas de GARE11"}), day(2026, 10, 30), true).unwrap();
        (store, market, b)
    }

    fn id_of(b: &Brief, words: &str) -> String {
        b.facts.iter().find(|f| f.text.contains(words)).map(|f| f.id.clone()).unwrap_or_else(|| panic!("{words}"))
    }

    #[test]
    fn the_brief_is_in_portuguese_with_dated_facts_and_the_two_opinions() {
        let (_, _, b) = meeting();
        assert_eq!(b.ticker, "GARE11");
        for m in &MINDS {
            assert!(b.prompt.contains(&format!("[{}]", m.id)));
        }
        assert!(b.prompt.contains("português do Brasil") && b.prompt.contains("Parecer") || b.prompt.contains("DOIS pareceres"));
        assert!(b.prompt.contains("\"portfolio\": {\"view\": \"aumentar|manter|reduzir|evitar_novo_aporte\""));
        assert!(b.prompt.contains("Ausência de informação não é evidência negativa"));
        let pvp = id_of(&b, "P/VP de GARE11");
        assert!(b.prompt.contains(&format!("{pvp} [calculado · Preço e CVM — Informe Mensal · ref. preço 29/10/2026; VP 09/2026] P/VP de GARE11: 0,904.")), "{}", b.prompt);
        assert!(b.prompt.contains("R$ 0,083 por cota"));
        let (chat, _, _) = brief(&gare11().0, &gare11().1, None, &json!({"proposal": "vender GARE11"}), day(2026, 10, 30), false).unwrap();
        assert!(chat.contains("wealth_decision") && !chat.contains("\"minds\""));
        assert!(brief(&gare11().0, &gare11().1, None, &json!({"proposal": "comprar algo"}), day(2026, 10, 30), true).is_err());
    }

    #[test]
    fn the_figures_of_a_text_are_read_as_written() {
        let f = |t: &str| figures_in(t).iter().map(|x| (x.value, x.places)).collect::<Vec<_>>();
        assert_eq!(f("DY de 12,24% e rendimento de R$ 0,083 (F18, D1 p.2) em 30/10/2026; GARE11 tem 160.000 cotistas"), [(12.24, 2), (0.083, 3), (160_000.0, 0)]);
        assert_eq!(f("payment obligation of R$155.9 million and 0.8% of net assets in 2027"), [(155_900_000.0, 7), (0.8, 1)]);
        assert_eq!(f("5 mentes, 3 votos e 12 meses"), Vec::<(f64, u32)>::new(), "small counts are not checked");
        assert_eq!(cited("F2; F7"), [Cited::Fact(2), Cited::Fact(7)]);
        assert_eq!(cited("D1 p.4"), [Cited::Doc(1, Some(4))]);
        assert_eq!(cited("D2, pág. 3 e F5"), [Cited::Doc(2, Some(3)), Cited::Fact(5)]);
    }

    #[test]
    fn a_point_whose_figure_is_not_in_its_source_is_flagged_and_a_source_that_does_not_exist_too() {
        let (_, _, b) = meeting();
        let pvp = id_of(&b, "P/VP de GARE11");
        let dy = id_of(&b, "DY dos últimos 12 meses");
        let answer = minds_with(json!({
            "valuation": {"vote": "for", "confidence": 70, "why": "Desconto", "points": [
                {"text": "P/VP de 0,90 com desconto", "source": pvp, "kind": "fato"},
                {"text": "DY de 12,2% em 12 meses", "source": pvp, "kind": "fato"},
                {"text": "Vacância de 7,6% no setor", "source": "F999", "kind": "fato"}
            ]}
        }));
        let mut minds = parse_minds(&answer).unwrap();
        let found = check_points(&mut minds, &b.facts, &b.documents, None);
        let v = &minds[0].points;
        assert_eq!(v[0].verified, Some(true), "0,90 is the P/VP 0,904 written with two places");
        assert_eq!(v[1].verified, Some(false));
        assert!(v[1].check.contains(&format!("está em {dy}")), "the figure is in another fact: {}", v[1].check);
        assert_eq!(v[2].verified, Some(false));
        assert!(found.iter().any(|x| x.severity == "material" && x.text.contains("F999")), "{found:#?}");
    }

    #[test]
    fn a_vote_against_on_missing_data_is_neutral_and_a_veto_needs_its_criterion() {
        let (_, _, b) = meeting();
        let weight = format!("{}; {}", id_of(&b, "pesa "), id_of(&b, "Sua meta para"));
        let answer = minds_with(json!({
            "forensic": {"vote": "against", "confidence": 60, "why": "Sem dados contábeis", "points": [{"text": "Não há demonstrações de resultado", "source": "", "kind": "ausente"}]},
            "legal": {"vote": "against", "confidence": 70, "veto": true, "vetoReason": "nao gostei", "points": [{"text": "Fato relevante de aquisição", "source": "", "kind": "hipotese"}]},
            "risk": {"vote": "against", "confidence": 80, "veto": true, "vetoReason": "concentracao_excessiva", "points": [{"text": "GARE11 pesa 19,5% da carteira, contra a meta de 7,5%", "source": weight, "kind": "fato"}]}
        }));
        let v = finalize(&answer, None, b.facts.clone(), b.documents.clone(), None, "GARE11", "comprar 100 cotas de GARE11", "asked", "", day(2026, 10, 30)).unwrap();
        let forensic = v.minds.iter().find(|m| m.id == "forensic").unwrap();
        assert_eq!((forensic.vote.as_str(), forensic.was.as_str()), ("neutral", "against"), "absence of data is not a vote against");
        assert!(v.audit.iter().any(|x| x.text.contains("ausência de informação não é evidência negativa")));
        let legal = v.minds.iter().find(|m| m.id == "legal").unwrap();
        assert!(!legal.veto, "no objective criterion");
        let risk = v.minds.iter().find(|m| m.id == "risk").unwrap();
        // 204 cotas at R$ 8,14 plus 100 more: well over twice the 7,5 % target.
        assert!(risk.veto, "the concentration criterion holds on the facts: {:#?}", v.audit);
        assert_eq!(v.vetoed_by(), ["risk"]);
    }

    #[test]
    fn the_asset_can_be_good_and_still_not_one_to_add_more_of() {
        let (_, _, b) = meeting();
        let answer = minds_with(json!({}));
        let v = finalize(&answer, None, b.facts.clone(), b.documents.clone(), None, "GARE11", "comprar 100 cotas de GARE11", "asked", "", day(2026, 10, 30)).unwrap();
        assert_eq!(v.version, 2);
        assert_eq!((v.asset.view.as_str(), v.portfolio_fit.view.as_str()), ("positivo", "evitar_novo_aporte"));
        assert!(!v.audit.iter().any(|x| x.text.contains("parecer B propõe aumentar")));
        // Opinion B saying to add more of an asset above its target: said by the code.
        let mut wrong = minds_with(json!({}));
        wrong["portfolio"] = json!({"view": "aumentar", "why": "Barato"});
        let v = finalize(&wrong, None, b.facts.clone(), b.documents.clone(), None, "GARE11", "comprar 100 cotas de GARE11", "asked", "", day(2026, 10, 30)).unwrap();
        assert!(v.audit.iter().any(|x| x.severity == "material" && x.text.contains("propõe aumentar GARE11")), "{:#?}", v.audit);
    }

    #[test]
    fn the_audit_revises_a_mind_and_writes_the_final_opinions() {
        let (_, _, b) = meeting();
        let cash = id_of(&b, "Caixa e ativos líquidos são");
        let answer = minds_with(json!({
            "risk": {"vote": "against", "confidence": 75, "why": "Caixa baixo", "points": [{"text": "Caixa de 0,80% do patrimônio para a obrigação", "source": cash, "kind": "fato"}]}
        }));
        let findings = precheck(&answer, &b.facts, &b.documents, None).unwrap();
        let prompt = audit_brief(&answer, &b.facts, &b.pages, &findings, "GARE11", "comprar 100 cotas de GARE11", false);
        assert!(prompt.contains("auditor") && prompt.contains("caixa imediato confundido com disponibilidade futura") && prompt.contains("\"revisions\""));
        let audit = json!({
            "audit": [{"severity": "material", "minds": ["risk"], "text": "O risco tratou o caixa imediato como toda a disponibilidade: o fato relevante cita recursos de uma emissão.", "sources": [cash]}],
            "revisions": [{"id": "risk", "vote": "neutral", "confidence": 55, "why": "A obrigação tem fonte prevista, a confirmar", "points": [{"text": "Caixa imediato de 0,80% do patrimônio", "source": cash, "kind": "fato"}], "missing": ["cronograma de pagamento"]}],
            "asset": {"view": "neutro", "confidence": 55, "why": "Bom desconto, obrigação a acompanhar"},
            "portfolio": {"view": "manter", "why": "Acima da meta"},
            "chair": {"question": "Manter sem novos aportes até a obrigação ser paga?", "dissent": "O desconto", "wouldChange": "A emissão concluída", "fit": "19,5% contra 7,5%"},
            "toVerify": ["O cronograma da obrigação de R$ 155,9 milhões"]
        });
        let v = finalize(&answer, Some(&audit), b.facts.clone(), b.documents.clone(), None, "GARE11", "comprar 100 cotas de GARE11", "asked", "", day(2026, 10, 30)).unwrap();
        let risk = v.minds.iter().find(|m| m.id == "risk").unwrap();
        assert!(risk.revised && risk.vote == "neutral" && risk.was == "against");
        assert_eq!(risk.points[0].verified, Some(true), "0,80% is the 0,80 % of the cash fact");
        assert_eq!(v.asset.view, "neutro");
        assert_eq!(v.portfolio_fit.view, "manter");
        assert!(v.question.starts_with("Manter sem novos aportes"));
        assert!(v.audit.iter().any(|x| x.by == "auditor" && x.severity == "material"));
        assert!(v.to_verify.iter().any(|x| x.contains("cronograma")));
        assert!(!v.facts.is_empty(), "the facts stay with the verdict, for the app to open them");
    }

    #[test]
    fn a_first_committee_answer_still_makes_a_verdict() {
        let answer = json!({
            "minds": [
                {"id": "valuation", "vote": "for", "confidence": 75, "points": [{"text": "P/VP 0,93", "source": "F4"}]},
                {"id": "legal", "vote": "against", "confidence": 80, "veto": true, "vetoReason": "emissao_diluitiva", "points": [{"text": "Emissão dilui", "source": "D1 p.2", "kind": "fato"}]},
                {"id": "forensic", "vote": "a favor", "confidence": 60, "points": ["Resultado cobre a distribuição"]},
                {"id": "risk", "vote": "contra", "confidence": 140, "points": [{"text": "Concentração", "source": "F2", "kind": "fato"}]},
                {"id": "someone", "vote": "for", "points": [{"text": "x"}]}
            ],
            "chair": {"question": "Vale esperar a assembleia?", "dissent": "O preço está bom", "wouldChange": "A emissão acima do VP", "fit": "Acima da meta de FIIs"}
        });
        let v = parse_verdict(&answer, "GGRC11", "comprar 50 cotas", "user", "", day(2026, 10, 21)).unwrap();
        assert_eq!(v.minds.len(), 5);
        assert_eq!(v.minds[3].confidence, 100);
        assert_eq!(v.minds[2].points[0].text, "Resultado cobre a distribuição");
        assert_eq!(v.question, "Vale esperar a assembleia?");
        let mut store = Store::default();
        keep(&mut store, v.clone());
        keep(&mut store, v);
        assert_eq!(store.verdicts.len(), 1);
        assert!(for_model(latest(&store, "GGRC11").unwrap()).contains("The chair asks: Vale esperar"));
        assert!(parse_verdict(&json!({"minds": []}), "GGRC11", "", "user", "", day(2026, 10, 21)).is_err());
        // A verdict kept by the first committee (its points as pairs) still reads.
        let old: Verdict = serde_json::from_value(json!({"ticker": "GGRC11", "minds": [{"id": "risk", "vote": "against", "points": [["Concentração", "F2"]]}]})).unwrap();
        assert_eq!(old.minds[0].points[0].source, "F2");
    }

    // ── The review's acceptance criteria, on GARE11 and GGRC11 as regression cases. None fixes how the minds vote:
    // each checks what this code guarantees whatever they answer.

    /// GGRC11's meeting with its manager's report read as D1: vacancy 8,5 % and R$ 0,080 a share on its page 2.
    fn ggrc11_meeting() -> (Library, Brief) {
        let (store, market) = gare11();
        let mut lib = Library::default();
        let pending = crate::library::Pending {
            key: "fnet:904".into(),
            ticker: "GGRC11".into(),
            kind: crate::library::DocKind::Report,
            title: "Relatório Gerencial 09/2026".into(),
            reference: "09/2026".into(),
            delivered: Some(day(2026, 10, 10)),
            url: "https://fnet.bmfbovespa.com.br/fnet/publico/exibirDocumento?id=904".into(),
            download: String::new(),
        };
        lib.ingest(&pending, crate::library::fixtures::report_pages("8,5", "0,080"), day(2026, 10, 11));
        let b = brief_full(&store, &market, Some(&lib), &json!({"proposal": "comprar 50 cotas de GGRC11"}), day(2026, 10, 30), true).unwrap();
        (lib, b)
    }

    #[test]
    fn aceite_1_o_rendimento_de_r_0_083_nao_e_arredondado_antes_do_dy() {
        let (_, _, b) = meeting();
        let annual = b.facts.iter().find(|f| f.text.contains("DY anualizado pelo último rendimento")).unwrap();
        assert!(annual.formula.contains("R$ 0,083 × 12"), "{}", annual.formula);
        assert!((annual.value.unwrap() - 0.083 * 12.0 / 8.14 * 100.0).abs() < 1e-9, "from R$ 0,083, never R$ 0,08");
        assert!(b.prompt.contains("R$ 0,083 por cota"), "the brain reads it as written");
    }

    #[test]
    fn aceite_2_o_pvp_usa_preco_e_vp_com_datas_identificadas() {
        for (b, ticker) in [(meeting().2, "GARE11"), (ggrc11_meeting().1, "GGRC11")] {
            let pvp = b.facts.iter().find(|f| f.text.starts_with(&format!("P/VP de {ticker}"))).unwrap_or_else(|| panic!("{ticker}"));
            assert!(pvp.formula.contains("preço R$") && pvp.formula.contains("VP R$"), "{}", pvp.formula);
            assert!(pvp.reference.starts_with("preço ") && pvp.reference.contains("; VP "), "both dates: {}", pvp.reference);
            assert!(b.prompt.contains(&pvp.line()), "the minds see both dates");
        }
    }

    #[test]
    fn aceite_3_a_meta_de_7_5_vem_da_carteira_e_a_mente_de_risco_nao_a_ignora() {
        let (_, _, b) = meeting();
        let target = b.facts.iter().find(|f| f.text.starts_with("Sua meta para GARE11")).unwrap();
        assert_eq!(target.value, Some(7.5));
        assert!(b.prompt.contains("Sua meta para GARE11: 7,5% da carteira."));
        let answer = minds_with(json!({
            "risk": {"vote": "neutral", "confidence": 50, "why": "…", "points": [{"text": "A meta da carteira para o ativo não foi informada", "source": "", "kind": "ausente"}], "missing": ["meta por ativo"]}
        }));
        let v = finalize(&answer, None, b.facts.clone(), b.documents.clone(), None, "GARE11", "comprar 100 cotas de GARE11", "asked", "", day(2026, 10, 30)).unwrap();
        assert!(v.audit.iter().any(|x| x.severity == "material" && x.minds == ["risk"] && x.text.contains(&target.id)), "{:#?}", v.audit);
        // GGRC11 has no target: said missing, to verify, never invented.
        let (_, b) = ggrc11_meeting();
        assert!(b.facts.iter().any(|f| f.status == evidence::MISSING && f.text.starts_with("Meta por ativo para GGRC11")));
        let v = finalize(&minds_with(json!({})), None, b.facts.clone(), b.documents.clone(), None, "GGRC11", "comprar 50 cotas de GGRC11", "asked", "", day(2026, 10, 30)).unwrap();
        assert!(v.to_verify.iter().any(|x| x.starts_with("Cadastrar a meta")));
    }

    #[test]
    fn aceite_4_um_ativo_acima_da_meta_pode_ser_bom_e_nao_receber_novo_aporte() {
        let (_, _, b) = meeting();
        let above = b.facts.iter().find(|f| f.text.contains("acima da meta")).unwrap();
        assert!(above.value.unwrap() > 0.0, "GARE11 is above its target");
        let v = finalize(&minds_with(json!({})), None, b.facts.clone(), b.documents.clone(), None, "GARE11", "comprar 100 cotas de GARE11", "asked", "", day(2026, 10, 30)).unwrap();
        assert_eq!((v.asset.view.as_str(), v.portfolio_fit.view.as_str()), ("positivo", "evitar_novo_aporte"), "two opinions, kept apart");
        assert!(b.prompt.contains("votar contra uma compra não é recomendar venda"), "{}", b.prompt);
    }

    #[test]
    fn aceite_5_falta_de_dado_contabil_nao_e_prejuizo_nem_fraude() {
        let (_, b) = ggrc11_meeting();
        let answer = minds_with(json!({
            "forensic": {"vote": "against", "confidence": 70, "why": "Não há DRE nem parecer do auditor", "points": [{"text": "Demonstrações financeiras não disponíveis", "source": "", "kind": "ausente"}, {"text": "Pode haver resultado não recorrente", "source": "", "kind": "hipotese"}]}
        }));
        let v = finalize(&answer, None, b.facts.clone(), b.documents.clone(), None, "GGRC11", "comprar 50 cotas de GGRC11", "asked", "", day(2026, 10, 30)).unwrap();
        let forensic = v.minds.iter().find(|m| m.id == "forensic").unwrap();
        assert_eq!(forensic.vote, "neutral", "a vote on missing data counts as neutral");
        assert_eq!(forensic.was, "against");
        assert!(b.prompt.contains("ausência de dado contábil não é prejuízo nem fraude"), "the forensic mind is told so");
    }

    #[test]
    fn aceite_6_uma_mente_nao_cita_documento_que_nao_sustenta_o_que_diz() {
        let (lib, b) = ggrc11_meeting();
        assert_eq!(b.documents[0].id, "D1");
        let answer = minds_with(json!({
            "valuation": {"vote": "for", "confidence": 60, "why": "…", "points": [{"text": "A vacância física foi a 8,5%", "source": "D1 p.2", "kind": "fato"}]},
            "risk": {"vote": "against", "confidence": 70, "why": "…", "points": [{"text": "A vacância chegou a 12,5%", "source": "D1 p.2", "kind": "fato"}, {"text": "LTV de 22,4%", "source": "D1 p.2", "kind": "fato"}]}
        }));
        let v = finalize(&answer, None, b.facts.clone(), b.documents.clone(), Some(&lib), "GGRC11", "comprar 50 cotas de GGRC11", "asked", "", day(2026, 10, 30)).unwrap();
        let valuation = v.minds.iter().find(|m| m.id == "valuation").unwrap();
        assert_eq!(valuation.points[0].verified, Some(true), "8,5 is on D1's page 2");
        let risk = v.minds.iter().find(|m| m.id == "risk").unwrap();
        assert_eq!(risk.points[0].verified, Some(false), "12,5 is on no page");
        assert_eq!(risk.points[1].verified, Some(false), "22,4 is on page 3, not 2");
        assert!(v.audit.iter().any(|x| x.severity == "material" && x.text.contains("12,5")), "{:#?}", v.audit);
    }

    #[test]
    fn aceite_7_uma_divergencia_material_e_sinalizada_antes_da_conclusao() {
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
        // The report says R$ 0,080; FundosNet's notice, R$ 0,083.
        lib.ingest(&pending, crate::library::fixtures::report_pages("7,5", "0,080"), day(2026, 10, 21));
        let b = brief_full(&store, &market, Some(&lib), &json!({"proposal": "comprar 100 cotas de GARE11"}), day(2026, 10, 30), true).unwrap();
        let answer = minds_with(json!({}));
        // Before the conclusion: in what the code found, which the auditor reads before writing the final opinions.
        let findings = precheck(&answer, &b.facts, &b.documents, Some(&lib)).unwrap();
        let divergence = findings.iter().find(|x| x.severity == "material" && x.text.starts_with("Fontes divergentes")).expect("flagged before the audit");
        let prompt = audit_brief(&answer, &b.facts, &b.pages, &findings, "GARE11", "comprar 100 cotas de GARE11", false);
        assert!(prompt.contains(&divergence.text));
        // And in the verdict, with what to verify.
        let v = finalize(&answer, None, b.facts.clone(), b.documents.clone(), Some(&lib), "GARE11", "comprar 100 cotas de GARE11", "asked", "", day(2026, 10, 30)).unwrap();
        assert!(v.audit.iter().any(|x| x.text == divergence.text));
        assert!(v.to_verify.iter().any(|x| x.starts_with("Conferir a divergência")));
    }
}
