// What the capabilities that let the phone change things (assistant.rs, home.rs) share: the ids a phone gives its
// requests, how often it may ask, and text the computer did not write itself (a device's name, a TV's app, Hermes' own
// words when it stopped) made fit for the phone: one line, cut to size, and nothing in it that says where things are or
// would let anyone in — no address, MAC address, path, e-mail or anything that looks like a key or a token. Pure, tested
// below.

use std::collections::VecDeque;

/// What a word that must not cross the link becomes.
pub const BLANK: &str = "•••";
/// The longest id a phone may give a request (it comes back in the answer).
pub const MAX_ID: usize = 64;

/// The phone's id for a request: a string of 1 to 64 characters, none of them a control character. None: there is
/// nothing to answer to, and the request is ignored.
pub fn request_id(msg: &serde_json::Value) -> Option<&str> {
    msg["id"].as_str().filter(|id| !id.is_empty() && id.chars().count() <= MAX_ID && !id.chars().any(char::is_control))
}

/// When a connection asked lately (ms), so it asks at most so many times in a window.
#[derive(Debug, Default)]
pub struct Rate(VecDeque<u64>);

impl Rate {
    /// Already `max` in the window that ends at `now`.
    pub fn full(&mut self, now: u64, max: usize, window: u64) -> bool {
        while self.0.front().is_some_and(|&t| now.saturating_sub(t) >= window) {
            self.0.pop_front();
        }
        self.0.len() >= max
    }

    /// One more at `now`.
    pub fn count(&mut self, now: u64) {
        self.0.push_back(now);
    }

    /// Counts one more at `now` if the window has room for it.
    pub fn allow(&mut self, now: u64, max: usize, window: u64) -> bool {
        if self.full(now, max, window) {
            return false;
        }
        self.count(now);
        true
    }
}

/// `text` as one line of at most `max` characters: control characters become spaces, every string of `hidden` (what
/// the computer knows must stay on it: the TVs' and Home Assistant's addresses…) is blanked whatever its case, and so is
/// every word that is or holds an address, a path, an e-mail or something like a key.
pub fn clean_line(text: &str, max: usize, hidden: &[String]) -> String {
    let mut line: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    for secret in hidden.iter().map(|h| h.trim()).filter(|h| h.len() >= 4) {
        line = blank_ignoring_case(&line, secret);
    }
    let words: Vec<&str> = line.split_whitespace().map(|w| if telling(w) { BLANK } else { w }).collect();
    clip(&words.join(" "), max)
}

/// The first `max` characters, on a character boundary.
fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => s[..i].trim_end().to_string(),
        None => s.to_string(),
    }
}

/// Every occurrence of `needle` in `text` blanked, ASCII letters compared whatever their case (addresses and MAC
/// addresses are written either way). Lower-casing ASCII keeps every byte where it was, so the places found in the
/// lower-cased text are the places in `text`.
fn blank_ignoring_case(text: &str, needle: &str) -> String {
    let (haystack, needle) = (text.to_ascii_lowercase(), needle.to_ascii_lowercase());
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    while let Some(found) = haystack[from..].find(&needle) {
        let at = from + found;
        out.push_str(&text[from..at]);
        out.push_str(BLANK);
        from = at + needle.len();
    }
    out.push_str(&text[from..]);
    out
}

/// A word that says where something is or could let someone in.
fn telling(word: &str) -> bool {
    let w = word.trim_matches(|c: char| "()[]{}<>\"'`,;.!?:".contains(c));
    if w.is_empty() {
        return false;
    }
    let lower = w.to_ascii_lowercase();
    w.contains("://")
        || lower.starts_with("www.")
        || w.find('@').is_some_and(|i| i > 0 && i + 1 < w.len())
        || w.contains('\\')
        || w.starts_with('/')
        || w.starts_with("~/")
        || w.contains(":/")
        || path_like(w)
        || ipv4_inside(w)
        || ipv6_or_mac(word)
        || host_and_port(w)
        || local_host(&lower)
        || key_like(w)
}

