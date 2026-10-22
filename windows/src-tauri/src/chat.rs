// The chat, whoever answers it: the conversation, the system prompt, and which
// provider a turn goes to. Each provider's own wire format lives in its module:
// claude.rs (Anthropic), openai_compat.rs (OpenAI, Google AI, OpenRouter) and
// local_chat.rs (Ollama, LM Studio, any OpenAI-compatible server).
//
// API keys never leave the credential store and file bytes never cross the IPC
// boundary: the island sends the question and gets the answer's text back.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::chat_history::{self, Kept, Shelf, ShelfView};
use crate::settings::Settings;
use crate::{claude, local_chat, openai_compat, secrets};

pub const ANTHROPIC: &str = "anthropic";

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChatContext {
    File { name: String, path: String },
    Window { app_name: String, title: String, url: Option<String> },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatReply {
    pub text: String,
}

/// A model a provider offers, for the picker in the chat view.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
}

// ── Conversation ──────────────────────────────────────────────────────────────

/// The conversation is kept twice: in the wire format of the provider that
/// answered last (Claude's carries web search blocks no other provider would
/// understand), and as plain text turns. Switching provider mid-conversation
/// rebuilds the history from the plain turns, so nothing in one provider's
/// format is ever sent to another.
#[derive(Default)]
pub struct Chat {
    inner: Mutex<Conversation>,
    /// Where the island's conversations are kept between launches (chat_history.rs). The phone's chat keeps none.
    shelf: Option<Shelf>,
}

#[derive(Default)]
struct Conversation {
    /// Bumped by every reset, so an answer that lands after "New chat" is dropped.
    epoch: u64,
    /// The provider `native` belongs to.
    owner: Option<String>,
    native: Vec<Value>,
    /// `{"role", "content": text}` turns, the same whoever answered.
    plain: Vec<Value>,
    /// The session Hermes Agent keeps this conversation in (hermes.rs), made on first use; a new chat gets a new one.
    /// A kept conversation keeps its session, so Hermes goes on with what it remembers of it.
    hermes_session: Option<String>,
    /// Its place among the kept conversations, and when it began: from its first recorded turn.
    id: Option<String>,
    created: i64,
    /// The kept conversation the island was on has been taken up (at first use: the keystore is not asked at start).
    resumed: bool,
}

impl Conversation {
    /// A new, empty conversation; the kept one, if any, is not taken up again.
    fn fresh(epoch: u64) -> Self {
        Self { epoch, resumed: true, ..Default::default() }
    }

    fn resume(&mut self, kept: Kept) {
        self.owner = kept.owner;
        self.native = kept.native;
        self.plain = kept.plain;
        self.hermes_session = kept.hermes_session;
        self.id = Some(kept.id);
        self.created = kept.created;
    }
}

/// How many earlier messages a turn sends at most: a conversation kept for days must still fit the model's window.
/// Hermes keeps its own history and is not sent these.
const MAX_SENT: usize = 120;

/// The last of the history, whole turns only: from a message the user wrote (not a tool's result), so no tool call
/// is ever sent without its answer.
fn recent(history: &[Value]) -> Vec<Value> {
    if history.len() <= MAX_SENT {
        return history.to_vec();
    }
    let user_turn = |m: &Value| {
        m["role"] == "user"
            && match &m["content"] {
                Value::String(_) => true,
                Value::Array(blocks) => !blocks.iter().any(|b| b["type"] == "tool_result"),
                _ => false,
            }
    };
    match (history.len() - MAX_SENT..history.len()).find(|&i| user_turn(&history[i])) {
        Some(from) => history[from..].to_vec(),
        None => history.to_vec(),
    }
}

/// What a provider needs to build one turn.
pub struct Turn {
    epoch: u64,
    provider: String,
    /// No earlier turn: the file or window context rides along with this one.
    pub first: bool,
    /// The earlier turns, in this provider's format.
    pub history: Vec<Value>,
}

