// The island's conversations, kept between launches: a conversation goes on where it was after Coucou, or the
// computer, restarts — its words back on the island, the history a provider is sent again, and the session Hermes
// keeps it in, which Hermes picks up with everything it remembers of it (Hermes stores its sessions itself). "+"
// starts a new conversation and keeps this one in the list, from which a kept one can be opened again or erased.
//
// They live in chats.bin under platform::local_dir(), sealed (AES-256-GCM) with a key of their own kept in the
// system's keystore: the file alone reads as nothing, and nothing of it ever leaves this computer. A file that can't
// be opened (its key gone) is set aside, never lost. The phone's conversation is not kept here: the phone keeps its
// own.

use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::platform;

/// The keystore entry of the key the file is sealed with.
pub const KEY: &str = "chat-history-key";
const FILE: &str = "chats.bin";
/// What the file starts with, and what its seal covers besides the conversations.
const MAGIC: &[u8] = b"coucou-chats-1\n";
/// At most this many conversations are kept: the one left longest goes first.
pub const MAX_CHATS: usize = 50;
/// A conversation's words, at most: its oldest turns go first.
const MAX_PLAIN: usize = 400;
/// A provider's own record of a conversation (its tool calls and their results) beyond this is not kept: the next
/// turn rebuilds it from the conversation's words.
const MAX_NATIVE_BYTES: usize = 1 << 20;
/// All the kept conversations together, at most: first the records of the ones left longest go, then those
/// conversations themselves; never the one on the island.
const MAX_TOTAL_BYTES: usize = 24 << 20;
/// A file bigger than this is not read (one Coucou wrote is never this big).
const MAX_FILE_BYTES: u64 = 64 << 20;
/// A title, at most, in characters.
const TITLE_CHARS: usize = 60;

/// One kept conversation, as chat.rs holds it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Kept {
    pub id: String,
    pub title: String,
    /// Unix seconds.
    pub created: i64,
    pub updated: i64,
    /// The provider `native` belongs to.
    pub owner: Option<String>,
    pub native: Vec<Value>,
    pub plain: Vec<Value>,
    pub hermes_session: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Shelved {
    /// The conversation on the island, when it is one of these.
    current: Option<String>,
    chats: Vec<Kept>,
}

/// One line of a conversation, as the island shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    pub role: String,
    pub content: String,
}

/// A kept conversation in the island's list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub updated: i64,
    /// How many questions it holds.
    pub turns: usize,
}

/// What the island shows: the conversation on screen (None before its first answer) and the kept ones, newest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShelfView {
    pub current: Option<String>,
    pub messages: Vec<Line>,
    pub chats: Vec<Summary>,
}

struct Loaded {
    /// None: the keystore gave no key this time, so nothing is written (the file stays as it was).
    key: Option<LessSafeKey>,
    shelved: Mutex<Shelved>,
}

/// The kept conversations and their file.
pub struct Shelf {
    path: PathBuf,
    /// Read at first use, not at start: a keystore may be slow to answer just after login.
    loaded: OnceLock<Loaded>,
}

impl Shelf {
    /// The island's, in its usual place.
    pub fn island() -> Self {
        Self { path: platform::local_dir().join(FILE), loaded: OnceLock::new() }
    }

    /// One sealed with `key`, read now (tests).
    #[cfg(test)]
    pub fn with_key(path: PathBuf, key: [u8; 32]) -> Self {
        let shelf = Self { path, loaded: OnceLock::new() };
        let key = sealing_key(&key);
        let shelved = read(&shelf.path, key.as_ref());
        let _ = shelf.loaded.set(Loaded { key, shelved: Mutex::new(shelved) });
        shelf
    }

    fn loaded(&self) -> &Loaded {
        self.loaded.get_or_init(|| {
            let key = keystore_key().and_then(|k| sealing_key(&k));
            let shelved = read(&self.path, key.as_ref());
            Loaded { key, shelved: Mutex::new(shelved) }
        })
    }

