// Text helpers for Mochi's hands: what a command printed, a web page as plain text, a file's lines with their
// numbers, and the size limits everything sent back to the model is held to. Pure functions, tested below.

/// The largest index ≤ `at` that is on a character boundary of `text`.
pub fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The smallest index ≥ `at` that is on a character boundary of `text`.
pub fn ceil_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

/// At most about `max` bytes of `text`: its start and its end, with a note of what was left out between them.
/// The end of a command's output (the error, the summary) matters as much as its start.
pub fn clip_middle(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let head = floor_boundary(text, max / 2);
    let tail = ceil_boundary(text, text.len() - max / 2);
    format!("{}\n[… {} bytes left out …]\n{}", &text[..head], tail - head, &text[tail..])
}

/// At most `max` bytes from the start of `text`, and whether something was cut.
pub fn clip_end(text: &str, max: usize) -> (&str, bool) {
    if text.len() <= max {
        return (text, false);
    }
    (&text[..floor_boundary(text, max)], true)
}

/// The first `max` characters of a line of text, on one line: what a step or an approval shows.
pub fn one_line(text: &str, max: usize) -> String {
    let flat: String = text.chars().map(|c| if c == '\n' || c == '\r' || c == '\t' { ' ' } else { c }).collect();
    let flat = flat.trim();
    match flat.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", &flat[..at]),
        None => flat.to_string(),
    }
}

/// Terminal colours and cursor moves out: ESC [ … final, ESC ] … BEL or ESC \, and two-character escapes.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                // Parameters and intermediates, then one final byte in @..~.
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            Some(_) => {
                chars.next();
            }
            None => {}
        }
    }
    out
}

/// What a command printed, as text a person (and the model) reads: UTF-8 (anything else replaced), no colours,
/// Windows line ends made plain, and a line redrawn with carriage returns (a progress bar) kept as its last state.
pub fn clean_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let text = strip_ansi(&text).replace("\r\n", "\n");
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let kept = line.rsplit('\r').find(|part| !part.is_empty()).unwrap_or("");
        out.push_str(kept);
    }
    out
}

/// True for bytes that are not text: a NUL in the first 8 KB, as git decides.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|b| *b == 0)
}

/// Lines `offset..offset+limit` (1-based) of `text`, numbered like `cat -n`, and a last line that says where the
/// file goes on when it does. Also returns how many lines the file has.
pub fn numbered(text: &str, offset: usize, limit: usize, max_bytes: usize) -> (String, usize) {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let total = lines.len();
    let start = offset.max(1);
    let mut out = String::new();
    let mut last = start.saturating_sub(1);
    for (n, line) in lines.iter().enumerate().skip(start - 1).take(limit) {
        let line = line.strip_suffix('\n').unwrap_or(line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        // One very long line (minified code) must not take the whole budget.
        let (line, cut) = clip_end(line, 2000);
        let row = format!("{:>6}\t{}{}\n", n + 1, line, if cut { " [line cut]" } else { "" });
        if out.len() + row.len() > max_bytes {
            break;
        }
        out.push_str(&row);
        last = n + 1;
    }
    if total == 0 {
        out.push_str("[the file is empty]\n");
    } else if start > total {
        out.push_str(&format!("[the file has {total} lines; offset {start} is past its end]\n"));
    } else if last < total {
        out.push_str(&format!("[lines {start}-{last} of {total} shown; read on with offset={}]\n", last + 1));
    }
    (out, total)
}

// ── A web page as plain text ──────────────────────────────────────────────────

/// Tags whose content is never text a reader would see.
const SKIPPED: &[&str] = &["script", "style", "noscript", "svg", "template", "iframe", "object", "canvas"];
/// Tags that start a new line of text.
const BLOCKS: &[&str] = &[
    "p", "div", "br", "hr", "li", "ul", "ol", "dl", "dt", "dd", "h1", "h2", "h3", "h4", "h5", "h6", "tr", "table",
    "section", "article", "header", "footer", "nav", "aside", "main", "pre", "blockquote", "form", "figure",
    "figcaption", "details", "summary", "title", "body", "html", "head",
];

/// The visible text of an HTML page: no scripts, styles or tags, entities decoded, one blank line at most between
/// blocks, and links kept as "text <address>" when the address is a web one.
pub fn html_to_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len() / 3);
    let mut i = 0;
    let mut link: Option<String> = None;
    while i < html.len() {
        if bytes[i] != b'<' {
            let next = html[i..].find('<').map_or(html.len(), |n| i + n);
            out.push_str(&decode_entities(&html[i..next]));
            i = next;
            continue;
        }
        if lower[i..].starts_with("<!--") {
            i = lower[i..].find("-->").map_or(html.len(), |n| i + n + 3);
            continue;
        }
        let Some(close) = html[i..].find('>').map(|n| i + n) else { break };
        let inner = &html[i + 1..close];
        let tag = &lower[i + 1..close];
        let closing = tag.starts_with('/');
        let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        i = close + 1;
        if !closing && SKIPPED.contains(&name.as_str()) {
            let end = format!("</{name}");
            i = lower[i..].find(&end).map_or(html.len(), |n| {
                let from = i + n;
                lower[from..].find('>').map_or(html.len(), |m| from + m + 1)
            });
            continue;
        }
        if name == "a" {
            if closing {
                if let Some(href) = link.take() {
                    out.push_str(&format!(" <{href}>"));
                }
            } else {
                link = attribute(inner, "href")
                    .filter(|h| (h.starts_with("http://") || h.starts_with("https://")) && h.len() < 200);
            }
            continue;
        }
        if BLOCKS.contains(&name.as_str()) {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            if name == "li" && !closing {
                out.push_str("- ");
            }
        } else if name == "td" || name == "th" {
            out.push(' ');
        }
    }
    tidy(&out)
}

