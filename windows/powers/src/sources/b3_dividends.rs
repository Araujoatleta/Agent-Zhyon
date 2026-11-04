//! The cash dividends a listed company announced, as B3's own listed-companies pages show them ("Proventos em
//! dinheiro"): each with its type (dividendo, juros sobre capital próprio, rendimento), the value per share, the last
//! day with the right (data-com, "lastDatePrior"), the payment date when set, and the share class it is for (its
//! ISIN). Free and public, no key: the JSON B3's site reads, asked by the company's four-letter code.

use crate::date::Date;
use crate::sources::fnet::Payout;

const BASE: &str = "https://sistemaswebb3-listados.b3.com.br/listedCompaniesProxy/CompanyCall/GetListedSupplementCompany";

/// The address of a company's supplement (its dividends among it), by its four-letter code ("BBAS" for BBAS3).
pub fn url(issuer: &str) -> String {
    let params = format!("{{\"issuingCompany\":\"{}\",\"language\":\"pt-br\"}}", issuer.trim().to_uppercase());
    format!("{BASE}/{}", base64(params.as_bytes()))
}

/// One cash dividend as B3 lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct CashDividend {
    /// The share class it is for ("BRBBASACNOR3").
    pub isin: String,
    /// "DIVIDENDO", "JRS CAP PROPRIO", "RENDIMENTO"…
    pub label: String,
    pub value: f64,
    /// The data-com: the last day a share bought still has the right.
    pub record: Date,
    /// None while the company has not set it ("A definir").
    pub payment: Option<Date>,
    pub approved: Option<Date>,
}

impl CashDividend {
    /// Juros sobre capital próprio: 15 % of income tax held at the source.
    pub fn is_jcp(&self) -> bool {
        let l = crate::text::plain(&self.label);
        l.contains("jrs") || l.contains("juros") || l.contains("jcp")
    }

    /// Its type in plain Portuguese.
    pub fn kind_pt(&self) -> &'static str {
        let l = crate::text::plain(&self.label);
        if self.is_jcp() {
            "Juros sobre capital próprio"
        } else if l.contains("rend") {
            "Rendimento"
        } else {
            "Dividendo"
        }
    }

    /// As the engine keeps a payout (FundosNet's shape): JCP is not tax free; the type goes in `period`.
    pub fn payout(&self) -> Payout {
        Payout { record: self.record, payment: self.payment, value: self.value, amortization: false, tax_free: Some(!self.is_jcp()), period: self.kind_pt().into(), document: 0, code: None }
    }
}

/// The cash dividends in B3's answer (an object, or a list of one), newest first. Fields read by name, as B3 writes
/// them; a dividend without a value or a data-com is left out.
pub fn parse(json: &str) -> Result<Vec<CashDividend>, String> {
    let v: serde_json::Value = serde_json::from_str(json.trim()).map_err(|_| "B3 did not answer with its list of dividends".to_string())?;
    let company = match &v {
        serde_json::Value::Array(a) => a.first().cloned().unwrap_or_default(),
        other => other.clone(),
    };
    let Some(list) = field(&company, "cashDividends").and_then(|x| x.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out: Vec<CashDividend> = list
        .iter()
        .filter_map(|d| {
            let s = |k: &str| field(d, k).and_then(|x| x.as_str()).map(str::trim).unwrap_or("").to_string();
            let value = field(d, "rate").and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(crate::text::num)))?;
            let record = date_br(&s("lastDatePrior"))?;
            (value > 0.0).then(|| CashDividend {
                isin: if s("isinCode").is_empty() { s("assetIssued") } else { s("isinCode") },
                label: s("label"),
                value,
                record,
                payment: date_br(&s("paymentDate")),
                approved: date_br(&s("approvedOn")),
            })
        })
        .collect();
    out.sort_by(|a, b| b.record.cmp(&a.record).then(b.payment.cmp(&a.payment)));
    Ok(out)
}