    /// The conversation the island was on.
    pub fn current(&self) -> Option<Kept> {
        let shelved = self.loaded().shelved.lock().unwrap();
        let id = shelved.current.as_deref()?;
        shelved.chats.iter().find(|k| k.id == id).cloned()
    }

    pub fn get(&self, id: &str) -> Option<Kept> {
        self.loaded().shelved.lock().unwrap().chats.iter().find(|k| k.id == id).cloned()
    }

    /// A conversation as it now is, which is also the one on the island.
    pub fn put(&self, kept: Kept) {
        let loaded = self.loaded();
        let mut shelved = loaded.shelved.lock().unwrap();
        let kept = trimmed(kept);
        shelved.current = Some(kept.id.clone());
        shelved.chats.retain(|k| k.id != kept.id);
        shelved.chats.push(kept);
        within_budget(&mut shelved, MAX_TOTAL_BYTES);
        save(&self.path, loaded.key.as_ref(), &shelved);
    }

    /// The conversation on the island: a kept one, or None for a new one.
    pub fn set_current(&self, id: Option<String>) {
        let loaded = self.loaded();
        let mut shelved = loaded.shelved.lock().unwrap();
        if shelved.current == id {
            return;
        }
        shelved.current = id;
        save(&self.path, loaded.key.as_ref(), &shelved);
    }

    /// Erases a conversation for good.
    pub fn remove(&self, id: &str) {
        let loaded = self.loaded();
        let mut shelved = loaded.shelved.lock().unwrap();
        let before = shelved.chats.len();
        shelved.chats.retain(|k| k.id != id);
        if shelved.current.as_deref() == Some(id) {
            shelved.current = None;
        } else if shelved.chats.len() == before {
            return;
        }
        save(&self.path, loaded.key.as_ref(), &shelved);
    }

    /// The kept conversations, the latest first.
    pub fn summaries(&self) -> Vec<Summary> {
        let shelved = self.loaded().shelved.lock().unwrap();
        let mut chats: Vec<Summary> = shelved
            .chats
            .iter()
            .map(|k| Summary {
                id: k.id.clone(),
                title: k.title.clone(),
                updated: k.updated,
                turns: k.plain.iter().filter(|m| m["role"] == "user").count(),
            })
            .collect();
        chats.sort_by(|a, b| b.updated.cmp(&a.updated));
        chats
    }
}

/// At most MAX_CHATS conversations and `budget` bytes in all: the ones left longest give way first — their provider
/// records, then themselves — and the one on the island never does.
fn within_budget(shelved: &mut Shelved, budget: usize) {
    shelved.chats.sort_by_key(|k| k.updated);
    if shelved.chats.len() > MAX_CHATS {
        let excess = shelved.chats.len() - MAX_CHATS;
        shelved.chats.drain(..excess);
    }
    let size = |k: &Kept| serde_json::to_vec(k).map_or(0, |v| v.len());
    let mut sizes: Vec<usize> = shelved.chats.iter().map(size).collect();
    let mut total: usize = sizes.iter().sum();
    let current = shelved.current.clone();
    let on_island = |k: &Kept| current.as_deref() == Some(k.id.as_str());
    let mut i = 0;
    while total > budget && i < shelved.chats.len() {
        if on_island(&shelved.chats[i]) || shelved.chats[i].native.is_empty() {
            i += 1;
            continue;
        }
        shelved.chats[i].native.clear();
        shelved.chats[i].owner = None;
        let now = size(&shelved.chats[i]);
        total = total - sizes[i] + now;
        sizes[i] = now;
        i += 1;
    }
    let mut i = 0;
    while total > budget && i < shelved.chats.len() {
        if on_island(&shelved.chats[i]) {
            i += 1;
            continue;
        }
        shelved.chats.remove(i);
        total -= sizes.remove(i);
    }
}

/// The lines the island shows of a conversation's words.
pub fn lines(plain: &[Value]) -> Vec<Line> {
    plain
        .iter()
        .filter_map(|m| {
            let role = m["role"].as_str().filter(|r| *r == "user" || *r == "assistant")?;
            Some(Line { role: role.to_string(), content: m["content"].as_str()?.to_string() })
        })
        .collect()
}

