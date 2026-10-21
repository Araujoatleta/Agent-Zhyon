//! The CVM's open data on real-estate funds (FII): the monthly report every fund files ("Informe Mensal
//! Estruturado"), one ZIP a year with three CSVs (general data, its complement, assets and liabilities). It gives each
//! month's net asset value per share (VP/cota), net assets, shares, the month's dividend yield and returns, the
//! number of holders, and the fund's segment and mandate. Free, no key, updated weekly.
//!
//! The columns are found by their names, and the names the CVM used before and after its 2024 change (CNPJ_Fundo →
//! CNPJ_Fundo_Classe) both work. Percentages may be written as fractions (0.0085) or percents (0.85): each column is
//! read the way most of its values are written.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::table::Table;
use crate::text::{decode, num};

/// Where a year's reports are.
pub fn url(year: i32) -> String {
    format!("https://dados.cvm.gov.br/dados/FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_{year}.zip")
}

/// One fund's month.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FiiMonth {
    /// Digits only.
    pub cnpj: String,
    pub date: Date,
    pub name: String,
    pub isin: String,
    pub segment: String,
    pub mandate: String,
    pub management: String,
    /// R$.
    pub vp_share: Option<f64>,
    pub net_assets: Option<f64>,
    pub shares: Option<f64>,
    /// Percent of the month: 0.85 means 0.85 %.
    pub dividend_yield: Option<f64>,
    pub return_total: Option<f64>,
    pub return_assets: Option<f64>,
    pub amortization: Option<f64>,
    pub holders: Option<f64>,
    /// Dividends declared and not paid yet (a liability), R$.
    pub to_distribute: Option<f64>,
    /// Cash and liquid assets, R$.
    pub cash: Option<f64>,
    pub total_assets: Option<f64>,
}

pub fn digits(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// A column of percentages, read as percents: multiplied by 100 when most of its values are fractions.
fn percent_column(values: &mut [Option<f64>]) {
    let mut seen: Vec<f64> = values.iter().flatten().map(|v| v.abs()).filter(|v| *v > 0.0).collect();
    if seen.is_empty() {
        return;
    }
    seen.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if seen[seen.len() / 2] < 0.05 {
        for v in values.iter_mut().flatten() {
            *v *= 100.0;
        }
    }
}

/// Keyed by fund and month; a later version of a report replaces an earlier one.
type Key = (String, Date);

fn versions(t: &Table) -> (Option<usize>, Option<usize>, Option<usize>) {
    (t.find(&["cnpj_fundo", "cnpj"]), t.find(&["data_referencia", "dt_refer", "data_ref"]), t.find(&["versao"]))
}

/// The rows of a table that are each (fund, month)'s latest version.
fn latest<'a>(t: &'a Table) -> HashMap<Key, &'a Vec<String>> {
    let (c, d, v) = versions(t);
    let mut out: HashMap<Key, (&Vec<String>, f64)> = HashMap::new();
    for row in &t.rows {
        let cnpj = digits(Table::cell(row, c));
        let Some(date) = Date::parse(Table::cell(row, d)) else { continue };
        if cnpj.is_empty() {
            continue;
        }
        let version = num(Table::cell(row, v)).unwrap_or(0.0);
        let key = (cnpj, date);
        match out.get(&key) {
            Some((_, have)) if *have > version => {}
            _ => {
                out.insert(key, (row, version));
            }
        }
    }
    out.into_iter().map(|(k, (r, _))| (k, r)).collect()
}

