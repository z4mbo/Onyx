use super::{
    cli::CliSession,
    driver::{
        DriverFuture, ProviderActivity, ProviderApproval, ProviderDriver, ProviderEvent,
        ProviderSession, ProviderSessionConfig,
    },
    process::{JsonProcess, ProcessOutput, find_executable, platform_command},
};
use crate::model::ProviderId;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{sync::mpsc, time::timeout};
use tokio_util::sync::CancellationToken;

const MAX_PERSISTENT_STREAM: usize = 256 * 1024 * 1024;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(12);

pub struct CodexDriver;

impl ProviderDriver for CodexDriver {
    fn provider(&self) -> ProviderId {
        ProviderId::Codex
    }

    fn connect<'a>(
        &'a self,
        config: ProviderSessionConfig,
    ) -> DriverFuture<'a, Result<Box<dyn ProviderSession>, String>> {
        Box::pin(async move {
            match CodexSession::connect(config.clone()).await {
                Ok(session) => Ok(Box::new(session) as Box<dyn ProviderSession>),
                Err(error) => Ok(Box::new(CliSession::with_fallback_notice(
                    config,
                    format!("Codex app-server was unavailable: {error}"),
                )) as Box<dyn ProviderSession>),
            }
        })
    }
}

struct CodexSession {
    process: JsonProcess,
    thread_id: String,
    next_request_id: u64,
}

impl CodexSession {
    async fn connect(config: ProviderSessionConfig) -> Result<Self, String> {
        let executable = find_executable("codex")
            .ok_or_else(|| "Codex is not installed or was not found on PATH".to_string())?;
        let args = vec!["app-server".to_string(), "--stdio".to_string()];
        let mut command = platform_command(&executable, &args);
        command.current_dir(&config.workspace);
        let mut process =
            JsonProcess::spawn(command, "Codex app-server", MAX_PERSISTENT_STREAM).await?;

        process.send_json(&initialize_request(1)).await?;
        let initialized = timeout(STARTUP_TIMEOUT, wait_for_response(&mut process, 1))
            .await
            .map_err(|_| "Codex app-server initialization timed out".to_string())??;
        response_result(&initialized)?;
        process
            .send_json(&json!({ "method": "initialized" }))
            .await?;

        let thread_request = thread_request(2, &config);
        process.send_json(&thread_request).await?;
        let thread_response = timeout(STARTUP_TIMEOUT, wait_for_response(&mut process, 2))
            .await
            .map_err(|_| "Codex thread initialization timed out".to_string())??;
        let thread_id = response_result(&thread_response)?
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or_else(|| "Codex thread response did not include a thread id".to_string())?
            .to_string();

        Ok(Self {
            process,
            thread_id,
            next_request_id: 3,
        })
    }

