// The record of what Mochi's home did: every action and routine, who asked (the island's chat, the phone, Settings,
// a routine), on which device, and how it went. One JSON line each in home-log.jsonl, next to coucou.log, on this
// computer only. Settings → Mochi's home shows the latest. Never a token or a key.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Past this size the file keeps only its newest lines.
const MAX_BYTES: u64 = 512 * 1024;
const KEEP_LINES: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Local time, "2026-10-13 21:05:12".
    pub at: String,
    /// "island", "phone", "settings" or "routine".
    pub origin: String,
    pub device: String,
    /// The action in the user's words ("Turn on", "Open YouTube").
    pub action: String,
    pub ok: bool,
    /// What the device reported afterwards, or why it failed.
    pub detail: String,
}

impl Entry {
    pub fn now(origin: &str, device: &str, action: &str, ok: bool, detail: &str) -> Entry {
        let t = crate::platform::local_time();
        Entry {
            at: format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second),
            origin: origin.into(),
            device: device.chars().take(100).collect(),
            action: action.chars().take(120).collect(),
            ok,
            detail: detail.chars().take(300).collect(),
        }
    }
}

pub fn path() -> PathBuf {
    crate::settings::local_dir().join("home-log.jsonl")
}

/// Adds one entry. A failure to write is not the action's failure: it is only logged.
pub fn append(entry: &Entry) {
    let dir = crate::settings::local_dir();
    if crate::platform::ensure_private_dir(&dir).is_err() {
        return;
    }
    if let Err(e) = append_to(&path(), entry) {
        crate::log::line(format!("home: could not write the record: {e}"));
    }
}

pub fn append_to(path: &Path, entry: &Entry) -> std::io::Result<()> {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        let kept = lines[lines.len().saturating_sub(KEEP_LINES)..].join("\n");
        std::fs::write(path, format!("{kept}\n"))?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    writeln!(file, "{}", serde_json::to_string(entry).unwrap_or_default())
}

/// The newest `count` entries, newest first.
pub fn recent(count: usize) -> Vec<Entry> {
    recent_from(&path(), count)
}

pub fn recent_from(path: &Path, count: usize) -> Vec<Entry> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines().rev().filter_map(|l| serde_json::from_str(l).ok()).take(count).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_kept_newest_first_and_the_file_stays_small() {
        let dir = std::env::temp_dir().join(format!("coucou-home-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("home-log.jsonl");
        for i in 0..3 {
            append_to(&file, &Entry::now("island", "TV da sala", &format!("Volume {i}%"), i != 1, "ok")).unwrap();
        }
        let got = recent_from(&file, 2);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].action, "Volume 2%");
        assert!(!got[1].ok);
        assert_eq!(got[0].at.len(), 19);
        // A broken line is skipped, not fatal.
        std::fs::write(&file, "not json\n").unwrap();
        append_to(&file, &Entry::now("phone", "Luz", "Turn on", true, "")).unwrap();
        assert_eq!(recent_from(&file, 10).len(), 1);
        // Past the size limit, only the newest lines stay.
        let big = Entry::now("routine", "x", "y", true, &"z".repeat(290));
        while std::fs::metadata(&file).unwrap().len() <= MAX_BYTES {
            append_to(&file, &big).unwrap();
        }
        append_to(&file, &Entry::now("settings", "last", "Turn off", true, "")).unwrap();
        let lines = std::fs::read_to_string(&file).unwrap().lines().count();
        assert!(lines <= KEEP_LINES + 1, "{lines}");
        assert_eq!(recent_from(&file, 1)[0].device, "last");
        assert!(recent_from(&dir.join("missing.jsonl"), 5).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
