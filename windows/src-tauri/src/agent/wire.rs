// How the tools are offered to a provider and how its answer is read, in the two formats the chat's providers
// speak: Anthropic's Messages API (Claude) and the OpenAI chat completions API (OpenAI, Google AI's compatible
// endpoint, OpenRouter, Ollama, LM Studio and any compatible server).
//
// The assistant's message goes back into the conversation as the provider wrote it: Claude's content blocks whole
// (its thinking and web search blocks must come back unchanged), an OpenAI-style message with the fields that carry
// meaning between turns (tool calls, and the reasoning some models need to see again).

use serde_json::{json, Map, Value};

use super::tools::{Call, Output, ToolDef};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Anthropic,
    OpenAi,
}

/// Why the model stopped writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// It has answered.
    Done,
    /// It asked for tools.
    Tools,
    /// Claude's own web search paused the turn: send the conversation again and it carries on.
    Paused,
    /// The output limit cut it off.
    Cut,
    /// It declined (Claude's refusal, a content filter).
    Refused(String),
}

/// One answer of the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub text: String,
    pub calls: Vec<Call>,
    pub stop: Stop,
    /// The assistant's message, as it goes back into the conversation.
    pub message: Value,
}

pub struct Request<'a> {
    pub flavor: Flavor,
    pub model: &'a str,
    pub system: &'a str,
    pub messages: &'a [Value],
    pub tools: &'a [ToolDef],
    pub max_tokens: u32,
    /// OpenAI's reasoning models want `max_completion_tokens`; everyone else `max_tokens`.
    pub max_tokens_field: &'a str,
    /// Claude's server-side web search.
    pub web_search: bool,
}

pub fn request_body(r: &Request) -> Value {
    match r.flavor {
        Flavor::Anthropic => {
            let mut tools: Vec<Value> = Vec::new();
            if r.web_search {
                tools.push(json!({ "type": "web_search_20260209", "name": "web_search", "max_uses": 5 }));
            }
            tools.extend(r.tools.iter().map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.schema })));
            json!({
                "model": r.model,
                "max_tokens": r.max_tokens,
                "system": r.system,
                "tools": tools,
                "fallbacks": "default",
                "messages": r.messages,
            })
        }
        Flavor::OpenAi => {
            let mut messages = vec![json!({ "role": "system", "content": r.system })];
            messages.extend(r.messages.iter().cloned());
            let tools: Vec<Value> = r
                .tools
                .iter()
                .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.schema } }))
                .collect();
            let mut body = json!({ "model": r.model, "messages": messages, "tools": tools });
            body[r.max_tokens_field] = json!(r.max_tokens);
            body
        }
    }
}

pub fn parse_reply(flavor: Flavor, response: &Value) -> Result<Reply, String> {
    match flavor {
        Flavor::Anthropic => parse_anthropic(response),
        Flavor::OpenAi => parse_openai(response),
    }
}

fn parse_anthropic(response: &Value) -> Result<Reply, String> {
    let stop_reason = response.get("stop_reason").and_then(Value::as_str).unwrap_or("");
    if stop_reason == "refusal" {
        let why = response
            .pointer("/stop_details/explanation")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| crate::i18n::t("Claude declined this one."));
        return Ok(Reply { text: String::new(), calls: Vec::new(), stop: Stop::Refused(why), message: Value::Null });
    }
    let content = response.get("content").and_then(Value::as_array).cloned().ok_or_else(|| crate::i18n::t("Unexpected API response."))?;
    let text = crate::claude::response_text(&content).unwrap_or_default();
    let calls: Vec<Call> = content
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
        .map(|b| {
            let input = b.get("input").cloned().unwrap_or(Value::Null);
            let broken = (!input.is_object()).then(|| "the arguments are not an object".to_string());
            Call {
                id: b.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
                name: b.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                input,
                broken,
            }
        })
        .collect();
    let stop = match stop_reason {
        _ if !calls.is_empty() && stop_reason != "max_tokens" => Stop::Tools,
        "pause_turn" => Stop::Paused,
        "max_tokens" => Stop::Cut,
        _ => Stop::Done,
    };
    Ok(Reply { text, calls, stop, message: json!({ "role": "assistant", "content": content }) })
}

/// Text from an OpenAI-style `content`: a string, or a list of parts of which the text ones count.
fn content_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.trim().to_string(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string(),
        _ => String::new(),
    }
}

