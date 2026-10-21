//! The CVM's open data on listed companies: their yearly (DFP) and quarterly (ITR) financial statements, as the
//! standard accounts every company files — the income statement (DRE), the balance sheet (BPA, BPP) and the cash
//! flow (DFC, indirect method) — consolidated; and the company register's securities (FCA), which ties a ticker
//! ("TIMS3") to its company's CNPJ. One ZIP a year per kind, read by byte range so only the tables needed come down.
//! Free, no key.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::table::Table;
use crate::text::{num, plain};


/// "dfp" (yearly) or "itr" (quarterly).
pub fn url(kind: &str, year: i32) -> String {
    format!("https://dados.cvm.gov.br/dados/CIA_ABERTA/DOC/{}/DADOS/{kind}_cia_aberta_{year}.zip", kind.to_uppercase())
}

pub fn fca_url(year: i32) -> String {
    format!("https://dados.cvm.gov.br/dados/CIA_ABERTA/DOC/FCA/DADOS/fca_cia_aberta_{year}.zip")
}

/// The tables read from a DFP or ITR archive (consolidated statements), by the end of their names.
pub const TABLES: &[&str] = &["DRE_con", "BPA_con", "BPP_con", "DFC_MI_con"];
/// The table of the capital's composition (shares issued and in treasury), in the same archives.
pub const CAPITAL: &str = "composicao_capital";

/// The archive entry of a table: "itr_cia_aberta_DRE_con_2026.csv".
pub fn entry_name(kind: &str, table: &str, year: i32) -> String {
    format!("{kind}_cia_aberta_{table}_{year}.csv")
}

/// A company's shares outstanding (issued and paid in, less those in treasury) at its latest filing in a "composição
/// do capital" table, and that filing's date. None when the table has no row for it.
pub fn parse_shares(text: &str, cnpj: &str) -> Option<(Date, f64)> {
    let t = Table::csv(text);
    let c_cnpj = t.find(&["cnpj_cia", "cnpj"]);
    let c_ref = t.find(&["dt_refer"]);
    let c_ver = t.find(&["versao"]);
    let c_total = t.find(&["qt_acao_total_cap_integr", "qt_acao_total_capital_integralizado"]);
    let c_treasury = t.find(&["qt_acao_total_tesouro", "qt_acao_total_tesouraria"]);
    let mut best: Option<(Date, f64, f64)> = None;
    for row in &t.rows {
        if crate::text::cnpj_chars(Table::cell(row, c_cnpj)) != cnpj {
            continue;
        }
        let (Some(date), Some(total)) = (Date::parse(Table::cell(row, c_ref)), num(Table::cell(row, c_total))) else { continue };
        let version = num(Table::cell(row, c_ver)).unwrap_or(0.0);
        let shares = total - num(Table::cell(row, c_treasury)).unwrap_or(0.0).max(0.0);
        if shares <= 0.0 {
            continue;
        }
        if best.is_none_or(|(d, v, _)| (date, version) > (d, v)) {
            best = Some((date, version, shares));
        }
    }
    best.map(|(d, _, s)| (d, s))
}

/// One account of one statement, in reais.
#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub cnpj: String,
    pub company: String,
    pub reference: Date,
    pub start: Option<Date>,
    pub end: Date,
    pub code: String,
    pub description: String,
    pub value: f64,
}

