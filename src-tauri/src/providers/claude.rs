use super::{
    cli::CliSession,
    driver::{
        DriverFuture, ProviderActivity, ProviderApproval, ProviderDriver, ProviderEvent,
        ProviderSession, ProviderSessionConfig,
    },
    normalize::{NormalizedEvent, StreamNormalizer},
    process::{JsonProcess, ProcessOutput, find_executable, platform_command},
};
use crate::model::{AccessMode, ContextUsage, InteractionMode, ProviderId, SpeedMode};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{sync::mpsc, time::timeout};
use tokio_util::sync::CancellationToken;

const MAX_PERSISTENT_STREAM: usize = 256 * 1024 * 1024;
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(60);
const INITIALIZE_REQUEST_ID: &str = "onyx-initialize";

pub struct ClaudeDriver;

impl ProviderDriver for ClaudeDriver {
    fn provider(&self) -> ProviderId {
        ProviderId::Claude
    }

    fn connect<'a>(
        &'a self,
        config: ProviderSessionConfig,
    ) -> DriverFuture<'a, Result<Box<dyn ProviderSession>, String>> {
        Box::pin(async move {
            match ClaudeSession::connect(config.clone()).await {
                Ok(session) => Ok(Box::new(session) as Box<dyn ProviderSession>),
                Err(error) => Ok(Box::new(CliSession::with_fallback_notice(
                    config,
                    format!("Claude bidirectional stream mode was unavailable: {error}"),
                )) as Box<dyn ProviderSession>),
            }
        })
    }
}

struct ClaudeSession {
    process: JsonProcess,
    continuation: Option<String>,
}

