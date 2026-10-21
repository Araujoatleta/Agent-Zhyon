//! `coucou-hook --mcp`: Mochi's home as an MCP server over stdio, for Hermes Agent or any other MCP client on this
//! computer (docs/HERMES.md). Each line on stdin is one JSON-RPC message. The tools are listed from
//! `house_tools.json`; a call is relayed to Coucou over the same pipe as the hooks, and Coucou runs it with the same
//! rules as Mochi's own (the devices the user ticked, a click for what asks first, the record of every action) and
//! answers with the result. Nothing runs in this process, and when Coucou is closed or keeps the house to itself,
//! the answer says so.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// The event name Coucou knows these relayed calls by.
pub const EVENT: &str = "CoucouMcp";
/// The protocol versions this server speaks, newest first.
const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
/// A call may wait for the user's click (two minutes at most, on Coucou's side).
const CALL_BUDGET: Duration = Duration::from_secs(150);
/// How long a Coucou started for a call gets to open its pipe.
const START_BUDGET: Duration = Duration::from_secs(25);
/// Where Coucou writes its own path, next to this relay (hooks.rs, note_app_path): what starts it when it is closed.
const APP_FILE: &str = "coucou-app.txt";
const TOOLS: &str = include_str!("house_tools.json");
const INSTRUCTIONS: &str = "Coucou is the app where Mochi lives on the user's screen. The home_ and zheus_ tools act on the \
user's home through Coucou: only the devices the user let agents use, and the actions each one reported. Some actions \
wait for the user's approval in Coucou. Call home_devices first when you do not know a device. The wealth_ and life_ \
tools are Zheus' superpowers: the user's portfolio read against Brazil's official data (B3, the CVM, FundosNet, the \
Banco Central), and their studies, English and training. They only read and keep the user's own records in Coucou: \
they never buy, sell, pay, send or post anything. Call wealth_portfolio before talking about the user's investments, \
and life_day for their day.";

pub fn tools() -> Value {
    serde_json::from_str(TOOLS).unwrap_or_else(|_| json!([]))
}

/// Reads JSON-RPC messages until stdin closes, and answers each request on stdout.
pub fn run() -> ! {
    let mut client = String::new();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => answer(&message, &mut client, &relay),
            Err(_) => Some(error(Value::Null, -32700, "Parse error")),
        };
        if let Some(reply) = reply {
            let _ = writeln!(out, "{reply}");
            let _ = out.flush();
        }
    }
    std::process::exit(0);
}

/// What became of a call handed to Coucou.
pub enum Relayed {
    Answer(Value),
    /// Nothing to talk to: Coucou is closed, and could not be started.
    Closed,
    /// Coucou took the call and did not answer within `CALL_BUDGET`.
    Silent,
}

/// Hands one call to Coucou and waits for its answer, within `CALL_BUDGET`. A Coucou that is closed is started first
/// (the user asked their assistant something only Coucou can answer), and given `START_BUDGET` to open.
fn relay(payload: &Value) -> Relayed {
    if !super::reachable() && !(start_coucou() && opened_within(START_BUDGET)) {
        return Relayed::Closed;
    }
    let (tx, rx) = mpsc::channel::<Option<String>>();
    let mut line = payload.to_string();
    line.push('\n');
    std::thread::spawn(move || {
        let _ = tx.send(super::talk(&line, true));
    });
    match rx.recv_timeout(CALL_BUDGET).ok().flatten().and_then(|answer| serde_json::from_str(&answer).ok()) {
        Some(answer) => Relayed::Answer(answer),
        None => Relayed::Silent,
    }
}

/// The Coucou installed on this computer, as it wrote itself down next to this relay: only an absolute path to an
/// existing program named like it.
fn app_path() -> Option<PathBuf> {
    app_path_in(std::env::current_exe().ok()?.parent()?)
}

fn app_path_in(dir: &std::path::Path) -> Option<PathBuf> {
    let path = PathBuf::from(std::fs::read_to_string(dir.join(APP_FILE)).ok()?.trim());
    let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    (path.is_absolute() && path.is_file() && name.starts_with("coucou")).then_some(path)
}

/// Starts Coucou, at most once a minute: a Coucou that cannot start is not started again at every call.
fn start_coucou() -> bool {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
            return false;
        }
        *last = Some(Instant::now());
    }
    app_path().is_some_and(|app| super::launch(&app))
}

