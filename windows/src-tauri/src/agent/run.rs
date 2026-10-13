// One turn of Mochi with hands: ask the model, run the tools it asks for (each through its gate, the click included),
// give it the results, and again, until it answers. No HTTP and no window here: the model is behind `Transport` and
// the user behind `Hands`, so the whole loop is tested below with a scripted model and a scripted user.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering;

use serde::Serialize;
use serde_json::Value;

use super::policy::Gate;
use super::text;
use super::tools::{self, Call, Env, Output, Request};
use super::wire::{self, Flavor, Stop};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Rounds of tool calls in one turn before Mochi stops and says so.
pub const MAX_ROUNDS: usize = 30;
/// Claude's web search may pause a turn this many times.
const MAX_PAUSES: usize = 5;

/// What the model is told when the user said no.
pub const DENIED: &str = "The user denied this action, so it was not done. Do not try another way to do the same thing: stop and ask the user what they want instead.";

/// Posts one request to the provider and returns its JSON answer.
pub trait Transport: Send + Sync {
    fn post<'a>(&'a self, body: &'a Value) -> BoxFuture<'a, Result<Value, String>>;
}

/// The person on the other side: sees the steps, approves or denies.
pub trait Hands: Send + Sync {
    fn step(&self, step: &Step);
    /// Waits for the user's answer to `request`: true to go ahead. False when denied, unanswered or stopped.
    fn approve<'a>(&'a self, request: Request) -> BoxFuture<'a, bool>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Running,
    /// Waiting for a click.
    Waiting,
    Ok,
    Error,
    Denied,
}

/// One thing Mochi does, as the island and the phone show it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub index: usize,
    pub tool: String,
    /// The command, the path or the address, on one line.
    pub summary: String,
    pub status: Status,
    /// "exit 0", "120" (lines), "+3 −1"…
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// A command's last lines, or why it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

/// What a turn needs besides the tools.
pub struct Job {
    pub flavor: Flavor,
    pub model: String,
    pub system: String,
    pub max_tokens: u32,
    pub max_tokens_field: &'static str,
    /// The earlier turns, in the provider's format.
    pub history: Vec<Value>,
    /// The user's new message, in the provider's format.
    pub user: Value,
}

/// A finished turn: the answer to show, and every message of the turn (the user's first) for the history.
#[derive(Debug)]
pub struct Finished {
    pub text: String,
    pub messages: Vec<Value>,
    /// How many tool calls were made.
    pub actions: usize,
}

/// Why a turn ended without an answer.
#[derive(Debug, PartialEq, Eq)]
pub enum Failed {
    /// The provider failed or refused; the message is for the user.
    Provider(String),
    /// The user stopped it.
    Canceled,
}

pub async fn run(job: Job, env: &Env, transport: &dyn Transport, hands: &dyn Hands) -> Result<Finished, Failed> {
    let defs = tools::definitions(env);
    let mut turn: Vec<Value> = vec![job.user.clone()];
    let mut index = 0;
    let mut pauses = 0;
    for _ in 0..MAX_ROUNDS {
        if env.cancel.load(Ordering::SeqCst) {
            return Err(Failed::Canceled);
        }
        let mut messages = job.history.clone();
        messages.extend(turn.iter().cloned());
        let body = wire::request_body(&wire::Request {
            flavor: job.flavor,
            model: &job.model,
            system: &job.system,
            messages: &messages,
            tools: &defs,
            max_tokens: job.max_tokens,
            max_tokens_field: job.max_tokens_field,
            web_search: env.web_search,
        });
        let response = transport.post(&body).await.map_err(Failed::Provider)?;
        let reply = wire::parse_reply(job.flavor, &response).map_err(Failed::Provider)?;
        if let Stop::Refused(why) = &reply.stop {
            return Err(Failed::Provider(why.clone()));
        }
        turn.push(reply.message.clone());
        if reply.stop == Stop::Paused {
            pauses += 1;
            if pauses <= MAX_PAUSES {
                continue;
            }
        }
        if reply.calls.is_empty() {
            let mut answer = reply.text.clone();
            if answer.is_empty() {
                answer = if index > 0 { crate::i18n::t("Done.") } else { crate::i18n::t("No response text.") };
            }
            if reply.stop == Stop::Cut {
                answer.push('…');
            }
            return Ok(Finished { text: answer, messages: turn, actions: index });
        }

        let cut = reply.stop == Stop::Cut;
        let mut results: Vec<(String, Output)> = Vec::new();
        let mut denied = false;
        for call in &reply.calls {
            if cut {
                results.push((call.id.clone(), Output::error("Your answer was cut off by the output limit before this call was complete, so nothing was run. Do it in smaller pieces (for example, create the file with the first part, then add the rest with edit_file).")));
                continue;
            }
            if denied {
                results.push((call.id.clone(), Output::error("Not run: the user denied an earlier action of this step.")));
                continue;
            }
            if env.cancel.load(Ordering::SeqCst) {
                return Err(Failed::Canceled);
            }
            let (out, status) = one_call(call, env, hands, index).await;
            index += 1;
            if env.cancel.load(Ordering::SeqCst) {
                return Err(Failed::Canceled);
            }
            denied = status == Status::Denied;
            results.push((call.id.clone(), out));
        }
        turn.extend(wire::result_messages(job.flavor, &results));
    }
    // Too many rounds: say so, and close the turn with an answer so the history stays well formed.
    let text = crate::i18n::t("I stopped here: that was a lot of steps in one go. Tell me to go on if you want more.");
    turn.push(wire::assistant_text(job.flavor, &text));
    Ok(Finished { text, messages: turn, actions: index })
}