impl ClaudeSession {
    async fn connect(config: ProviderSessionConfig) -> Result<Self, String> {
        let executable = find_executable("claude")
            .ok_or_else(|| "Claude Code is not installed or was not found on PATH".to_string())?;
        let mut args = vec![
            "--print".to_string(),
            "--verbose".to_string(),
            "--input-format".to_string(),
            "stream-json".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--include-partial-messages".to_string(),
            "--permission-prompt-tool".to_string(),
            "stdio".to_string(),
            "--permission-mode".to_string(),
            match (config.interaction_mode, config.access_mode) {
                (InteractionMode::Plan, _) => "plan",
                (_, AccessMode::ApprovalRequired) => "manual",
                (_, AccessMode::AutoAcceptEdits) => "acceptEdits",
                (_, AccessMode::FullAccess) => "bypassPermissions",
            }
            .to_string(),
        ];
        if config.access_mode == AccessMode::FullAccess
            && config.interaction_mode != InteractionMode::Plan
        {
            args.push("--dangerously-skip-permissions".to_string());
        }
        if let Some(reasoning) = config.reasoning {
            args.extend(["--effort".to_string(), reasoning.as_str().to_string()]);
        }
        if config.speed_mode == SpeedMode::Fast {
            args.extend(["--settings".to_string(), r#"{"fastMode":true}"#.to_string()]);
        }
        if let Some(id) = config.continuation.as_deref() {
            args.extend(["--resume".to_string(), id.to_string()]);
        }
        if let Some(model) = config.model() {
            args.extend(["--model".to_string(), model.to_string()]);
        }
        let mut command = platform_command(&executable, &args);
        command
            .current_dir(&config.workspace)
            .env_remove("CLAUDECODE");
        let mut process = JsonProcess::spawn(
            command,
            "Claude Code stream transport",
            MAX_PERSISTENT_STREAM,
        )
        .await?;
        process.send_json(&initialize_request()).await?;
        let discovered_continuation =
            timeout(INITIALIZE_TIMEOUT, wait_for_initialize(&mut process))
                .await
                .map_err(|_| "Claude stream control initialization timed out".to_string())??;
        Ok(Self {
            process,
            continuation: discovered_continuation.or(config.continuation),
        })
    }

    async fn run_turn_inner(
        &mut self,
        prompt: &str,
        cancellation: &CancellationToken,
        events: &mpsc::Sender<ProviderEvent>,
    ) -> Result<(), String> {
        self.process.send_json(&user_message(prompt)).await?;
        let mut normalizer = StreamNormalizer::new(ProviderId::Claude);

        loop {
            let value = tokio::select! {
                _ = cancellation.cancelled() => {
                    self.process.shutdown().await;
                    return Err("Turn cancelled".to_string());
                }
                output = self.process.next_stdout() => {
                    match output? {
                        ProcessOutput::Stdout(line) => serde_json::from_str::<Value>(&line)
                            .map_err(|error| format!("Claude Code returned invalid stream JSON: {error}"))?,
                        ProcessOutput::Exited(status) => {
                            let detail = self.process.stderr_tail();
                            return Err(if detail.is_empty() {
                                format!("Claude Code stream transport exited with {status}")
                            } else {
                                format!("Claude Code stream transport exited with {status}: {detail}")
                            });
                        }
                    }
                }
            };

            if value.get("type").and_then(Value::as_str) == Some("control_request") {
                self.handle_control_request(&value, cancellation, events)
                    .await?;
                continue;
            }

            for event in normalizer.parse(&value.to_string()) {
                match event {
                    NormalizedEvent::Delta(delta) => {
                        send_event(events, ProviderEvent::TextDelta(delta)).await?;
                    }
                    NormalizedEvent::Text(text) => {
                        send_event(events, ProviderEvent::Text(text)).await?;
                    }
                    NormalizedEvent::Session(id) => {
                        self.continuation = Some(id.clone());
                        send_event(events, ProviderEvent::Continuation(id)).await?;
                    }
                    NormalizedEvent::Activity(message) => {
                        send_event(
                            events,
                            ProviderEvent::Activity(ProviderActivity {
                                title: message.content,
                                detail: None,
                                kind: message.kind,
                            }),
                        )
                        .await?;
                    }
                }
            }

            if value.get("type").and_then(Value::as_str) == Some("result") {
                if let Some(usage) = parse_context_usage(&value) {
                    send_event(events, ProviderEvent::ContextUsage(usage)).await?;
                }
                if value
                    .get("is_error")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    return Err(value
                        .get("result")
                        .and_then(Value::as_str)
                        .unwrap_or("Claude Code turn failed")
                        .to_string());
                }
                return Ok(());
            }
        }
    }

    async fn handle_control_request(
        &mut self,
        value: &Value,
        cancellation: &CancellationToken,
        events: &mpsc::Sender<ProviderEvent>,
    ) -> Result<(), String> {
        let request_id = value
            .get("request_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "Claude permission request did not include an id".to_string())?;
        let request = value
            .get("request")
            .ok_or_else(|| "Claude control request did not include a request".to_string())?;
        if request.get("subtype").and_then(Value::as_str) != Some("can_use_tool") {
            self.process
                .send_json(&control_error(
                    request_id,
                    "Unsupported Claude control request",
                ))
                .await?;
            return Ok(());
        }

        let tool_name = request
            .get("tool_name")
            .and_then(Value::as_str)
            .unwrap_or("tool");
        let title = request
            .get("title")
            .or_else(|| request.get("display_name"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("Allow Claude Code to use {tool_name}?"));
        let input = request.get("input").cloned().unwrap_or_else(|| json!({}));
        let detail = request
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                serde_json::to_string_pretty(&input).unwrap_or_else(|_| input.to_string())
            });
        let (sender, receiver) = tokio::sync::oneshot::channel();
        send_event(
            events,
            ProviderEvent::Approval(ProviderApproval {
                title,
                detail,
                risk: format!("allows Claude Code to invoke {tool_name}"),
                responder: sender,
            }),
        )
        .await?;
        let approved = tokio::select! {
            _ = cancellation.cancelled() => {
                self.process.shutdown().await;
                return Err("Turn cancelled".to_string());
            }
            result = receiver => result.unwrap_or(false),
        };
        self.process
            .send_json(&control_response(request_id, approved, input))
            .await
    }
}

fn parse_context_usage(value: &Value) -> Option<ContextUsage> {
    let usage = value.get("usage")?;
    let input = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_add(
            usage
                .get("cache_creation_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        )
        .saturating_add(
            usage
                .get("cache_read_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        );
    let output = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let max_tokens = value
        .get("modelUsage")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|models| models.values())
        .filter_map(|model| model.get("contextWindow").and_then(Value::as_u64))
        .max();
    let used = input.saturating_add(output);
    (used > 0).then_some(ContextUsage {
        used_tokens: used,
        max_tokens,
        input_tokens: Some(input),
        cached_input_tokens: usage.get("cache_read_input_tokens").and_then(Value::as_u64),
        output_tokens: Some(output),
        reasoning_output_tokens: None,
    })
}

impl ProviderSession for ClaudeSession {
    fn provider(&self) -> ProviderId {
        ProviderId::Claude
    }

    fn continuation(&self) -> Option<String> {
        self.continuation.clone()
    }

    fn run_turn<'a>(
        &'a mut self,
        prompt: &'a str,
        cancellation: &'a CancellationToken,
        events: mpsc::Sender<ProviderEvent>,
    ) -> DriverFuture<'a, Result<(), String>> {
        Box::pin(async move {
            if let Some(id) = self.continuation.clone() {
                send_event(&events, ProviderEvent::Continuation(id)).await?;
            }
            self.run_turn_inner(prompt, cancellation, &events).await
        })
    }

    fn shutdown<'a>(&'a mut self) -> DriverFuture<'a, ()> {
        Box::pin(async move { self.process.shutdown().await })
    }
}