    async fn run_turn_inner(
        &mut self,
        prompt: &str,
        cancellation: &CancellationToken,
        events: &mpsc::Sender<ProviderEvent>,
    ) -> Result<(), String> {
        let request_id = self.take_request_id();
        self.process
            .send_json(&turn_start_request(request_id, &self.thread_id, prompt))
            .await?;
        let mut turn_id = None;
        let mut streamed_items = HashSet::new();
        let mut pending_error = None;

        loop {
            let value = tokio::select! {
                _ = cancellation.cancelled() => {
                    self.process.shutdown().await;
                    return Err("Turn cancelled".to_string());
                }
                output = self.process.next_stdout() => {
                    match output? {
                        ProcessOutput::Stdout(line) => serde_json::from_str::<Value>(&line)
                            .map_err(|error| format!("Codex app-server returned invalid JSON: {error}"))?,
                        ProcessOutput::Exited(status) => {
                            let detail = self.process.stderr_tail();
                            return Err(if detail.is_empty() {
                                format!("Codex app-server exited with {status}")
                            } else {
                                format!("Codex app-server exited with {status}: {detail}")
                            });
                        }
                    }
                }
            };

            if is_client_response(&value, request_id) {
                let result = response_result(&value)?;
                turn_id = result
                    .pointer("/turn/id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                continue;
            }

            let method = value.get("method").and_then(Value::as_str);
            match method {
                Some("item/agentMessage/delta") => {
                    if let Some(delta) = value.pointer("/params/delta").and_then(Value::as_str) {
                        if let Some(item_id) =
                            value.pointer("/params/itemId").and_then(Value::as_str)
                        {
                            streamed_items.insert(item_id.to_string());
                        }
                        send_event(events, ProviderEvent::TextDelta(delta.to_string())).await?;
                    }
                }
                Some("item/started") => {
                    if let Some(activity) = item_activity(&value, false) {
                        send_event(events, ProviderEvent::Activity(activity)).await?;
                    }
                }
                Some("item/completed") => {
                    let item = value.pointer("/params/item").unwrap_or(&Value::Null);
                    if item.get("type").and_then(Value::as_str) == Some("agentMessage") {
                        let item_id = item.get("id").and_then(Value::as_str).unwrap_or_default();
                        if !streamed_items.contains(item_id)
                            && let Some(text) = item.get("text").and_then(Value::as_str)
                        {
                            send_event(events, ProviderEvent::Text(text.to_string())).await?;
                        }
                    } else if let Some(activity) = item_activity(&value, true) {
                        send_event(events, ProviderEvent::Activity(activity)).await?;
                    }
                }
                Some(
                    method @ ("item/commandExecution/requestApproval"
                    | "item/fileChange/requestApproval"
                    | "item/permissions/requestApproval"
                    | "mcpServer/elicitation/request"
                    | "applyPatchApproval"
                    | "execCommandApproval"),
                ) => {
                    self.handle_approval(&value, method, cancellation, events)
                        .await?;
                }
                Some("item/tool/requestUserInput") => {
                    send_event(
                        events,
                        ProviderEvent::Activity(ProviderActivity::error(
                            "Codex requested interactive input, but this zAI version only supports boolean approvals",
                        )),
                    )
                    .await?;
                    self.respond_to_server_request(&value, json!({ "answers": {} }))
                        .await?;
                }
                Some("currentTime/read") => {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        .min(i64::MAX as u64);
                    self.respond_to_server_request(&value, json!({ "currentTimeAt": now as i64 }))
                        .await?;
                }
                Some("error") => {
                    let message = value
                        .pointer("/params/error/message")
                        .or_else(|| value.pointer("/params/message"))
                        .and_then(Value::as_str)
                        .unwrap_or("Codex reported an error")
                        .to_string();
                    pending_error = Some(message.clone());
                    send_event(
                        events,
                        ProviderEvent::Activity(ProviderActivity::error(message)),
                    )
                    .await?;
                }
                Some("turn/completed") => {
                    let completed_turn = value.pointer("/params/turn").unwrap_or(&Value::Null);
                    if let Some(expected) = turn_id.as_deref()
                        && completed_turn.get("id").and_then(Value::as_str) != Some(expected)
                    {
                        continue;
                    }
                    return match completed_turn.get("status").and_then(Value::as_str) {
                        Some("completed") => Ok(()),
                        Some("interrupted") => Err("Turn cancelled".to_string()),
                        Some(status) => Err(completed_turn
                            .pointer("/error/message")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .or(pending_error)
                            .unwrap_or_else(|| format!("Codex turn ended with status {status}"))),
                        None => Err(pending_error.unwrap_or_else(|| {
                            "Codex turn completed without a status".to_string()
                        })),
                    };
                }
                Some(method) if value.get("id").is_some() => {
                    self.reject_server_request(&value, method).await?;
                }
                _ => {}
            }
        }
    }

    async fn handle_approval(
        &mut self,
        value: &Value,
        method: &str,
        cancellation: &CancellationToken,
        events: &mpsc::Sender<ProviderEvent>,
    ) -> Result<(), String> {
        let response_id = value
            .get("id")
            .cloned()
            .ok_or_else(|| "Codex approval request did not include an id".to_string())?;
        let params = value.get("params").cloned().unwrap_or(Value::Null);
        let is_command = method.contains("commandExecution") || method == "execCommandApproval";
        let title = match method {
            "item/permissions/requestApproval" => "Allow Codex additional permissions?",
            "mcpServer/elicitation/request" => "Allow this MCP server request?",
            _ if is_command => "Allow Codex to run this command?",
            _ => "Allow Codex to change files?",
        };
        let detail = approval_detail(&params, is_command);
        let risk = match method {
            "item/permissions/requestApproval" => {
                "grants additional filesystem or network permissions to Codex"
            }
            "mcpServer/elicitation/request" => "responds to an external MCP server request",
            _ if is_command => "executes a command requested by Codex",
            _ => "writes files requested by Codex",
        };
        let (sender, receiver) = tokio::sync::oneshot::channel();
        send_event(
            events,
            ProviderEvent::Approval(ProviderApproval {
                title: title.to_string(),
                detail,
                risk: risk.to_string(),
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
            .send_json(&approval_response(method, response_id, approved, &params))
            .await
    }

    async fn respond_to_server_request(
        &mut self,
        value: &Value,
        result: Value,
    ) -> Result<(), String> {
        let id = value
            .get("id")
            .cloned()
            .ok_or_else(|| "Codex server request did not include an id".to_string())?;
        self.process
            .send_json(&json!({ "id": id, "result": result }))
            .await
    }

    async fn reject_server_request(&mut self, value: &Value, method: &str) -> Result<(), String> {
        let id = value
            .get("id")
            .cloned()
            .ok_or_else(|| "Codex server request did not include an id".to_string())?;
        self.process
            .send_json(&json!({
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("zAI does not implement Codex server request {method}")
                }
            }))
            .await
    }

    fn take_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        id
    }
}

impl ProviderSession for CodexSession {
    fn provider(&self) -> ProviderId {
        ProviderId::Codex
    }

    fn continuation(&self) -> Option<String> {
        Some(self.thread_id.clone())
    }

    fn run_turn<'a>(
        &'a mut self,
        prompt: &'a str,
        cancellation: &'a CancellationToken,
        events: mpsc::Sender<ProviderEvent>,
    ) -> DriverFuture<'a, Result<(), String>> {
        Box::pin(async move {
            send_event(&events, ProviderEvent::Continuation(self.thread_id.clone())).await?;
            self.run_turn_inner(prompt, cancellation, &events).await
        })
    }

