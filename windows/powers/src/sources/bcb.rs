//! The Banco Central's open data: the SGS series (the Selic target, the CDI, the IPCA over twelve months, the
//! dollar) and the Focus survey's median expectations (the Selic and the IPCA at the end of this year and the next).
//! Free, no key.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::date::Date;
use crate::text::num;

/// The series Zheus reads, by their SGS codes.
pub const SELIC: u32 = 432; // Selic target set by the Copom, % a year
pub const CDI: u32 = 4389; // CDI, % a year (252 business days)
pub const IPCA_12M: u32 = 13522; // IPCA over twelve months, %
pub const IPCA_MONTH: u32 = 433; // IPCA of the month, %
pub const USD: u32 = 1; // US dollar, selling, R$

pub fn series_url(code: u32, last: u32) -> String {
    format!("https://api.bcb.gov.br/dados/serie/bcdata.sgs.{code}/dados/ultimos/{last}?formato=json")
}

/// The Focus survey's latest medians for `indicator` ("Selic", "IPCA") at the end of each year.
pub fn focus_url(indicator: &str) -> String {
    let filter = format!("Indicador eq '{indicator}' and baseCalculo eq 0").replace(' ', "%20").replace('\'', "%27");
    format!(
        "https://olinda.bcb.gov.br/olinda/servico/Expectativas/versao/v1/odata/ExpectativasMercadoAnuais?$top=40&$filter={filter}&$orderby=Data%20desc&$format=json&$select=Indicador,Data,DataReferencia,Mediana"
    )
}

/// The points of an SGS answer (`[{"data": "20/10/2026", "valor": "15.00"}]`), oldest first.
pub fn parse_series(body: &str) -> Result<Vec<(Date, f64)>, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("the Banco Central answered something else than a series: {e}"))?;
    let list = v.as_array().ok_or("the Banco Central's series is not a list")?;
    let mut out: Vec<(Date, f64)> = list
        .iter()
        .filter_map(|p| {
            let date = Date::parse(p["data"].as_str()?)?;
            let value = match &p["valor"] {
                Value::String(s) => s.trim().parse::<f64>().ok().or_else(|| num(s))?,
                Value::Number(n) => n.as_f64()?,
                _ => return None,
            };
            Some((date, value))
        })
        .collect();
    out.sort_by_key(|(d, _)| *d);
    Ok(out)
}

/// One median of the Focus survey: what the market expects `indicator` to be at the end of `year`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Expectation {
    pub indicator: String,
    /// The survey's day.
    pub date: Date,
    pub year: i32,
    pub median: f64,
}

/// The latest survey's medians, one per year, from an Olinda answer (`{"value": [...]}`).
pub fn parse_focus(body: &str) -> Result<Vec<Expectation>, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("the Focus survey answered something else: {e}"))?;
    let list = v["value"].as_array().ok_or("the Focus survey has no values")?;
    let mut all: Vec<Expectation> = list
        .iter()
        .filter_map(|p| {
            Some(Expectation {
                indicator: p["Indicador"].as_str()?.to_string(),
                date: Date::parse(p["Data"].as_str()?)?,
                year: p["DataReferencia"].as_str().and_then(|y| y.trim().parse().ok()).or_else(|| p["DataReferencia"].as_i64().map(|y| y as i32))?,
                median: p["Mediana"].as_f64().or_else(|| p["Mediana"].as_str().and_then(num))?,
            })
        })
        .collect();
    let Some(latest) = all.iter().map(|e| e.date).max() else { return Ok(Vec::new()) };
    all.retain(|e| e.date == latest);
    all.sort_by_key(|e| e.year);
    all.dedup_by_key(|e| e.year);
    Ok(all)
}

/// The economy as Zheus tells it: the latest of each series and the market's expectations.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Macro {
    pub selic: Option<f64>,
    pub cdi: Option<f64>,
    pub ipca_12m: Option<f64>,
    pub usd: Option<f64>,
    /// The day of the latest value read.
    pub date: Option<Date>,
    pub selic_expected: Vec<Expectation>,
    pub ipca_expected: Vec<Expectation>,
}

impl Macro {
    /// The Selic the market expects at the end of `year`.
    pub fn selic_at(&self, year: i32) -> Option<f64> {
        self.selic_expected.iter().find(|e| e.year == year).map(|e| e.median)
    }

    pub fn ipca_at(&self, year: i32) -> Option<f64> {
        self.ipca_expected.iter().find(|e| e.year == year).map(|e| e.median)
    }

    /// The real interest rate (Selic over the expected inflation of the next twelve months, roughly).
    pub fn real_rate(&self, year: i32) -> Option<f64> {
        let selic = self.selic?;
        let ipca = self.ipca_at(year).or(self.ipca_12m)?;
        Some(((1.0 + selic / 100.0) / (1.0 + ipca / 100.0) - 1.0) * 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_and_expectations_are_read() {
        let s = parse_series(r#"[{"data":"17/10/2026","valor":"14.90"},{"data":"20/10/2026","valor":"15.00"},{"data":"x","valor":"1"}]"#).unwrap();
        assert_eq!(s, vec![(Date::new(2026, 10, 17).unwrap(), 14.9), (Date::new(2026, 10, 20).unwrap(), 15.0)]);
        assert!(parse_series("<html>blocked</html>").is_err());
        let focus = parse_focus(
            r#"{"@odata.context":"x","value":[
              {"Indicador":"Selic","Data":"2026-10-16","DataReferencia":"2026","Mediana":15.0},
              {"Indicador":"Selic","Data":"2026-10-16","DataReferencia":"2027","Mediana":12.5},
              {"Indicador":"Selic","Data":"2026-10-09","DataReferencia":"2026","Mediana":15.25}]}"#,
        )
        .unwrap();
        assert_eq!(focus.len(), 2, "only the latest survey");
        assert_eq!(focus[1].year, 2027);
        let m = Macro { selic: Some(15.0), ipca_12m: Some(5.0), selic_expected: focus, ..Macro::default() };
        assert_eq!(m.selic_at(2027), Some(12.5));
        assert!((m.real_rate(2026).unwrap() - 9.5238).abs() < 1e-3);
        assert!(focus_url("Selic").contains("Indicador%20eq%20%27Selic%27"));
        assert_eq!(series_url(SELIC, 1), "https://api.bcb.gov.br/dados/serie/bcdata.sgs.432/dados/ultimos/1?formato=json");
    }
}