impl Chat {
    /// The island's chat: it goes on with the conversation it was in when Coucou last closed.
    pub fn kept(shelf: Shelf) -> Self {
        Self { inner: Mutex::new(Conversation::default()), shelf: Some(shelf) }
    }

    /// The conversation, the kept one taken up first if this is the first use.
    fn lock(&self) -> std::sync::MutexGuard<'_, Conversation> {
        let mut c = self.inner.lock().unwrap();
        if !c.resumed {
            c.resumed = true;
            if let Some(kept) = self.shelf.as_ref().and_then(Shelf::current) {
                c.resume(kept);
            }
        }
        c
    }

    /// "New chat": the next turn starts a new conversation. The one before stays among the kept ones.
    pub fn reset(&self) {
        {
            let mut c = self.lock();
            let epoch = c.epoch + 1;
            *c = Conversation::fresh(epoch);
        }
        if let Some(shelf) = &self.shelf {
            shelf.set_current(None);
        }
    }

    /// Goes back to a kept conversation: its words on the island, its history for the provider, its Hermes session.
    pub fn open(&self, id: &str) -> Result<(), String> {
        let kept = self.shelf.as_ref().and_then(|s| s.get(id));
        let Some(kept) = kept else {
            return Err(crate::i18n::t("This conversation is no longer kept."));
        };
        {
            let mut c = self.lock();
            let epoch = c.epoch + 1;
            *c = Conversation::fresh(epoch);
            c.resume(kept);
        }
        if let Some(shelf) = &self.shelf {
            shelf.set_current(Some(id.to_string()));
        }
        Ok(())
    }

    /// Erases a kept conversation for good; erasing the one on the island starts a new one.
    pub fn forget(&self, id: &str) {
        {
            let mut c = self.lock();
            if c.id.as_deref() == Some(id) {
                let epoch = c.epoch + 1;
                *c = Conversation::fresh(epoch);
            }
        }
        if let Some(shelf) = &self.shelf {
            shelf.remove(id);
        }
    }

    /// What the island shows: the conversation on it and the kept ones.
    pub fn view(&self) -> ShelfView {
        let (current, messages) = {
            let c = self.lock();
            (c.id.clone(), chat_history::lines(&c.plain))
        };
        let chats = self.shelf.as_ref().map(Shelf::summaries).unwrap_or_default();
        ShelfView { current, messages, chats }
    }

    /// The conversation as it now is, kept: after each recorded turn.
    fn keep(&self, c: &mut Conversation) {
        let Some(shelf) = &self.shelf else { return };
        if c.plain.is_empty() {
            return;
        }
        let now = chat_history::now();
        let id = match &c.id {
            Some(id) => id.clone(),
            None => {
                let id = chat_history::new_id();
                c.id = Some(id.clone());
                c.created = now;
                id
            }
        };
        shelf.put(Kept {
            id,
            title: chat_history::title_of(&c.plain),
            created: c.created,
            updated: now,
            owner: c.owner.clone(),
            native: c.native.clone(),
            plain: c.plain.clone(),
            hermes_session: c.hermes_session.clone(),
        });
    }

    /// The session Hermes Agent keeps this conversation in: "coucou-" and random hex, the same until "New chat".
    pub fn hermes_session(&self) -> String {
        let mut c = self.lock();
        c.hermes_session
            .get_or_insert_with(|| {
                let mut bytes = [0u8; 8];
                let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes);
                format!("coucou-{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
            })
            .clone()
    }

    /// Starts a turn with `provider`, converting the history if another
    /// provider answered the previous turns.
    pub fn begin(&self, provider: &str) -> Turn {
        let mut c = self.lock();
        if c.owner.as_deref() != Some(provider) {
            c.native = c.plain.clone();
            c.owner = Some(provider.to_string());
        }
        Turn {
            epoch: c.epoch,
            provider: provider.to_string(),
            first: c.plain.is_empty(),
            history: recent(&c.native),
        }
    }

    /// Records a finished turn: the user message and the answer in the
    /// provider's format, and their plain text. Only a successful turn is
    /// recorded, so the history always matches what the model saw.
    pub fn commit(&self, turn: &Turn, user: Value, assistant: Value, user_text: &str, answer: &str) {
        let mut c = self.lock();
        if c.epoch != turn.epoch || c.owner.as_deref() != Some(turn.provider.as_str()) {
            return;
        }
        c.native.push(user);
        c.native.push(assistant);
        c.plain.push(json!({ "role": "user", "content": user_text }));
        c.plain.push(json!({ "role": "assistant", "content": answer }));
        self.keep(&mut c);
    }

    /// Records a finished turn of Mochi with hands: every message of it in the provider's format (the user's, the
    /// tool calls, their results and the answer, in order), and the question and answer as plain text. The tool
    /// work stays in this provider's history only; another provider, or the chat without hands, starts from the
    /// plain turns.
    pub fn commit_turn(&self, turn: &Turn, messages: Vec<Value>, user_text: &str, answer: &str) {
        let mut c = self.lock();
        if c.epoch != turn.epoch || c.owner.as_deref() != Some(turn.provider.as_str()) {
            return;
        }
        c.native.extend(messages);
        c.plain.push(json!({ "role": "user", "content": user_text }));
        c.plain.push(json!({ "role": "assistant", "content": answer }));
        self.keep(&mut c);
    }
}