    fn shutdown<'a>(&'a mut self) -> DriverFuture<'a, ()> {
        Box::pin(async move { self.process.shutdown().await })
    }
}

async fn wait_for_response(process: &mut JsonProcess, id: u64) -> Result<Value, String> {
    loop {
        match process.next_stdout().await? {
            ProcessOutput::Stdout(line) => {
                let value: Value = serde_json::from_str(&line)
                    .map_err(|error| format!("Codex app-server returned invalid JSON: {error}"))?;
                if is_client_response(&value, id) {
                    return Ok(value);
                }
            }
            ProcessOutput::Exited(status) => {
                let detail = process.stderr_tail();
                return Err(if detail.is_empty() {
                    format!("Codex app-server exited with {status}")
                } else {
                    format!("Codex app-server exited with {status}: {detail}")
                });
            }
        }
    }
}

fn initialize_request(id: u64) -> Value {
    json!({
        "id": id,
        "method": "initialize",
        "params": {
            "clientInfo": { "name": "zai", "title": "zAI", "version": env!("CARGO_PKG_VERSION") },
            "capabilities": { "experimentalApi": true }
        }
    })
}

fn thread_request(id: u64, config: &ProviderSessionConfig) -> Value {
    let mut params = json!({
        "cwd": config.workspace,
        "approvalPolicy": "on-request",
        "sandbox": "workspace-write"
    });
    if let Some(model) = config.model() {
        params["model"] = json!(model);
    }
    if let Some(thread_id) = config.continuation.as_deref() {
        params["threadId"] = json!(thread_id);
        json!({ "id": id, "method": "thread/resume", "params": params })
    } else {
        json!({ "id": id, "method": "thread/start", "params": params })
    }
}

fn turn_start_request(id: u64, thread_id: &str, prompt: &str) -> Value {
    json!({
        "id": id,
        "method": "turn/start",
        "params": {
            "threadId": thread_id,
            "input": [{ "type": "text", "text": prompt }]
        }
    })
}

fn approval_response(method: &str, id: Value, approved: bool, params: &Value) -> Value {
    let result = match method {
        "applyPatchApproval" | "execCommandApproval" => json!({
            "decision": if approved {
                json!("approved")
            } else {
                json!({ "denied": { "rejection": "User denied the request in zAI" } })
            }
        }),
        "item/permissions/requestApproval" => json!({
            "permissions": if approved {
                params.get("permissions").cloned().unwrap_or_else(|| json!({}))
            } else {
                json!({})
            },
            "scope": "turn"
        }),
        "mcpServer/elicitation/request" => json!({
            "action": if approved { "accept" } else { "decline" },
            "content": null,
            "_meta": null
        }),
        _ => json!({ "decision": if approved { "accept" } else { "decline" } }),
    };
    json!({ "id": id, "result": result })
}

fn response_result(value: &Value) -> Result<&Value, String> {
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Codex app-server request failed");
        return Err(message.to_string());
    }
    value
        .get("result")
        .ok_or_else(|| "Codex app-server response did not include a result".to_string())
}

fn id_matches(value: &Value, id: u64) -> bool {
    value.get("id").is_some_and(|value| {
        value.as_u64() == Some(id) || value.as_str().is_some_and(|value| value == id.to_string())
    })
}

fn is_client_response(value: &Value, id: u64) -> bool {
    value.get("method").is_none()
        && (value.get("result").is_some() || value.get("error").is_some())
        && id_matches(value, id)
}