/// The value of attribute `name` in the inside of a tag (`a href="…" class=…`).
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find(name).map(|n| from + n) {
        from = at + name.len();
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = lower[from..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let value_start = tag.len() - rest.len() + 1;
        let value = tag[value_start..].trim_start();
        let (quote, body) = match value.chars().next() {
            Some(q @ ('"' | '\'')) => (Some(q), &value[1..]),
            _ => (None, value),
        };
        let end = match quote {
            Some(q) => body.find(q).unwrap_or(body.len()),
            None => body.find(|c: char| c.is_whitespace()).unwrap_or(body.len()),
        };
        return Some(decode_entities(&body[..end]));
    }
    None
}

/// `&amp;`, `&lt;`, `&#233;`, `&#x1F600;`… as the characters they stand for; an unknown one is left as written.
pub fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest[1..].find(';').map(|n| n + 1).filter(|n| *n <= 10);
        let Some(end) = end else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "hellip" => Some('…'),
            "copy" => Some('©'),
            _ => name
                .strip_prefix("#x")
                .or_else(|| name.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|d| d.parse::<u32>().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Spaces collapsed inside lines, lines trimmed, and at most one empty line in a row.
fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = 0;
    for line in text.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank += 1;
            if blank == 1 && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        blank = 0;
        out.push_str(&line);
        out.push('\n');
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_output_keeps_its_start_and_its_end() {
        let text = format!("{}{}", "a".repeat(1000), "z".repeat(1000));
        let clipped = clip_middle(&text, 100);
        assert!(clipped.starts_with(&"a".repeat(50)));
        assert!(clipped.ends_with(&"z".repeat(50)));
        assert!(clipped.contains("1900 bytes left out"));
        assert_eq!(clip_middle("short", 100), "short");
        // Never inside a character.
        let wide = "é".repeat(500);
        let clipped = clip_middle(&wide, 101);
        assert!(clipped.starts_with("éé") && clipped.ends_with("éé"));
        assert_eq!(clip_end("ééé", 3), ("é", true));
        assert_eq!(clip_end("abc", 3), ("abc", false));
    }

    #[test]
    fn one_line_flattens_and_cuts_with_an_ellipsis() {
        assert_eq!(one_line("npm test\n&& echo ok", 100), "npm test && echo ok");
        assert_eq!(one_line("abcdef", 3), "abc…");
        assert_eq!(one_line("  padded  ", 20), "padded");
    }

    #[test]
    fn colours_and_progress_bars_are_cleaned_from_output() {
        assert_eq!(strip_ansi("\u{1b}[1;32mok\u{1b}[0m done"), "ok done");
        assert_eq!(strip_ansi("\u{1b}]0;title\u{7}text"), "text");
        assert_eq!(strip_ansi("\u{1b}]8;;http://x\u{1b}\\link"), "link");
        assert_eq!(clean_output(b"10%\r50%\r100%\nnext\r\n"), "100%\nnext\n");
        assert_eq!(clean_output(b"line\r\n"), "line\n");
        assert_eq!(clean_output(&[0x66, 0x6f, 0xff, 0x6f]), "fo\u{fffd}o");
    }

    #[test]
    fn binary_is_told_by_a_nul_byte() {
        assert!(looks_binary(b"PK\x03\x04\x00\x00"));
        assert!(!looks_binary("texte accentué".as_bytes()));
    }

    #[test]
    fn lines_are_numbered_and_the_rest_is_announced() {
        let text = "one\ntwo\r\nthree\nfour\n";
        let (out, total) = numbered(text, 1, 2, 10_000);
        assert_eq!(total, 4);
        assert_eq!(out, "     1\tone\n     2\ttwo\n[lines 1-2 of 4 shown; read on with offset=3]\n");
        let (out, _) = numbered(text, 3, 10, 10_000);
        assert_eq!(out, "     3\tthree\n     4\tfour\n");
        let (out, _) = numbered(text, 9, 10, 10_000);
        assert!(out.contains("past its end"));
        let (out, total) = numbered("", 1, 10, 10_000);
        assert_eq!((out.as_str(), total), ("[the file is empty]\n", 0));
        // The byte budget stops early and still says where to go on.
        let (out, _) = numbered(text, 1, 10, 20);
        assert!(out.starts_with("     1\tone\n"));
        assert!(out.contains("read on with offset=2"));
        // No final newline: the last line is still there.
        assert_eq!(numbered("a\nb", 1, 10, 100).0, "     1\ta\n     2\tb\n");
    }

    #[test]
    fn a_page_becomes_its_visible_text() {
        let html = r#"<html><head><title>Docs &amp; more</title><style>p{color:red}</style>
            <script>var x = "<p>not text</p>";</script></head>
            <body><h1>Hello</h1><p>First   para with <b>bold</b> and <a href="https://x.dev/a?b=1&amp;c=2">a link</a>.</p>
            <!-- a comment <p>hidden</p> -->
            <ul><li>one</li><li>two &#233; &#x1F600;</li></ul><a href="/relative">rel</a>
            <table><tr><td>a</td><td>b</td></tr></table></body></html>"#;
        let text = html_to_text(html);
        assert!(text.starts_with("Docs & more"), "{text}");
        assert!(text.contains("Hello"));
        assert!(text.contains("First para with bold and a link <https://x.dev/a?b=1&c=2>."), "{text}");
        assert!(text.contains("- one\n- two é 😀"), "{text}");
        assert!(text.contains("rel"));
        assert!(!text.contains("/relative"));
        assert!(text.contains("a b"));
        for hidden in ["color:red", "not text", "hidden", "var x"] {
            assert!(!text.contains(hidden), "{hidden} leaked: {text}");
        }
        assert!(!text.contains("\n\n\n"));
    }

    #[test]
    fn broken_html_never_panics() {
        for html in ["<", "<p", "<script>unterminated", "&", "&#xZZ;", "<a href=", "<a href=\"x", "text<!--", "é<b>é</b>é"] {
            let _ = html_to_text(html);
        }
        assert_eq!(decode_entities("a &unknown; b & c &#9999999999;"), "a &unknown; b & c &#9999999999;");
        assert_eq!(decode_entities("&lt;tag&gt; &quot;q&quot;"), "<tag> \"q\"");
    }
}