fn parse_openai(response: &Value) -> Result<Reply, String> {
    let Some(message) = response.pointer("/choices/0/message") else {
        if let Some(msg) = response.pointer("/error/message").and_then(Value::as_str) {
            return Err(msg.to_string());
        }
        return Err(crate::i18n::t("Unexpected API response."));
    };
    let finish = response.pointer("/choices/0/finish_reason").and_then(Value::as_str).unwrap_or("");
    let raw_content = message.get("content").cloned().unwrap_or(Value::Null);
    let text = local_thinking_removed(&content_text(&raw_content));
    let mut calls = Vec::new();
    let mut echoed = Vec::new();
    for (i, tc) in message.get("tool_calls").and_then(Value::as_array).into_iter().flatten().enumerate() {
        let id = tc.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string).unwrap_or_else(|| format!("call_{i}"));
        let name = tc.pointer("/function/name").and_then(Value::as_str).unwrap_or_default().to_string();
        let (input, broken) = match tc.pointer("/function/arguments") {
            Some(Value::String(s)) if s.trim().is_empty() => (json!({}), None),
            Some(Value::String(s)) => match serde_json::from_str::<Value>(s) {
                Ok(v) if v.is_object() => (v, None),
                Ok(_) => (Value::Null, Some("the arguments are not an object".to_string())),
                Err(e) => (Value::Null, Some(e.to_string())),
            },
            Some(v @ Value::Object(_)) => (v.clone(), None),
            None | Some(Value::Null) => (json!({}), None),
            Some(_) => (Value::Null, Some("the arguments are not an object".to_string())),
        };
        // The call goes back as the provider sent it (Google keeps a thought signature in it), with an id for sure.
        let mut kept = tc.clone();
        kept["id"] = json!(id);
        if kept.get("type").is_none() {
            kept["type"] = json!("function");
        }
        echoed.push(kept);
        calls.push(Call { id, name, input, broken });
    }
    if calls.is_empty() && finish == "content_filter" {
        return Ok(Reply { text: String::new(), calls, stop: Stop::Refused(crate::i18n::t("The provider's content filter stopped the answer.")), message: Value::Null });
    }
    let stop = if !calls.is_empty() && finish != "length" {
        Stop::Tools
    } else if finish == "length" {
        Stop::Cut
    } else {
        Stop::Done
    };
    let mut back = Map::new();
    back.insert("role".into(), json!("assistant"));
    back.insert("content".into(), if raw_content.is_null() && !echoed.is_empty() { Value::Null } else { json!(text) });
    if !echoed.is_empty() {
        back.insert("tool_calls".into(), Value::Array(echoed));
    }
    // What reasoning models need to see again in a tool loop (DeepSeek and Kimi: reasoning_content; OpenRouter:
    // reasoning_details). Only when the provider sent it.
    for key in ["reasoning_content", "reasoning_details"] {
        if let Some(v) = message.get(key).filter(|v| !v.is_null()) {
            back.insert(key.into(), v.clone());
        }
    }
    Ok(Reply { text, calls, stop, message: Value::Object(back) })
}