fn opened_within(budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    while Instant::now() < deadline {
        if super::reachable() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// A tool's answer that is an error the model reads (not a protocol error).
fn tool_error(text: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

/// The answer to one message; None for a notification. `relay` reaches Coucou (a closure, so tests need no Coucou).
pub fn answer(message: &Value, client: &mut String, relay: &dyn Fn(&Value) -> Relayed) -> Option<Value> {
    // A notification ("notifications/initialized", "notifications/cancelled"…) is never answered.
    let id = message.get("id").filter(|id| id.is_string() || id.is_number())?.clone();
    let method = message["method"].as_str().unwrap_or("");
    match method {
        "initialize" => {
            if let Some(name) = message.pointer("/params/clientInfo/name").and_then(Value::as_str) {
                *client = name.chars().filter(|c| !c.is_control()).take(60).collect();
            }
            let asked = message.pointer("/params/protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = VERSIONS.iter().find(|v| **v == asked).unwrap_or(&VERSIONS[0]);
            Some(result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "coucou", "title": "Coucou — Mochi's home", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            ))
        }
        "ping" => Some(result(id, json!({}))),
        "tools/list" => Some(result(id, json!({ "tools": tools() }))),
        "tools/call" => {
            let Some(name) = message.pointer("/params/name").and_then(Value::as_str) else {
                return Some(error(id, -32602, "Missing tool name"));
            };
            if !tools().as_array().is_some_and(|t| t.iter().any(|tool| tool["name"] == name)) {
                return Some(error(id, -32602, &format!("Unknown tool: {name}")));
            }
            let arguments = message.pointer("/params/arguments").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
            let payload = json!({ "hook_event_name": EVENT, "client": client, "name": name, "arguments": arguments });
            let answer = match relay(&payload) {
                Relayed::Answer(answer) if answer["content"].is_array() => answer,
                Relayed::Answer(_) => tool_error("Coucou answered something that is not a tool result. Try again; if it repeats, Coucou needs updating."),
                Relayed::Closed => tool_error("Coucou is not open on this computer, and it could not be started. Ask the user to open Coucou, then try again."),
                Relayed::Silent => tool_error("Coucou took the call but did not answer in time (it may still be reading the official sources). Try again in a minute."),
            };
            Some(result(id, answer))
        }
        _ => Some(error(id, -32601, &format!("Method not found: {method}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn it_introduces_itself_and_lists_the_house_tools() {
        let mut client = String::new();
        let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26", "clientInfo": {"name": "hermes-agent", "version": "1"}, "capabilities": {}}});
        let reply = answer(&init, &mut client, &|_| Relayed::Closed).unwrap();
        assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(reply["result"]["serverInfo"]["name"], "coucou");
        assert_eq!(client, "hermes-agent");
        let unknown = json!({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {"protocolVersion": "1999-01-01"}});
        assert_eq!(answer(&unknown, &mut client, &|_| Relayed::Closed).unwrap()["result"]["protocolVersion"], VERSIONS[0]);
        // A notification gets no answer.
        assert!(answer(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}), &mut client, &|_| Relayed::Closed).is_none());
        let list = answer(&json!({"jsonrpc": "2.0", "id": "a", "method": "tools/list"}), &mut client, &|_| Relayed::Closed).unwrap();
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(&names[..5], ["home_devices", "home_control", "home_routine", "zheus_world", "zheus_mission"]);
        assert_eq!(names.len(), 21, "and the sixteen superpowers");
        assert!(names[5..].iter().all(|n| n.starts_with("wealth_") || n.starts_with("life_")));
        assert!(list["result"]["tools"].as_array().unwrap().iter().all(|t| t["inputSchema"]["type"] == "object"));
        let other = answer(&json!({"jsonrpc": "2.0", "id": 3, "method": "resources/list"}), &mut client, &|_| Relayed::Closed).unwrap();
        assert_eq!(other["error"]["code"], -32601);
    }

    #[test]
    fn a_call_goes_to_coucou_and_its_answer_comes_back() {
        let mut client = "hermes-agent".to_string();
        let sent = RefCell::new(Vec::new());
        let relay = |payload: &Value| {
            sent.borrow_mut().push(payload.clone());
            Relayed::Answer(json!({"content": [{"type": "text", "text": "Done: turn on on TV da sala. The device now reports: on."}], "isError": false}))
        };
        let call = json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": "home_control", "arguments": {"device": "TV da sala", "action": "turn_on"}}});
        let reply = answer(&call, &mut client, &relay).unwrap();
        assert_eq!(reply["id"], 7);
        assert_eq!(reply["result"]["isError"], false);
        assert_eq!(
            sent.borrow()[0],
            json!({"hook_event_name": EVENT, "client": "hermes-agent", "name": "home_control", "arguments": {"device": "TV da sala", "action": "turn_on"}})
        );
        // Coucou closed: the model is told, the protocol goes on.
        let closed = answer(&call, &mut client, &|_| Relayed::Closed).unwrap();
        assert_eq!(closed["result"]["isError"], true);
        assert!(closed["result"]["content"][0]["text"].as_str().unwrap().contains("not open"));
        // Open but slow (a reading of the official sources): the model is told to try again, not to open Coucou.
        let slow = answer(&call, &mut client, &|_| Relayed::Silent).unwrap();
        assert!(slow["result"]["content"][0]["text"].as_str().unwrap().contains("did not answer in time"));
        // A tool that is not the house's never leaves this process.
        let bad = json!({"jsonrpc": "2.0", "id": 8, "method": "tools/call", "params": {"name": "run_command", "arguments": {"command": "dir"}}});
        assert_eq!(answer(&bad, &mut client, &|_| panic!("relayed")).unwrap()["error"]["code"], -32602);
    }

    #[test]
    fn only_coucou_itself_is_started() {
        let dir = std::env::temp_dir().join(format!("coucou-mcp-app-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = dir.join(if cfg!(windows) { "Coucou.exe" } else { "coucou" });
        std::fs::write(&app, b"").unwrap();
        assert_eq!(app_path_in(&dir), None, "nothing written down yet");
        std::fs::write(dir.join(APP_FILE), format!("{}\n", app.display())).unwrap();
        assert_eq!(app_path_in(&dir), Some(app.clone()));
        let other = dir.join("tool.exe");
        std::fs::write(&other, b"").unwrap();
        std::fs::write(dir.join(APP_FILE), other.display().to_string()).unwrap();
        assert_eq!(app_path_in(&dir), None, "another program is never started");
        std::fs::write(dir.join(APP_FILE), "coucou").unwrap();
        assert_eq!(app_path_in(&dir), None, "a relative path is not trusted");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
