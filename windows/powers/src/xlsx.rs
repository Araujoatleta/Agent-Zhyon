//! Excel files (.xlsx) as B3's Área do Investidor and other sites export them: each sheet's name and its rows of
//! text, shared strings and inline strings resolved, gaps filled. Numbers come as Excel writes them ("9.20"); a
//! date stored as a number (days since 1899-12-30) is turned into "dd/mm/yyyy" when its column says it is a date.

use crate::date::Date;
use crate::text::norm;
use crate::xml::{tree, Node};
use crate::zip::Archive;

#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub name: String,
    pub rows: Vec<Vec<String>>,
}

/// "A" → 0, "Z" → 25, "AA" → 26, from a cell reference ("BC12").
fn column_of(reference: &str) -> Option<usize> {
    let letters: String = reference.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if letters.is_empty() {
        return None;
    }
    let mut n = 0usize;
    for c in letters.chars() {
        n = n * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    Some(n - 1)
}

fn shared_strings(xml: &str) -> Vec<String> {
    tree(xml).all("si").into_iter().map(|si| si.all("t").into_iter().map(|t| t.text.clone()).collect::<String>()).collect()
}

fn cell_text(cell: &Node, shared: &[String]) -> String {
    let kind = cell.attr("t").unwrap_or("n");
    let value = cell.child("v").map(|v| v.text.trim().to_string()).unwrap_or_default();
    match kind {
        "s" => value.parse::<usize>().ok().and_then(|i| shared.get(i)).cloned().unwrap_or_default(),
        "inlineStr" => cell.all("t").into_iter().map(|t| t.text.clone()).collect(),
        "b" => if value == "1" { "TRUE".into() } else { "FALSE".into() },
        _ => value,
    }
}

/// An Excel serial day as a date: 46316 → 21/10/2026.
pub fn excel_date(serial: f64) -> Option<Date> {
    if !(20000.0..80000.0).contains(&serial) {
        return None;
    }
    Some(Date { y: 1899, m: 12, d: 30 }.add_days(serial.floor() as i64))
}

/// Every sheet of an .xlsx file, in the workbook's order.
pub fn read(bytes: &[u8]) -> Result<Vec<Sheet>, String> {
    let archive = Archive::new(bytes).map_err(|e| format!("not an Excel file: {e}"))?;
    let text = |name: &str| archive.read_named(name).map(|b| crate::text::decode(&b));
    let shared = text("xl/sharedStrings.xml").map(|x| shared_strings(&x)).unwrap_or_default();
    let workbook = tree(&text("xl/workbook.xml")?);
    let rels = text("xl/_rels/workbook.xml.rels").map(|x| tree(&x)).unwrap_or_default();
    let mut out = Vec::new();
    for (i, sheet) in workbook.all("sheet").into_iter().enumerate() {
        let name = sheet.attr("name").unwrap_or("").to_string();
        let target = sheet
            .attr("id")
            .and_then(|id| rels.all("Relationship").into_iter().find(|r| r.attr("Id") == Some(id)))
            .and_then(|r| r.attr("Target"))
            .map(|t| t.trim_start_matches('/').trim_start_matches("xl/").to_string())
            .unwrap_or_else(|| format!("worksheets/sheet{}.xml", i + 1));
        let Ok(xml) = text(&format!("xl/{target}")) else { continue };
        let doc = tree(&xml);
        let mut rows: Vec<Vec<String>> = Vec::new();
        for row in doc.all("row") {
            let mut cells: Vec<String> = Vec::new();
            for (j, c) in row.all("c").into_iter().enumerate() {
                let col = c.attr("r").and_then(column_of).unwrap_or(j.max(cells.len()));
                if col >= 16384 {
                    continue;
                }
                if cells.len() <= col {
                    cells.resize(col + 1, String::new());
                }
                cells[col] = cell_text(c, &shared);
            }
            let at = row.attr("r").and_then(|r| r.parse::<usize>().ok()).map(|r| r.saturating_sub(1)).unwrap_or(rows.len());
            while rows.len() < at.min(rows.len() + 1000) {
                rows.push(Vec::new());
            }
            rows.push(cells);
        }
        dates_in_date_columns(&mut rows);
        out.push(Sheet { name, rows });
    }
    if out.is_empty() {
        return Err("the Excel file has no sheet".into());
    }
    Ok(out)
}

/// Numbers in a column whose header says "data" (the first row that has text), as dates.
fn dates_in_date_columns(rows: &mut [Vec<String>]) {
    let Some(header_at) = rows.iter().position(|r| r.iter().any(|c| c.chars().any(|ch| ch.is_alphabetic()))) else { return };
    let date_cols: Vec<usize> = rows[header_at]
        .iter()
        .enumerate()
        .filter(|(_, h)| {
            let n = norm(h);
            n.starts_with("data") || n.contains("_data") || n.contains("vencimento") || n == "date"
        })
        .map(|(i, _)| i)
        .collect();
    for row in rows.iter_mut().skip(header_at + 1) {
        for &i in &date_cols {
            if let Some(cell) = row.get_mut(i) {
                if let Some(d) = cell.parse::<f64>().ok().and_then(excel_date) {
                    *cell = d.br();
                }
            }
        }
    }
}

/// A small .xlsx with one sheet per (name, rows), every cell a shared string, for tests and fixtures.
pub fn build(sheets: &[(&str, Vec<Vec<&str>>)]) -> Vec<u8> {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let mut strings: Vec<String> = Vec::new();
    let mut files: Vec<(String, String)> = Vec::new();
    let mut book = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>"#);
    let mut rels = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (i, (name, rows)) in sheets.iter().enumerate() {
        book.push_str(&format!(r#"<sheet name="{}" sheetId="{}" r:id="rId{}"/>"#, esc(name), i + 1, i + 1));
        rels.push_str(&format!(r#"<Relationship Id="rId{}" Type="worksheet" Target="worksheets/sheet{}.xml"/>"#, i + 1, i + 1));
        let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><worksheet><sheetData>"#);
        for (r, row) in rows.iter().enumerate() {
            xml.push_str(&format!(r#"<row r="{}">"#, r + 1));
            for (c, value) in row.iter().enumerate() {
                if value.is_empty() {
                    continue;
                }
                let col = (b'A' + c as u8) as char;
                if value.parse::<f64>().is_ok() {
                    xml.push_str(&format!(r#"<c r="{col}{}"><v>{value}</v></c>"#, r + 1));
                } else {
                    let idx = strings.iter().position(|s| s == value).unwrap_or_else(|| {
                        strings.push(value.to_string());
                        strings.len() - 1
                    });
                    xml.push_str(&format!(r#"<c r="{col}{}" t="s"><v>{idx}</v></c>"#, r + 1));
                }
            }
            xml.push_str("</row>");
        }
        xml.push_str("</sheetData></worksheet>");
        files.push((format!("xl/worksheets/sheet{}.xml", i + 1), xml));
    }
    book.push_str("</sheets></workbook>");
    rels.push_str("</Relationships>");
    let mut sst = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><sst>"#);
    for s in &strings {
        sst.push_str(&format!("<si><t>{}</t></si>", esc(s)));
    }
    sst.push_str("</sst>");
    files.push(("xl/workbook.xml".into(), book));
    files.push(("xl/_rels/workbook.xml.rels".into(), rels));
    files.push(("xl/sharedStrings.xml".into(), sst));
    let refs: Vec<(&str, &[u8])> = files.iter().map(|(n, x)| (n.as_str(), x.as_bytes())).collect();
    crate::zip::build(&refs, true)
}

/// A cell of a workbook AXYON writes (backup.rs).
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Text(String),
    Number(f64),
    /// Reais: "R$ 1.234,56" in the reader's own way of writing numbers.
    Money(f64),
    /// A column's title: bold on the accent.
    Head(String),
}

/// A sheet of a workbook AXYON writes: its columns' widths (characters), its rows, and whether it is out of sight.
#[derive(Debug, Clone, Default)]
pub struct Tab {
    pub name: String,
    pub widths: Vec<f64>,
    pub rows: Vec<Vec<Cell>>,
    pub hidden: bool,
}

/// "A" for 0, "Z" for 25, "AA" for 26.
fn letters(mut column: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (column % 26) as u8);
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// A workbook Excel, Numbers and Google Sheets open: the tabs in order (the first in sight), the titles' row frozen,
/// text written inline, numbers as numbers.
pub fn write(tabs: &[Tab]) -> Vec<u8> {
    const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    const RELS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let esc = |s: &str| {
        s.chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .collect::<String>()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let mut files: Vec<(String, String)> = Vec::new();
    let mut types = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#);
    let mut book = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="{MAIN}" xmlns:r="{RELS}"><sheets>"#);
    let mut rels = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (i, tab) in tabs.iter().enumerate() {
        let n = i + 1;
        // Excel takes 31 characters for a sheet's name, none of : \ / ? * [ ].
        let name: String = tab.name.chars().filter(|c| !":\\/?*[]".contains(*c)).take(31).collect();
        let state = if tab.hidden && i > 0 { r#" state="hidden""# } else { "" };
        book.push_str(&format!(r#"<sheet name="{}" sheetId="{n}"{state} r:id="rId{n}"/>"#, esc(&name)));
        rels.push_str(&format!(r#"<Relationship Id="rId{n}" Type="{RELS}/worksheet" Target="worksheets/sheet{n}.xml"/>"#));
        types.push_str(&format!(r#"<Override PartName="/xl/worksheets/sheet{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#));
        let mut xml = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="{MAIN}" xmlns:r="{RELS}">"#);
        if tab.rows.first().is_some_and(|r| r.iter().any(|c| matches!(c, Cell::Head(_)))) {
            xml.push_str(r#"<sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews>"#);
        }
        if !tab.widths.is_empty() {
            xml.push_str("<cols>");
            for (c, w) in tab.widths.iter().enumerate() {
                xml.push_str(&format!(r#"<col min="{0}" max="{0}" width="{w}" customWidth="1"/>"#, c + 1));
            }
            xml.push_str("</cols>");
        }
        xml.push_str("<sheetData>");
        for (r, row) in tab.rows.iter().enumerate() {
            xml.push_str(&format!(r#"<row r="{}">"#, r + 1));
            for (c, cell) in row.iter().enumerate() {
                let at = format!("{}{}", letters(c), r + 1);
                match cell {
                    Cell::Text(t) if t.is_empty() => {}
                    Cell::Text(t) => xml.push_str(&format!(r#"<c r="{at}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#, esc(t))),
                    Cell::Head(t) => xml.push_str(&format!(r#"<c r="{at}" s="1" t="inlineStr"><is><t>{}</t></is></c>"#, esc(t))),
                    Cell::Number(x) if x.is_finite() => xml.push_str(&format!(r#"<c r="{at}" s="3"><v>{x}</v></c>"#)),
                    Cell::Money(x) if x.is_finite() => xml.push_str(&format!(r#"<c r="{at}" s="2"><v>{x}</v></c>"#)),
                    _ => {}
                }
            }
            xml.push_str("</row>");
        }
        xml.push_str("</sheetData></worksheet>");
        files.push((format!("xl/worksheets/sheet{n}.xml"), xml));
    }
    let styles_id = tabs.len() + 1;
    rels.push_str(&format!(r#"<Relationship Id="rId{styles_id}" Type="{RELS}/styles" Target="styles.xml"/></Relationships>"#));
    book.push_str("</sheets></workbook>");
    types.push_str("</Types>");
    let styles = format!(
        r##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="{MAIN}"><numFmts count="2"><numFmt numFmtId="164" formatCode="&quot;R$&quot;\ #,##0.00"/><numFmt numFmtId="165" formatCode="#,##0.########"/></numFmts><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><color rgb="FFFFFFFF"/><name val="Calibri"/></font></fonts><fills count="3"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill><fill><patternFill patternType="solid"><fgColor rgb="FF4B4FD6"/><bgColor indexed="64"/></patternFill></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="4"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="1" fillId="2" borderId="0" xfId="0" applyFont="1" applyFill="1"/><xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/><xf numFmtId="165" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"##
    );
    let root = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#);
    let mut all: Vec<(String, String)> = vec![("[Content_Types].xml".into(), types), ("_rels/.rels".into(), root), ("xl/workbook.xml".into(), book), ("xl/_rels/workbook.xml.rels".into(), rels), ("xl/styles.xml".into(), styles)];
    all.extend(files);
    let refs: Vec<(&str, &[u8])> = all.iter().map(|(n, x)| (n.as_str(), x.as_bytes())).collect();
    crate::zip::build(&refs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_workbook_written_is_read_back_whole() {
        let tabs = vec![
            Tab { name: "Lançamentos".into(), widths: vec![12.0, 10.0], rows: vec![vec![Cell::Head("Data".into()), Cell::Head("Ativo".into()), Cell::Head("Preço".into())], vec![Cell::Text("21/10/2026".into()), Cell::Text("GARE11 & cia <x>".into()), Cell::Money(8.5)]], hidden: false },
            Tab { name: "Oculta".into(), widths: vec![], rows: (0..30).map(|i| vec![Cell::Number(i as f64)]).collect(), hidden: true },
        ];
        let bytes = write(&tabs);
        let sheets = read(&bytes).unwrap();
        assert_eq!(sheets.len(), 2);
        assert_eq!(sheets[0].name, "Lançamentos");
        assert_eq!(sheets[0].rows[1], vec!["21/10/2026", "GARE11 & cia <x>", "8.5"]);
        assert_eq!(sheets[1].rows[29][0], "29");
        assert_eq!(letters(0), "A");
        assert_eq!(letters(27), "AB");
        assert_eq!(letters(701), "ZZ");
        assert_eq!(letters(702), "AAA");
        // The archive's checksums are right: what Excel checks before it opens a file.
        let archive = crate::zip::Archive::new(&bytes).unwrap();
        let book = archive.read_named("xl/workbook.xml").unwrap();
        assert!(String::from_utf8(book).unwrap().contains(r#"state="hidden""#));
    }

    #[test]
    fn sheets_come_back_with_their_names_and_cells() {
        let bytes = build(&[
            ("Acoes", vec![vec!["Produto", "Código de Negociação", "Quantidade"], vec!["TIMS3 - TIM S.A.", "TIMS3", "21"]]),
            ("Fundo de Investimento", vec![vec!["Código de Negociação", "Quantidade", "Data"], vec!["GGRC11", "150", "46316"]]),
        ]);
        let sheets = read(&bytes).unwrap();
        assert_eq!(sheets.len(), 2);
        assert_eq!(sheets[0].name, "Acoes");
        assert_eq!(sheets[0].rows[1], vec!["TIMS3 - TIM S.A.", "TIMS3", "21"]);
        assert_eq!(sheets[1].rows[1][2], "21/10/2026", "a serial in a date column is a date");
        assert!(read(b"not excel").is_err());
    }

    #[test]
    fn gaps_and_references() {
        assert_eq!(column_of("A1"), Some(0));
        assert_eq!(column_of("AB7"), Some(27));
        assert_eq!(column_of("12"), None);
        assert_eq!(excel_date(46316.0), Date::new(2026, 10, 21));
        assert_eq!(excel_date(3.0), None);
    }
}