/// The months of every fund, from the three CSVs of a year's ZIP (any may be missing). `wanted` keeps only those
/// funds (CNPJ digits); empty keeps all.
pub fn parse(general: Option<&str>, complement: Option<&str>, balance: Option<&str>, wanted: &[String]) -> Vec<FiiMonth> {
    let keep = |cnpj: &str| wanted.is_empty() || wanted.iter().any(|w| w == cnpj);
    let mut months: HashMap<Key, FiiMonth> = HashMap::new();
    if let Some(text) = general {
        let t = Table::csv(text);
        let name = t.find(&["nome_fundo", "denom_social", "nome"]);
        let isin = t.find(&["codigo_isin", "isin"]);
        let segment = t.find(&["segmento_atuacao", "segmento"]);
        let mandate = t.find(&["mandato"]);
        let management = t.find(&["tipo_gestao", "gestao"]);
        let shares = t.find(&["quantidade_cotas_emitidas", "cotas_emitidas"]);
        for ((cnpj, date), row) in latest(&t) {
            if !keep(&cnpj) {
                continue;
            }
            let m = months.entry((cnpj.clone(), date)).or_insert_with(|| FiiMonth { cnpj, date, ..FiiMonth::default() });
            m.name = Table::cell(row, name).to_string();
            m.isin = Table::cell(row, isin).to_uppercase();
            m.segment = Table::cell(row, segment).to_string();
            m.mandate = Table::cell(row, mandate).to_string();
            m.management = Table::cell(row, management).to_string();
            m.shares = num(Table::cell(row, shares)).or(m.shares);
        }
    }
    if let Some(text) = complement {
        let t = Table::csv(text);
        let vp = t.find(&["valor_patrimonial_cotas", "valor_patrimonial_cota", "vl_patrim_cota"]);
        let pl = t.find(&["patrimonio_liquido", "vl_patrim_liq"]);
        let shares = t.find(&["cotas_emitidas", "quantidade_cotas"]);
        let dy = t.find(&["percentual_dividend_yield_mes", "dividend_yield"]);
        let total = t.find(&["percentual_rentabilidade_efetiva_mes", "rentabilidade_efetiva"]);
        let assets = t.find(&["percentual_rentabilidade_patrimonial_mes", "rentabilidade_patrimonial"]);
        let amort = t.find(&["percentual_amortizacao_cotas_mes", "amortizacao"]);
        let holders = t.find(&["total_numero_cotistas", "numero_cotistas"]);
        let total_assets = t.find(&["valor_ativo"]);
        let rows = latest(&t);
        let mut pct: Vec<[Option<f64>; 4]> = Vec::new();
        let mut keys: Vec<Key> = Vec::new();
        for ((cnpj, date), row) in &rows {
            if !keep(cnpj) {
                continue;
            }
            keys.push((cnpj.clone(), *date));
            pct.push([
                num(Table::cell(row, dy)),
                num(Table::cell(row, total)),
                num(Table::cell(row, assets)),
                num(Table::cell(row, amort)),
            ]);
            let m = months.entry((cnpj.clone(), *date)).or_insert_with(|| FiiMonth { cnpj: cnpj.clone(), date: *date, ..FiiMonth::default() });
            m.vp_share = num(Table::cell(row, vp));
            m.net_assets = num(Table::cell(row, pl));
            m.shares = num(Table::cell(row, shares)).or(m.shares);
            m.holders = num(Table::cell(row, holders));
            m.total_assets = num(Table::cell(row, total_assets));
        }
        for col in 0..4 {
            let mut values: Vec<Option<f64>> = pct.iter().map(|p| p[col]).collect();
            percent_column(&mut values);
            for (key, v) in keys.iter().zip(values) {
                if let Some(m) = months.get_mut(key) {
                    match col {
                        0 => m.dividend_yield = v,
                        1 => m.return_total = v,
                        2 => m.return_assets = v,
                        _ => m.amortization = v,
                    }
                }
            }
        }
    }
    if let Some(text) = balance {
        let t = Table::csv(text);
        let to_distribute = t.find(&["rendimentos_distribuir", "rendimentos_a_distribuir"]);
        let cash = t.find(&["disponibilidades"]);
        let liquid = t.find(&["total_necessidades_liquidez"]);
        for ((cnpj, date), row) in latest(&t) {
            if !keep(&cnpj) {
                continue;
            }
            let m = months.entry((cnpj.clone(), date)).or_insert_with(|| FiiMonth { cnpj, date, ..FiiMonth::default() });
            m.to_distribute = num(Table::cell(row, to_distribute));
            m.cash = num(Table::cell(row, liquid)).or_else(|| num(Table::cell(row, cash)));
        }
    }
    let mut out: Vec<FiiMonth> = months.into_values().collect();
    out.sort_by(|a, b| a.cnpj.cmp(&b.cnpj).then(a.date.cmp(&b.date)));
    out
}

