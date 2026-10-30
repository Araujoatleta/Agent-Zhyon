//! The reader of documents ("the intern who reads everything"): the managers' reports, material facts, notices,
//! assemblies and offerings of the user's funds (FundosNet) and the material facts, notices and results releases of
//! their companies (the CVM's IPE), each read once: its text page by page (the app reads the PDF itself: PDFKit on the
//! iPhone), the figures a report states found in it by this code (vacancy, default, dividend and result per share,
//! LTV, holders), what changed against the same kind of document before, and the brain's digest of it, which cites
//! its pages. Kept in a file of its own beside the store (powers-docs.json): the text is large, and only these
//! features read it. A document is analysed once, whoever asks: what is public is read once.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::date::Date;
use crate::model::{AssetClass, Market, Store};
use crate::sources::fnet::{self, Topic};
use crate::text::{clip, fold, grouped, pct};

/// The most pages kept of a document, and characters of a page.
pub const MOST_PAGES: usize = 60;
pub const PAGE_CHARS: usize = 5_000;
/// The most documents kept in all (the oldest go first).
pub const MOST_DOCS: usize = 150;
/// How many times a document that could not be read is tried again (once a day).
pub const TRIES: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    MaterialFact,
    Report,
    Release,
    Assembly,
    Offering,
    Notice,
    Other,
}

impl DocKind {
    pub fn id(self) -> &'static str {
        match self {
            DocKind::MaterialFact => "material_fact",
            DocKind::Report => "report",
            DocKind::Release => "release",
            DocKind::Assembly => "assembly",
            DocKind::Offering => "offering",
            DocKind::Notice => "notice",
            DocKind::Other => "other",
        }
    }

    /// In English, for the brain.
    pub fn label(self) -> &'static str {
        match self {
            DocKind::MaterialFact => "material fact",
            DocKind::Report => "manager's monthly report",
            DocKind::Release => "results release",
            DocKind::Assembly => "assembly",
            DocKind::Offering => "offering of new shares",
            DocKind::Notice => "notice",
            DocKind::Other => "document",
        }
    }

    /// Worth telling the user as soon as it is read.
    pub fn urgent(self) -> bool {
        matches!(self, DocKind::MaterialFact | DocKind::Assembly | DocKind::Offering)
    }

    fn of_topic(t: Topic) -> Option<DocKind> {
        match t {
            Topic::MaterialFact => Some(DocKind::MaterialFact),
            Topic::Report => Some(DocKind::Report),
            Topic::Assembly => Some(DocKind::Assembly),
            Topic::Offering => Some(DocKind::Offering),
            Topic::Notice => Some(DocKind::Notice),
            Topic::Payout | Topic::Filing | Topic::Other => None,
        }
    }
}

/// A figure a document states, as this code found it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Figure {
    pub key: String,
    /// In Portuguese, as reports say it ("Vacância física").
    pub label: String,
    pub value: f64,
    /// "%", "R$" or "" (a count).
    pub unit: String,
    /// The page it was found on, from 1.
    pub page: u32,
}

impl Figure {
    pub fn shown(&self) -> String {
        match self.unit.as_str() {
            "%" => pct(self.value, 1),
            "R$" => format!("R$ {}", grouped(self.value, if self.value < 10.0 { 3 } else { 2 })),
            _ => grouped(self.value, 0),
        }
    }
}

/// A point of a digest, with the page it rests on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Point {
    pub text: String,
    pub page: Option<u32>,
}

/// The brain's reading of a document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Digest {
    pub headline: String,
    pub points: Vec<Point>,
    /// "better", "worse", "same" or "mixed", for a holder.
    pub tone: String,
    /// Whether it may hurt the user's thesis on the asset.
    pub thesis_risk: bool,
    pub made: Option<Date>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Doc {
    /// "fnet:904" or "ipe:<protocol>".
    pub key: String,
    pub ticker: String,
    pub kind: DocKind,
    pub title: String,
    /// What it refers to: a month ("09/2026") or a day.
    #[serde(default)]
    pub reference: String,
    pub delivered: Option<Date>,
    /// Its page, to open.
    pub url: String,
    /// The file.
    pub download: String,
    #[serde(default)]
    pub pages: Vec<String>,
    #[serde(default)]
    pub figures: Vec<Figure>,
    #[serde(default)]
    pub digest: Option<Digest>,
    #[serde(default)]
    pub read: Option<Date>,
    #[serde(default)]
    pub tries: u8,
    #[serde(default)]
    pub tried: Option<Date>,
    #[serde(default)]
    pub problem: String,
}

impl Doc {
    pub fn is_read(&self) -> bool {
        self.read.is_some() && !self.pages.is_empty()
    }

    /// Its whole text, pages marked.
    pub fn text(&self) -> String {
        self.pages.iter().enumerate().map(|(i, p)| format!("[Page {}]\n{p}", i + 1)).collect::<Vec<_>>().join("\n\n")
    }

    /// A line for a list: its headline when the brain read it, else what it is.
    pub fn headline(&self) -> String {
        match &self.digest {
            Some(d) if !d.headline.trim().is_empty() => d.headline.trim().to_string(),
            _ => self.title.clone(),
        }
    }
}

/// A document to read: what it is and where its file is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub key: String,
    pub ticker: String,
    pub kind: DocKind,
    pub title: String,
    pub reference: String,
    pub delivered: Option<Date>,
    pub url: String,
    pub download: String,
}