/// "Documents/Coucou/notes.txt", "hermes_cli/main.py", "example.com/login": folders, a file in one, or an address
/// without its scheme. Two words joined by a slash are words ("Sala/Cozinha", "On/Off", "volume_up/volume_down").
fn path_like(w: &str) -> bool {
    let (Some((first, _)), Some((_, last))) = (w.split_once('/'), w.rsplit_once('/')) else { return false };
    let extension = last.rsplit_once('.').is_some_and(|(name, ext)| !name.is_empty() && (1..=5).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()));
    let domain = first.contains('.') && first.chars().any(|c| c.is_ascii_alphabetic());
    w.matches('/').count() >= 2 || extension || domain
}

/// A name of the local network: "homeassistant.local", "tv-sala.lan", "nas.home.arpa".
fn local_host(lower: &str) -> bool {
    [".local", ".lan", ".home.arpa", ".internal"].iter().any(|end| lower.len() > end.len() && lower.ends_with(end))
}

/// Four numbers of 0 to 255 joined by dots, somewhere in the word ("192.168.1.20:1925").
fn ipv4_inside(w: &str) -> bool {
    w.split(|c: char| !(c.is_ascii_digit() || c == '.')).any(|run| {
        let parts: Vec<&str> = run.trim_matches('.').split('.').collect();
        parts.len() >= 4 && parts.iter().all(|p| !p.is_empty() && p.len() <= 3 && p.parse::<u16>().is_ok_and(|n| n <= 255))
    })
}

/// An IPv6 address ("fe80::1", "[2804:14c::10]:47821") or a MAC address ("A4:B1:C2:D3:E4:F5", "a4-b1-…",
/// "a4b1.c2d3.e4f5"). A time ("21:05:12") or a code ("ABCD-1234") is neither.
fn ipv6_or_mac(word: &str) -> bool {
    // Its colons are part of it ("::1"), so only the punctuation around a word is taken off here.
    let w = word.trim_matches(|c: char| "()<>\"'`,;!?.".contains(c)).trim_start_matches('[');
    let w = w.split(']').next().unwrap_or("").split('%').next().unwrap_or("");
    let hex = |g: &str| g.chars().all(|c| c.is_ascii_hexdigit());
    let colons = w.matches(':').count();
    if (w.contains("::") || colons >= 5) && colons >= 2 {
        let groups: Vec<&str> = w.split(':').collect();
        if groups.iter().all(|g| g.len() <= 4 && hex(g)) && groups.iter().any(|g| !g.is_empty()) {
            return true;
        }
    }
    let dashed: Vec<&str> = w.split('-').collect();
    if dashed.len() == 6 && dashed.iter().all(|g| g.len() == 2 && hex(g)) {
        return true;
    }
    let dotted: Vec<&str> = w.split('.').collect();
    dotted.len() == 3 && dotted.iter().all(|g| g.len() == 4 && hex(g))
}

/// A name with a port: "homeassistant.local:8123", "localhost:8642".
fn host_and_port(w: &str) -> bool {
    let Some((host, port)) = w.rsplit_once(':') else { return false };
    (1..=5).contains(&port.len())
        && port.chars().all(|c| c.is_ascii_digit())
        && !host.is_empty()
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && (host.contains('.') || host.eq_ignore_ascii_case("localhost"))
}