/// The months of a year's ZIP (as downloaded).
pub fn parse_zip(bytes: &[u8], wanted: &[String]) -> Result<Vec<FiiMonth>, String> {
    let archive = crate::zip::Archive::new(bytes)?;
    let read = |part: &str| -> Option<String> {
        archive.entries.iter().find(|e| e.name.to_ascii_lowercase().contains(part)).and_then(|e| archive.read(e).ok()).map(|b| decode(&b))
    };
    let general = read("geral");
    let complement = read("complemento");
    let balance = read("ativo_passivo");
    if general.is_none() && complement.is_none() {
        return Err("the CVM's file has none of the expected tables".into());
    }
    Ok(parse(general.as_deref(), complement.as_deref(), balance.as_deref(), wanted))
}

/// Ticker roots ("GGRC") to the CNPJ of their fund, from the ISINs of the shares ("BRGGRCCTF006").
pub fn cnpj_by_root(months: &[FiiMonth]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for m in months {
        if m.isin.len() >= 6 && m.isin.starts_with("BR") {
            out.insert(m.isin[2..6].to_string(), m.cnpj.clone());
        }
    }
    out
}

/// "papel" (receivables: CRI and other securities), "tijolo" (buildings), "fof" (funds of funds), "hibrido", or ""
/// — from the fund's own mandate and segment.
pub fn subtype(m: &FiiMonth) -> &'static str {
    let seg = crate::text::plain(&m.segment);
    let man = crate::text::plain(&m.mandate);
    if seg.contains("titulo") || seg.contains("val mob") || seg.contains("recebiveis") || man.contains("titulos") {
        "papel"
    } else if seg.contains("fundo") || seg.contains("fof") {
        "fof"
    } else if seg.contains("hibrido") || man.contains("hibrido") {
        "hibrido"
    } else if !seg.is_empty() || man.contains("renda") || man.contains("desenvolvimento") {
        "tijolo"
    } else {
        ""
    }
}

#[cfg(test)]
pub mod fixtures {
    /// A year of three funds' reports as the CVM writes them (Latin-1 aside), versions and all.
    pub fn general() -> String {
        "CNPJ_Fundo_Classe;Data_Referencia;Versao;Data_Entrega;Nome_Fundo_Classe;Codigo_ISIN;Quantidade_Cotas_Emitidas;Mandato;Segmento_Atuacao;Tipo_Gestao\n\
         11.111.111/0001-11;2026-08-01;1;2026-09-15;GGR COVEPI RENDA FII;BRGGRCCTF006;1000000;Renda;Logística;Ativa\n\
         11.111.111/0001-11;2026-09-01;1;2026-10-15;GGR COVEPI RENDA FII;BRGGRCCTF006;1000000;Renda;Logística;Ativa\n\
         22.222.222/0001-22;2026-09-01;1;2026-10-15;BTG PACTUAL CREDITO IMOBILIARIO FII;BRBTCICTF003;2000000;Títulos e Valores Mobiliários;Títulos e Val. Mob.;Ativa\n\
         33.333.333/0001-33;2026-09-01;1;2026-10-15;OUTRO FII;BROUTRCTF000;10;Renda;Shoppings;Passiva\n"
            .to_string()
    }