/// Local reasoning models write `<think>…</think>` in their answer: not for the user.
fn local_thinking_removed(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<think>") {
        let Some(len) = rest[start..].find("</think>") else {
            rest = &rest[..start];
            break;
        };
        out.push_str(&rest[..start]);
        rest = &rest[start + len + "</think>".len()..];
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// The answers to a round of tool calls, in the provider's format: one user message of tool_result blocks for Claude
/// (all of them together), one `tool` message per call for the others.
pub fn result_messages(flavor: Flavor, results: &[(String, Output)]) -> Vec<Value> {
    match flavor {
        Flavor::Anthropic => {
            let blocks: Vec<Value> = results
                .iter()
                .map(|(id, out)| {
                    let mut block = json!({ "type": "tool_result", "tool_use_id": id, "content": nonempty(&out.text) });
                    if out.is_error {
                        block["is_error"] = json!(true);
                    }
                    block
                })
                .collect();
            vec![json!({ "role": "user", "content": blocks })]
        }
        Flavor::OpenAi => results
            .iter()
            .map(|(id, out)| {
                let content = if out.is_error { format!("Error: {}", out.text) } else { nonempty(&out.text).to_string() };
                json!({ "role": "tool", "tool_call_id": id, "content": content })
            })
            .collect(),
    }
}

/// An assistant message of plain text, to close a turn that had to stop (the conversation must not end on results).
pub fn assistant_text(flavor: Flavor, text: &str) -> Value {
    match flavor {
        Flavor::Anthropic => json!({ "role": "assistant", "content": [{ "type": "text", "text": text }] }),
        Flavor::OpenAi => json!({ "role": "assistant", "content": text }),
    }
}

/// Providers refuse an empty tool result.
fn nonempty(text: &str) -> &str {
    if text.trim().is_empty() {
        "(nothing)"
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> ToolDef {
        ToolDef { name: "read_file", description: "Read".into(), schema: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}) }
    }

    #[test]
    fn claude_gets_its_web_search_and_the_tools_with_input_schemas() {
        let messages = vec![json!({"role":"user","content":"hi"})];
        let body = request_body(&Request {
            flavor: Flavor::Anthropic, model: "claude-x", system: "sys", messages: &messages, tools: &[tool()],
            max_tokens: 16000, max_tokens_field: "max_tokens", web_search: true,
        });
        assert_eq!(body["tools"][0]["name"], "web_search");
        assert_eq!(body["tools"][1]["name"], "read_file");
        assert_eq!(body["tools"][1]["input_schema"]["required"][0], "path");
        assert_eq!(body["system"], "sys");
        assert_eq!(body["max_tokens"], 16000);
        assert_eq!(body["fallbacks"], "default");
        assert!(body.get("tool_choice").is_none(), "forced tool choice is refused by current models");
    }

    #[test]
    fn openai_style_gets_functions_and_the_system_message_first() {
        let messages = vec![json!({"role":"user","content":"hi"})];
        let body = request_body(&Request {
            flavor: Flavor::OpenAi, model: "kimi", system: "sys", messages: &messages, tools: &[tool()],
            max_tokens: 8192, max_tokens_field: "max_completion_tokens", web_search: false,
        });
        assert_eq!(body["messages"][0], json!({"role":"system","content":"sys"}));
        assert_eq!(body["messages"][1]["content"], "hi");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["parameters"]["type"], "object");
        assert_eq!(body["max_completion_tokens"], 8192);
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn claude_tool_calls_and_stops_are_read() {
        let r = parse_reply(Flavor::Anthropic, &json!({
            "stop_reason": "tool_use",
            "content": [
                {"type":"thinking","thinking":"","signature":"sig"},
                {"type":"text","text":"Let me look."},
                {"type":"tool_use","id":"toolu_1","name":"read_file","input":{"path":"a.rs"}},
            ]
        })).unwrap();
        assert_eq!(r.stop, Stop::Tools);
        assert_eq!(r.text, "Let me look.");
        assert_eq!(r.calls, vec![Call { id: "toolu_1".into(), name: "read_file".into(), input: json!({"path":"a.rs"}), broken: None }]);
        // The thinking block goes back unchanged.
        assert_eq!(r.message["content"][0]["signature"], "sig");

        let paused = parse_reply(Flavor::Anthropic, &json!({"stop_reason":"pause_turn","content":[{"type":"server_tool_use","id":"s","name":"web_search","input":{}}]})).unwrap();
        assert_eq!(paused.stop, Stop::Paused);
        let cut = parse_reply(Flavor::Anthropic, &json!({"stop_reason":"max_tokens","content":[{"type":"tool_use","id":"t","name":"write_file","input":{"path":"x"}}]})).unwrap();
        assert_eq!(cut.stop, Stop::Cut, "a call cut off by the limit is not run");
        let refused = parse_reply(Flavor::Anthropic, &json!({"stop_reason":"refusal","stop_details":{"explanation":"No."},"content":[]})).unwrap();
        assert_eq!(refused.stop, Stop::Refused("No.".into()));
        let done = parse_reply(Flavor::Anthropic, &json!({"stop_reason":"end_turn","content":[{"type":"text","text":"All done."}]})).unwrap();
        assert_eq!((done.stop, done.text.as_str()), (Stop::Done, "All done."));
        assert!(parse_reply(Flavor::Anthropic, &json!({"id":"x"})).is_err());
    }

    #[test]
    fn openai_tool_calls_are_read_whatever_shape_the_arguments_take() {
        let r = parse_reply(Flavor::OpenAi, &json!({"choices":[{"finish_reason":"tool_calls","message":{
            "role":"assistant","content":null,"reasoning_content":"thinking…",
            "tool_calls":[
                {"id":"call_a","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"a.rs\"}"}},
                {"id":"","type":"function","function":{"name":"system_info","arguments":""}},
                {"id":"call_c","type":"function","function":{"name":"read_file","arguments":"{not json"}},
                {"id":"call_d","function":{"name":"list_directory","arguments":{"path":"."}},"extra_content":{"google":{"thought_signature":"abc"}}}
            ]}}]})).unwrap();
        assert_eq!(r.stop, Stop::Tools);
        assert_eq!(r.calls[0].input, json!({"path":"a.rs"}));
        assert_eq!(r.calls[1].id, "call_1", "a missing id is made up and sent back with the call");
        assert_eq!(r.calls[1].input, json!({}));
        assert!(r.calls[2].broken.is_some());
        assert_eq!(r.calls[3].input, json!({"path":"."}));
        let back = &r.message;
        assert_eq!(back["content"], Value::Null);
        assert_eq!(back["tool_calls"][1]["id"], "call_1");
        assert_eq!(back["tool_calls"][3]["extra_content"]["google"]["thought_signature"], "abc");
        assert_eq!(back["tool_calls"][3]["type"], "function");
        assert_eq!(back["reasoning_content"], "thinking…");
        assert!(back.get("refusal").is_none());

        // Some providers say "stop" even with tool calls: the calls decide.
        let r = parse_reply(Flavor::OpenAi, &json!({"choices":[{"finish_reason":"stop","message":{"content":"ok","tool_calls":[{"id":"x","function":{"name":"system_info","arguments":"{}"}}]}}]})).unwrap();
        assert_eq!(r.stop, Stop::Tools);
        let r = parse_reply(Flavor::OpenAi, &json!({"choices":[{"finish_reason":"stop","message":{"content":"<think>hmm</think>Hello!"}}]})).unwrap();
        assert_eq!((r.stop, r.text.as_str()), (Stop::Done, "Hello!"));
        let r = parse_reply(Flavor::OpenAi, &json!({"choices":[{"finish_reason":"length","message":{"content":"Partial"}}]})).unwrap();
        assert_eq!(r.stop, Stop::Cut);
        let r = parse_reply(Flavor::OpenAi, &json!({"choices":[{"finish_reason":"content_filter","message":{"content":null}}]})).unwrap();
        assert!(matches!(r.stop, Stop::Refused(_)));
        assert_eq!(parse_reply(Flavor::OpenAi, &json!({"error":{"message":"upstream failed"}})).unwrap_err(), "upstream failed");
        let r = parse_reply(Flavor::OpenAi, &json!({"choices":[{"message":{"content":[{"type":"text","text":"Hi "},{"type":"text","text":"there"}]}}]})).unwrap();
        assert_eq!(r.text, "Hi there");
    }

    #[test]
    fn results_go_back_in_each_providers_format() {
        let results = vec![
            ("toolu_1".to_string(), Output { text: "file text".into(), is_error: false, detail: None, output: None }),
            ("toolu_2".to_string(), Output { text: "denied".into(), is_error: true, detail: None, output: None }),
            ("toolu_3".to_string(), Output { text: "".into(), is_error: false, detail: None, output: None }),
        ];
        let claude = result_messages(Flavor::Anthropic, &results);
        assert_eq!(claude.len(), 1, "every result in one user message");
        assert_eq!(claude[0]["role"], "user");
        assert_eq!(claude[0]["content"][0], json!({"type":"tool_result","tool_use_id":"toolu_1","content":"file text"}));
        assert_eq!(claude[0]["content"][1]["is_error"], true);
        assert_eq!(claude[0]["content"][2]["content"], "(nothing)");
        let openai = result_messages(Flavor::OpenAi, &results);
        assert_eq!(openai.len(), 3);
        assert_eq!(openai[0], json!({"role":"tool","tool_call_id":"toolu_1","content":"file text"}));
        assert_eq!(openai[1]["content"], "Error: denied");
        assert_eq!(assistant_text(Flavor::Anthropic, "x")["content"][0]["text"], "x");
        assert_eq!(assistant_text(Flavor::OpenAi, "x")["content"], "x");
    }
}