/// A company's document listed by the CVM (IPE), as kept with its figures (model.rs `CompanyData.documents`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Listed {
    pub key: String,
    pub category: String,
    pub kind: String,
    pub subject: String,
    pub reference: String,
    pub delivered: Option<Date>,
    pub download: String,
}

impl Listed {
    /// What it is about, from its category, type and subject.
    pub fn doc_kind(&self) -> Option<DocKind> {
        let all = crate::text::plain(&format!("{} {} {}", self.category, self.kind, self.subject));
        if all.contains("fato relevante") {
            Some(DocKind::MaterialFact)
        } else if all.contains("release") || all.contains("apresentac") || all.contains("resultado") {
            Some(DocKind::Release)
        } else if all.contains("assembleia") {
            Some(DocKind::Assembly)
        } else if all.contains("oferta") || all.contains("emissao") {
            Some(DocKind::Offering)
        } else if all.contains("comunicado") || all.contains("aviso aos acionistas") {
            Some(DocKind::Notice)
        } else {
            None
        }
    }
}

/// What changed in a figure since the same kind of document before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub key: String,
    pub label: String,
    pub before: f64,
    pub after: f64,
    pub unit: String,
    pub page: u32,
}

impl Change {
    pub fn shown(&self) -> String {
        let f = |v: f64| Figure { key: String::new(), label: String::new(), value: v, unit: self.unit.clone(), page: 0 }.shown();
        format!("{}: {} → {} (p. {})", self.label, f(self.before), f(self.after), self.page)
    }
}

/// The documents read, newest first as they come.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Library {
    pub docs: Vec<Doc>,
}

/// The assets whose documents are read: those held (funds and shares) and those of a thesis.
pub fn followed(store: &Store) -> Vec<String> {
    let mut out: Vec<String> = store
        .portfolio
        .positions
        .iter()
        .filter(|p| p.quantity > 0.0 && matches!(p.class, AssetClass::Fii | AssetClass::Acao))
        .map(|p| p.ticker.clone())
        .collect();
    for t in store.theses.iter().map(|t| &t.ticker) {
        if !out.contains(t) {
            out.push(t.clone());
        }
    }
    out
}

impl Library {
    pub fn get(&self, key: &str) -> Option<&Doc> {
        self.docs.iter().find(|d| d.key == key)
    }

    fn get_mut(&mut self, key: &str) -> Option<&mut Doc> {
        self.docs.iter_mut().find(|d| d.key == key)
    }