/// The accounts of one table's CSV for the companies in `wanted` (CNPJ digits): the latest version of each filing,
/// the current period only ("ÚLTIMO", not the comparative one), in reais (thousands multiplied out).
pub fn parse_accounts(text: &str, wanted: &[String]) -> Vec<Account> {
    let t = Table::csv(text);
    let c_cnpj = t.find(&["cnpj_cia", "cnpj"]);
    let c_ref = t.find(&["dt_refer"]);
    let c_ver = t.find(&["versao"]);
    let c_name = t.find(&["denom_cia"]);
    let c_scale = t.find(&["escala_moeda"]);
    let c_order = t.find(&["ordem_exerc"]);
    let c_start = t.find(&["dt_ini_exerc"]);
    let c_end = t.find(&["dt_fim_exerc"]);
    let c_code = t.find(&["cd_conta"]);
    let c_desc = t.find(&["ds_conta"]);
    let c_value = t.find(&["vl_conta"]);
    // The latest version of each (company, reference date).
    let mut version: HashMap<(String, Date), f64> = HashMap::new();
    let mut kept = Vec::new();
    for row in &t.rows {
        let cnpj = crate::text::cnpj_chars(Table::cell(row, c_cnpj));
        if !wanted.is_empty() && !wanted.contains(&cnpj) {
            continue;
        }
        if !plain(Table::cell(row, c_order)).starts_with("ultimo") {
            continue;
        }
        let Some(reference) = Date::parse(Table::cell(row, c_ref)) else { continue };
        let v = num(Table::cell(row, c_ver)).unwrap_or(0.0);
        let entry = version.entry((cnpj.clone(), reference)).or_insert(v);
        if v > *entry {
            *entry = v;
        }
        kept.push((row, cnpj, reference, v));
    }
    kept.into_iter()
        .filter(|(_, cnpj, reference, v)| version.get(&(cnpj.clone(), *reference)) == Some(v))
        .filter_map(|(row, cnpj, reference, _)| {
            let end = Date::parse(Table::cell(row, c_end)).unwrap_or(reference);
            let scale = if plain(Table::cell(row, c_scale)).contains("mil") { 1000.0 } else { 1.0 };
            Some(Account {
                cnpj,
                company: Table::cell(row, c_name).to_string(),
                reference,
                start: Date::parse(Table::cell(row, c_start)),
                end,
                code: Table::cell(row, c_code).to_string(),
                description: Table::cell(row, c_desc).to_string(),
                value: num(Table::cell(row, c_value))? * scale,
            })
        })
        .collect()
}

/// A company's figures for one period: a quarter, a year so far, or a year (flows), or a day (balances).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Figures {
    pub start: Option<Date>,
    pub end: Date,
    pub revenue: Option<f64>,
    pub gross_profit: Option<f64>,
    /// Operating result before the financial result and taxes (EBIT).
    pub ebit: Option<f64>,
    pub financial_result: Option<f64>,
    pub net_income: Option<f64>,
    /// Depreciation and amortization (from the cash flow).
    pub depreciation: Option<f64>,
    pub operating_cash: Option<f64>,
    /// Purchases of fixed and intangible assets (a positive amount).
    pub capex: Option<f64>,
    /// Dividends and interest on equity paid (a positive amount).
    pub dividends_paid: Option<f64>,
    pub total_assets: Option<f64>,
    pub current_assets: Option<f64>,
    pub cash: Option<f64>,
    pub investments: Option<f64>,
    pub receivables: Option<f64>,
    pub current_liabilities: Option<f64>,
    pub debt: Option<f64>,
    pub leases: Option<f64>,
    pub equity: Option<f64>,
}

fn add(slot: &mut Option<f64>, v: f64) {
    *slot = Some(slot.unwrap_or(0.0) + v);
}

/// One company's accounts (from any of the four tables) as figures per period, oldest first.
pub fn figures(accounts: &[Account]) -> Vec<Figures> {
    let mut periods: BTreeMap<(Date, Option<Date>), Figures> = BTreeMap::new();
    for a in accounts {
        let f = periods.entry((a.end, a.start)).or_insert_with(|| Figures { start: a.start, end: a.end, ..Figures::default() });
        let desc = plain(&a.description);
        let code = a.code.as_str();
        let under = |prefix: &str| code.starts_with(prefix) && code.len() > prefix.len();
        match code {
            "3.01" => f.revenue = Some(a.value),
            "3.03" => f.gross_profit = Some(a.value),
            "3.05" => f.ebit = Some(a.value),
            "3.06" => f.financial_result = Some(a.value),
            "3.11" => f.net_income = Some(a.value),
            "3.09" if f.net_income.is_none() => f.net_income = Some(a.value),
            "6.01" => f.operating_cash = Some(a.value),
            "1" => f.total_assets = Some(a.value),
            "1.01" => f.current_assets = Some(a.value),
            "1.01.01" => f.cash = Some(a.value),
            "1.01.02" => f.investments = Some(a.value),
            "1.01.03" => f.receivables = Some(a.value),
            "2.01" => f.current_liabilities = Some(a.value),
            "2.01.04" | "2.02.01" => add(&mut f.debt, a.value),
            "2.03" => f.equity = Some(a.value),
            _ => {}
        }
        // Depreciation and amortization: the adjustments of the operating cash (6.01.01.xx), not loans' amortization.
        if under("6.01.01") && (desc.contains("deprecia") || desc.contains("amortiza")) && !desc.contains("emprestimo") && !desc.contains("financiamento") {
            add(&mut f.depreciation, a.value.abs());
        }
        if under("6.02") && a.value < 0.0 && (desc.contains("imobilizado") || desc.contains("intangivel")) && !desc.contains("venda") && !desc.contains("alienacao") {
            add(&mut f.capex, -a.value);
        }
        if under("6.03") && a.value < 0.0 && (desc.contains("dividendo") || desc.contains("juros sobre capital") || desc.contains("juros sobre o capital")) {
            add(&mut f.dividends_paid, -a.value);
        }
        if (under("2.01") || under("2.02")) && desc.contains("arrendamento") && code.matches('.').count() == 2 {
            add(&mut f.leases, a.value);
        }
    }
    // A filing sent with every value at zero (it happens) tells nothing: as if it were not there.
    periods.into_values().filter(|f| !all_zero(f)).collect()
}