/// The dividends of one share class: its ISIN when B3's quotes gave it, else every class of the company.
pub fn of_class(all: &[CashDividend], isin: &str) -> Vec<Payout> {
    let mine: Vec<&CashDividend> = all.iter().filter(|d| !isin.is_empty() && d.isin.eq_ignore_ascii_case(isin)).collect();
    let chosen: Vec<&CashDividend> = if mine.is_empty() { all.iter().collect() } else { mine };
    let mut out: Vec<Payout> = Vec::new();
    for d in chosen {
        let p = d.payout();
        // The same dividend listed for two classes: once.
        if !out.iter().any(|x| x.record == p.record && x.payment == p.payment && (x.value - p.value).abs() < 1e-12 && x.period == p.period) {
            out.push(p);
        }
    }
    out
}

/// A field by its name, whatever its capitals.
fn field<'a>(v: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    v.as_object()?.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, x)| x)
}

/// "30/09/2026" (or "2026-09-30T00:00:00"); "A definir" and B3's far-future placeholder are none.
fn date_br(s: &str) -> Option<Date> {
    Date::parse(s).filter(|d| d.y < 2100)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for (i, shift) in [18u32, 12, 6, 0].iter().enumerate() {
            if i <= chunk.len() {
                out.push(TABLE[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
pub mod fixtures {
    /// BBAS's supplement as B3 lists it (its shape: two classes' lines, JCP and dividends, a payment still to set).
    pub const BBAS: &str = r#"[{"code":"BBAS","stockCapital":"120.000.000.000,00","cashDividends":[
        {"assetIssued":"BRBBASACNOR3","paymentDate":"27/11/2026","rate":"0,23474710930","relatedTo":"3º Trimestre/2026","approvedOn":"30/09/2026","isinCode":"BRBBASACNOR3","label":"JRS CAP PROPRIO","lastDatePrior":"10/10/2026","remarks":""},
        {"assetIssued":"BRBBASACNOR3","paymentDate":"A definir","rate":"0,04512","relatedTo":"","approvedOn":"28/10/2026","isinCode":"BRBBASACNOR3","label":"DIVIDENDO","lastDatePrior":"07/11/2026","remarks":""},
        {"assetIssued":"BRBBASACNOR3","paymentDate":"29/08/2026","rate":"0,40180730000","relatedTo":"2º Trimestre/2026","approvedOn":"30/07/2026","isinCode":"BRBBASACNOR3","label":"DIVIDENDO","lastDatePrior":"11/08/2026","remarks":""},
        {"assetIssued":"BRBBASR04OR8","paymentDate":"29/08/2026","rate":"0,1","relatedTo":"","approvedOn":"","isinCode":"BRBBASR04OR8","label":"DIVIDENDO","lastDatePrior":"11/08/2026","remarks":""}
    ],"stockDividends":[],"subscriptions":[]}]"#;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_address_carries_the_company_in_base64() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert!(url("bbas").ends_with("eyJpc3N1aW5nQ29tcGFueSI6IkJCQVMiLCJsYW5ndWFnZSI6InB0LWJyIn0="), "{}", url("bbas"));
    }

    #[test]
    fn each_dividend_with_its_type_dates_and_class() {
        let all = parse(fixtures::BBAS).unwrap();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0].record, Date::new(2026, 11, 7).unwrap(), "newest data-com first");
        assert_eq!(all[0].payment, None, "\"A definir\" is no date");
        assert!((all[0].value - 0.04512).abs() < 1e-12, "the value as written");
        let jcp = all.iter().find(|d| d.is_jcp()).unwrap();
        assert_eq!(jcp.kind_pt(), "Juros sobre capital próprio");
        assert_eq!(jcp.payout().tax_free, Some(false));
        let on = of_class(&all, "BRBBASACNOR3");
        assert_eq!(on.len(), 3, "the other class's line is not BBAS3's");
        assert!(of_class(&all, "").len() >= 3, "no ISIN known: every class");
        assert!(parse("[{\"code\":\"XXXX\"}]").unwrap().is_empty());
        assert!(parse("<html>").is_err());
    }
}
