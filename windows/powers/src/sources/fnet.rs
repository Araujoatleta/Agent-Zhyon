//! FundosNet (B3's system where funds file their documents, fnet.bmfbovespa.com.br): each fund's documents — material
//! facts, notices, assemblies and their proposals, offerings and preemptive rights, managers' reports — and the
//! structured notice of every dividend and amortization ("Aviso aos Cotistas - Estruturado"), with its record date
//! (data-com), payment date and value per share. Public, no key.
//!
//! The list is the JSON its own search page reads; a structured document downloads as base64 of its XML.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::date::Date;
use crate::text::{num, plain};
use crate::xml::tree;

pub const BASE: &str = "https://fnet.bmfbovespa.com.br/fnet/publico";

/// The latest `count` documents of a fund (CNPJ digits), newest first.
pub fn list_url(cnpj: &str, count: usize) -> String {
    format!(
        "{BASE}/pesquisarGerenciadorDocumentosDados?d=1&s=0&l={count}&o%5B0%5D%5BdataEntrega%5D=desc&cnpjFundo={cnpj}&tipoFundo=1&idCategoriaDocumento=0&idTipoDocumento=0&idEspecieDocumento=0&situacao=A"
    )
}

/// A document's page, for the user and for an assistant to read.
pub fn view_url(id: u64) -> String {
    format!("{BASE}/exibirDocumento?id={id}&cvm=true")
}

pub fn download_url(id: u64) -> String {
    format!("{BASE}/downloadDocumento?id={id}")
}

/// What a document is about, for what it asks of the holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    /// Dividends and amortizations (structured or not).
    Payout,
    /// An assembly: its call, the manager's proposal, the minutes.
    Assembly,
    /// An offering of new shares: preemptive rights, subscription.
    Offering,
    MaterialFact,
    Notice,
    /// The manager's monthly report.
    Report,
    /// The periodic reports to the CVM (monthly, quarterly, yearly).
    Filing,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: u64,
    pub category: String,
    pub kind: String,
    pub species: String,
    /// The reference the fund gave: a month ("09/2026") or a date (an assembly's day).
    pub reference: String,
    pub delivered: Option<Date>,
    pub info: String,
    pub topic: Topic,
    /// Structured (XML) rather than a PDF.
    pub structured: bool,
}

impl Document {
    pub fn url(&self) -> String {
        view_url(self.id)
    }

    /// The reference as a date, when it is one ("15/10/2026").
    pub fn reference_date(&self) -> Option<Date> {
        Date::parse(&self.reference)
    }
}

/// What a document is about, from its category, type, species and notes.
pub fn topic(category: &str, kind: &str, species: &str, info: &str) -> Topic {
    let all = plain(&format!("{category} {kind} {species} {info}"));
    let cat = plain(category);
    let has = |w: &str| all.contains(w);
    if has("rendimento") || has("amortiza") || has("distribuicao de resultado") {
        Topic::Payout
    } else if has("assembleia") || has("convocacao") || has("consulta formal") || has("proposta do administrador") {
        Topic::Assembly
    } else if has("oferta") || has("emissao") || has("subscricao") || has("direito de preferencia") || has("cotas adicionais") {
        Topic::Offering
    } else if cat.contains("fato relevante") || has("fato relevante") {
        Topic::MaterialFact
    } else if cat.contains("relatorio") && (has("gerencial") || has("relatorio") || has("relatorio mensal")) {
        Topic::Report
    } else if cat.contains("informe") || has("informe mensal") || has("informe trimestral") || has("informe anual") || has("demonstra") {
        Topic::Filing
    } else if cat.contains("comunicado") || cat.contains("aviso") {
        Topic::Notice
    } else {
        Topic::Other
    }
}