fn all_zero(f: &Figures) -> bool {
    [
        f.revenue, f.gross_profit, f.ebit, f.financial_result, f.net_income, f.depreciation, f.operating_cash, f.capex, f.dividends_paid, f.total_assets,
        f.current_assets, f.cash, f.investments, f.receivables, f.current_liabilities, f.debt, f.leases, f.equity,
    ]
    .iter()
    .flatten()
    .all(|v| v.abs() < 0.5)
}

/// Tickers ("TIMS3") to (CNPJ digits, company), from the FCA's securities table.
pub fn tickers(text: &str) -> HashMap<String, (String, String)> {
    let t = Table::csv(text);
    let c_cnpj = t.find(&["cnpj_companhia", "cnpj_cia", "cnpj"]);
    let c_code = t.find(&["codigo_negociacao", "cod_negociacao"]);
    let c_name = t.find(&["nome_empresarial", "denom_social", "nome"]);
    let mut out = HashMap::new();
    for row in &t.rows {
        let cnpj = crate::text::cnpj_chars(Table::cell(row, c_cnpj));
        if cnpj.is_empty() {
            continue;
        }
        for code in Table::cell(row, c_code).split([',', ';', '/', ' ']) {
            if let Some(ticker) = crate::text::ticker(code) {
                out.insert(ticker, (cnpj.clone(), Table::cell(row, c_name).to_string()));
            }
        }
    }
    out
}

#[cfg(test)]
pub mod fixtures {
    const HEAD: &str = "CNPJ_CIA;DT_REFER;VERSAO;DENOM_CIA;CD_CVM;GRUPO_DFP;MOEDA;ESCALA_MOEDA;ORDEM_EXERC;DT_INI_EXERC;DT_FIM_EXERC;CD_CONTA;DS_CONTA;VL_CONTA;ST_CONTA_FIXA";

    /// TIM-like statements for the first half of 2026 (thousands of reais), with an older version and the
    /// comparative period that must be skipped.
    pub fn dre() -> String {
        let r = |ver: u8, order: &str, ini: &str, fim: &str, code: &str, desc: &str, v: &str| {
            format!("02.558.115/0001-21;2026-06-30;{ver};TIM S.A.;22209;DF Consolidado - Demonstração do Resultado;REAL;MIL;{order};{ini};{fim};{code};{desc};{v};S")
        };
        [
            HEAD.to_string(),
            r(1, "ÚLTIMO", "2026-04-01", "2026-06-30", "3.01", "Receita de Venda de Bens e/ou Serviços", "6000000"),
            r(2, "ÚLTIMO", "2026-04-01", "2026-06-30", "3.01", "Receita de Venda de Bens e/ou Serviços", "6500000"),
            r(2, "ÚLTIMO", "2026-04-01", "2026-06-30", "3.05", "Resultado Antes do Resultado Financeiro e dos Tributos", "1500000"),
            r(2, "ÚLTIMO", "2026-04-01", "2026-06-30", "3.11", "Lucro/Prejuízo Consolidado do Período", "900000"),
            r(2, "ÚLTIMO", "2026-01-01", "2026-06-30", "3.01", "Receita de Venda de Bens e/ou Serviços", "12800000"),
            r(2, "PENÚLTIMO", "2025-04-01", "2025-06-30", "3.01", "Receita de Venda de Bens e/ou Serviços", "6100000"),
        ]
        .join("\n")
    }

