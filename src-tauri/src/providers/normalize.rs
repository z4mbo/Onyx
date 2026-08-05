use crate::model::{Message, MessageKind, MessageRole, ProviderId};
use serde_json::Value;

pub enum NormalizedEvent {
    Delta(String),
    Text(String),
    Session(String),
    Activity(Message),
}

pub struct StreamNormalizer {
    provider: ProviderId,
    saw_delta: bool,
}

impl StreamNormalizer {
    pub fn new(provider: ProviderId) -> Self {
        Self {
            provider,
            saw_delta: false,
        }
    }

    pub fn parse(&mut self, line: &str) -> Vec<NormalizedEvent> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            let text = line.trim();
            return (!text.is_empty())
                .then(|| NormalizedEvent::Text(format!("{text}\n")))
                .into_iter()
                .collect();
        };

        match self.provider {
            ProviderId::Claude => self.parse_claude(&value),
            ProviderId::Codex => self.parse_codex(&value),
            ProviderId::Gemini => self.parse_acp_fallback(&value),
            ProviderId::Kimi => self.parse_kimi(&value),
            // OpenCode has no one-shot stream transport; the native HTTP
            // driver is the only path.
            ProviderId::Opencode | ProviderId::Openrouter => Vec::new(),
        }
    }

    fn parse_claude(&mut self, value: &Value) -> Vec<NormalizedEvent> {
        let mut events = Vec::new();
        if let Some(session_id) = string_at(value, &["session_id"]) {
            events.push(NormalizedEvent::Session(session_id.to_string()));
        }

        match string_at(value, &["type"]) {
            Some("stream_event")
                if string_at(value, &["event", "type"]) == Some("content_block_delta") =>
            {
                if let Some(text) = string_at(value, &["event", "delta", "text"]) {
                    self.saw_delta = true;
                    events.push(NormalizedEvent::Delta(text.to_string()));
                }
            }
            Some("assistant") => {
                if let Some(blocks) = value.pointer("/message/content").and_then(Value::as_array) {
                    for block in blocks {
                        match string_at(block, &["type"]) {
                            Some("text") if !self.saw_delta => {
                                if let Some(text) = string_at(block, &["text"]) {
                                    events.push(NormalizedEvent::Text(text.to_string()));
                                }
                            }
                            Some("tool_use") => {
                                let name = string_at(block, &["name"]).unwrap_or("tool");
                                events.push(activity(
                                    tool_title(name, block.get("input")),
                                    block.get("input"),
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some("result") if !self.saw_delta => {
                if let Some(text) = string_at(value, &["result"]) {
                    events.push(NormalizedEvent::Text(text.to_string()));
                }
            }
            _ => {}
        }
        events
    }

    fn parse_codex(&mut self, value: &Value) -> Vec<NormalizedEvent> {
        let mut events = Vec::new();
        match string_at(value, &["type"]) {
            Some("thread.started") => {
                if let Some(id) = string_at(value, &["thread_id"]) {
                    events.push(NormalizedEvent::Session(id.to_string()));
                }
            }
            Some("item.completed") => {
                let item = value.get("item").unwrap_or(&Value::Null);
                match string_at(item, &["type"]) {
                    Some("agent_message") => {
                        if let Some(text) = string_at(item, &["text"]) {
                            events.push(NormalizedEvent::Text(text.to_string()));
                        }
                    }
                    Some("command_execution") => {
                        let command = string_at(item, &["command"]).unwrap_or("command");
                        events.push(activity(
                            format!("Ran {command}"),
                            item.get("aggregated_output"),
                        ));
                    }
                    Some("file_change") => {
                        events.push(activity(
                            "Applied file changes".to_string(),
                            item.get("changes"),
                        ));
                    }
                    _ => {}
                }
            }
            Some("item.started") => {
                let item = value.get("item").unwrap_or(&Value::Null);
                if string_at(item, &["type"]) == Some("command_execution") {
                    let command = string_at(item, &["command"]).unwrap_or("command");
                    events.push(activity(format!("Running {command}"), None));
                }
            }
            Some("error") => {
                let message = string_at(value, &["message"]).unwrap_or("Codex reported an error");
                events.push(NormalizedEvent::Activity(Message::new(
                    MessageRole::System,
                    MessageKind::Error,
                    message,
                )));
            }
            _ => {}
        }
        events
    }

    fn parse_acp_fallback(&mut self, value: &Value) -> Vec<NormalizedEvent> {
        let mut events = Vec::new();
        let event_type = string_at(value, &["type"]).or_else(|| string_at(value, &["event"]));

        if matches!(
            event_type,
            Some("init") | Some("session_start") | Some("session.started")
        ) && let Some(id) = string_at(value, &["session_id"])
            .or_else(|| string_at(value, &["sessionId"]))
            .or_else(|| string_at(value, &["id"]))
        {
            events.push(NormalizedEvent::Session(id.to_string()));
        }

        match event_type {
            Some("message") | Some("assistant") | Some("content")
                if string_at(value, &["role"]).is_none_or(|role| role == "assistant") =>
            {
                if let Some(text) = string_at(value, &["content"])
                    .or_else(|| string_at(value, &["text"]))
                    .or_else(|| string_at(value, &["delta"]))
                {
                    self.saw_delta = true;
                    events.push(NormalizedEvent::Delta(text.to_string()));
                }
            }
            Some("tool_use") | Some("tool_call") => {
                let name = string_at(value, &["name"])
                    .or_else(|| string_at(value, &["tool_name"]))
                    .unwrap_or("tool");
                events.push(activity(
                    tool_title(name, value.get("input")),
                    value.get("input"),
                ));
            }
            Some("tool_result") => {
                events.push(activity(
                    "Tool completed".to_string(),
                    value.get("output").or_else(|| value.get("content")),
                ));
            }
            Some("error") => {
                let message =
                    string_at(value, &["message"]).unwrap_or("Provider reported an error");
                events.push(NormalizedEvent::Activity(Message::new(
                    MessageRole::System,
                    MessageKind::Error,
                    message,
                )));
            }
            Some("result") if !self.saw_delta => {
                if let Some(text) = string_at(value, &["result"])
                    .or_else(|| string_at(value, &["content"]))
                    .or_else(|| string_at(value, &["text"]))
                {
                    events.push(NormalizedEvent::Text(text.to_string()));
                }
            }
            _ => {}
        }
        events
    }

    fn parse_kimi(&mut self, value: &Value) -> Vec<NormalizedEvent> {
        let mut events = Vec::new();
        if string_at(value, &["type"]) == Some("session.resume_hint")
            && let Some(id) =
                string_at(value, &["session_id"]).or_else(|| string_at(value, &["sessionId"]))
        {
            events.push(NormalizedEvent::Session(id.to_string()));
        }
        match string_at(value, &["role"]) {
            Some("assistant") => {
                if let Some(content) = text_content(value.get("content")) {
                    events.push(NormalizedEvent::Text(content));
                }
                if let Some(calls) = value.get("tool_calls").and_then(Value::as_array) {
                    for call in calls {
                        let name = string_at(call, &["function", "name"])
                            .or_else(|| string_at(call, &["name"]))
                            .unwrap_or("tool");
                        let detail = call
                            .pointer("/function/arguments")
                            .or_else(|| call.get("arguments"));
                        events.push(activity(format!("Running {name}"), detail));
                    }
                }
            }
            Some("tool") => {
                let detail = text_content(value.get("content")).unwrap_or_default();
                events.push(NormalizedEvent::Activity(Message::new(
                    MessageRole::Tool,
                    MessageKind::Tool,
                    if detail.is_empty() {
                        "Tool completed".to_string()
                    } else {
                        detail
                    },
                )));
            }
            _ => {}
        }
        events
    }
}

/// Builds a one-line title that shows what a tool call actually does —
/// "Running Bash · cargo test" instead of an opaque "Running Bash".
fn tool_title(name: &str, input: Option<&Value>) -> String {
    const MAX_SNIPPET: usize = 96;
    let snippet = input.and_then(|input| {
        [
            "command",
            "file_path",
            "path",
            "pattern",
            "query",
            "url",
            "prompt",
            "description",
        ]
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
    });
    match snippet {
        Some(snippet) => {
            let line = snippet.lines().next().unwrap_or_default().trim();
            let mut end = line.len().min(MAX_SNIPPET);
            while end > 0 && !line.is_char_boundary(end) {
                end -= 1;
            }
            let ellipsis = if end < line.len() || snippet.lines().count() > 1 {
                "…"
            } else {
                ""
            };
            format!("Running {name} · {}{ellipsis}", &line[..end])
        }
        None => format!("Running {name}"),
    }
}

fn string_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str()
}

fn text_content(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
        Some(Value::Array(parts)) => {
            let text = parts
                .iter()
                .filter_map(|part| {
                    part.as_str()
                        .or_else(|| part.get("text").and_then(Value::as_str))
                })
                .collect::<Vec<_>>()
                .join("");
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

fn activity(title: String, detail: Option<&Value>) -> NormalizedEvent {
    let content = detail
        .and_then(|value| {
            if let Some(value) = value.as_str() {
                Some(format!("{title}\n{value}"))
            } else if value.is_null() {
                None
            } else {
                serde_json::to_string_pretty(value)
                    .ok()
                    .map(|value| format!("{title}\n{value}"))
            }
        })
        .unwrap_or(title);
    NormalizedEvent::Activity(Message::new(MessageRole::Tool, MessageKind::Tool, content))
}

#[cfg(test)]
mod tests {
    use super::{NormalizedEvent, StreamNormalizer};
    use crate::model::ProviderId;

    #[test]
    fn claude_stream_returns_session_and_delta() {
        let mut parser = StreamNormalizer::new(ProviderId::Claude);
        let events = parser.parse(
            r#"{"type":"stream_event","session_id":"claude-1","event":{"type":"content_block_delta","delta":{"text":"hello"}}}"#,
        );
        assert!(
            events.iter().any(
                |event| matches!(event, NormalizedEvent::Session(value) if value == "claude-1")
            )
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, NormalizedEvent::Delta(value) if value == "hello"))
        );
    }

    #[test]
    fn codex_stream_returns_thread_and_message() {
        let mut parser = StreamNormalizer::new(ProviderId::Codex);
        assert!(matches!(
            parser
                .parse(r#"{"type":"thread.started","thread_id":"codex-1"}"#)
                .as_slice(),
            [NormalizedEvent::Session(value)] if value == "codex-1"
        ));
        assert!(matches!(
            parser
                .parse(r#"{"type":"item.completed","item":{"type":"agent_message","text":"done"}}"#)
                .as_slice(),
            [NormalizedEvent::Text(value)] if value == "done"
        ));
    }

    #[test]
    fn claude_tool_use_titles_show_the_command() {
        let mut parser = StreamNormalizer::new(ProviderId::Claude);
        let events = parser.parse(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"cargo test --workspace"}}]}}"#,
        );
        let [NormalizedEvent::Activity(message)] = events.as_slice() else {
            panic!("expected one activity");
        };
        let title = message.content.lines().next().unwrap();
        assert_eq!(title, "Running Bash · cargo test --workspace");

        let long = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"Bash","input":{{"command":"{}"}}}}]}}}}"#,
            "x".repeat(200)
        );
        let events = parser.parse(&long);
        let [NormalizedEvent::Activity(message)] = events.as_slice() else {
            panic!("expected one activity");
        };
        assert!(message.content.lines().next().unwrap().ends_with('…'));
    }

    #[test]
    fn gemini_stream_returns_session_and_delta() {
        let mut parser = StreamNormalizer::new(ProviderId::Gemini);
        assert!(matches!(
            parser
                .parse(r#"{"type":"init","session_id":"gemini-1"}"#)
                .as_slice(),
            [NormalizedEvent::Session(value)] if value == "gemini-1"
        ));
        assert!(matches!(
            parser
                .parse(r#"{"type":"message","role":"assistant","content":"hello"}"#)
                .as_slice(),
            [NormalizedEvent::Delta(value)] if value == "hello"
        ));
    }

    #[test]
    fn kimi_stream_returns_text_and_resume_id() {
        let mut parser = StreamNormalizer::new(ProviderId::Kimi);
        assert!(matches!(
            parser
                .parse(r#"{"role":"assistant","content":"hello"}"#)
                .as_slice(),
            [NormalizedEvent::Text(value)] if value == "hello"
        ));
        assert!(matches!(
            parser
                .parse(r#"{"role":"meta","type":"session.resume_hint","session_id":"abc"}"#)
                .as_slice(),
            [NormalizedEvent::Session(value)] if value == "abc"
        ));
    }
}