fn text(v: &Value, key: &str) -> String {
    match &v[key] {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// The documents of a list answer (`{"data": [...]}`), newest first as given.
pub fn parse_list(body: &str) -> Result<Vec<Document>, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("FundosNet answered something else than its list: {e}"))?;
    let data = v["data"].as_array().ok_or("FundosNet's list has no data")?;
    Ok(data
        .iter()
        .filter_map(|d| {
            let id = d["id"].as_u64().or_else(|| text(d, "id").parse().ok())?;
            let category = text(d, "categoriaDocumento");
            let kind = text(d, "tipoDocumento");
            let species = text(d, "especieDocumento");
            let info = text(d, "informacoesAdicionais");
            let structured = plain(&category).contains("estruturado") || plain(&kind).contains("estruturado");
            Some(Document {
                id,
                topic: topic(&category, &kind, &species, &info),
                category,
                kind,
                species,
                reference: text(d, "dataReferencia"),
                delivered: Date::parse(&text(d, "dataEntrega")),
                info,
                structured,
            })
        })
        .collect())
}

/// A dividend (rendimento) or an amortization per share, as the fund announced it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Payout {
    /// The last day to hold the share to receive it (data-com / data-base).
    pub record: Date,
    pub payment: Option<Date>,
    /// R$ per share.
    pub value: f64,
    pub amortization: bool,
    /// Free of income tax for individuals (as the fund says).
    pub tax_free: Option<bool>,
    /// "Setembro 2026": the period it refers to.
    pub period: String,
    /// The document it came from.
    pub document: u64,
}

/// A downloaded document: FundosNet sends base64 of the file; anything that is not base64 is taken as it is.
pub fn unwrap_download(body: &[u8]) -> Vec<u8> {
    let trimmed: Vec<u8> = body.iter().copied().filter(|b| !b.is_ascii_whitespace() && *b != b'"').collect();
    if trimmed.is_empty() || trimmed.starts_with(b"<") || trimmed.starts_with(b"%PDF") {
        return body.to_vec();
    }
    base64_decode(&trimmed).unwrap_or_else(|| body.to_vec())
}

fn base64_decode(input: &[u8]) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let data: Vec<u8> = input.iter().copied().filter(|&c| c != b'=').collect();
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    for chunk in data.chunks(4) {
        let mut acc = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= value(c)? << (18 - 6 * i);
        }
        let bytes = acc.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

/// The dividends and amortizations of a structured notice ("Rendimentos e Amortizações"), from its XML.
pub fn parse_payouts(xml: &str, document: u64) -> Vec<Payout> {
    let root = tree(xml);
    let mut out = Vec::new();
    for (tag, amortization) in [("Rendimento", false), ("Amortizacao", true)] {
        for node in root.all(tag) {
            let Some(record) = node.text_of("database").or_else(|| node.text_of("datacom")).and_then(Date::parse) else { continue };
            let Some(value) = node.text_of("valorprovento").or_else(|| node.text_of("valor")).and_then(num) else { continue };
            if value <= 0.0 {
                continue;
            }
            let tax_free = node.text_of("isento").map(|t| matches!(plain(t).as_str(), "true" | "sim" | "s" | "1"));
            let period = [node.text_of("periodo"), node.text_of("ano")].into_iter().flatten().collect::<Vec<_>>().join(" ");
            out.push(Payout {
                record,
                payment: node.text_of("datapagamento").and_then(Date::parse),
                value,
                amortization,
                tax_free,
                period,
                document,
            });
        }
    }
    out
}

/// The share code a structured notice names ("GGRC11"), if it does.
pub fn ticker_in(xml: &str) -> Option<String> {
    let root = tree(xml);
    for name in ["CodNegociacaoCota", "CodigoNegociacao", "CodNegociacao"] {
        if let Some(t) = root.all(name).first().and_then(|n| crate::text::ticker(&n.text)) {
            return Some(t);
        }
    }
    None
}