    /// What to read now, most important first (a material fact, then the latest report, an assembly, an offering, a
    /// notice), at most `most`: a document of an asset followed, delivered within its window, never read, or not read
    /// today after a failure (at most `TRIES` times).
    pub fn pending(&self, store: &Store, market: &Market, today: Date, most: usize) -> Vec<Pending> {
        let mut out: Vec<Pending> = Vec::new();
        let wanted = |p: &Pending| -> bool {
            match self.get(&p.key) {
                None => true,
                Some(d) => !d.is_read() && d.tries < TRIES && d.tried != Some(today),
            }
        };
        let window = |k: DocKind| -> i64 {
            match k {
                DocKind::Report | DocKind::Release => 70,
                DocKind::MaterialFact => 120,
                _ => 60,
            }
        };
        for t in followed(store) {
            if let Some(fund) = market.funds.get(&t) {
                let mut reports = 0;
                for d in fund.documents.iter().filter(|d| !d.structured) {
                    let Some(kind) = DocKind::of_topic(d.topic) else { continue };
                    if d.delivered.is_none_or(|day| day.until(today) > window(kind)) {
                        continue;
                    }
                    if kind == DocKind::Report {
                        reports += 1;
                        if reports > 2 {
                            continue;
                        }
                    }
                    let title = [d.kind.as_str(), d.category.as_str()].into_iter().find(|s| !s.trim().is_empty()).unwrap_or(kind.label()).to_string();
                    let p = Pending {
                        key: format!("fnet:{}", d.id),
                        ticker: t.clone(),
                        kind,
                        title: if d.reference.is_empty() { title } else { format!("{title} {}", d.reference) },
                        reference: d.reference.clone(),
                        delivered: d.delivered,
                        url: d.url(),
                        download: fnet::download_url(d.id),
                    };
                    if wanted(&p) {
                        out.push(p);
                    }
                }
            }
            if let Some(company) = market.companies.get(&t) {
                for d in &company.documents {
                    let Some(kind) = d.doc_kind() else { continue };
                    if d.delivered.is_none_or(|day| day.until(today) > window(kind)) || d.download.is_empty() {
                        continue;
                    }
                    let subject = if d.subject.trim().is_empty() { d.kind.clone() } else { d.subject.clone() };
                    let p = Pending {
                        key: d.key.clone(),
                        ticker: t.clone(),
                        kind,
                        title: clip(&format!("{}: {}", d.category, subject), 120),
                        reference: d.reference.clone(),
                        delivered: d.delivered,
                        url: d.download.clone(),
                        download: d.download.clone(),
                    };
                    if wanted(&p) {
                        out.push(p);
                    }
                }
            }
        }
        out.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.delivered.cmp(&a.delivered)));
        out.truncate(most);
        out
    }

    /// A document read: its pages (as the app read them), the figures in them, kept; the oldest go past `MOST_DOCS`.
    pub fn ingest(&mut self, p: &Pending, pages: Vec<String>, today: Date) -> &Doc {
        let pages: Vec<String> = pages.into_iter().take(MOST_PAGES).map(|page| clean(&page)).collect();
        let figures = if matches!(p.kind, DocKind::Report | DocKind::Release) { figures(&pages) } else { Vec::new() };
        let doc = Doc {
            key: p.key.clone(),
            ticker: p.ticker.clone(),
            kind: p.kind,
            title: p.title.clone(),
            reference: p.reference.clone(),
            delivered: p.delivered,
            url: p.url.clone(),
            download: p.download.clone(),
            problem: if pages.iter().all(|x| x.trim().is_empty()) { "No text could be read from it.".into() } else { String::new() },
            pages,
            figures,
            digest: None,
            read: Some(today),
            tries: 0,
            tried: Some(today),
        };
        self.docs.retain(|d| d.key != doc.key);
        self.docs.push(doc);
        self.prune();
        self.docs.iter().find(|d| d.key == p.key).expect("just kept")
    }

    /// A document that could not be read today: tried again tomorrow, at most `TRIES` times.
    pub fn failed(&mut self, p: &Pending, problem: &str, today: Date) {
        if let Some(d) = self.get_mut(&p.key) {
            d.tries = d.tries.saturating_add(1);
            d.tried = Some(today);
            d.problem = clip(problem, 200);
            return;
        }
        self.docs.push(Doc {
            key: p.key.clone(),
            ticker: p.ticker.clone(),
            kind: p.kind,
            title: p.title.clone(),
            reference: p.reference.clone(),
            delivered: p.delivered,
            url: p.url.clone(),
            download: p.download.clone(),
            pages: Vec::new(),
            figures: Vec::new(),
            digest: None,
            read: None,
            tries: 1,
            tried: Some(today),
            problem: clip(problem, 200),
        });
        self.prune();
    }

    fn prune(&mut self) {
        if self.docs.len() <= MOST_DOCS {
            return;
        }
        self.docs.sort_by(|a, b| b.delivered.cmp(&a.delivered));
        self.docs.truncate(MOST_DOCS);
    }

    /// The same asset's document of the same kind delivered before this one.
    pub fn previous(&self, doc: &Doc) -> Option<&Doc> {
        self.docs
            .iter()
            .filter(|d| d.ticker == doc.ticker && d.kind == doc.kind && d.key != doc.key && d.is_read() && d.delivered < doc.delivered)
            .max_by_key(|d| d.delivered)
    }

    /// What changed in the figures since the previous document of its kind.
    pub fn changes(&self, key: &str) -> Vec<Change> {
        let Some(doc) = self.get(key) else { return Vec::new() };
        let Some(before) = self.previous(doc) else { return Vec::new() };
        doc.figures
            .iter()
            .filter_map(|f| {
                let b = before.figures.iter().find(|x| x.key == f.key)?;
                let moved = (f.value - b.value).abs() > 1e-9 && (f.value - b.value).abs() >= b.value.abs() * 0.005;
                moved.then(|| Change { key: f.key.clone(), label: f.label.clone(), before: b.value, after: f.value, unit: f.unit.clone(), page: f.page })
            })
            .collect()
    }

    /// The documents read, newest first, at most `most`.
    pub fn feed(&self, most: usize) -> Vec<&Doc> {
        let mut read: Vec<&Doc> = self.docs.iter().filter(|d| d.is_read()).collect();
        read.sort_by(|a, b| b.delivered.cmp(&a.delivered).then(a.kind.cmp(&b.kind)));
        read.truncate(most);
        read
    }

    /// The read documents still waiting for the brain's digest, newest first.
    pub fn undigested(&self, most: usize) -> Vec<&Doc> {
        let mut out: Vec<&Doc> = self.docs.iter().filter(|d| d.is_read() && d.digest.is_none()).collect();
        out.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.delivered.cmp(&a.delivered)));
        out.truncate(most);
        out
    }

    /// The brain's digest of a document, as JSON (`{"headline", "points": [{"text", "page"}], "tone", "thesisRisk"}`),
    /// kept with it; pages it names beyond the document's are dropped.
    pub fn set_digest(&mut self, key: &str, v: &Value, today: Date) -> Result<&Digest, String> {
        let doc = self.get_mut(key).ok_or("No such document.")?;
        let pages = doc.pages.len() as u32;
        let headline = v["headline"].as_str().unwrap_or("").trim().to_string();
        let points: Vec<Point> = v["points"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        let text = p["text"].as_str().or_else(|| p.as_str())?.trim().to_string();
                        let page = p["page"].as_u64().map(|n| n as u32).filter(|n| *n >= 1 && *n <= pages.max(1));
                        (!text.is_empty()).then_some(Point { text: clip(&text, 400), page })
                    })
                    .take(6)
                    .collect()
            })
            .unwrap_or_default();
        if headline.is_empty() && points.is_empty() {
            return Err("The digest has neither a headline nor points.".into());
        }
        let tone = match v["tone"].as_str().unwrap_or("").trim() {
            t @ ("better" | "worse" | "same" | "mixed") => t.to_string(),
            _ => "mixed".into(),
        };
        doc.digest = Some(Digest {
            headline: clip(&headline, 160),
            points,
            tone,
            thesis_risk: v["thesisRisk"].as_bool().or_else(|| v["thesis_risk"].as_bool()).unwrap_or(false),
            made: Some(today),
        });
        Ok(doc.digest.as_ref().expect("just set"))
    }

    /// What the brain needs to digest a document: what it is, what the code found in it and against the previous one,
    /// the previous digest, and its text (the pages that matter most first), with the answer's form.
    pub fn digest_brief(&self, key: &str, apprentice: bool, language: &str) -> Result<String, String> {
        let doc = self.get(key).ok_or("No such document.")?;
        if !doc.is_read() {
            return Err("This document was not read yet.".into());
        }
        let mut out = format!(
            "You are AXYON's reader. The user follows {} and AXYON read this {} of it: \"{}\"{}. Say only what matters to a holder: what changed since the previous one, in at most five points, each with the page it comes from. If nothing that matters changed, say so in one point. Nothing invented: only what the document says.",
            doc.ticker,
            doc.kind.label(),
            doc.title,
            doc.delivered.map(|d| format!(", delivered on {}", d.br())).unwrap_or_default()
        );
        if apprentice {
            out.push_str(" The user is learning to invest: plain words, and each term explained the first time (vacância: the share of the space with no tenant).");
        }
        if !doc.figures.is_empty() {
            let found: Vec<String> = doc.figures.iter().map(|f| format!("{} {} (p. {})", f.label, f.shown(), f.page)).collect();
            out.push_str(&format!("\nWhat AXYON's code found in its figures: {}.", found.join("; ")));
        }
        let changes = self.changes(key);
        if !changes.is_empty() {
            out.push_str(&format!("\nAgainst the previous one: {}.", changes.iter().map(Change::shown).collect::<Vec<_>>().join("; ")));
        }
        if let Some(prev) = self.previous(doc).and_then(|p| p.digest.as_ref().map(|d| (p, d))) {
            let pts: Vec<&str> = prev.1.points.iter().map(|p| p.text.as_str()).collect();
            out.push_str(&format!("\nThe previous one ({}) said: {} {}", prev.0.title, prev.1.headline, pts.join(" ")));
        }
        out.push_str(&format!(
            "\nAnswer with only this JSON, its text in {language}: {{\"headline\": \"one line\", \"points\": [{{\"text\": \"…\", \"page\": 3}}], \"tone\": \"better|worse|same|mixed\", \"thesisRisk\": false}}"
        ));
        out.push_str("\n\nThe document, page by page:\n");
        out.push_str(&best_pages(doc, 24_000));
        Ok(out)
    }

    /// The documents of `ticker` for the committee: each with its id (D1…), its digest's points with their pages,
    /// what changed in its figures, and the pages that matter most, within `budget` characters.
    pub fn for_committee(&self, ticker: &str, budget: usize) -> (String, Vec<String>) {
        let mut docs: Vec<&Doc> = self.docs.iter().filter(|d| d.ticker == ticker && d.is_read()).collect();
        docs.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.delivered.cmp(&a.delivered)));
        docs.truncate(4);
        let mut out = String::new();
        let mut keys = Vec::new();
        let share = budget / docs.len().max(1);
        for (i, d) in docs.iter().enumerate() {
            let id = format!("D{}", i + 1);
            keys.push(d.key.clone());
            out.push_str(&format!("\n{id} — {} ({}{}):", d.title, d.kind.label(), d.delivered.map(|x| format!(", {}", x.br())).unwrap_or_default()));
            if let Some(g) = &d.digest {
                for p in &g.points {
                    out.push_str(&format!("\n  • {}{}", p.text, p.page.map(|n| format!(" [{id} p.{n}]")).unwrap_or_default()));
                }
            }
            for c in self.changes(&d.key) {
                out.push_str(&format!("\n  • {} [{id} p.{}]", c.shown(), c.page));
            }
            out.push_str(&format!("\n  Pages:\n{}", indent(&best_pages(d, share.saturating_sub(600).max(800)))));
        }
        (out, keys)
    }

    /// The text of a document from `offset` characters, `limit` at most, for the brain to read it again.
    pub fn read(&self, key: &str, offset: usize, limit: usize) -> Option<String> {
        let doc = self.get(key)?;
        let text = doc.text();
        let chars: Vec<char> = text.chars().collect();
        let start = offset.min(chars.len());
        let end = (start + limit).min(chars.len());
        let mut out: String = chars[start..end].iter().collect();
        if end < chars.len() {
            out.push_str(&format!("\n[…{} more characters: read again with offset {end}]", chars.len() - end));
        }
        Some(out)
    }
}

