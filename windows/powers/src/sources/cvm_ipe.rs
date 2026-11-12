//! The CVM's open data on what listed companies file besides their statements (IPE, "informações periódicas e
//! eventuais"): material facts, notices to the market and to shareholders, assemblies, results releases and
//! presentations, each with the address of its file at the CVM (RAD). One ZIP a year, a CSV inside. Free, no key.

use std::collections::HashMap;

use crate::date::Date;
use crate::library::Listed;
use crate::table::{records, separator, Table};
use crate::text::{cnpj_chars, norm, num};

pub fn url(year: i32) -> String {
    format!("https://dados.cvm.gov.br/dados/CIA_ABERTA/DOC/IPE/DADOS/ipe_cia_aberta_{year}.zip")
}

pub fn entry_name(year: i32) -> String {
    format!("ipe_cia_aberta_{year}.csv")
}

/// The documents of the companies whose CNPJs (digits) are `cnpjs`, by CNPJ, newest first: the latest version of
/// each, only what the reader reads (library.rs, `Listed::doc_kind`), at most `most` each. Only the lines that name one
/// of the CNPJs are split, so a year of the whole market reads quickly.
pub fn parse(text: &str, cnpjs: &[String], most: usize) -> HashMap<String, Vec<Listed>> {
    let mut out: HashMap<String, Vec<(f64, Listed)>> = HashMap::new();
    let mut lines = text.lines();
    let Some(head) = lines.next() else { return HashMap::new() };
    let sep = separator(head);
    let header: Vec<String> = records(head, sep).into_iter().next().unwrap_or_default();
    let col = |names: &[&str]| header.iter().position(|h| names.iter().any(|n| norm(h) == *n));
    let (c_cnpj, c_cat, c_kind, c_species, c_subject, c_ref, c_delivered, c_protocol, c_version, c_link) = (
        col(&["cnpj_companhia", "cnpj_cia", "cnpj"]),
        col(&["categoria"]),
        col(&["tipo"]),
        col(&["especie"]),
        col(&["assunto"]),
        col(&["data_referencia", "dt_referencia"]),
        col(&["data_entrega", "dt_entrega"]),
        col(&["protocolo_entrega", "protocolo"]),
        col(&["versao"]),
        col(&["link_download", "link_documento", "link"]),
    );
    if c_cnpj.is_none() || c_link.is_none() {
        return HashMap::new();
    }
    let digits: Vec<&String> = cnpjs.iter().collect();
    let shown: Vec<String> = cnpjs.iter().map(|c| crate::text::cnpj_format(c)).collect();
    for line in lines {
        if !shown.iter().any(|c| line.contains(c.as_str())) && !digits.iter().any(|c| line.contains(c.as_str())) {
            continue;
        }
        let Some(row) = records(line, sep).into_iter().next() else { continue };
        let cnpj = cnpj_chars(Table::cell(&row, c_cnpj));
        if !cnpjs.contains(&cnpj) {
            continue;
        }
        let protocol = Table::cell(&row, c_protocol).trim().to_string();
        let listed = Listed {
            key: format!("ipe:{}", if protocol.is_empty() { Table::cell(&row, c_link).to_string() } else { protocol }),
            category: Table::cell(&row, c_cat).trim().to_string(),
            kind: Table::cell(&row, c_kind).trim().to_string(),
            subject: [Table::cell(&row, c_species), Table::cell(&row, c_subject)].iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" — "),
            reference: Table::cell(&row, c_ref).trim().to_string(),
            delivered: Date::parse(Table::cell(&row, c_delivered).get(..10).unwrap_or(Table::cell(&row, c_delivered))),
            download: Table::cell(&row, c_link).trim().to_string(),
        };
        if listed.doc_kind().is_none() || listed.download.is_empty() {
            continue;
        }
        let version = num(Table::cell(&row, c_version)).unwrap_or(0.0);
        let list = out.entry(cnpj).or_default();
        match list.iter_mut().find(|(_, l)| l.key == listed.key) {
            Some(slot) if slot.0 < version => *slot = (version, listed),
            Some(_) => {}
            None => list.push((version, listed)),
        }
    }
    out.into_iter()
        .map(|(cnpj, list)| {
            let mut docs: Vec<Listed> = list.into_iter().map(|(_, l)| l).collect();
            docs.sort_by(|a, b| b.delivered.cmp(&a.delivered));
            docs.truncate(most);
            (cnpj, docs)
        })
        .collect()
}

#[cfg(test)]
pub mod fixtures {
    pub fn csv() -> String {
        [
            "CNPJ_Companhia;Nome_Companhia;Codigo_CVM;Data_Referencia;Categoria;Tipo;Especie;Assunto;Data_Entrega;Tipo_Apresentacao;Protocolo_Entrega;Versao;Link_Download",
            "02.558.115/0001-21;TIM S.A.;022233;2026-10-20;Fato Relevante;;;Aquisição de ativos;2026-10-20;AP - Apresentação;001234;1;https://www.rad.cvm.gov.br/ENET/frmDownloadDocumento.aspx?Tela=ext&numSequencia=1&numVersao=1&numProtocolo=1234&descTipo=IPE&CodigoInstituicao=1",
            "02.558.115/0001-21;TIM S.A.;022233;2026-10-20;Fato Relevante;;;Aquisição de ativos;2026-10-21;RE - Reapresentação;001234;2;https://www.rad.cvm.gov.br/ENET/frmDownloadDocumento.aspx?numVersao=2",
            "02.558.115/0001-21;TIM S.A.;022233;2026-09-30;Dados Econômico-Financeiros;Release de Resultados;;3T26;2026-10-27;AP - Apresentação;001300;1;https://www.rad.cvm.gov.br/ENET/x?release",
            "02.558.115/0001-21;TIM S.A.;022233;2026-10-01;Calendário de Eventos Corporativos;;;;2026-10-01;AP - Apresentação;001100;1;https://www.rad.cvm.gov.br/ENET/x?calendar",
            "11.222.333/0001-81;OUTRA S.A.;099999;2026-10-20;Fato Relevante;;;Algo;2026-10-20;AP - Apresentação;005555;1;https://www.rad.cvm.gov.br/ENET/x?other",
        ]
        .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::DocKind;

    #[test]
    fn a_companys_material_facts_and_releases_with_their_latest_version() {
        let got = parse(&fixtures::csv(), &["02558115000121".to_string()], 10);
        assert_eq!(got.len(), 1, "only the companies asked for");
        let docs = &got["02558115000121"];
        let keys: Vec<(&str, Option<DocKind>)> = docs.iter().map(|d| (d.key.as_str(), d.doc_kind())).collect();
        assert_eq!(keys, [("ipe:001300", Some(DocKind::Release)), ("ipe:001234", Some(DocKind::MaterialFact))], "newest first; not the calendar");
        assert!(docs[1].download.ends_with("numVersao=2"), "the latest version");
        assert_eq!(docs[1].delivered, Date::new(2026, 10, 21));
        assert_eq!(docs[0].subject, "3T26");
        assert_eq!(url(2026), "https://dados.cvm.gov.br/dados/CIA_ABERTA/DOC/IPE/DADOS/ipe_cia_aberta_2026.zip");
        assert!(parse("", &["02558115000121".to_string()], 10).is_empty());
        assert!(parse("A;B\n1;2", &["02558115000121".to_string()], 10).is_empty(), "a file without the columns gives nothing");
    }
}