fn item_activity(value: &Value, completed: bool) -> Option<ProviderActivity> {
    let item = value.pointer("/params/item")?;
    match item.get("type")?.as_str()? {
        "commandExecution" => {
            let command = item
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("command");
            Some(ProviderActivity::tool(
                if completed {
                    format!("Ran {command}")
                } else {
                    format!("Running {command}")
                },
                completed
                    .then(|| item.get("aggregatedOutput"))
                    .flatten()
                    .and_then(value_text),
            ))
        }
        "fileChange" if completed => Some(ProviderActivity::tool(
            "Applied file changes",
            item.get("changes").and_then(value_text),
        )),
        "mcpToolCall" => {
            let tool = item
                .get("tool")
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("MCP tool");
            Some(ProviderActivity::tool(
                if completed {
                    format!("Completed {tool}")
                } else {
                    format!("Running {tool}")
                },
                None,
            ))
        }
        _ => None,
    }
}

fn approval_detail(params: &Value, command: bool) -> String {
    if command && let Some(value) = params.get("command").and_then(value_text) {
        return value;
    }
    serde_json::to_string_pretty(params).unwrap_or_else(|_| params.to_string())
}

fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Array(values) => Some(
            values
                .iter()
                .filter_map(|value| value.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        ),
        Value::Null => None,
        value => serde_json::to_string_pretty(value).ok(),
    }
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
    use super::{
        approval_response, initialize_request, is_client_response, item_activity, thread_request,
        turn_start_request,
    };
    use crate::{model::ProviderId, providers::driver::ProviderSessionConfig};
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn app_server_requests_use_v2_methods() {
        assert_eq!(initialize_request(1)["method"], "initialize");
        let config = ProviderSessionConfig {
            provider: ProviderId::Codex,
            model: Some("gpt-5.4".to_string()),
            workspace: PathBuf::from("/tmp/project"),
            continuation: Some("thread-1".to_string()),
        };
        let request = thread_request(2, &config);
        assert_eq!(request["method"], "thread/resume");
        assert_eq!(request["params"]["approvalPolicy"], "on-request");
        assert_eq!(request["params"]["sandbox"], "workspace-write");
        let turn = turn_start_request(3, "thread-1", "hello");
        assert_eq!(turn["params"]["input"][0]["type"], "text");
    }

    #[test]
    fn approval_response_preserves_server_id() {
        assert_eq!(
            approval_response(
                "item/commandExecution/requestApproval",
                json!("approval-7"),
                true,
                &json!({})
            ),
            json!({"id":"approval-7","result":{"decision":"accept"}})
        );
        assert_eq!(
            approval_response(
                "item/fileChange/requestApproval",
                json!(8),
                false,
                &json!({})
            )["result"]["decision"],
            "decline"
        );
    }

    #[test]
    fn legacy_approval_denial_matches_current_review_decision_schema() {
        let response = approval_response("execCommandApproval", json!(3), false, &json!({}));
        assert_eq!(
            response["result"]["decision"],
            json!({ "denied": { "rejection": "User denied the request in zAI" } })
        );
        assert_eq!(
            approval_response("applyPatchApproval", json!(4), true, &json!({}))["result"]["decision"],
            "approved"
        );
    }

    #[test]
    fn server_request_id_collision_is_not_a_client_response() {
        let request = json!({
            "id": 3,
            "method": "item/commandExecution/requestApproval",
            "params": { "command": "cargo test" }
        });
        assert!(!is_client_response(&request, 3));
        assert!(is_client_response(
            &json!({ "id": 3, "result": { "turn": { "id": "turn-1" } } }),
            3
        ));
        assert!(is_client_response(
            &json!({ "id": "3", "error": { "message": "failed" } }),
            3
        ));
        assert!(!is_client_response(&json!({ "id": 3 }), 3));
    }

    #[test]
    fn additional_permission_approval_is_scoped_to_turn() {
        let permissions = json!({"network":{"enabled":true}});
        let response = approval_response(
            "item/permissions/requestApproval",
            json!(9),
            true,
            &json!({"permissions": permissions}),
        );
        assert_eq!(response["result"]["scope"], "turn");
        assert_eq!(response["result"]["permissions"], permissions);
    }

    #[test]
    fn mcp_elicitation_response_includes_required_metadata() {
        let response =
            approval_response("mcpServer/elicitation/request", json!(10), true, &json!({}));
        assert_eq!(response["result"]["action"], "accept");
        assert!(response["result"]["content"].is_null());
        assert!(response["result"]["_meta"].is_null());
    }

    #[test]
    fn command_notification_becomes_canonical_activity() {
        let event = json!({
            "method": "item/completed",
            "params": {"item": {
                "type": "commandExecution",
                "command": "cargo test",
                "aggregatedOutput": "ok"
            }}
        });
        let activity = item_activity(&event, true).expect("activity");
        assert_eq!(activity.title, "Ran cargo test");
        assert_eq!(activity.detail.as_deref(), Some("ok"));
    }
}