// ── System prompt ─────────────────────────────────────────────────────────────

/// Mochi's instructions. Greets the user by their first name when the account
/// has one worth using (identity.rs), and only claims web search where the
/// provider runs it (Claude).
pub fn system_prompt(web_search: bool) -> String {
    system_prompt_for(crate::identity::first_name(), web_search)
}

fn system_prompt_for(first_name: Option<&str>, web_search: bool) -> String {
    let opening = match first_name {
        Some(name) => format!("You are AXYON, {name}'s personal AI assistant living at the top of their screen."),
        None => "You are AXYON, a personal AI assistant living at the top of the user's screen.".to_string(),
    };
    let abilities = if web_search {
        "You have web search access and can help with absolutely anything — research, coding, finding places, recommendations, tasks, questions."
    } else {
        "You can help with absolutely anything — research, coding, recommendations, tasks, questions. You have no web access: say so when something needs current information."
    };
    format!(
        "{opening} {abilities} \
Respond in the user's language. Be thorough and complete — use as much detail as the task requires. \
Use light Markdown when it helps: short paragraphs, bullet lists, **bold**, `inline code` and fenced code blocks. Avoid tables and big headings: the chat window is small."
    )
}

/// The window context line, as ClaudeService.chat() writes it.
pub fn window_line(app_name: &str, title: &str, url: Option<&str>) -> String {
    let mut text = format!("Context — App: {app_name}, Window: {title}");
    if let Some(url) = url {
        text.push_str(&format!(", URL: {url}"));
    }
    text
}

/// The plain-text record of what the user asked, context included.
pub fn plain_question(first: bool, context: Option<&ChatContext>, query: &str) -> String {
    match context.filter(|_| first) {
        Some(ChatContext::File { name, .. }) => format!("File: {name}\n\n{query}"),
        Some(ChatContext::Window { app_name, title, url }) => {
            format!("{}\n\n{query}", window_line(app_name, title, url.as_deref()))
        }
        None => query.to_string(),
    }
}

// ── Which provider ────────────────────────────────────────────────────────────

/// The model chosen for `provider`, or its default.
pub fn model_for(settings: &Settings, provider: &str) -> String {
    if provider == ANTHROPIC {
        let m = settings.model.trim();
        return if m.is_empty() { claude::DEFAULT_MODEL.to_string() } else { m.to_string() };
    }
    settings
        .chat_models
        .get(provider)
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .or_else(|| openai_compat::provider(provider).map(|p| p.default_model.to_string()))
        .unwrap_or_default()
}