/// A document's file as it came: a PDF for the app to read (PDFKit, and the iPhone's text recognition for scans), or
/// its text already, page by page.
#[derive(Debug, Clone, PartialEq)]
pub enum File {
    Pdf(Vec<u8>),
    Text(Vec<String>),
}

/// What a document's download holds: FundosNet's base64 of the file, a PDF, a zip with the PDF in it (the CVM's), a
/// web page, or plain text.
pub fn file_of(body: &[u8]) -> Result<File, String> {
    let body = fnet::unwrap_download(body);
    if body.starts_with(b"%PDF") {
        return Ok(File::Pdf(body));
    }
    if body.starts_with(b"PK\x03\x04") {
        let archive = crate::zip::Archive::new(&body)?;
        if let Some(entry) = archive.find(".pdf") {
            return Ok(File::Pdf(archive.read(entry)?));
        }
        let entry = [".htm", ".html", ".txt", ".xml"].iter().find_map(|s| archive.find(s)).ok_or("The archive has no document in it.")?;
        return file_of(&archive.read(entry)?);
    }
    let text = crate::text::decode(&body);
    let start = text.trim_start();
    let plain = if start.starts_with('<') { strip_markup(start) } else { start.to_string() };
    let words = plain.split_whitespace().count();
    if words < 5 {
        return Err("The document came empty.".into());
    }
    // A long text in pages of about the length of a printed one, so the pages cited mean something.
    let mut pages = Vec::new();
    let mut page = String::new();
    for line in plain.lines() {
        if page.len() + line.len() > 3_000 && !page.is_empty() {
            pages.push(std::mem::take(&mut page));
        }
        page.push_str(line);
        page.push('\n');
    }
    if !page.trim().is_empty() {
        pages.push(page);
    }
    Ok(File::Text(pages.into_iter().take(MOST_PAGES).collect()))
}