/// One call: checked, approved if it must be, run. Every change of state is a step for the island and the phone.
async fn one_call(call: &Call, env: &Env, hands: &dyn Hands, index: usize) -> (Output, Status) {
    let prepared = match tools::prepare(call, env) {
        Ok(p) => p,
        Err(e) => {
            hands.step(&Step {
                index,
                tool: call.name.clone(),
                summary: String::new(),
                status: Status::Error,
                detail: None,
                output: Some(text::one_line(&e, 300)),
            });
            return (Output::error(e), Status::Error);
        }
    };
    let mut step = Step {
        index,
        tool: prepared.tool.clone(),
        summary: text::one_line(&prepared.summary, 400),
        status: Status::Running,
        detail: None,
        output: None,
    };
    match &prepared.gate {
        Gate::Refuse(why) => {
            step.status = Status::Denied;
            hands.step(&step);
            return (Output::error(*why), Status::Denied);
        }
        Gate::Ask => {
            step.status = Status::Waiting;
            hands.step(&step);
            if !hands.approve(prepared.request()).await {
                step.status = Status::Denied;
                hands.step(&step);
                return (Output::error(DENIED), Status::Denied);
            }
            step.status = Status::Running;
        }
        Gate::Free => {}
    }
    hands.step(&step);
    let out = tools::execute(&prepared, env).await;
    // A command that exited with an error is a normal result for the model, but a red mark for the user.
    let failed_command = prepared.tool == "run_command" && out.detail.as_deref() != Some("exit 0");
    step.status = if out.is_error || failed_command { Status::Error } else { Status::Ok };
    step.detail = out.detail.clone();
    step.output = out.output.clone();
    hands.step(&step);
    let status = step.status;
    (out, status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::policy::Level;
    use super::super::shell::Shell;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    /// A model that answers from a script and remembers what it was sent.
    struct Script {
        replies: Mutex<Vec<Value>>,
        seen: Mutex<Vec<Value>>,
    }

    impl Script {
        fn new(replies: Vec<Value>) -> Script {
            Script { replies: Mutex::new(replies.into_iter().rev().collect()), seen: Mutex::new(Vec::new()) }
        }
    }

    impl Transport for Script {
        fn post<'a>(&'a self, body: &'a Value) -> BoxFuture<'a, Result<Value, String>> {
            self.seen.lock().unwrap().push(body.clone());
            let next = self.replies.lock().unwrap().pop();
            Box::pin(async move { next.ok_or_else(|| "the script ran out".to_string()) })
        }
    }

    /// A user who answers every approval the same way, and remembers the steps.
    struct User {
        allow: bool,
        steps: Mutex<Vec<Step>>,
        asked: Mutex<Vec<Request>>,
        cancel_on_ask: Option<Arc<AtomicBool>>,
    }

    impl User {
        fn new(allow: bool) -> User {
            User { allow, steps: Mutex::new(Vec::new()), asked: Mutex::new(Vec::new()), cancel_on_ask: None }
        }
    }

    impl Hands for User {
        fn step(&self, step: &Step) {
            self.steps.lock().unwrap().push(step.clone());
        }
        fn approve<'a>(&'a self, request: Request) -> BoxFuture<'a, bool> {
            self.asked.lock().unwrap().push(request);
            if let Some(c) = &self.cancel_on_ask {
                c.store(true, Ordering::SeqCst);
            }
            let allow = self.allow;
            Box::pin(async move { allow })
        }
    }

    struct Folder(PathBuf);
    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn folder(name: &str) -> Folder {
        let dir = std::env::temp_dir().join(format!("coucou-agent-run-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), "first line\n").unwrap();
        Folder(dir)
    }

    fn env(dir: &Folder, level: Level) -> Env {
        Env { folder: dir.0.clone(), home: dir.0.clone(), level, shell: Shell::detect(), cancel: Arc::new(AtomicBool::new(false)), web_search: false, house: None }
    }

    fn job(flavor: Flavor) -> Job {
        let user = match flavor {
            Flavor::Anthropic => json!({"role":"user","content":[{"type":"text","text":"do it"}]}),
            Flavor::OpenAi => json!({"role":"user","content":"do it"}),
        };
        Job { flavor, model: "m".into(), system: "sys".into(), max_tokens: 1000, max_tokens_field: "max_tokens", history: vec![], user }
    }

    fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    fn claude_call(id: &str, name: &str, input: Value) -> Value {
        json!({"stop_reason":"tool_use","content":[{"type":"text","text":"On it."},{"type":"tool_use","id":id,"name":name,"input":input}]})
    }

    fn claude_text(text: &str) -> Value {
        json!({"stop_reason":"end_turn","content":[{"type":"text","text":text}]})
    }

    #[test]
    fn a_free_read_then_an_answer() {
        let dir = folder("read");
        let env = env(&dir, Level::Ask);
        let model = Script::new(vec![claude_call("t1", "read_file", json!({"path":"notes.txt"})), claude_text("It says hello.")]);
        let user = User::new(false);
        let done = block_on(run(job(Flavor::Anthropic), &env, &model, &user)).unwrap();
        assert_eq!(done.text, "It says hello.");
        assert_eq!(done.actions, 1);
        // user, assistant(tool_use), user(tool_result), assistant(text)
        assert_eq!(done.messages.len(), 4);
        assert_eq!(done.messages[2]["content"][0]["type"], "tool_result");
        assert!(done.messages[2]["content"][0]["content"].as_str().unwrap().contains("first line"));
        assert!(user.asked.lock().unwrap().is_empty(), "reading the folder needs no click");
        let steps = user.steps.lock().unwrap();
        assert_eq!(steps.first().unwrap().status, Status::Running);
        assert_eq!(steps.last().unwrap().status, Status::Ok);
        // The second request carries the whole turn so far.
        let seen = model.seen.lock().unwrap();
        assert_eq!(seen[1]["messages"].as_array().unwrap().len(), 3);
        assert!(seen[0]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "run_command"));
    }

    #[test]
    fn a_denied_change_is_not_made_and_the_model_is_told_to_stop() {
        let dir = folder("deny");
        let env = env(&dir, Level::Ask);
        let model = Script::new(vec![
            json!({"choices":[{"finish_reason":"tool_calls","message":{"content":null,"tool_calls":[
                {"id":"a","type":"function","function":{"name":"write_file","arguments":"{\"path\":\"notes.txt\",\"content\":\"gone\"}"}},
                {"id":"b","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"notes.txt\"}"}}
            ]}}]}),
            json!({"choices":[{"finish_reason":"stop","message":{"content":"OK, I won't."}}]}),
        ]);
        let user = User::new(false);
        let done = block_on(run(job(Flavor::OpenAi), &env, &model, &user)).unwrap();
        assert_eq!(done.text, "OK, I won't.");
        assert_eq!(std::fs::read_to_string(dir.0.join("notes.txt")).unwrap(), "first line\n");
        let asked = user.asked.lock().unwrap();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].tool, "write_file");
        assert_eq!(asked[0].after.as_deref(), Some("gone"));
        // Both calls are answered: the denied one, and the next one, not run.
        let tool_msgs: Vec<&Value> = done.messages.iter().filter(|m| m["role"] == "tool").collect();
        assert_eq!(tool_msgs.len(), 2);
        assert!(tool_msgs[0]["content"].as_str().unwrap().contains("denied"));
        assert!(tool_msgs[1]["content"].as_str().unwrap().contains("Not run"));
        assert!(user.steps.lock().unwrap().iter().any(|s| s.status == Status::Denied));
    }

    #[test]
    fn an_approved_change_is_made() {
        let dir = folder("allow");
        let env = env(&dir, Level::Ask);
        let model = Script::new(vec![
            claude_call("t1", "edit_file", json!({"path":"notes.txt","old_text":"first","new_text":"only"})),
            claude_text("Changed."),
        ]);
        let user = User::new(true);
        let done = block_on(run(job(Flavor::Anthropic), &env, &model, &user)).unwrap();
        assert_eq!(done.text, "Changed.");
        assert_eq!(std::fs::read_to_string(dir.0.join("notes.txt")).unwrap(), "only line\n");
        let steps = user.steps.lock().unwrap();
        let statuses: Vec<Status> = steps.iter().map(|s| s.status).collect();
        assert_eq!(statuses, vec![Status::Waiting, Status::Running, Status::Ok]);
        assert_eq!(steps.last().unwrap().detail.as_deref(), Some("+1 −1"));
    }

    #[test]
    fn read_only_refuses_a_change_without_asking() {
        let dir = folder("readonly");
        let env = env(&dir, Level::ReadOnly);
        let model = Script::new(vec![claude_call("t1", "write_file", json!({"path":"x.txt","content":"x"})), claude_text("I can't.")]);
        let user = User::new(true);
        let done = block_on(run(job(Flavor::Anthropic), &env, &model, &user)).unwrap();
        assert_eq!(done.text, "I can't.");
        assert!(!dir.0.join("x.txt").exists());
        assert!(user.asked.lock().unwrap().is_empty());
        assert!(model.seen.lock().unwrap()[0]["tools"].as_array().unwrap().iter().all(|t| t["name"] != "write_file"));
    }

    #[test]
    fn a_stop_during_an_approval_ends_the_turn() {
        let dir = folder("cancel");
        let env = env(&dir, Level::Ask);
        let model = Script::new(vec![claude_call("t1", "run_command", json!({"command":"echo hi"})), claude_text("never")]);
        let mut user = User::new(true);
        user.cancel_on_ask = Some(env.cancel.clone());
        assert_eq!(block_on(run(job(Flavor::Anthropic), &env, &model, &user)).unwrap_err(), Failed::Canceled);
        assert_eq!(model.seen.lock().unwrap().len(), 1, "no second request after a stop");
    }

    #[test]
    fn a_pause_is_resumed_and_a_cut_call_is_not_run() {
        let dir = folder("pause");
        let env = env(&dir, Level::Edits);
        let model = Script::new(vec![
            json!({"stop_reason":"pause_turn","content":[{"type":"server_tool_use","id":"s1","name":"web_search","input":{"query":"x"}}]}),
            json!({"stop_reason":"max_tokens","content":[{"type":"tool_use","id":"t1","name":"write_file","input":{"path":"big.txt","content":"partial"}}]}),
            claude_text("Here it is."),
        ]);
        let user = User::new(true);
        let done = block_on(run(job(Flavor::Anthropic), &env, &model, &user)).unwrap();
        assert_eq!(done.text, "Here it is.");
        assert!(!dir.0.join("big.txt").exists(), "a call cut off by the limit is never run");
        let seen = model.seen.lock().unwrap();
        // After the pause the same conversation is sent again, ending on the paused assistant message.
        assert_eq!(seen[1]["messages"].as_array().unwrap().last().unwrap()["role"], "assistant");
        let results = &done.messages[3]["content"][0];
        assert_eq!(results["is_error"], true);
        assert!(results["content"].as_str().unwrap().contains("cut off"));
    }

    #[test]
    fn a_refusal_or_a_provider_error_ends_the_turn_with_its_reason() {
        let dir = folder("refuse");
        let env = env(&dir, Level::Ask);
        let model = Script::new(vec![json!({"stop_reason":"refusal","stop_details":{"explanation":"Not this."},"content":[]})]);
        let err = block_on(run(job(Flavor::Anthropic), &env, &model, &User::new(true))).unwrap_err();
        assert_eq!(err, Failed::Provider("Not this.".into()));
        let model = Script::new(vec![]);
        assert!(matches!(block_on(run(job(Flavor::Anthropic), &env, &model, &User::new(true))), Err(Failed::Provider(_))));
    }

    #[test]
    fn a_model_that_never_stops_is_stopped_and_the_history_ends_on_an_answer() {
        let dir = folder("rounds");
        let env = env(&dir, Level::Ask);
        let replies: Vec<Value> = (0..MAX_ROUNDS).map(|i| claude_call(&format!("t{i}"), "system_info", json!({}))).collect();
        let model = Script::new(replies);
        let done = block_on(run(job(Flavor::Anthropic), &env, &model, &User::new(true))).unwrap();
        assert_eq!(done.actions, MAX_ROUNDS);
        assert!(done.text.contains("stopped"));
        assert_eq!(done.messages.last().unwrap()["role"], "assistant");
    }

    #[test]
    fn turn_on_the_tv_and_open_youtube_then_a_denied_plug() {
        use super::super::tools::tests::{house, FakeHouse};
        let dir = folder("home");
        let mut env = env(&dir, Level::Ask);
        let runner = Arc::new(FakeHouse { calls: Default::default() });
        env.house = Some(house(runner.clone()));
        // "Mochi, liga a TV da sala e abre o YouTube": two calls in order, then the answer.
        let model = Script::new(vec![
            claude_call("t1", "home_control", json!({"device":"TV da sala","action":"turn_on"})),
            claude_call("t2", "home_control", json!({"device":"TV da sala","action":"open_app","value":"YouTube"})),
            claude_text("A TV da sala está ligada e o YouTube está aberto."),
        ]);
        let user = User::new(false);
        let done = block_on(run(job(Flavor::Anthropic), &env, &model, &user)).unwrap();
        assert_eq!(done.text, "A TV da sala está ligada e o YouTube está aberto.");
        assert_eq!(runner.calls.lock().unwrap().as_slice(), ["philips:aa turn_on", "philips:aa open_app"]);
        assert!(user.asked.lock().unwrap().is_empty(), "a TV needs no click");
        let seen = model.seen.lock().unwrap();
        assert!(seen[0]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "home_control"));
        // The device's own answer reached the model.
        let second = seen[1]["messages"].as_array().unwrap();
        assert!(second.last().unwrap()["content"][0]["content"].as_str().unwrap().contains("The device now reports"));
        drop(seen);

        // A plug asks first; denied, nothing is sent.
        let model = Script::new(vec![claude_call("t1", "home_control", json!({"device":"Aquecedor","action":"turn_on"})), claude_text("OK.")]);
        let user = User::new(false);
        block_on(run(job(Flavor::Anthropic), &env, &model, &user)).unwrap();
        assert_eq!(user.asked.lock().unwrap()[0].tool, "home_control");
        assert_eq!(user.asked.lock().unwrap()[0].summary, "Aquecedor · Turn on");
        assert_eq!(runner.calls.lock().unwrap().len(), 2, "the denied plug was never touched");
    }

    #[test]
    fn a_bad_call_is_answered_with_an_error_and_the_turn_goes_on() {
        let dir = folder("badcall");
        let env = env(&dir, Level::Ask);
        let model = Script::new(vec![claude_call("t1", "teleport", json!({})), claude_text("Sorry.")]);
        let user = User::new(true);
        let done = block_on(run(job(Flavor::Anthropic), &env, &model, &user)).unwrap();
        assert_eq!(done.text, "Sorry.");
        assert_eq!(done.messages[2]["content"][0]["is_error"], true);
        assert_eq!(user.steps.lock().unwrap()[0].status, Status::Error);
    }
}