fn user_message(prompt: &str) -> Value {
    json!({
        "type": "user",
        "session_id": "",
        "message": { "role": "user", "content": prompt },
        "parent_tool_use_id": null
    })
}

fn initialize_request() -> Value {
    json!({
        "type": "control_request",
        "request_id": INITIALIZE_REQUEST_ID,
        "request": { "subtype": "initialize", "hooks": null }
    })
}

async fn wait_for_initialize(process: &mut JsonProcess) -> Result<Option<String>, String> {
    let mut continuation = None;
    loop {
        match process.next_stdout().await? {
            ProcessOutput::Stdout(line) => {
                let value: Value = serde_json::from_str(&line).map_err(|error| {
                    format!("Claude Code returned invalid control protocol JSON: {error}")
                })?;
                let response = value.get("response").unwrap_or(&Value::Null);
                if let Some(id) = value.get("session_id").and_then(Value::as_str) {
                    continuation = Some(id.to_string());
                }
                if value.get("type").and_then(Value::as_str) == Some("control_response")
                    && response.get("request_id").and_then(Value::as_str)
                        == Some(INITIALIZE_REQUEST_ID)
                {
                    return match response.get("subtype").and_then(Value::as_str) {
                        Some("success") => Ok(continuation),
                        _ => Err(response
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("Claude rejected stream control initialization")
                            .to_string()),
                    };
                }
            }
            ProcessOutput::Exited(status) => {
                let detail = process.stderr_tail();
                return Err(if detail.is_empty() {
                    format!("Claude Code stream transport exited with {status}")
                } else {
                    format!("Claude Code stream transport exited with {status}: {detail}")
                });
            }
        }
    }
}

fn control_response(request_id: &str, approved: bool, input: Value) -> Value {
    let permission = if approved {
        json!({ "behavior": "allow", "updatedInput": input })
    } else {
        json!({ "behavior": "deny", "message": "Denied by user", "interrupt": false })
    };
    json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": request_id,
            "response": permission
        }
    })
}

fn control_error(request_id: &str, message: &str) -> Value {
    json!({
        "type": "control_response",
        "response": {
            "subtype": "error",
            "request_id": request_id,
            "error": message
        }
    })
}

async fn send_event(
    sender: &mpsc::Sender<ProviderEvent>,
    event: ProviderEvent,
) -> Result<(), String> {
    sender
        .send(event)
        .await
        .map_err(|_| "Provider event receiver closed".to_string())
}

#[cfg(test)]
mod tests {
    use super::{control_error, control_response, initialize_request, user_message};
    use serde_json::json;

    #[test]
    fn streaming_user_message_matches_claude_sdk_shape() {
        assert_eq!(
            user_message("hello"),
            json!({
                "type":"user",
                "session_id":"",
                "message":{"role":"user","content":"hello"},
                "parent_tool_use_id":null
            })
        );
    }

    #[test]
    fn initializes_claude_bidirectional_control_protocol() {
        let request = initialize_request();
        assert_eq!(request["type"], "control_request");
        assert_eq!(request["request"]["subtype"], "initialize");
        assert_eq!(request["request"]["hooks"], serde_json::Value::Null);
    }

    #[test]
    fn permission_allow_round_trips_original_input() {
        let response = control_response("request-1", true, json!({"command":"cargo test"}));
        assert_eq!(response["response"]["subtype"], "success");
        assert_eq!(
            response["response"]["response"]["updatedInput"]["command"],
            "cargo test"
        );
    }

    #[test]
    fn unsupported_control_requests_receive_protocol_error() {
        let response = control_error("request-2", "unsupported");
        assert_eq!(response["response"]["subtype"], "error");
        assert_eq!(response["response"]["request_id"], "request-2");
    }
}