/// A web page's words: tags out, its scripts and styles out, entities read, one line per block.
fn strip_markup(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let lower = html.to_ascii_lowercase();
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let rest = &lower[i..];
            let skip_to = if rest.starts_with("<script") { rest.find("</script>").map(|e| e + 9) } else if rest.starts_with("<style") { rest.find("</style>").map(|e| e + 8) } else { None };
            let end = skip_to.or_else(|| rest.find('>').map(|e| e + 1)).unwrap_or(rest.len());
            let tag = &rest[..end.min(rest.len())];
            if ["<br", "<p", "</p", "<div", "</div", "<tr", "</tr", "<li", "<h1", "<h2", "<h3", "<table", "</table"].iter().any(|t| tag.starts_with(t)) {
                out.push('\n');
            } else if tag.starts_with("<td") || tag.starts_with("<th") {
                out.push(' ');
            }
            i += end;
            continue;
        }
        let next = html[i..].find('<').map(|n| i + n).unwrap_or(bytes.len());
        out.push_str(&html[i..next]);
        i = next;
    }
    let mut out = out.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'");
    for (entity, letter) in [
        ("&aacute;", "á"), ("&eacute;", "é"), ("&iacute;", "í"), ("&oacute;", "ó"), ("&uacute;", "ú"), ("&atilde;", "ã"), ("&otilde;", "õ"),
        ("&ccedil;", "ç"), ("&acirc;", "â"), ("&ecirc;", "ê"), ("&ocirc;", "ô"), ("&agrave;", "à"), ("&Aacute;", "Á"), ("&Eacute;", "É"),
        ("&Oacute;", "Ó"), ("&Atilde;", "Ã"), ("&Ccedil;", "Ç"), ("&ordm;", "º"), ("&ordf;", "ª"), ("&amp;", "&"),
    ] {
        out = out.replace(entity, letter);
    }
    out.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

fn indent(s: &str) -> String {
    s.lines().map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n")
}

/// Whitespace collapsed, at most `PAGE_CHARS` characters.
fn clean(page: &str) -> String {
    let mut out = String::with_capacity(page.len().min(PAGE_CHARS + 8));
    let mut blank = 0;
    for line in page.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(&line);
        out.push('\n');
        if out.chars().count() >= PAGE_CHARS {
            break;
        }
    }
    clip(out.trim(), PAGE_CHARS)
}

/// Words that make a page of a report matter to a holder.
const WEIGHTY: &[&str] = &[
    "vacancia", "inadimpl", "resultado", "distribui", "rendimento", "dividendo", "emissao", "processo", "contingencia", "ltv", "cri", "locat",
    "contrato", "reajuste", "venda", "aquisicao", "amortiza", "reserva", "guidance", "endividamento", "divida", "lucro", "ebitda", "receita",
    "fato relevante", "assembleia", "cotistas", "prazo", "risco", "rating", "default",
];

/// The pages of a document that matter most within `budget` characters, in their order: the first page, then the
/// pages with the most weighty words.
fn best_pages(doc: &Doc, budget: usize) -> String {
    let mut scored: Vec<(usize, usize)> = doc
        .pages
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let low = lower(p);
            let score = WEIGHTY.iter().map(|w| low.matches(w).count()).sum::<usize>() + if i == 0 { 1000 } else { 0 };
            (i, score)
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut chosen = Vec::new();
    let mut used = 0;
    for (i, _) in scored {
        let len = doc.pages[i].chars().count() + 12;
        if used + len > budget && !chosen.is_empty() {
            continue;
        }
        used += len;
        chosen.push(i);
    }
    chosen.sort_unstable();
    chosen.iter().map(|&i| format!("[Page {}]\n{}", i + 1, clip(&doc.pages[i], budget))).collect::<Vec<_>>().join("\n\n")
}

/// Lower case, accents off, the rest kept (numbers keep their commas).
fn lower(s: &str) -> String {
    s.chars().map(|c| fold(c).to_lowercase().next().unwrap_or(c)).collect()
}

/// What a report says of itself, as this code reads it: the words to look for, the unit, and whether the number comes
/// before the words ("45.123 cotistas").
struct Spec {
    key: &'static str,
    label: &'static str,
    words: &'static [&'static str],
    unit: &'static str,
    before: bool,
}