/// A file rides along only if it is one of Coucou's own copies of a dropped
/// file (files.rs puts them in the inbox). The page names the path, so without
/// this any file the user can read could be sent to a chat provider.
pub(crate) fn checked_context(context: ChatContext) -> Result<ChatContext, String> {
    match context {
        ChatContext::File { name, path } => {
            let inbox = crate::files::inbox_dir();
            if !is_inside(&inbox, std::path::Path::new(&path)) {
                return Err(crate::i18n::t("Only a file dropped on the island can be sent with a question."));
            }
            Ok(ChatContext::File { name, path })
        }
        other => Ok(other),
    }
}

/// True when `path` is a regular file directly inside `dir`, both resolved
/// (no `..`, no symlink pointing out of it).
fn is_inside(dir: &std::path::Path, path: &std::path::Path) -> bool {
    let (Ok(dir), Ok(file)) = (dir.canonicalize(), path.canonicalize()) else { return false };
    file.parent() == Some(dir.as_path())
        && std::fs::symlink_metadata(&file).map(|m| m.is_file()).unwrap_or(false)
}

/// One chat turn with the provider chosen in the settings.
pub async fn send(
    app: &AppHandle,
    chat: &Chat,
    settings: &Settings,
    query: String,
    context: Option<ChatContext>,
) -> Result<ChatReply, String> {
    let context = context.map(checked_context).transpose()?;
    let provider = settings.chat_provider.as_str();
    let model = model_for(settings, provider);
    if provider == ANTHROPIC || provider.is_empty() {
        return claude::send(chat, &model, query, context).await;
    }
    if let Some(p) = openai_compat::provider(provider) {
        return openai_compat::send(chat, p, &model, query, context).await;
    }
    if let Some(server) = local_chat::server(settings, provider) {
        return local_chat::send(app, chat, &server, &model, query, context).await;
    }
    Err(format!("Unknown chat provider: {provider}"))
}

/// One turn of the phone's conversation (phone_link/chat_backend.rs): the same providers as
/// [`send`], with no file or window context and no `AppHandle`. `on_delta` gets the streamed
/// text so far, which only the model servers produce; the cloud providers answer all at once.
pub async fn send_for_phone(
    settings: &Settings,
    chat: &Chat,
    provider: &str,
    model: &str,
    query: String,
    on_delta: impl FnMut(String),
) -> Result<ChatReply, String> {
    if provider == ANTHROPIC {
        return claude::send(chat, model, query, None).await;
    }
    if let Some(p) = openai_compat::provider(provider) {
        return openai_compat::send(chat, p, model, query, None).await;
    }
    if let Some(server) = local_chat::server(settings, provider) {
        return local_chat::send_with(chat, &server, model, query, None, on_delta).await;
    }
    Err(format!("Unknown chat provider: {provider}"))
}

