// Mochi's home for other agents (coucou-hook --mcp, docs/HERMES.md). Hermes Agent, or another MCP client on this
// computer, calls home_devices, home_control or home_routine; coucou-hook relays each call over the hooks' pipe
// (pipe.rs) and it lands here. Mochi's own rules apply: only once the user shared the house (Settings → Hermes Agent),
// only the devices ticked for Mochi and the actions each reported, a click on the island or a paired phone for what
// asks first, and every action in the home's record under the agent's name. Zheus' superpowers (wealth_*, life_*)
// come the same way, once the user turned them on (Settings → Zheus' superpowers). No other tool of Mochi's is
// reachable.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use super::policy::{Gate, Level};
use super::run::{Hands, DENIED};
use super::shell::Shell;
use super::tools::{self, Call, Env, RoutineInfo, POWER_TOOLS};
use super::{Agent, LiveHands, Origin, WAIT_PHONE};
use crate::i18n::{t, tf};
use crate::settings::{self, Settings};
use crate::{log, Shared};

/// The only tools other agents reach: the house's, and Zheus One's (which only read, or show a plan for the user's yes).
pub const HOUSE_TOOLS: &[&str] = &["home_devices", "home_control", "home_routine", "zheus_world", "zheus_mission"];

/// One approval at a time for the other agents: a second waits for the first card to be answered.
static ASKING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn reply(text: impl Into<String>, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }], "isError": is_error })
}

/// The name the home's record and the card give the agent: Hermes, or another agent.
pub fn origin_of(client: &str) -> &'static str {
    if client.to_ascii_lowercase().contains("hermes") {
        "hermes"
    } else {
        "agent"
    }
}

fn who(origin: &str) -> String {
    if origin == "hermes" {
        "AXYON Core".to_string()
    } else {
        t("another agent")
    }
}

/// The routines, after the devices: an agent learns them from home_devices.
fn routines_text(routines: &[RoutineInfo]) -> String {
    if routines.is_empty() {
        return "\nThe user has no routine.".to_string();
    }
    let mut out = String::from("\nThe user's routines (home_routine runs one):");
    for r in routines {
        out.push_str(&format!("\n- {} ({})", r.name, r.steps.join("; ")));
        if r.asks {
            out.push_str(" — waits for the user's approval");
        }
        if !r.problems.is_empty() {
            out.push_str(&format!(" — cannot run as it is: {}", r.problems.join("; ")));
        }
    }
    out
}

/// One call relayed by coucou-hook --mcp: `{"name", "arguments", "client"}`. The answer is an MCP tool result.
pub async fn call(app: &AppHandle, payload: &Value) -> Value {
    let settings = app.state::<Shared>().settings.lock().unwrap().clone();
    let name = payload["name"].as_str().unwrap_or("");
    let power = POWER_TOOLS.contains(&name);
    if !HOUSE_TOOLS.contains(&name) && !power {
        return reply(format!("There is no tool named \"{name}\"."), true);
    }
    if power && !settings.powers_enabled {
        return reply(tools::NO_POWERS, true);
    }
    if !power && !settings.home_mcp {
        return reply("The user has not shared AXYON's home with other agents. They can do it in AXYON: Settings → AXYON Core → AXYON's home for AXYON Core.", true);
    }
    let client = payload["client"].as_str().unwrap_or("");
    let origin = origin_of(client);
    let house = if power { None } else { tokio::time::timeout(Duration::from_secs(8), crate::smart_home::agent_env_for(app, origin)).await.ok().flatten() };
    let home = crate::platform::home_dir();
    let env = Env {
        folder: home.clone(),
        home,
        level: Level::Ask,
        shell: Shell::detect(),
        cancel: Arc::new(AtomicBool::new(false)),
        web_search: false,
        house,
        powers: if power { crate::powers::runner(app) } else { None },
    };
    let arguments = if payload["arguments"].is_object() { payload["arguments"].clone() } else { json!({}) };
    let call = Call { id: "mcp".into(), name: name.into(), input: arguments, broken: None };
    let prepared = match tools::prepare(&call, &env) {
        Ok(prepared) => prepared,
        Err(why) => return reply(why, true),
    };
    match prepared.gate {
        Gate::Refuse(why) => return reply(why, true),
        Gate::Ask => {
            let _turn = ASKING.lock().await;
            let agent = app.state::<Agent>();
            if agent.card().is_some() {
                return reply("Another action is waiting for the user's click in AXYON. Try again in a moment.", true);
            }
            let mut request = prepared.request();
            request.reason = Some(tf("Asked by {agent}", &[("agent", &who(origin))]));
            let hands = LiveHands {
                app: app.clone(),
                run: 0,
                origin: Origin::Island,
                project: None,
                folder: env.folder.clone(),
                home: env.home.clone(),
                phone_steps: None,
                lines: Mutex::new(Vec::new()),
                wait: WAIT_PHONE,
            };
            if !hands.approve(request).await {
                log::line(format!("mcp: {name} for {origin} not allowed"));
                return reply(DENIED, true);
            }
        }
        Gate::Free => {}
    }
    let output = tools::execute(&prepared, &env).await;
    let mut text = output.text;
    if name == "home_devices" {
        if let Some(house) = &env.house {
            text.push_str(&routines_text(&house.routines));
        }
    }
    log::line(format!("mcp: {name} for {origin} {}", if output.is_error { "failed" } else { "done" }));
    reply(text, output.is_error)
}