    pub fn dfc() -> String {
        let r = |code: &str, desc: &str, v: &str| {
            format!("02.558.115/0001-21;2026-06-30;2;TIM S.A.;22209;DF Consolidado - Fluxo de Caixa (Método Indireto);REAL;MIL;ÚLTIMO;2026-01-01;2026-06-30;{code};{desc};{v};S")
        };
        [
            HEAD.to_string(),
            r("6.01", "Caixa Líquido Atividades Operacionais", "6000000"),
            r("6.01.01.02", "Depreciação e amortização", "2400000"),
            r("6.01.01.05", "Amortização de empréstimos (não é D&A)", "-100"),
            r("6.02.01", "Aquisição de imobilizado e intangível", "-2200000"),
            r("6.02.02", "Venda de imobilizado", "50000"),
            r("6.03.03", "Dividendos e juros sobre capital próprio pagos", "-1300000"),
        ]
        .join("\n")
    }

    pub fn bp() -> String {
        let r = |code: &str, desc: &str, v: &str| format!("02.558.115/0001-21;2026-06-30;2;TIM S.A.;22209;DF Consolidado - Balanço Patrimonial;REAL;MIL;ÚLTIMO;;2026-06-30;{code};{desc};{v};S");
        [
            HEAD.to_string(),
            r("1.01.01", "Caixa e Equivalentes de Caixa", "3000000"),
            r("1.01.02", "Aplicações Financeiras", "1000000"),
            r("2.01.04", "Empréstimos e Financiamentos", "1500000"),
            r("2.02.01", "Empréstimos e Financiamentos", "4500000"),
            r("2.02.02", "Passivo de arrendamento", "12000000"),
            r("2.03", "Patrimônio Líquido Consolidado", "25000000"),
        ]
        .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_companys_statements_become_figures_per_period() {
        let wanted = vec!["02558115000121".to_string()];
        let mut accounts = parse_accounts(&fixtures::dre(), &wanted);
        accounts.extend(parse_accounts(&fixtures::dfc(), &wanted));
        accounts.extend(parse_accounts(&fixtures::bp(), &wanted));
        assert!(accounts.iter().all(|a| a.value.abs() >= 100_000.0 || a.description.contains("não é")), "thousands multiplied out");
        assert!(parse_accounts(&fixtures::dre(), &["999".into()]).is_empty());
        let f = figures(&accounts);
        let quarter = f.iter().find(|p| p.start == Date::new(2026, 4, 1)).unwrap();
        assert_eq!(quarter.revenue, Some(6_500_000_000.0), "the latest version, not the first");
        assert_eq!(quarter.ebit, Some(1_500_000_000.0));
        assert_eq!(quarter.net_income, Some(900_000_000.0));
        let half = f.iter().find(|p| p.start == Date::new(2026, 1, 1)).unwrap();
        assert_eq!(half.revenue, Some(12_800_000_000.0));
        assert_eq!(half.depreciation, Some(2_400_000_000.0), "loans' amortization is not D&A");
        assert_eq!(half.capex, Some(2_200_000_000.0));
        assert_eq!(half.dividends_paid, Some(1_300_000_000.0));
        assert_eq!(half.operating_cash, Some(6_000_000_000.0));
        let balance = f.iter().find(|p| p.start.is_none()).unwrap();
        assert_eq!(balance.debt, Some(6_000_000_000.0));
        assert_eq!(balance.leases, Some(12_000_000_000.0));
        assert_eq!(balance.cash, Some(3_000_000_000.0));
        assert_eq!(balance.equity, Some(25_000_000_000.0));
        assert!(f.iter().all(|p| p.end.y == 2026), "the comparative period is skipped");
    }

    #[test]
    fn shares_outstanding_from_the_capital_table() {
        let text = "CNPJ_CIA;DT_REFER;VERSAO;DENOM_CIA;CD_CVM;QT_ACAO_ORDIN_CAP_INTEGR;QT_ACAO_PREF_CAP_INTEGR;QT_ACAO_TOTAL_CAP_INTEGR;QT_ACAO_ORDIN_TESOURO;QT_ACAO_PREF_TESOURO;QT_ACAO_TOTAL_TESOURO\n\
            02.421.421/0001-11;2026-03-31;1;TIM S.A.;024929;2420804398;0;2420804398;1000000;0;1000000\n\
            02.421.421/0001-11;2026-06-30;1;TIM S.A.;024929;2420804398;0;2420804398;2000000;0;2000000\n\
            02.421.421/0001-11;2026-06-30;2;TIM S.A.;024929;2420804398;0;2420804398;2500000;0;2500000\n\
            33.000.167/0001-01;2026-06-30;1;PETROBRAS;009512;7442454142;5602042788;13044496930;0;0;0\n";
        assert_eq!(parse_shares(text, "02421421000111"), Some((Date::new(2026, 6, 30).unwrap(), 2_420_804_398.0 - 2_500_000.0)), "the latest filing's latest version, less the treasury");
        assert_eq!(parse_shares(text, "33000167000101").map(|s| s.1), Some(13_044_496_930.0));
        assert_eq!(parse_shares(text, "11111111000111"), None);
    }

    #[test]
    fn a_filing_sent_with_every_value_at_zero_is_left_out() {
        // As the CVM's DFP 2025 had TIM's: every account, of both years, at 0.
        let head = "CNPJ_CIA;DT_REFER;VERSAO;DENOM_CIA;CD_CVM;GRUPO_DFP;MOEDA;ESCALA_MOEDA;ORDEM_EXERC;DT_INI_EXERC;DT_FIM_EXERC;CD_CONTA;DS_CONTA;VL_CONTA;ST_CONTA_FIXA";
        let row = |order: &str, start: &str, end: &str, code: &str, desc: &str, v: &str| format!("02.421.421/0001-11;2025-12-31;1;TIM S.A.;024929;DF Consolidado - Demonstração do Resultado;REAL;MIL;{order};{start};{end};{code};{desc};{v};S");
        let zeros = [head.to_string(), row("ÚLTIMO", "2025-01-01", "2025-12-31", "3.01", "Receita de Venda de Bens e/ou Serviços", "0.0000000000"), row("ÚLTIMO", "2025-01-01", "2025-12-31", "3.05", "Resultado Antes do Resultado Financeiro e dos Tributos", "0.0000000000")].join("\n");
        let wanted = vec!["02421421000111".to_string()];
        assert_eq!(parse_accounts(&zeros, &wanted).len(), 2, "read");
        assert!(figures(&parse_accounts(&zeros, &wanted)).is_empty(), "but no figure");
        let real = [head.to_string(), row("ÚLTIMO", "2026-01-01", "2026-03-31", "3.01", "Receita de Venda de Bens e/ou Serviços", "6806150.0000000000")].join("\n");
        let f = figures(&parse_accounts(&real, &wanted));
        assert_eq!(f[0].revenue, Some(6_806_150_000.0), "thousands, the data's way of writing them");
    }

    #[test]
    fn tickers_lead_to_their_company() {
        let fca = "CNPJ_Companhia;Data_Referencia;Versao;ID_Documento;Nome_Empresarial;Valor_Mobiliario;Codigo_Negociacao;Mercado\n\
                   02.558.115/0001-21;2026-01-01;1;1;TIM S.A.;Ações Ordinárias;TIMS3;Bolsa\n\
                   02.558.157/0001-62;2026-01-01;1;2;TELEFÔNICA BRASIL S.A.;Ações Ordinárias;VIVT3;Bolsa\n\
                   00.000.000/0001-91;2026-01-01;1;3;BANCO DO BRASIL S.A.;Debêntures;;Balcão\n";
        let map = tickers(fca);
        assert_eq!(map["TIMS3"], ("02558115000121".to_string(), "TIM S.A.".to_string()));
        assert_eq!(map["VIVT3"].0, "02558157000162");
        assert_eq!(map.len(), 2);
        assert_eq!(url("itr", 2026), "https://dados.cvm.gov.br/dados/CIA_ABERTA/DOC/ITR/DADOS/itr_cia_aberta_2026.zip");
        assert_eq!(entry_name("itr", "DRE_con", 2026), "itr_cia_aberta_DRE_con_2026.csv");
    }
}