/// The models `provider` offers. Asked only when the user opens the picker on
/// that provider, and only once it has a key (or, for a local server, an
/// address): nothing is sent anywhere before that.
pub async fn models(settings: &Settings, provider: &str) -> Result<Vec<ModelInfo>, String> {
    let no_key = || crate::i18n::t("No API key — add it in Settings.");
    if provider == ANTHROPIC {
        let key = secrets::get(claude::KEY).ok_or_else(no_key)?;
        return claude::models(&key).await;
    }
    if let Some(p) = openai_compat::provider(provider) {
        let key = secrets::get(p.key).ok_or_else(no_key)?;
        return openai_compat::models(p, &key).await;
    }
    if let Some(server) = local_chat::server(settings, provider) {
        return local_chat::models(&server).await;
    }
    if provider == crate::hermes::PROVIDER {
        let models = crate::hermes::Client::from_settings(settings)?.models().await?;
        return Ok(models.into_iter().map(|id| ModelInfo { label: id.clone(), id }).collect());
    }
    Err(format!("Unknown chat provider: {provider}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The island's chat over a kept file of its own, as it is after a restart when `path` already holds one.
    fn island_chat(path: &std::path::Path) -> Chat {
        Chat::kept(Shelf::with_key(path.to_path_buf(), [9; 32]))
    }

    fn temp_file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("coucou-chat-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("chats.bin")
    }

    fn say(chat: &Chat, provider: &str, question: &str, answer: &str) {
        let t = chat.begin(provider);
        chat.commit(&t, json!({"role":"user","content":question}), json!({"role":"assistant","content":answer}), question, answer);
    }

    #[test]
    fn the_island_goes_on_where_it_was_after_a_restart() {
        let path = temp_file("restart");
        let chat = island_chat(&path);
        let session = chat.hermes_session();
        say(&chat, "anthropic", "Onde aporto este mês?", "Em GGRC11.");
        drop(chat);

        // Coucou (or the computer) restarts.
        let chat = island_chat(&path);
        let view = chat.view();
        assert_eq!(view.messages.len(), 2);
        assert_eq!(view.messages[0].content, "Onde aporto este mês?");
        assert_eq!(view.chats.len(), 1);
        assert_eq!(view.current, Some(view.chats[0].id.clone()));
        // Hermes is asked in the same session, so it remembers it too; another provider is sent the history again.
        assert_eq!(chat.hermes_session(), session);
        let t = chat.begin("anthropic");
        assert!(!t.first);
        assert_eq!(t.history.len(), 2);
    }

    #[test]
    fn a_new_chat_keeps_the_old_one_which_can_be_opened_again() {
        let path = temp_file("new");
        let chat = island_chat(&path);
        let old_session = chat.hermes_session();
        say(&chat, "hermes", "Assunto X", "Sobre X…");
        let old = chat.view().current.unwrap();

        chat.reset();
        let view = chat.view();
        assert!(view.messages.is_empty() && view.current.is_none());
        assert_ne!(chat.hermes_session(), old_session, "a new chat, a new Hermes session");
        say(&chat, "hermes", "Assunto Y", "Sobre Y…");
        assert_eq!(chat.view().chats.len(), 2);

        // Back to X, also after a restart.
        chat.open(&old).unwrap();
        assert_eq!(chat.view().messages[0].content, "Assunto X");
        assert_eq!(chat.hermes_session(), old_session);
        drop(chat);
        let chat = island_chat(&path);
        assert_eq!(chat.view().current.as_deref(), Some(old.as_str()));
        assert!(chat.open("c-gone").is_err());
    }

    #[test]
    fn an_answer_that_lands_after_new_chat_or_another_conversation_is_dropped() {
        let chat = island_chat(&temp_file("late"));
        say(&chat, "anthropic", "um", "1");
        let first = chat.view().current.unwrap();
        let late = chat.begin("anthropic");
        chat.reset();
        chat.commit(&late, json!({"role":"user","content":"dois"}), json!({"role":"assistant","content":"2"}), "dois", "2");
        assert!(chat.view().messages.is_empty());
        let late = chat.begin("anthropic");
        chat.open(&first).unwrap();
        chat.commit(&late, json!({"role":"user","content":"três"}), json!({"role":"assistant","content":"3"}), "três", "3");
        assert_eq!(chat.view().messages.len(), 2, "the kept conversation is as it was");
    }

    #[test]
    fn erasing_the_conversation_on_the_island_starts_anew() {
        let path = temp_file("erase");
        let chat = island_chat(&path);
        say(&chat, "anthropic", "apague isto", "ok");
        let id = chat.view().current.unwrap();
        chat.forget(&id);
        let view = chat.view();
        assert!(view.messages.is_empty() && view.chats.is_empty() && view.current.is_none());
        drop(chat);
        assert!(island_chat(&path).view().chats.is_empty(), "gone from the file too");
    }

    #[test]
    fn the_phone_chat_keeps_nothing() {
        let chat = Chat::default();
        say(&chat, "anthropic", "oi", "olá");
        let view = chat.view();
        assert_eq!(view.messages.len(), 2);
        assert!(view.current.is_none() && view.chats.is_empty());
    }

    #[test]
    fn a_long_conversation_sends_its_last_whole_turns() {
        let mut history = Vec::new();
        for i in 0..100 {
            history.push(json!({"role":"user","content":format!("q{i}")}));
            // A turn with a tool call: its result is a user message that is not a question.
            history.push(json!({"role":"assistant","content":[{"type":"tool_use","id":format!("t{i}"),"name":"read","input":{}}]}));
            history.push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":format!("t{i}"),"content":"…"}]}));
            history.push(json!({"role":"assistant","content":"a"}));
        }
        let sent = recent(&history);
        assert!(sent.len() <= MAX_SENT);
        assert_eq!(sent[0]["role"], "user");
        assert!(sent[0]["content"].is_string(), "it starts at a question, never at a tool's result");
        assert_eq!(sent.last(), history.last());
        let short = &history[..8];
        assert_eq!(recent(short), short.to_vec());
    }

    fn turn_texts(history: &[Value]) -> Vec<(String, Value)> {
        history
            .iter()
            .map(|m| (m["role"].as_str().unwrap().to_string(), m["content"].clone()))
            .collect()
    }

    #[test]
    fn a_turn_is_recorded_only_once_it_succeeds() {
        let chat = Chat::default();
        let t = chat.begin("anthropic");
        assert!(t.first);
        assert!(t.history.is_empty());
        // A failed turn records nothing: the next one is still the first.
        let t = chat.begin("anthropic");
        assert!(t.first);
        chat.commit(&t, json!({"role":"user","content":[{"type":"text","text":"hi"}]}), json!({"role":"assistant","content":[{"type":"text","text":"hello"}]}), "hi", "hello");
        let t = chat.begin("anthropic");
        assert!(!t.first);
        assert_eq!(t.history.len(), 2);
        assert!(t.history[0]["content"].is_array());
    }

    #[test]
    fn switching_provider_never_sends_claude_blocks_to_another_one() {
        let chat = Chat::default();
        let t = chat.begin("anthropic");
        let blocks = json!([
            {"type":"server_tool_use","id":"srvtoolu_1","name":"web_search","input":{"query":"x"}},
            {"type":"web_search_tool_result","tool_use_id":"srvtoolu_1","content":[]},
            {"type":"text","text":"Found it."}
        ]);
        chat.commit(&t, json!({"role":"user","content":[{"type":"text","text":"look"}]}), json!({"role":"assistant","content":blocks}), "look", "Found it.");

        let t = chat.begin("openai");
        assert!(!t.first);
        assert_eq!(
            turn_texts(&t.history),
            vec![("user".into(), json!("look")), ("assistant".into(), json!("Found it."))]
        );
        let raw = serde_json::to_string(&t.history).unwrap();
        assert!(!raw.contains("web_search"), "{raw}");

        // And back: Claude gets the plain turns too, including OpenAI's answer.
        chat.commit(&t, json!({"role":"user","content":"more"}), json!({"role":"assistant","content":"Sure."}), "more", "Sure.");
        let t = chat.begin("anthropic");
        assert_eq!(t.history.len(), 4);
        assert_eq!(t.history[3], json!({"role":"assistant","content":"Sure."}));
        assert!(!serde_json::to_string(&t.history).unwrap().contains("web_search"));
    }

    #[test]
    fn a_turn_with_hands_keeps_its_tool_messages_for_the_same_provider_only() {
        let chat = Chat::default();
        let t = chat.begin("openrouter+hands");
        let turn = vec![
            json!({"role":"user","content":"list my files"}),
            json!({"role":"assistant","content":null,"tool_calls":[{"id":"a","type":"function","function":{"name":"list_directory","arguments":"{}"}}]}),
            json!({"role":"tool","tool_call_id":"a","content":"src/\nREADME.md"}),
            json!({"role":"assistant","content":"You have src and a README."}),
        ];
        chat.commit_turn(&t, turn.clone(), "list my files", "You have src and a README.");
        let again = chat.begin("openrouter+hands");
        assert_eq!(again.history, turn);
        // The same provider without hands gets the plain turns: no tool message reaches a request without tools.
        let plain = chat.begin("openrouter");
        assert_eq!(
            turn_texts(&plain.history),
            vec![("user".into(), json!("list my files")), ("assistant".into(), json!("You have src and a README."))]
        );
        // An answer that lands after a reset is dropped.
        let t = chat.begin("anthropic+hands");
        chat.reset();
        chat.commit_turn(&t, vec![json!({"role":"user","content":"x"})], "x", "y");
        assert!(chat.begin("anthropic+hands").first);
    }

    #[test]
    fn an_answer_that_lands_after_a_reset_or_a_switch_is_dropped() {
        let chat = Chat::default();
        let t = chat.begin("openai");
        chat.reset();
        chat.commit(&t, json!({"role":"user","content":"q"}), json!({"role":"assistant","content":"a"}), "q", "a");
        assert!(chat.begin("openai").first);

        let t = chat.begin("openai");
        let _other = chat.begin("google");
        chat.commit(&t, json!({"role":"user","content":"q"}), json!({"role":"assistant","content":"a"}), "q", "a");
        assert!(chat.begin("google").first);
    }

    #[test]
    fn the_prompt_greets_by_first_name_and_claims_web_search_only_for_claude() {
        let p = system_prompt_for(Some("Louis"), true);
        assert!(p.starts_with("You are AXYON, Louis's personal AI assistant living at the top of their screen."));
        assert!(p.contains("web search access"));
        assert!(p.contains("light Markdown"));
        let p = system_prompt_for(None, false);
        assert!(p.starts_with("You are AXYON, a personal AI assistant living at the top of the user's screen."));
        assert!(!p.contains("web search"));
        assert!(p.contains("no web access"));
    }

    #[test]
    fn context_goes_with_the_first_question_only() {
        let file = ChatContext::File { name: "a.txt".into(), path: "/x/a.txt".into() };
        assert_eq!(plain_question(true, Some(&file), "why?"), "File: a.txt\n\nwhy?");
        assert_eq!(plain_question(false, Some(&file), "why?"), "why?");
        let win = ChatContext::Window { app_name: "Code".into(), title: "main.rs".into(), url: None };
        assert_eq!(plain_question(true, Some(&win), "q"), "Context — App: Code, Window: main.rs\n\nq");
        assert_eq!(window_line("Edge", "Docs", Some("https://x.dev")), "Context — App: Edge, Window: Docs, URL: https://x.dev");
    }

    #[test]
    fn the_model_comes_from_the_settings_or_the_provider_default() {
        let mut s = Settings::default();
        assert_eq!(model_for(&s, "anthropic"), claude::DEFAULT_MODEL);
        assert_eq!(model_for(&s, "openai"), openai_compat::provider("openai").unwrap().default_model);
        assert_eq!(model_for(&s, "ollama"), "");
        s.chat_models.insert("openai".into(), " gpt-x ".into());
        s.chat_models.insert("ollama".into(), "llama3.2".into());
        s.model = "claude-haiku-4-5".into();
        assert_eq!(model_for(&s, "openai"), "gpt-x");
        assert_eq!(model_for(&s, "ollama"), "llama3.2");
        assert_eq!(model_for(&s, "anthropic"), "claude-haiku-4-5");
    }

    #[test]
    fn only_files_in_the_inbox_ride_along() {
        let base = std::env::temp_dir().join(format!("coucou-chat-ctx-{}", std::process::id()));
        let inbox = base.join("inbox");
        std::fs::create_dir_all(inbox.join("sub")).unwrap();
        std::fs::write(inbox.join("a.txt"), b"a").unwrap();
        std::fs::write(base.join("secret.txt"), b"s").unwrap();
        std::fs::write(inbox.join("sub").join("b.txt"), b"b").unwrap();
        assert!(is_inside(&inbox, &inbox.join("a.txt")));
        assert!(!is_inside(&inbox, &base.join("secret.txt")));
        assert!(!is_inside(&inbox, &inbox.join("..").join("secret.txt")));
        assert!(!is_inside(&inbox, &inbox.join("sub").join("b.txt")));
        assert!(!is_inside(&inbox, &inbox.join("missing.txt")));
        assert!(!is_inside(&inbox, &inbox));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(base.join("secret.txt"), inbox.join("link.txt")).unwrap();
            assert!(!is_inside(&inbox, &inbox.join("link.txt")));
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
