//! Just enough XML for spreadsheets (XLSX) and FundosNet's structured documents: elements, attributes, text, CDATA
//! and the entities; namespaces are dropped from names ("x:row" is "row"). No DTD, no validation.

/// One piece of an XML text.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Start { name: String, attrs: Vec<(String, String)> },
    /// `<name/>`.
    Empty { name: String, attrs: Vec<(String, String)> },
    End { name: String },
    Text(String),
}

/// The text of a name without its prefix: "x:row" → "row".
fn local(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

/// `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;` and `&#NN;` / `&#xHH;` back to their characters; anything else kept.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail.find(';').filter(|&e| e <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let ch = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e if e.starts_with("#x") || e.starts_with("#X") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn attributes(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        let Some(eq) = rest.find('=') else { break };
        let name = rest[..eq].trim();
        let after = rest[eq + 1..].trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
        let Some(close) = after[1..].find(quote) else { break };
        out.push((local(name), unescape(&after[1..1 + close])));
        rest = after[close + 2..].trim_start();
    }
    out
}

/// The tokens of an XML text, in order. Declarations, comments and doctypes are skipped; text between elements comes
/// as it is (whitespace included), unescaped.
pub fn tokens(xml: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut rest = xml;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            out.push(Token::Text(unescape(rest)));
            break;
        };
        if lt > 0 {
            out.push(Token::Text(unescape(&rest[..lt])));
        }
        let tag = &rest[lt..];
        if let Some(body) = tag.strip_prefix("<![CDATA[") {
            let end = body.find("]]>").unwrap_or(body.len());
            out.push(Token::Text(body[..end].to_string()));
            rest = body.get(end + 3..).unwrap_or("");
            continue;
        }
        if tag.starts_with("<!--") {
            rest = tag.find("-->").map(|e| &tag[e + 3..]).unwrap_or("");
            continue;
        }
        let Some(gt) = tag.find('>') else { break };
        let inner = &tag[1..gt];
        rest = &tag[gt + 1..];
        if inner.starts_with('?') || inner.starts_with('!') {
            continue;
        }
        if let Some(name) = inner.strip_prefix('/') {
            out.push(Token::End { name: local(name.trim()) });
            continue;
        }
        let empty = inner.ends_with('/');
        let inner = inner.trim_end_matches('/');
        let (name, attrs) = match inner.find(char::is_whitespace) {
            Some(i) => (&inner[..i], attributes(&inner[i..])),
            None => (inner, Vec::new()),
        };
        let name = local(name.trim());
        out.push(if empty { Token::Empty { name, attrs } } else { Token::Start { name, attrs } });
    }
    out
}

/// An element with its attributes, its children and its own text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// The first child named `name` (case-insensitive).
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// Every element named `name` below this one, at any depth, in document order.
    pub fn all<'a>(&'a self, name: &str) -> Vec<&'a Node> {
        let mut out = Vec::new();
        self.collect(name, &mut out);
        out
    }

    fn collect<'a>(&'a self, name: &str, out: &mut Vec<&'a Node>) {
        for c in &self.children {
            if c.name.eq_ignore_ascii_case(name) {
                out.push(c);
            }
            c.collect(name, out);
        }
    }

    /// The trimmed text of the child named `part`, else of the first child whose name holds it (case-insensitive):
    /// "DataPagamento" for "pagamento". None when it is empty.
    pub fn text_of(&self, part: &str) -> Option<&str> {
        let part = part.to_ascii_lowercase();
        let named = |c: &&Node| c.name.to_ascii_lowercase() == part;
        let holds = |c: &&Node| c.name.to_ascii_lowercase().contains(&part);
        self.children.iter().find(named).or_else(|| self.children.iter().find(holds)).map(|c| c.text.trim()).filter(|t| !t.is_empty())
    }

    /// All its text and its children's, joined.
    pub fn deep_text(&self) -> String {
        let mut out = self.text.clone();
        for c in &self.children {
            out.push_str(&c.deep_text());
        }
        out
    }
}

/// The document as a tree under a root without a name. A missing end tag closes at the end of the text.
pub fn tree(xml: &str) -> Node {
    let mut stack = vec![Node::default()];
    for token in tokens(xml) {
        match token {
            Token::Start { name, attrs } => stack.push(Node { name, attrs, ..Node::default() }),
            Token::Empty { name, attrs } => {
                let node = Node { name, attrs, ..Node::default() };
                stack.last_mut().expect("root").children.push(node);
            }
            Token::End { name } => {
                // Close up to the matching element (tolerating a missing end tag in between).
                if stack.iter().skip(1).any(|n| n.name == name) {
                    while stack.len() > 1 {
                        let node = stack.pop().expect("open element");
                        let done = node.name == name;
                        stack.last_mut().expect("root").children.push(node);
                        if done {
                            break;
                        }
                    }
                }
            }
            Token::Text(t) => stack.last_mut().expect("root").text.push_str(&t),
        }
    }
    while stack.len() > 1 {
        let node = stack.pop().expect("open element");
        stack.last_mut().expect("root").children.push(node);
    }
    stack.pop().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elements_attributes_text_and_entities() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- comment -->
<x:worksheet xmlns:x="ns"><x:row r="1"><x:c r="A1" t="s"><x:v>0</x:v></x:c><x:c r='B1' t="inlineStr"><x:is><x:t>Pre&#231;o &amp; m&#xE9;dio</x:t></x:is></x:c></x:row><x:row r="2"/></x:worksheet>"#;
        let root = tree(xml);
        let sheet = root.child("worksheet").unwrap();
        let rows = sheet.all("row");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].attr("r"), Some("1"));
        let cells = rows[0].all("c");
        assert_eq!(cells[0].child("v").unwrap().text, "0");
        assert_eq!(cells[1].all("t")[0].text, "Preço & médio");
        assert_eq!(cells[1].attr("t"), Some("inlineStr"));
        assert_eq!(unescape("a &unknown; b &lt;"), "a &unknown; b <");
        assert_eq!(unescape("R&D"), "R&D");
    }

    #[test]
    fn cdata_and_lookup_by_part_of_a_name() {
        let root = tree("<Doc><Rendimento><DataBase>2026-10-08</DataBase><DataPagamento> 2026-10-15 </DataPagamento><Obs><![CDATA[a < b]]></Obs></Rendimento></Doc>");
        let r = root.all("rendimento")[0];
        assert_eq!(r.text_of("pagamento"), Some("2026-10-15"));
        assert_eq!(r.text_of("database"), Some("2026-10-08"));
        assert_eq!(r.text_of("obs"), Some("a < b"));
        assert_eq!(r.text_of("nada"), None);
        // A missing end tag does not lose what came after it.
        let broken = tree("<a><b>1<c>2</a>");
        assert_eq!(broken.child("a").unwrap().deep_text(), "12");
    }
}