/// A conversation's name in the list: its first question, without the file or window it came with.
pub fn title_of(plain: &[Value]) -> String {
    let first = plain.iter().find(|m| m["role"] == "user").and_then(|m| m["content"].as_str()).unwrap_or("");
    // chat::plain_question puts the context first, then a blank line, then the question.
    let question = match first.split_once("\n\n") {
        Some((context, rest)) if context.starts_with("File: ") || context.starts_with("Context — ") => rest,
        _ => first,
    };
    let line = question.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if line.chars().count() <= TITLE_CHARS {
        return line.to_string();
    }
    let cut: String = line.chars().take(TITLE_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

/// A new conversation's id.
pub fn new_id() -> String {
    let mut bytes = [0u8; 8];
    let _ = SystemRandom::new().fill(&mut bytes);
    format!("c-{}", to_hex(&bytes))
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// A conversation within what is kept of one: its last turns, and no provider record too big to keep.
fn trimmed(mut kept: Kept) -> Kept {
    if kept.plain.len() > MAX_PLAIN {
        let mut cut = kept.plain.len() - MAX_PLAIN;
        // Whole turns: from a question.
        while cut < kept.plain.len() && kept.plain[cut]["role"] != "user" {
            cut += 1;
        }
        kept.plain.drain(..cut);
    }
    if serde_json::to_vec(&kept.native).map_or(usize::MAX, |v| v.len()) > MAX_NATIVE_BYTES {
        kept.native.clear();
        kept.owner = None;
    }
    kept
}

// ── The key and the seal ──────────────────────────────────────────────────────

/// The key from the keystore, made and stored there the first time. None when the keystore can't be read now: then
/// nothing is written, so a key that is only out of reach for the moment never has a file sealed with another one.
fn keystore_key() -> Option<[u8; 32]> {
    match crate::secrets::read_internal(KEY) {
        Ok(Some(hex)) => {
            let key = from_hex(&hex);
            if key.is_none() {
                note("the conversations' key in the keystore is not one: they are not kept this time".into());
            }
            key
        }
        Ok(None) => {
            let mut key = [0u8; 32];
            SystemRandom::new().fill(&mut key).ok()?;
            match crate::secrets::set_internal(KEY, &to_hex(&key)) {
                Ok(()) => Some(key),
                Err(err) => {
                    note(format!("could not keep the conversations' key in the keystore: {err}"));
                    None
                }
            }
        }
        Err(err) => {
            note(format!("could not read the conversations' key from the keystore: {err}"));
            None
        }
    }
}

fn sealing_key(key: &[u8; 32]) -> Option<LessSafeKey> {
    UnboundKey::new(&AES_256_GCM, key).ok().map(LessSafeKey::new)
}

fn seal(key: &LessSafeKey, plain: &[u8]) -> Option<Vec<u8>> {
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new().fill(&mut nonce).ok()?;
    let mut sealed = plain.to_vec();
    key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(MAGIC), &mut sealed).ok()?;
    let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + sealed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Some(out)
}

fn unseal(key: &LessSafeKey, bytes: &[u8]) -> Option<Vec<u8>> {
    let rest = bytes.strip_prefix(MAGIC)?;
    if rest.len() < NONCE_LEN {
        return None;
    }
    let (nonce, sealed) = rest.split_at(NONCE_LEN);
    let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
    let mut buf = sealed.to_vec();
    let plain = key.open_in_place(nonce, Aad::from(MAGIC), &mut buf).ok()?;
    Some(plain.to_vec())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(text: &str) -> Option<[u8; 32]> {
    let text = text.trim();
    if text.len() != 64 || !text.is_ascii() {
        return None;
    }
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(key)
}

// ── The file ──────────────────────────────────────────────────────────────────

fn read(path: &Path, key: Option<&LessSafeKey>) -> Shelved {
    let Some(key) = key else { return Shelved::default() };
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_FILE_BYTES) {
        set_aside(path, "too big");
        return Shelved::default();
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Shelved::default(),
        Err(err) => {
            // Unreadable is not lost: left as it is, and the first save replaces it.
            note(format!("can't read {}: {err}", path.display()));
            return Shelved::default();
        }
    };
    match unseal(key, &bytes).and_then(|json| serde_json::from_slice::<Shelved>(&json).ok()) {
        Some(shelved) => shelved,
        None => {
            set_aside(path, "not readable with this computer's key");
            Shelved::default()
        }
    }
}

/// Keeps a file that can't be used as chats.bin.unreadable-YYYYMMDD-HHMMSS, as recap.rs does.
fn set_aside(path: &Path, why: &str) {
    let t = platform::local_time();
    let aside = path.with_extension(format!(
        "bin.unreadable-{:04}{:02}{:02}-{:02}{:02}{:02}",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    ));
    match std::fs::rename(path, &aside) {
        Ok(()) => note(format!("{} was {why} — kept as {}", path.display(), aside.display())),
        Err(err) => note(format!("{} was {why} and could not be set aside: {err}", path.display())),
    }
}

fn save(path: &Path, key: Option<&LessSafeKey>, shelved: &Shelved) {
    let Some(key) = key else { return };
    let written = serde_json::to_vec(shelved)
        .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))
        .and_then(|json| seal(key, &json).ok_or_else(|| std::io::Error::other("could not seal")))
        .and_then(|bytes| write_file(path, &bytes));
    if let Err(err) = written {
        note(format!("could not keep the conversations: {err}"));
    }
}

