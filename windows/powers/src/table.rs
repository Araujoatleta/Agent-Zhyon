//! Tables from files and from pasted text: CSV with `;`, `,` or tabs (quotes and all), and columns found by their
//! header the way a person would find them ("Preço Médio", "preco medio", "PM"), so a renamed or reordered column in
//! an official file does not break the reading.

use crate::text::norm;

/// Rows of cells, the header first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// The separator a CSV uses: the one of `;`, tab and `,` that appears most in its first line.
pub fn separator(first_line: &str) -> char {
    let count = |c: char| first_line.matches(c).count();
    [(';', count(';')), ('\t', count('\t')), (',', count(','))]
        .into_iter()
        .max_by_key(|&(_, n)| n)
        .filter(|&(_, n)| n > 0)
        .map(|(c, _)| c)
        .unwrap_or(';')
}

/// Every record of a CSV text, its cells unquoted. A quoted cell may hold the separator, quotes ("") and newlines.
pub fn records(text: &str, sep: char) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cell.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
            continue;
        }
        match c {
            '"' if cell.trim().is_empty() => {
                cell.clear();
                quoted = true;
            }
            c if c == sep => row.push(std::mem::take(&mut cell)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut cell));
                if row.iter().any(|c| !c.trim().is_empty()) {
                    out.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            c => cell.push(c),
        }
    }
    if any && (!cell.is_empty() || !row.is_empty()) {
        row.push(cell);
        if row.iter().any(|c| !c.trim().is_empty()) {
            out.push(row);
        }
    }
    out
}

impl Table {
    /// A CSV text with its header on the first line.
    pub fn csv(text: &str) -> Table {
        let sep = separator(text.lines().next().unwrap_or(""));
        let mut all = records(text, sep);
        if all.is_empty() {
            return Table::default();
        }
        let headers = all.remove(0).into_iter().map(|h| h.trim().to_string()).collect();
        Table { headers, rows: all }
    }

    /// Rows whose header is the first row (anywhere) that has a cell matching every one of `must` (see `find`).
    /// Exports often start with a title or blank lines.
    pub fn from_rows(rows: Vec<Vec<String>>, must: &[&[&str]]) -> Option<Table> {
        let at = rows.iter().position(|row| {
            let names: Vec<String> = row.iter().map(|c| norm(c)).collect();
            must.iter().all(|cands| names.iter().any(|n| matches(n, cands)))
        })?;
        let headers = rows[at].iter().map(|h| h.trim().to_string()).collect();
        Some(Table { headers, rows: rows[at + 1..].to_vec() })
    }

    /// The column whose header matches one of `candidates`, in their order: equal first, then starting with, then
    /// containing (all after `norm`). None when there is none.
    pub fn find(&self, candidates: &[&str]) -> Option<usize> {
        let names: Vec<String> = self.headers.iter().map(|h| norm(h)).collect();
        for c in candidates {
            if let Some(i) = names.iter().position(|n| n == c) {
                return Some(i);
            }
        }
        for c in candidates {
            if let Some(i) = names.iter().position(|n| n.starts_with(c)) {
                return Some(i);
            }
        }
        for c in candidates {
            if c.len() >= 4 {
                if let Some(i) = names.iter().position(|n| n.contains(c)) {
                    return Some(i);
                }
            }
        }
        None
    }

    /// A row's cell in column `i`, trimmed ("" past its end).
    pub fn cell<'a>(row: &'a [String], i: Option<usize>) -> &'a str {
        i.and_then(|i| row.get(i)).map(|s| s.trim()).unwrap_or("")
    }
}

fn matches(name: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|c| name == *c || name.starts_with(c) || (c.len() >= 4 && name.contains(c)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_with_quotes_and_any_separator() {
        let t = Table::csv("Ativo;Quantidade;\"Preço; Médio\"\nGGRC11;150;\"9,20\"\r\n\nGARE11;180;8,55\n");
        assert_eq!(t.headers, vec!["Ativo", "Quantidade", "Preço; Médio"]);
        assert_eq!(t.rows, vec![vec!["GGRC11", "150", "9,20"], vec!["GARE11", "180", "8,55"]]);
        let c = Table::csv("a,b\n\"x \"\"y\"\"\",2");
        assert_eq!(c.rows[0], vec!["x \"y\"", "2"]);
        assert_eq!(separator("a\tb\tc"), '\t');
        assert_eq!(separator("só uma coluna"), ';');
        let multi = records("a;\"linha 1\nlinha 2\";c\n", ';');
        assert_eq!(multi[0][1], "linha 1\nlinha 2");
    }

    #[test]
    fn columns_are_found_by_what_their_header_means() {
        let t = Table::csv("CNPJ_Fundo_Classe;Data_Referencia;Valor_Patrimonial_Cotas;Percentual_Dividend_Yield_Mes\n1;2;3;4");
        assert_eq!(t.find(&["cnpj_fundo"]), Some(0));
        assert_eq!(t.find(&["valor_patrimonial_cota"]), Some(2));
        assert_eq!(t.find(&["dividend_yield"]), Some(3));
        assert_eq!(t.find(&["nao_existe"]), None);
        let rows = vec![
            vec!["Relatório de posição".to_string()],
            vec![],
            vec!["Produto".into(), "Código de Negociação".into(), "Quantidade".into()],
            vec!["GGRC11 - FII".into(), "GGRC11".into(), "150".into()],
        ];
        let t = Table::from_rows(rows, &[&["codigo_de_negociacao", "codigo"], &["quantidade"]]).unwrap();
        assert_eq!(t.rows.len(), 1);
        assert_eq!(Table::cell(&t.rows[0], t.find(&["quantidade"])), "150");
        assert_eq!(Table::cell(&t.rows[0], Some(9)), "");
    }
}