const SPECS: &[Spec] = &[
    Spec { key: "vacancy_physical", label: "Vacância física", words: &["vacancia fisica"], unit: "%", before: false },
    Spec { key: "vacancy_financial", label: "Vacância financeira", words: &["vacancia financeira"], unit: "%", before: false },
    Spec { key: "default", label: "Inadimplência", words: &["inadimplencia"], unit: "%", before: false },
    Spec { key: "occupancy", label: "Ocupação", words: &["taxa de ocupacao", "ocupacao fisica", "ocupacao"], unit: "%", before: false },
    Spec { key: "ltv", label: "LTV", words: &["ltv", "loan to value"], unit: "%", before: false },
    Spec { key: "cap_rate", label: "Cap rate", words: &["cap rate"], unit: "%", before: false },
    Spec {
        key: "dividend_per_share",
        label: "Rendimento por cota",
        words: &["rendimento por cota", "rendimentos por cota", "distribuicao por cota", "dividendo por cota", "distribuicao de rendimentos de", "distribuiu", "distribuira"],
        unit: "R$",
        before: false,
    },
    Spec { key: "result_per_share", label: "Resultado por cota", words: &["resultado por cota"], unit: "R$", before: false },
    Spec { key: "holders", label: "Cotistas", words: &["cotistas"], unit: "", before: true },
];

/// The figures a report states, each found once (the first time it is said, with its page).
pub fn figures(pages: &[String]) -> Vec<Figure> {
    let mut out: Vec<Figure> = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let low: Vec<char> = lower(page).chars().collect();
        let text: String = low.iter().collect();
        for spec in SPECS {
            if out.iter().any(|f| f.key == spec.key) {
                continue;
            }
            for word in spec.words {
                let mut from = 0;
                let mut found = None;
                while let Some(at) = text[from..].find(word).map(|x| x + from) {
                    let start = text[..at].chars().count();
                    let end = start + word.chars().count();
                    found = if spec.before { number_before(&low, start) } else { number_after(&low, end, spec.unit) };
                    if found.is_some() {
                        break;
                    }
                    from = at + word.len();
                }
                if let Some(value) = found.filter(|v| sane(spec.unit, *v)) {
                    out.push(Figure { key: spec.key.into(), label: spec.label.into(), value, unit: spec.unit.into(), page: i as u32 + 1 });
                    break;
                }
            }
        }
    }
    out
}

fn sane(unit: &str, v: f64) -> bool {
    match unit {
        "%" => (0.0..=100.0).contains(&v),
        "R$" => v > 0.0 && v < 1_000.0,
        _ => v >= 10.0 && v < 1e9,
    }
}

/// The number written in `chars` from `at` on: digits with dots and commas, as Brazil writes them.
fn number_at(chars: &[char], at: usize) -> Option<(f64, usize)> {
    let mut end = at;
    while end < chars.len() && (chars[end].is_ascii_digit() || ((chars[end] == ',' || chars[end] == '.') && end + 1 < chars.len() && chars[end + 1].is_ascii_digit())) {
        end += 1;
    }
    if end == at {
        return None;
    }
    let s: String = chars[at..end].iter().collect();
    crate::text::num(&s).map(|v| (v, end))
}

/// The first number within a short reach after the words: a percent followed by "%", a money value after "R$" (or
/// a number with decimals).
fn number_after(chars: &[char], from: usize, unit: &str) -> Option<f64> {
    let reach = (from + 90).min(chars.len());
    let mut i = from;
    while i < reach {
        if chars[i].is_ascii_digit() && (i == 0 || !chars[i - 1].is_ascii_alphanumeric()) {
            let (value, end) = number_at(chars, i)?;
            let mut j = end;
            while j < chars.len() && chars[j] == ' ' {
                j += 1;
            }
            let before: String = chars[i.saturating_sub(4)..i].iter().collect();
            let ok = match unit {
                "%" => j < chars.len() && chars[j] == '%',
                "R$" => before.contains("r$"),
                _ => true,
            };
            if ok {
                return Some(value);
            }
            i = end;
            continue;
        }
        // A new sentence: what follows is about something else.
        if chars[i] == '.' && i + 1 < chars.len() && chars[i + 1] == ' ' && i > from + 3 {
            return None;
        }
        i += 1;
    }
    None
}