/// Whole file to a temporary sibling, flushed, then renamed over the old one: a crash never leaves half a file.
fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        platform::ensure_private_dir(dir)?;
    }
    let temp = path.with_extension(format!("bin.coucou-{}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options
        .open(&temp)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

/// One line in coucou.log. Tests must never write to the real one.
fn note(message: String) {
    #[cfg(not(test))]
    crate::log::line(message);
    #[cfg(test)]
    let _ = message;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("coucou-chats-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(FILE)
    }

    fn turn(question: &str, answer: &str) -> Vec<Value> {
        vec![json!({ "role": "user", "content": question }), json!({ "role": "assistant", "content": answer })]
    }

    fn kept(id: &str, updated: i64, question: &str) -> Kept {
        let plain = turn(question, "ok");
        Kept { id: id.into(), title: title_of(&plain), created: updated, updated, plain, ..Default::default() }
    }

    #[test]
    fn the_file_reads_as_nothing_without_its_key() {
        let key = sealing_key(&[7; 32]).unwrap();
        let sealed = seal(&key, b"{\"chats\":[]}").unwrap();
        assert!(sealed.starts_with(MAGIC));
        assert!(!sealed[MAGIC.len()..].windows(5).any(|w| w == b"chats"), "the words are sealed");
        assert_eq!(unseal(&key, &sealed).unwrap(), b"{\"chats\":[]}");
        assert!(unseal(&sealing_key(&[8; 32]).unwrap(), &sealed).is_none(), "another key opens nothing");
        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(unseal(&key, &tampered).is_none(), "a changed byte opens nothing");
    }

    #[test]
    fn conversations_come_back_after_a_restart() {
        let path = temp("restart");
        let shelf = Shelf::with_key(path.clone(), [1; 32]);
        let mut first = kept("c-1", 100, "Onde aporto este mês?");
        first.hermes_session = Some("coucou-abc".into());
        shelf.put(first.clone());
        shelf.put(kept("c-2", 200, "E a Selic?"));
        shelf.set_current(Some("c-1".into()));

        let again = Shelf::with_key(path.clone(), [1; 32]);
        assert_eq!(again.current(), Some(first));
        let ids: Vec<_> = again.summaries().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, ["c-2", "c-1"], "the latest first");

        // Another computer's key (or a lost one): the file is set aside, and nothing is lost.
        let other = Shelf::with_key(path.clone(), [2; 32]);
        assert!(other.summaries().is_empty());
        assert!(!path.exists());
        let aside = std::fs::read_dir(path.parent().unwrap()).unwrap().count();
        assert_eq!(aside, 1, "kept as chats.bin.unreadable-…");
    }

    #[test]
    fn erasing_and_the_oldest_going_first() {
        let shelf = Shelf::with_key(temp("erase"), [3; 32]);
        for i in 0..(MAX_CHATS as i64 + 2) {
            shelf.put(kept(&format!("c-{i}"), 1000 + i, "q"));
        }
        let ids: Vec<_> = shelf.summaries().into_iter().map(|s| s.id).collect();
        assert_eq!(ids.len(), MAX_CHATS);
        assert!(!ids.contains(&"c-0".to_string()) && !ids.contains(&"c-1".to_string()));
        let last = format!("c-{}", MAX_CHATS + 1);
        assert_eq!(shelf.current().map(|k| k.id), Some(last.clone()));
        shelf.remove(&last);
        assert_eq!(shelf.current(), None, "erasing the one on screen leaves none");
        assert!(shelf.get(&last).is_none());
    }

    #[test]
    fn all_of_them_stay_within_a_budget_the_one_on_the_island_kept_whole() {
        let big = |id: &str, updated: i64| {
            let mut k = kept(id, updated, "q");
            k.native = vec![json!({ "role": "user", "content": "x".repeat(400_000) })];
            k.owner = Some("anthropic".into());
            k
        };
        let mut shelved = Shelved { current: Some("c-0".into()), chats: (0..6).map(|i| big(&format!("c-{i}"), i)).collect() };
        within_budget(&mut shelved, 1_000_000);
        let total: usize = shelved.chats.iter().map(|k| serde_json::to_vec(k).unwrap().len()).sum();
        assert!(total <= 1_000_000);
        let first = shelved.chats.iter().find(|k| k.id == "c-0").expect("the one on the island stays");
        assert!(!first.native.is_empty(), "with its record, though it is the oldest");
        assert_eq!(shelved.chats.len(), 6, "the others only lost their records");

        // Words alone too many: the ones left longest go.
        let mut shelved = Shelved { current: Some("c-5".into()), chats: (0..6).map(|i| kept(&format!("c-{i}"), i, &"q".repeat(1000))).collect() };
        within_budget(&mut shelved, 3_000);
        let ids: Vec<_> = shelved.chats.iter().map(|k| k.id.as_str()).collect();
        assert!(ids.contains(&"c-5") && !ids.contains(&"c-0"));
    }

    #[test]
    fn a_long_conversation_keeps_whole_turns_and_no_huge_record() {
        let mut k = kept("c-1", 1, "first");
        for i in 0..(MAX_PLAIN / 2 + 3) {
            k.plain.extend(turn(&format!("q{i}"), "a"));
        }
        k.native = vec![json!({ "role": "user", "content": "x".repeat(MAX_NATIVE_BYTES) })];
        k.owner = Some("anthropic".into());
        let t = trimmed(k);
        assert!(t.plain.len() <= MAX_PLAIN);
        assert_eq!(t.plain[0]["role"], "user");
        assert!(t.native.is_empty() && t.owner.is_none(), "rebuilt from the words at the next turn");
    }

    #[test]
    fn a_title_is_the_first_question() {
        assert_eq!(title_of(&turn("Onde aporto este mês?", "…")), "Onde aporto este mês?");
        assert_eq!(title_of(&turn("File: carteira.xlsx\n\nResuma isto", "…")), "Resuma isto");
        let long = "a".repeat(80);
        let title = title_of(&turn(&long, "…"));
        assert_eq!(title.chars().count(), TITLE_CHARS);
        assert!(title.ends_with('…'));
        assert_eq!(title_of(&[]), "");
    }

    #[test]
    fn the_island_sees_only_questions_and_answers() {
        let mut plain = turn("oi", "olá");
        plain.push(json!({ "role": "tool", "content": "…" }));
        assert_eq!(lines(&plain), vec![
            Line { role: "user".into(), content: "oi".into() },
            Line { role: "assistant".into(), content: "olá".into() },
        ]);
    }

    #[test]
    fn hex_keys() {
        let key = [0xab; 32];
        assert_eq!(from_hex(&to_hex(&key)), Some(key));
        assert_eq!(from_hex("zz"), None);
        assert!(new_id().starts_with("c-") && new_id().len() == 18);
    }
}