    pub fn complement() -> String {
        "CNPJ_Fundo_Classe;Data_Referencia;Versao;Valor_Ativo;Patrimonio_Liquido;Cotas_Emitidas;Valor_Patrimonial_Cotas;Percentual_Rentabilidade_Efetiva_Mes;Percentual_Rentabilidade_Patrimonial_Mes;Percentual_Dividend_Yield_Mes;Percentual_Amortizacao_Cotas_Mes;Total_Numero_Cotistas\n\
         11.111.111/0001-11;2026-08-01;1;9800000;9500000;1000000;9.50;0.0080;-0.0010;0.0090;0;12000\n\
         11.111.111/0001-11;2026-09-01;1;9700000;9400000;1000000;9.40;0.0070;-0.0105;0.0091;0;12100\n\
         11.111.111/0001-11;2026-09-01;2;9700000;9380000;1000000;9.38;0.0068;-0.0126;0.0092;0;12100\n\
         22.222.222/0001-22;2026-09-01;1;19000000;18000000;2000000;9.00;0.0110;0.0010;0.0100;0;50000\n\
         33.333.333/0001-33;2026-09-01;1;100;90;10;9.00;0.0050;0.0;0.0050;0;5\n"
            .to_string()
    }

    pub fn balance() -> String {
        "CNPJ_Fundo_Classe;Data_Referencia;Versao;Total_Necessidades_Liquidez;Disponibilidades;Rendimentos_Distribuir;Total_Passivo\n\
         11.111.111/0001-11;2026-09-01;1;450000;20000;91000;300000\n"
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_funds_months_are_joined_from_the_three_tables() {
        let months = parse(Some(&fixtures::general()), Some(&fixtures::complement()), Some(&fixtures::balance()), &[]);
        assert_eq!(months.len(), 4);
        let ggrc: Vec<&FiiMonth> = months.iter().filter(|m| m.cnpj == "11111111000111").collect();
        assert_eq!(ggrc.len(), 2);
        let sep = ggrc[1];
        assert_eq!(sep.date, Date::new(2026, 9, 1).unwrap());
        assert_eq!(sep.vp_share, Some(9.38), "the second version replaces the first");
        assert!((sep.dividend_yield.unwrap() - 0.92).abs() < 1e-9, "fractions read as percents");
        assert!((sep.return_assets.unwrap() + 1.26).abs() < 1e-9);
        assert_eq!(sep.holders, Some(12100.0));
        assert_eq!(sep.to_distribute, Some(91000.0));
        assert_eq!(sep.cash, Some(450000.0));
        assert_eq!(sep.isin, "BRGGRCCTF006");
        assert_eq!(subtype(sep), "tijolo");
        let btci = months.iter().find(|m| m.cnpj == "22222222000122").unwrap();
        assert_eq!(subtype(btci), "papel");
        let roots = cnpj_by_root(&months);
        assert_eq!(roots["GGRC"], "11111111000111");
        assert_eq!(roots["BTCI"], "22222222000122");
        let only = parse(Some(&fixtures::general()), Some(&fixtures::complement()), None, &["22222222000122".into()]);
        assert_eq!(only.len(), 1);
    }

    #[test]
    fn percents_written_as_percents_stay_percents_and_the_zip_reads() {
        let complement = "CNPJ_Fundo;Data_Referencia;Versao;Valor_Patrimonial_Cotas;Percentual_Dividend_Yield_Mes\n\
                          1;2026-09-01;1;10;0.85\n2;2026-09-01;1;10;1.10\n";
        let months = parse(None, Some(complement), None, &[]);
        assert!((months[0].dividend_yield.unwrap() - 0.85).abs() < 1e-9);
        let zip = crate::zip::build(
            &[
                ("inf_mensal_fii_geral_2026.csv", fixtures::general().as_bytes()),
                ("inf_mensal_fii_complemento_2026.csv", fixtures::complement().as_bytes()),
            ],
            true,
        );
        assert_eq!(parse_zip(&zip, &[]).unwrap().len(), 4);
        assert!(parse_zip(&crate::zip::build(&[("x.csv", b"a")], false), &[]).is_err());
        assert_eq!(url(2026), "https://dados.cvm.gov.br/dados/FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_2026.zip");
    }
}