#[cfg(test)]
pub mod fixtures {
    pub fn list() -> String {
        r#"{"draw":1,"recordsFiltered":4,"recordsTotal":4,"data":[
          {"id":901,"descricaoFundo":"FII GGR COVEPI","categoriaDocumento":"Aviso aos Cotistas - Estruturado","tipoDocumento":"Rendimentos e Amortizações","especieDocumento":"","dataReferencia":"10/2026","dataEntrega":"14/10/2026 18:01","status":"AC","informacoesAdicionais":"","situacaoDocumento":"A"},
          {"id":902,"descricaoFundo":"FII GGR COVEPI","categoriaDocumento":"Assembleia","tipoDocumento":"AGE","especieDocumento":"Edital de Convocação","dataReferencia":"10/11/2026","dataEntrega":"10/10/2026 09:00","status":"AC","informacoesAdicionais":"Aprovação de nova emissão de cotas"},
          {"id":903,"descricaoFundo":"FII GGR COVEPI","categoriaDocumento":"Fato Relevante","tipoDocumento":"","especieDocumento":"","dataReferencia":"05/10/2026","dataEntrega":"05/10/2026 20:00","status":"AC","informacoesAdicionais":"Inadimplência de locatário"},
          {"id":"904","descricaoFundo":"FII GGR COVEPI","categoriaDocumento":"Relatórios","tipoDocumento":"Relatório Gerencial","especieDocumento":"","dataReferencia":"09/2026","dataEntrega":"01/10/2026 10:00","status":"AC","informacoesAdicionais":""}
        ]}"#
        .to_string()
    }

    pub fn payout_xml() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DadosEconomicoFinanceiros>
  <DadosGerais><NomeFundo>GGR COVEPI RENDA FII</NomeFundo><CNPJFundo>11111111000111</CNPJFundo><CodISINCota>BRGGRCCTF006</CodISINCota><CodNegociacaoCota>GGRC11</CodNegociacaoCota></DadosGerais>
  <InformeRendimentos>
    <Rendimento><DataAprovacao>2026-10-10</DataAprovacao><DataBase>2026-10-14</DataBase><DataPagamento>2026-10-21</DataPagamento><ValorProventoCota>0,088</ValorProventoCota><PeriodoReferencia>Setembro</PeriodoReferencia><Ano>2026</Ano><RendimentoIsentoIR>true</RendimentoIsentoIR></Rendimento>
    <Amortizacao><DataBase>2026-10-14</DataBase><DataPagamento>2026-10-21</DataPagamento><ValorProventoCota>0.05</ValorProventoCota></Amortizacao>
    <Amortizacao><DataBase></DataBase><ValorProventoCota>0</ValorProventoCota></Amortizacao>
  </InformeRendimentos>
</DadosEconomicoFinanceiros>"#
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_read_and_each_document_gets_its_topic() {
        let docs = parse_list(&fixtures::list()).unwrap();
        assert_eq!(docs.len(), 4);
        assert_eq!(docs[0].topic, Topic::Payout);
        assert!(docs[0].structured);
        assert_eq!(docs[1].topic, Topic::Assembly);
        assert_eq!(docs[1].reference_date(), Date::new(2026, 11, 10));
        assert_eq!(docs[2].topic, Topic::MaterialFact);
        assert_eq!(docs[3].id, 904, "an id sent as text");
        assert_eq!(docs[3].topic, Topic::Report);
        assert_eq!(docs[0].delivered, Date::new(2026, 10, 14));
        assert_eq!(docs[0].url(), "https://fnet.bmfbovespa.com.br/fnet/publico/exibirDocumento?id=901&cvm=true");
        assert!(parse_list("<html>").is_err());
        assert_eq!(topic("Comunicado ao Mercado", "", "", "Direito de preferência na 5ª emissão"), Topic::Offering);
        assert_eq!(topic("Informes Periódicos", "Informe Mensal Estruturado", "", ""), Topic::Filing);
    }

    #[test]
    fn a_structured_notice_gives_its_dividend_and_amortization() {
        let xml = fixtures::payout_xml();
        let encoded = base64_encode(xml.as_bytes());
        let decoded = String::from_utf8(unwrap_download(format!("\"{encoded}\"").as_bytes())).unwrap();
        assert_eq!(decoded, xml);
        let payouts = parse_payouts(&decoded, 901);
        assert_eq!(payouts.len(), 2, "an empty amortization is not one");
        let r = &payouts[0];
        assert_eq!(r.record, Date::new(2026, 10, 14).unwrap());
        assert_eq!(r.payment, Date::new(2026, 10, 21));
        assert!((r.value - 0.088).abs() < 1e-12);
        assert_eq!(r.tax_free, Some(true));
        assert_eq!(r.period, "Setembro 2026");
        assert!(!r.amortization && payouts[1].amortization);
        assert_eq!(ticker_in(&xml).as_deref(), Some("GGRC11"));
        assert_eq!(unwrap_download(b"<xml/>"), b"<xml/>");
    }

    fn base64_encode(data: &[u8]) -> String {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
            let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }
}