/// A long run of letters and digits such as keys and tokens are made of (an API key, a JWT, a pairing token, a
/// certificate's hash): mixed case, long hexadecimal, or many digits. A long name in words ("media_player.tv_sala_2")
/// is not one.
fn key_like(w: &str) -> bool {
    let n = w.chars().count();
    if n < 24 || !w.chars().all(|c| c.is_ascii_alphanumeric() || "_-.+=".contains(c)) {
        return false;
    }
    let digits = w.chars().filter(char::is_ascii_digit).count();
    let mixed = w.chars().any(|c| c.is_ascii_uppercase()) && w.chars().any(|c| c.is_ascii_lowercase());
    let hex_run = w.split(|c: char| !c.is_ascii_hexdigit()).map(str::len).max().unwrap_or(0);
    digits > 0 && (mixed || hex_run >= 24 || digits * 4 >= n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(text: &str) -> String {
        clean_line(text, 300, &[])
    }

    #[test]
    fn addresses_paths_emails_and_keys_never_reach_the_phone() {
        for (text, gone) in [
            ("Hermes stopped: C:\\Users\\Lea\\AppData\\Local\\hermes\\config.yaml is broken", "Lea"),
            ("cannot read /home/lea/.hermes/config.yaml", "lea"),
            ("no answer from http://127.0.0.1:8642/v1/capabilities", "8642"),
            ("Not a valid address: 192.168.1.20", "192.168"),
            ("(192.168.1.20:1925)", "192.168"),
            ("TV at fe80::1c2b:3cff:fe4d:5e6f%eth0", "fe80"),
            ("reach [2804:14c:1:2::10]:47821", "2804"),
            ("MAC A4:B1:C2:D3:E4:F5 and a4-b1-c2-d3-e4-f5 and a4b1.c2d3.e4f5", "b1"),
            ("signed in as lea.martin@example.com", "example"),
            ("at homeassistant.local:8123 or localhost:8642", "8123"),
            ("token eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJ4In0.abc123", "eyJ"),
            ("key sk-ant-api03-AbCdEfGhIjKlMnOp1234567890", "sk-ant"),
            ("see www.example.org", "example"),
            ("relative Documents/Coucou/notes.txt", "Documents"),
            ("in hermes_cli/main.py line 3", "hermes_cli"),
            ("open example.com/login", "example"),
            ("TV at tv-sala.lan, NAS at nas.home.arpa", "sala"),
            ("pin 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08", "9f86"),
            ("token sk-test-0123456789abcdefghijkl", "sk-test"),
        ] {
            let out = clean(text);
            assert!(!out.contains(gone), "{text:?} gave {out:?}");
            assert!(out.contains(BLANK), "{out:?}");
        }
    }

    #[test]
    fn ordinary_words_are_kept_as_they_are() {
        for text in [
            "On · Volume 20% · YouTube",
            "Luz da sala · Turn off",
            "Sign in with ChatGPT: open the page and type the code ABCD-1234",
            "2026-10-13 21:05:12",
            "Install Python dependencies (6.4.0)",
            "step 2: TV did not report that it can set the volume to 20%. What it can do: turn_on, volume_up/volume_down, mute/unmute.",
            "TV @ Sala",
            "Error: 404",
            "Globo News",
            "Sala/Cozinha · On/Off",
            "ha:media_player.samsung_tv_sala_2 · Turn on",
            "Disney+ 4K",
        ] {
            assert_eq!(clean(text), text);
        }
    }

    #[test]
    fn what_the_computer_knows_to_hide_is_blanked_whatever_its_case_and_the_line_is_one_and_short() {
        let hidden = vec!["tv-sala.lan".to_string(), "Casa-NAS".to_string(), "ab".to_string()];
        assert_eq!(clean_line("TV-SALA.LAN is off, casa-nas too", 300, &hidden), "••• is off, ••• too");
        assert_eq!(clean_line("a tab\tand\nlines\u{0}", 300, &hidden), "a tab and lines", "a hidden string shorter than 4 is left");
        assert_eq!(clean_line(&"é".repeat(500), 40, &[]).chars().count(), 40);
        assert_eq!(clean_line("", 40, &hidden), "");
    }

    #[test]
    fn a_request_id_is_a_short_string_of_the_phones() {
        use serde_json::json;
        assert_eq!(request_id(&json!({ "id": "h1" })), Some("h1"));
        assert_eq!(request_id(&json!({ "id": "E621E1F8-C36C-495A-93FC-0C247A3E6E5F" })).map(str::len), Some(36));
        assert_eq!(request_id(&json!({ "id": "é".repeat(64) })).map(|i| i.chars().count()), Some(64));
        for bad in [json!({}), json!({ "id": "" }), json!({ "id": 7 }), json!({ "id": "a".repeat(65) }), json!({ "id": "a\nb" })] {
            assert_eq!(request_id(&bad), None, "{bad}");
        }
    }

    #[test]
    fn a_rate_keeps_at_most_so_many_in_the_window() {
        let mut rate = Rate::default();
        for i in 0..3 {
            assert!(rate.allow(1_000 + i, 3, 10_000));
        }
        assert!(!rate.allow(2_000, 3, 10_000));
        assert!(rate.full(10_999, 3, 10_000));
        assert!(rate.allow(11_000, 3, 10_000), "the first one left the window");
    }
}