/// The number just before the words ("45.123 cotistas").
fn number_before(chars: &[char], at: usize) -> Option<f64> {
    let mut end = at;
    while end > 0 && chars[end - 1] == ' ' {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && (chars[start - 1].is_ascii_digit() || chars[start - 1] == '.' || chars[start - 1] == ',') {
        start -= 1;
    }
    if start == end || at - end > 2 {
        return None;
    }
    // A count: its dots are thousands ("45.123").
    let s: String = chars[start..end].iter().filter(|c| **c != '.').collect();
    crate::text::num(&s)
}

#[cfg(test)]
pub mod fixtures {
    pub fn report_pages(vacancy: &str, dividend: &str) -> Vec<String> {
        vec![
            "GGR Covepi Renda FII\nRelatório Gerencial | Setembro 2026".to_string(),
            format!("Destaques do mês\nO Fundo distribuiu R$ {dividend} por cota, com resultado por cota de R$ 0,091.\nA vacância física encerrou o mês em {vacancy}%, e a inadimplência ficou em 0,8% da receita."),
            "Portfólio\nSão 45.123 cotistas. O LTV do fundo é de 22,4% e o cap rate das aquisições foi 9,1 %.".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FundData, Position};
    use crate::sources::fnet::Document;

    fn day(d: u32) -> Date {
        Date::new(2026, 10, d).unwrap()
    }

    fn fnet_doc(id: u64, category: &str, kind: &str, delivered: Date, reference: &str) -> Document {
        Document {
            id,
            category: category.into(),
            kind: kind.into(),
            species: String::new(),
            reference: reference.into(),
            delivered: Some(delivered),
            info: String::new(),
            topic: fnet::topic(category, kind, "", ""),
            structured: false,
            fund: String::new(),
            trading: String::new(),
        }
    }

    fn setup() -> (Store, Market) {
        let mut store = Store::default();
        store.portfolio.positions.push(Position {
            ticker: "GGRC11".into(),
            class: AssetClass::Fii,
            quantity: 150.0,
            average_price: 9.2,
            subtype: String::new(),
            current_value: None,
            note: String::new(),
        });
        let mut market = Market::default();
        let docs = vec![
            fnet_doc(905, "Fato Relevante", "", day(25), "25/10/2026"),
            fnet_doc(904, "Relatórios", "Relatório Gerencial", day(10), "09/2026"),
            fnet_doc(903, "Relatórios", "Relatório Gerencial", Date::new(2026, 9, 10).unwrap(), "08/2026"),
            fnet_doc(902, "Relatórios", "Relatório Gerencial", Date::new(2026, 8, 10).unwrap(), "07/2026"),
            fnet_doc(901, "Aviso aos Cotistas - Estruturado", "Rendimentos e Amortizações", day(14), "10/2026"),
            fnet_doc(800, "Relatórios", "Relatório Gerencial", Date::new(2026, 5, 10).unwrap(), "04/2026"),
        ];
        let mut docs = docs;
        docs[4].structured = true;
        market.funds.insert("GGRC11".into(), FundData { ticker: "GGRC11".into(), documents: docs, ..FundData::default() });
        (store, market)
    }

    #[test]
    fn a_download_is_a_pdf_or_its_pages() {
        let pdf = b"%PDF-1.7 the bytes".to_vec();
        assert_eq!(file_of(&pdf).unwrap(), File::Pdf(pdf.clone()));
        // FundosNet sends the file in base64, between quotes.
        let b64 = "\"JVBERi0xLjcgdGhlIGJ5dGVz\"";
        assert_eq!(file_of(b64.as_bytes()).unwrap(), File::Pdf(pdf.clone()));
        // The CVM's zip with the PDF in it.
        let zipped = crate::zip::build(&[("doc/fato.pdf", &pdf)], true);
        assert_eq!(file_of(&zipped).unwrap(), File::Pdf(pdf));
        let page = "<html><head><style>p{}</style><script>var x=1;</script></head><body><h1>Fato Relevante</h1><p>O fundo vendeu o im&oacute;vel de Campinas por R$ 52 milh&otilde;es.</p><table><tr><td>Cap rate</td><td>9,1%</td></tr></table></body></html>";
        let File::Text(pages) = file_of(page.as_bytes()).unwrap() else { panic!("text") };
        assert_eq!(pages.len(), 1);
        assert!(pages[0].contains("Fato Relevante\nO fundo vendeu o imóvel de Campinas por R$ 52 milhões.") && pages[0].contains("Cap rate 9,1%"), "{pages:?}");
        assert!(!pages[0].contains("var x") && !pages[0].contains("p{}"));
        let long: String = (0..400).map(|i| format!("Linha {i} do comunicado ao mercado com texto.\n")).collect();
        let File::Text(pages) = file_of(long.as_bytes()).unwrap() else { panic!("text") };
        assert!(pages.len() >= 5 && pages.iter().all(|p| p.len() <= 3_100));
        assert!(file_of(b"  ").is_err());
    }

    #[test]
    fn the_figures_of_a_report_are_found_with_their_pages() {
        let f = figures(&fixtures::report_pages("7,5", "0,088"));
        let get = |k: &str| f.iter().find(|x| x.key == k).map(|x| (x.value, x.page));
        assert_eq!(get("dividend_per_share"), Some((0.088, 2)));
        assert_eq!(get("result_per_share"), Some((0.091, 2)));
        assert_eq!(get("vacancy_physical"), Some((7.5, 2)));
        assert_eq!(get("default"), Some((0.8, 2)));
        assert_eq!(get("holders"), Some((45_123.0, 3)));
        assert_eq!(get("ltv"), Some((22.4, 3)));
        assert_eq!(get("cap_rate"), Some((9.1, 3)));
        assert!(get("vacancy_financial").is_none(), "not said: not invented");
        assert!(figures(&["A vacância física deve cair. O fundo tem 3 imóveis.".to_string()]).iter().all(|x| x.key != "vacancy_physical"), "a number of another sentence is not taken");
    }

    #[test]
    fn what_to_read_then_reading_it_and_what_changed() {
        let (store, market) = setup();
        let mut lib = Library::default();
        let due = lib.pending(&store, &market, day(28), 10);
        let keys: Vec<&str> = due.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, ["fnet:905", "fnet:904", "fnet:903"], "the material fact first, then the two latest reports; not the structured notice, nor the old ones");
        assert_eq!(due[1].download, "https://fnet.bmfbovespa.com.br/fnet/publico/downloadDocumento?id=904");
        assert!(due[1].title.contains("09/2026"));

        lib.ingest(&due[2], fixtures::report_pages("6,0", "0,085"), day(28));
        let doc = lib.ingest(&due[1], fixtures::report_pages("7,5", "0,088"), day(28));
        assert!(doc.is_read() && doc.figures.len() >= 6);
        let changes = lib.changes("fnet:904");
        let shown: Vec<String> = changes.iter().map(Change::shown).collect();
        assert!(shown.contains(&"Vacância física: 6,0% → 7,5% (p. 2)".to_string()), "{shown:?}");
        assert!(shown.iter().any(|s| s.starts_with("Rendimento por cota: R$ 0,085 → R$ 0,088")), "{shown:?}");
        assert!(!shown.iter().any(|s| s.starts_with("Cotistas")), "unchanged: not a change");

        // The material fact could not be read: tried again tomorrow, not today.
        lib.failed(&due[0], "PDF damaged", day(28));
        assert!(lib.pending(&store, &market, day(28), 10).is_empty());
        assert_eq!(lib.pending(&store, &market, day(29), 10).len(), 1);
        for _ in 0..TRIES {
            lib.failed(&due[0], "PDF damaged", day(29));
        }
        assert!(lib.pending(&store, &market, day(30), 10).is_empty(), "given up after {TRIES} tries");
    }

    #[test]
    fn a_digest_is_kept_and_briefs_the_next_one_and_the_committee() {
        let (store, market) = setup();
        let mut lib = Library::default();
        let due = lib.pending(&store, &market, day(28), 10);
        lib.ingest(&due[2], fixtures::report_pages("6,0", "0,085"), day(28));
        lib.ingest(&due[1], fixtures::report_pages("7,5", "0,088"), day(28));
        assert_eq!(lib.undigested(5).len(), 2);
        let brief = lib.digest_brief("fnet:904", true, "Portuguese").unwrap();
        assert!(brief.contains("Vacância física 7,5% (p. 2)") && brief.contains("Against the previous one"), "{brief}");
        assert!(brief.contains("[Page 2]") && brief.contains("plain words") && brief.contains("\"thesisRisk\""));
        lib.set_digest("fnet:903", &serde_json::json!({"headline": "Agosto estável", "points": [{"text": "Vacância em 6%", "page": 2}]}), day(28)).unwrap();
        let brief = lib.digest_brief("fnet:904", false, "Portuguese").unwrap();
        assert!(brief.contains("The previous one (Relatório Gerencial 08/2026) said: Agosto estável Vacância em 6%"), "{brief}");
        let d = lib.set_digest("fnet:904", &serde_json::json!({"headline": "Vacância subiu", "points": [{"text": "Vacância física 7,5%", "page": 2}, {"text": "fora", "page": 99}, "Sem novidade jurídica"], "tone": "worse", "thesisRisk": true}), day(28)).unwrap();
        assert_eq!(d.points.len(), 3);
        assert_eq!(d.points[1].page, None, "a page the document has not is dropped");
        assert!(d.thesis_risk && d.tone == "worse");
        assert!(lib.set_digest("fnet:904", &serde_json::json!({}), day(28)).is_err());
        assert_eq!(lib.feed(5)[0].headline(), "Vacância subiu");
        let (text, keys) = lib.for_committee("GGRC11", 6_000);
        assert_eq!(keys, ["fnet:904", "fnet:903"]);
        assert!(text.contains("D1 — Relatório Gerencial 09/2026") && text.contains("Vacância física 7,5% [D1 p.2]") && text.contains("[Page 2]"), "{text}");
        let again = lib.read("fnet:904", 0, 40).unwrap();
        assert!(again.starts_with("[Page 1]") && again.contains("more characters: read again with offset 40"));
    }

    #[test]
    fn a_companys_documents_from_the_cvm_are_read_too() {
        let mut store = Store::default();
        store.portfolio.positions.push(Position {
            ticker: "TIMS3".into(),
            class: AssetClass::Acao,
            quantity: 30.0,
            average_price: 18.6,
            subtype: String::new(),
            current_value: None,
            note: String::new(),
        });
        let mut market = Market::default();
        market.companies.insert(
            "TIMS3".into(),
            crate::model::CompanyData {
                ticker: "TIMS3".into(),
                documents: vec![
                    Listed { key: "ipe:1".into(), category: "Fato Relevante".into(), subject: "Aquisição".into(), delivered: Some(day(20)), download: "https://www.rad.cvm.gov.br/x?1".into(), ..Listed::default() },
                    Listed { key: "ipe:2".into(), category: "Calendário de Eventos Corporativos".into(), delivered: Some(day(20)), download: "x".into(), ..Listed::default() },
                    Listed { key: "ipe:3".into(), category: "Dados Econômico-Financeiros".into(), kind: "Release".into(), delivered: Some(day(22)), download: "y".into(), ..Listed::default() },
                ],
                ..crate::model::CompanyData::default()
            },
        );
        let due = Library::default().pending(&store, &market, day(28), 10);
        let got: Vec<(&str, DocKind)> = due.iter().map(|p| (p.key.as_str(), p.kind)).collect();
        assert_eq!(got, [("ipe:1", DocKind::MaterialFact), ("ipe:3", DocKind::Release)], "the calendar is not read");
        assert!(due[0].title.contains("Aquisição"));
    }
}