/// One click asked for another part of Coucou (a step of Zheus One's plan on a device set to ask first): the same
/// card as the agents', on the island and the paired phones. False when the user says no or does not answer, or when
/// another card is already waiting.
pub async fn ask_click(app: &AppHandle, tool: &str, summary: String, reason: String) -> bool {
    let _turn = ASKING.lock().await;
    let agent = app.state::<Agent>();
    if agent.card().is_some() {
        return false;
    }
    let request = tools::Request {
        tool: tool.into(),
        summary,
        cwd: None,
        before: None,
        after: None,
        new_file: false,
        outside: false,
        secret: false,
        reason: Some(reason),
    };
    let home = crate::platform::home_dir();
    let hands = LiveHands {
        app: app.clone(),
        run: 0,
        origin: Origin::Island,
        project: None,
        folder: home.clone(),
        home,
        phone_steps: None,
        lines: Mutex::new(Vec::new()),
        wait: WAIT_PHONE,
    };
    hands.approve(request).await
}

// ── Settings → Hermes Agent → Mochi's home for Hermes ─────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeMcpStatus {
    pub on: bool,
    /// The program an MCP client starts, with `--mcp`.
    pub command: String,
    /// Mochi's home is on, so there is something to share.
    pub home_on: bool,
}

fn status_of(settings: &Settings) -> HomeMcpStatus {
    HomeMcpStatus { on: settings.home_mcp, command: settings::hook_exe_path().to_string_lossy().to_string(), home_on: settings.home_enabled }
}

#[tauri::command]
pub fn home_mcp_status(shared: State<Shared>) -> HomeMcpStatus {
    status_of(&shared.settings.lock().unwrap())
}

/// Shares Mochi's home with the agents on this computer (or stops). Off by default.
#[tauri::command]
pub fn home_mcp_set(app: AppHandle, shared: State<Shared>, on: bool) -> HomeMcpStatus {
    let settings = {
        let mut current = shared.settings.lock().unwrap();
        current.home_mcp = on;
        current.clone()
    };
    if let Err(err) = settings::save(&settings) {
        log::line(format!("could not save settings: {err}"));
    }
    log::line(format!("mcp: Mochi's home {} with other agents", if on { "shared" } else { "no longer shared" }));
    let _ = app.emit("settings-changed", settings.clone());
    status_of(&settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermes_is_named_and_the_routines_come_with_the_devices() {
        assert_eq!(origin_of("hermes-agent"), "hermes");
        assert_eq!(origin_of("Hermes"), "hermes");
        assert_eq!(origin_of("claude-code"), "agent");
        let routines = vec![
            RoutineInfo { id: "r1".into(), name: "Boa noite".into(), asks: true, steps: vec!["TV da sala: turn off".into()], problems: vec![] },
            RoutineInfo { id: "r2".into(), name: "Cinema".into(), asks: false, steps: vec![], problems: vec!["the TV is gone".into()] },
        ];
        let text = routines_text(&routines);
        assert!(text.contains("- Boa noite (TV da sala: turn off) — waits for the user's approval"));
        assert!(text.contains("Cinema () — cannot run as it is: the TV is gone"));
        assert!(routines_text(&[]).contains("no routine"));
    }

    #[test]
    fn the_tools_offered_by_coucou_hook_are_the_house_tools_with_mochis_arguments() {
        let listed: Value = serde_json::from_str(include_str!("../../../hook/src/house_tools.json")).unwrap();
        let names: Vec<&str> = listed.as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, [HOUSE_TOOLS, POWER_TOOLS].concat(), "the house's, then the superpowers'");
        let control = &listed[1]["inputSchema"];
        let words: Vec<&str> = control["properties"]["action"]["enum"].as_array().unwrap().iter().map(|w| w.as_str().unwrap()).collect();
        assert_eq!(words, crate::smart_home::model::ACTION_WORDS, "the same actions as Mochi's own home_control");
        assert_eq!(control["required"], json!(["device", "action"]));
        assert!(listed[0]["inputSchema"]["properties"]["device"].is_object());
        assert_eq!(listed[2]["inputSchema"]["required"], json!(["routine"]));
        assert_eq!(listed[4]["inputSchema"]["required"], json!(["mission"]));
        assert_eq!(listed[4]["inputSchema"]["properties"]["mission"]["enum"], json!(["arrival", "situation"]));
    }
}
