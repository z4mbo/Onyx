use super::{
    cli::CliSession,
    driver::{
        DriverFuture, ProviderActivity, ProviderApproval, ProviderDriver, ProviderEvent,
        ProviderSession, ProviderSessionConfig, ProviderUserInput, ProviderUserInputOption,
        ProviderUserInputPrompt, ProviderUserInputResponse, TURN_CANCELLED_ERROR,
    },
    process::{JsonProcess, ProcessOutput, find_executable, platform_command},
};
use crate::model::{
    AccessMode, ApprovalDecision, ContextUsage, InteractionMode, MessageKind, ProviderId,
    ProviderModelOption, ReasoningEffort, SpeedMode,
};
use serde_json::{Value, json};
use std::{collections::HashMap, time::Duration};
use tokio::{
    sync::{mpsc, oneshot},
    time::{Instant, timeout, timeout_at},
};
use tokio_util::sync::CancellationToken;

const CATALOG_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_CATALOG_STREAM: usize = 4 * 1024 * 1024;
const MAX_MODELS: usize = 2_048;
const MAX_ID_BYTES: usize = 512;
const MAX_NAME_BYTES: usize = 512;
const MAX_DESCRIPTION_BYTES: usize = 4 * 1024;
const MAX_ACP_STREAM: usize = 128 * 1024 * 1024;
const MAX_ACTIVITY_BYTES: usize = 64 * 1024;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(12);
const CANCEL_TIMEOUT: Duration = Duration::from_secs(12);

pub struct KimiDriver;

impl ProviderDriver for KimiDriver {
    fn provider(&self) -> ProviderId {
        ProviderId::Kimi
    }

    fn connect<'a>(
        &'a self,
        config: ProviderSessionConfig,
    ) -> DriverFuture<'a, Result<Box<dyn ProviderSession>, String>> {
        Box::pin(async move {
            let (process, next_request_id) = match start_acp(&config).await {
                Ok(started) => started,
                Err(error) => {
                    return Ok(Box::new(CliSession::with_fallback_notice(
                        config,
                        format!("Kimi ACP was unavailable: {error}"),
                    )) as Box<dyn ProviderSession>);
                }
            };
            let session = KimiSession::establish(process, next_request_id, config).await?;
            Ok(Box::new(session) as Box<dyn ProviderSession>)
        })
    }
}

struct KimiSession {
    process: JsonProcess,
    session_id: String,
    next_request_id: u64,
}

impl KimiSession {
    async fn establish(
        mut process: JsonProcess,
        mut next_request_id: u64,
        config: ProviderSessionConfig,
    ) -> Result<Self, String> {
        let request_id = next_request_id;
        next_request_id = next_request_id.saturating_add(1);
        let (method, params) = match config.continuation.as_deref() {
            Some(session_id) => (
                "session/load",
                json!({
                    "sessionId": session_id,
                    "cwd": config.workspace.to_string_lossy(),
                    "mcpServers": []
                }),
            ),
            None => (
                "session/new",
                json!({
                    "cwd": config.workspace.to_string_lossy(),
                    "mcpServers": []
                }),
            ),
        };
        process
            .send_json(&json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": method,
                "params": params
            }))
            .await?;
        let response = timeout(STARTUP_TIMEOUT, wait_for_response(&mut process, request_id))
            .await
            .map_err(|_| "Kimi ACP session initialization timed out".to_string())??;
        let result = response_result(&response)?;
        let session_id = config
            .continuation
            .clone()
            .or_else(|| {
                result
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .ok_or_else(|| "Kimi ACP did not return a session id".to_string())?;
        let mut config_options = result
            .get("configOptions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        apply_session_config(
            &mut process,
            &session_id,
            &config,
            &mut config_options,
            &mut next_request_id,
        )
        .await?;

        Ok(Self {
            process,
            session_id,
            next_request_id,
        })
    }

    fn take_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        id
    }

    async fn run_turn_inner(
        &mut self,
        prompt: &str,
        cancellation: &CancellationToken,
        events: &mpsc::Sender<ProviderEvent>,
    ) -> Result<(), String> {
        let request_id = self.take_request_id();
        self.process
            .send_json(&json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": "session/prompt",
                "params": {
                    "sessionId": self.session_id,
                    "prompt": [{ "type": "text", "text": prompt }]
                }
            }))
            .await?;

        let mut cancelling = false;
        let mut cancel_deadline = None;
        let mut thoughts = String::new();
        let mut tool_titles = HashMap::<String, String>::new();
        loop {
            let output = if cancelling {
                timeout_at(
                    *cancel_deadline.get_or_insert_with(|| Instant::now() + CANCEL_TIMEOUT),
                    self.process.next_stdout(),
                )
                .await
                .map_err(|_| "Kimi ACP cancellation timed out".to_string())??
            } else {
                tokio::select! {
                    _ = cancellation.cancelled() => {
                        self.process.send_json(&json!({
                            "jsonrpc": "2.0",
                            "method": "session/cancel",
                            "params": { "sessionId": self.session_id }
                        })).await?;
                        cancelling = true;
                        cancel_deadline = Some(Instant::now() + CANCEL_TIMEOUT);
                        continue;
                    }
                    output = self.process.next_stdout() => output?,
                }
            };
            let value = match output {
                ProcessOutput::Stdout(line) => serde_json::from_str::<Value>(&line)
                    .map_err(|error| format!("Kimi ACP returned invalid JSON: {error}"))?,
                ProcessOutput::Exited(_) => {
                    return Err("Kimi ACP exited before the turn completed".to_string());
                }
            };

            if value.get("id").and_then(Value::as_u64) == Some(request_id) {
                let result = response_result(&value)?;
                let stop_reason = result
                    .get("stopReason")
                    .and_then(Value::as_str)
                    .unwrap_or("end_turn");
                return if cancelling || stop_reason == "cancelled" {
                    Err(TURN_CANCELLED_ERROR.to_string())
                } else {
                    Ok(())
                };
            }

            match value.get("method").and_then(Value::as_str) {
                Some("session/update") => {
                    if value.pointer("/params/sessionId").and_then(Value::as_str)
                        != Some(self.session_id.as_str())
                    {
                        continue;
                    }
                    handle_session_update(
                        value.pointer("/params/update").unwrap_or(&Value::Null),
                        events,
                        &mut thoughts,
                        &mut tool_titles,
                    )
                    .await?;
                }
                Some("session/request_permission") if value.get("id").is_some() => {
                    let was_cancelled =
                        handle_permission_request(&mut self.process, &value, cancellation, events)
                            .await?;
                    if was_cancelled && !cancelling {
                        self.process
                            .send_json(&json!({
                                "jsonrpc": "2.0",
                                "method": "session/cancel",
                                "params": { "sessionId": self.session_id }
                            }))
                            .await?;
                        cancelling = true;
                        cancel_deadline = Some(Instant::now() + CANCEL_TIMEOUT);
                    }
                }
                Some(_) if value.get("id").is_some() => {
                    respond_method_not_found(&mut self.process, &value).await?;
                }
                _ => {}
            }
        }
    }
}

impl ProviderSession for KimiSession {
    fn provider(&self) -> ProviderId {
        ProviderId::Kimi
    }

    fn continuation(&self) -> Option<String> {
        Some(self.session_id.clone())
    }

    fn survives_turn_cancellation(&self) -> bool {
        true
    }

    fn run_turn<'a>(
        &'a mut self,
        prompt: &'a str,
        cancellation: &'a CancellationToken,
        events: mpsc::Sender<ProviderEvent>,
        steer: mpsc::UnboundedReceiver<String>,
    ) -> DriverFuture<'a, Result<(), String>> {
        drop(steer);
        Box::pin(async move { self.run_turn_inner(prompt, cancellation, &events).await })
    }

    fn shutdown<'a>(&'a mut self) -> DriverFuture<'a, ()> {
        Box::pin(async move {
            self.process.shutdown().await;
        })
    }
}

async fn start_acp(config: &ProviderSessionConfig) -> Result<(JsonProcess, u64), String> {
    let executable = find_executable("kimi")
        .ok_or_else(|| "Kimi Code is not installed or was not found on PATH".to_string())?;
    let args = vec!["acp".to_string()];
    let mut command = platform_command(&executable, &args);
    command.current_dir(&config.workspace);
    let mut process = JsonProcess::spawn(command, "Kimi ACP", MAX_ACP_STREAM).await?;
    process
        .send_json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientCapabilities": {
                    "fs": { "readTextFile": false, "writeTextFile": false },
                    "terminal": false,
                    "session": {
                        "configOptions": {
                            "boolean": {}
                        }
                    }
                },
                "clientInfo": {
                    "name": "Onyx",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        }))
        .await?;
    let response = timeout(STARTUP_TIMEOUT, wait_for_response(&mut process, 1))
        .await
        .map_err(|_| "Kimi ACP initialization timed out".to_string())??;
    let result = response_result(&response)?;
    if result.get("protocolVersion").and_then(Value::as_u64) != Some(1) {
        process.shutdown().await;
        return Err("Kimi ACP does not support protocol version 1".to_string());
    }
    Ok((process, 2))
}

async fn apply_session_config(
    process: &mut JsonProcess,
    session_id: &str,
    config: &ProviderSessionConfig,
    config_options: &mut Vec<Value>,
    next_request_id: &mut u64,
) -> Result<(), String> {
    if let Some(model) = config.model() {
        set_advertised_option(
            process,
            session_id,
            config_options,
            "model",
            "model",
            model,
            next_request_id,
        )
        .await?;
    }
    if let Some(reasoning) = config.reasoning {
        set_advertised_option(
            process,
            session_id,
            config_options,
            "thought_level",
            "thinking",
            reasoning.as_str(),
            next_request_id,
        )
        .await?;
    }
    let mode = desired_mode(config.interaction_mode, config.access_mode);
    set_advertised_option(
        process,
        session_id,
        config_options,
        "mode",
        "mode",
        mode,
        next_request_id,
    )
    .await
}

fn desired_mode(interaction: InteractionMode, access: AccessMode) -> &'static str {
    match (interaction, access) {
        (InteractionMode::Plan, _) => "plan",
        (_, AccessMode::ApprovalRequired) => "default",
        // Kimi advertises YOLO as auto-approving tool actions while retaining
        // user questions. It is the provider's closest native access mode.
        (_, AccessMode::AutoAcceptEdits) => "yolo",
        (_, AccessMode::FullAccess) => "auto",
    }
}

#[allow(clippy::too_many_arguments)]
async fn set_advertised_option(
    process: &mut JsonProcess,
    session_id: &str,
    config_options: &mut Vec<Value>,
    category: &str,
    verified_kimi_id: &str,
    desired: &str,
    next_request_id: &mut u64,
) -> Result<(), String> {
    let option =
        find_config_option(config_options, category, verified_kimi_id).ok_or_else(|| {
            format!("Kimi ACP did not advertise its {verified_kimi_id} configuration")
        })?;
    let config_id = option
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "Kimi ACP returned a configuration without an id".to_string())?
        .to_string();
    if option.get("currentValue").and_then(Value::as_str) == Some(desired) {
        return Ok(());
    }
    if !select_option_contains(option.get("options").unwrap_or(&Value::Null), desired) {
        return Err(format!(
            "Kimi ACP does not advertise `{desired}` for its {verified_kimi_id} configuration"
        ));
    }

    let request_id = *next_request_id;
    *next_request_id = next_request_id.saturating_add(1);
    process
        .send_json(&json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "session/set_config_option",
            "params": {
                "sessionId": session_id,
                "configId": config_id,
                "value": desired
            }
        }))
        .await?;
    let response = timeout(STARTUP_TIMEOUT, wait_for_response(process, request_id))
        .await
        .map_err(|_| format!("Kimi ACP timed out while setting {verified_kimi_id}"))??;
    let result = response_result(&response)?;
    *config_options = result
        .get("configOptions")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| {
            "Kimi ACP configuration response did not include configOptions".to_string()
        })?;
    Ok(())
}

fn find_config_option<'a>(
    options: &'a [Value],
    category: &str,
    verified_kimi_id: &str,
) -> Option<&'a Value> {
    options
        .iter()
        .find(|option| option.get("category").and_then(Value::as_str) == Some(category))
        .or_else(|| {
            options
                .iter()
                .find(|option| option.get("id").and_then(Value::as_str) == Some(verified_kimi_id))
        })
}

fn select_option_contains(options: &Value, desired: &str) -> bool {
    options.as_array().is_some_and(|options| {
        options.iter().any(|option| {
            option.get("value").and_then(Value::as_str) == Some(desired)
                || option
                    .get("options")
                    .is_some_and(|children| select_option_contains(children, desired))
        })
    })
}

async fn wait_for_response(process: &mut JsonProcess, id: u64) -> Result<Value, String> {
    loop {
        match process.next_stdout().await? {
            ProcessOutput::Stdout(line) => {
                let value: Value = serde_json::from_str(&line)
                    .map_err(|error| format!("Kimi ACP returned invalid JSON: {error}"))?;
                if value.get("id").and_then(Value::as_u64) == Some(id) {
                    return Ok(value);
                }
                if value.get("method").is_some() && value.get("id").is_some() {
                    respond_method_not_found(process, &value).await?;
                }
            }
            ProcessOutput::Exited(_) => {
                return Err("Kimi ACP exited during initialization".to_string());
            }
        }
    }
}

fn response_result(value: &Value) -> Result<&Value, String> {
    if let Some(result) = value.get("result") {
        return Ok(result);
    }
    let message = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .filter(|message| message.len() <= MAX_DESCRIPTION_BYTES)
        .unwrap_or("request failed");
    Err(format!("Kimi ACP {message}"))
}

async fn respond_method_not_found(
    process: &mut JsonProcess,
    request: &Value,
) -> Result<(), String> {
    let Some(id) = request.get("id").cloned() else {
        return Ok(());
    };
    process
        .send_json(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": "Onyx did not advertise this ACP client method"
            }
        }))
        .await
}

async fn handle_session_update(
    update: &Value,
    events: &mpsc::Sender<ProviderEvent>,
    thoughts: &mut String,
    tool_titles: &mut HashMap<String, String>,
) -> Result<(), String> {
    match update.get("sessionUpdate").and_then(Value::as_str) {
        Some("agent_message_chunk") => {
            if let Some(text) = content_text(update.get("content").unwrap_or(&Value::Null)) {
                send_event(events, ProviderEvent::TextDelta(text)).await?;
            }
        }
        Some("agent_thought_chunk") => {
            if let Some(text) = content_text(update.get("content").unwrap_or(&Value::Null)) {
                push_bounded(thoughts, &text, MAX_ACTIVITY_BYTES);
                send_event(
                    events,
                    ProviderEvent::Activity(ProviderActivity {
                        id: Some("kimi-reasoning".to_string()),
                        title: "Reasoning".to_string(),
                        detail: Some(thoughts.clone()),
                        kind: MessageKind::Reasoning,
                    }),
                )
                .await?;
            }
        }
        Some("tool_call") | Some("tool_call_update") => {
            let Some(tool_call_id) = update.get("toolCallId").and_then(Value::as_str) else {
                return Ok(());
            };
            if let Some(title) = update.get("title").and_then(Value::as_str) {
                tool_titles.insert(tool_call_id.to_string(), bounded(title, MAX_NAME_BYTES));
            }
            let title = tool_titles
                .get(tool_call_id)
                .cloned()
                .unwrap_or_else(|| "Kimi tool".to_string());
            let detail = tool_detail(update);
            send_event(
                events,
                ProviderEvent::Activity(ProviderActivity {
                    id: Some(tool_call_id.to_string()),
                    title,
                    detail,
                    kind: MessageKind::Tool,
                }),
            )
            .await?;
        }
        Some("plan") => {
            let detail = update
                .get("entries")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|entry| {
                    let content = entry.get("content").and_then(Value::as_str)?;
                    let status = entry
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("pending");
                    Some(format!("[{status}] {content}"))
                })
                .collect::<Vec<_>>()
                .join("\n");
            send_event(
                events,
                ProviderEvent::Activity(ProviderActivity {
                    id: Some("kimi-plan".to_string()),
                    title: "Plan".to_string(),
                    detail: Some(bounded(&detail, MAX_ACTIVITY_BYTES)),
                    kind: MessageKind::Reasoning,
                }),
            )
            .await?;
        }
        Some("usage_update") => {
            if let (Some(used), Some(size)) = (
                update.get("used").and_then(Value::as_u64),
                update.get("size").and_then(Value::as_u64),
            ) {
                send_event(
                    events,
                    ProviderEvent::ContextUsage(ContextUsage {
                        used_tokens: used,
                        max_tokens: Some(size),
                        input_tokens: None,
                        cached_input_tokens: None,
                        output_tokens: None,
                        reasoning_output_tokens: None,
                    }),
                )
                .await?;
            }
        }
        _ => {}
    }
    Ok(())
}

async fn handle_permission_request(
    process: &mut JsonProcess,
    request: &Value,
    cancellation: &CancellationToken,
    events: &mpsc::Sender<ProviderEvent>,
) -> Result<bool, String> {
    let id = request
        .get("id")
        .cloned()
        .ok_or_else(|| "Kimi ACP permission request had no id".to_string())?;
    let params = request.get("params").unwrap_or(&Value::Null);
    let options = params
        .get("options")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tool_call = params.get("toolCall").unwrap_or(&Value::Null);
    let title = tool_call
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Kimi tool request");

    let outcome = if is_user_question(title, &options) {
        let (sender, receiver) = oneshot::channel();
        let question = ProviderUserInputPrompt::new(
            "Kimi question",
            vec![crate::model::ProviderUserInputQuestion {
                id: "choice".to_string(),
                header: "Question".to_string(),
                question: tool_call_question(tool_call),
                options: options
                    .iter()
                    .filter(|option| {
                        !matches!(
                            option.get("kind").and_then(Value::as_str),
                            Some("reject_once" | "reject_always")
                        )
                    })
                    .filter_map(|option| {
                        Some(ProviderUserInputOption {
                            label: bounded(
                                option.get("name").and_then(Value::as_str)?,
                                MAX_NAME_BYTES,
                            ),
                            description: String::new(),
                        })
                    })
                    .collect(),
                multi_select: false,
                allow_other: false,
                secret: false,
            }],
            None,
        )?;
        send_event(
            events,
            ProviderEvent::UserInput(ProviderUserInput {
                prompt: question,
                responder: sender,
            }),
        )
        .await?;
        tokio::select! {
            _ = cancellation.cancelled() => cancelled_outcome(),
            response = receiver => match response {
                Ok(ProviderUserInputResponse::Answered(answers)) => {
                    let answer = answers.get("choice").and_then(|values| values.first());
                    answer
                        .and_then(|answer| {
                            options.iter().find(|option| {
                                option.get("name").and_then(Value::as_str) == Some(answer.as_str())
                            })
                        })
                        .and_then(|option| option.get("optionId").and_then(Value::as_str))
                        .map(selected_outcome)
                        .unwrap_or_else(cancelled_outcome)
                }
                Ok(ProviderUserInputResponse::Cancelled) | Err(_) => cancelled_outcome(),
            }
        }
    } else {
        let (sender, receiver) = oneshot::channel();
        send_event(
            events,
            ProviderEvent::Approval(ProviderApproval {
                title: bounded(title, MAX_NAME_BYTES),
                detail: tool_detail(tool_call)
                    .unwrap_or_else(|| "Kimi requested permission".into()),
                risk: tool_call
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_string(),
                responder: sender,
            }),
        )
        .await?;
        tokio::select! {
            _ = cancellation.cancelled() => cancelled_outcome(),
            decision = receiver => permission_outcome(
                decision.unwrap_or(ApprovalDecision::Deny),
                &options,
            ),
        }
    };
    let was_cancelled = outcome.get("outcome").and_then(Value::as_str) == Some("cancelled");
    process
        .send_json(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "outcome": outcome }
        }))
        .await?;
    Ok(was_cancelled && cancellation.is_cancelled())
}

fn is_user_question(title: &str, options: &[Value]) -> bool {
    title == "AskUserQuestion"
        && options.iter().any(|option| {
            option.get("kind").and_then(Value::as_str) == Some("allow_once")
                && option.get("name").and_then(Value::as_str).is_some()
        })
}

fn tool_call_question(tool_call: &Value) -> String {
    tool_call
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|item| {
            item.pointer("/content/text")
                .and_then(Value::as_str)
                .map(|text| bounded(text, MAX_DESCRIPTION_BYTES))
        })
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "Choose an answer".to_string())
}

fn permission_outcome(decision: ApprovalDecision, options: &[Value]) -> Value {
    let preferred_kinds: &[&str] = match decision {
        ApprovalDecision::Allow => &["allow_once", "allow_always"],
        ApprovalDecision::AllowSession => &["allow_always", "allow_once"],
        ApprovalDecision::Deny => &["reject_once", "reject_always"],
    };
    preferred_kinds
        .iter()
        .find_map(|kind| {
            options.iter().find(|option| {
                option.get("kind").and_then(Value::as_str) == Some(*kind)
                    && option.get("optionId").and_then(Value::as_str).is_some()
            })
        })
        .and_then(|option| option.get("optionId").and_then(Value::as_str))
        .map(selected_outcome)
        .unwrap_or_else(cancelled_outcome)
}

fn selected_outcome(option_id: &str) -> Value {
    json!({ "outcome": "selected", "optionId": option_id })
}

fn cancelled_outcome() -> Value {
    json!({ "outcome": "cancelled" })
}

fn content_text(content: &Value) -> Option<String> {
    (content.get("type").and_then(Value::as_str) == Some("text"))
        .then(|| content.get("text").and_then(Value::as_str))
        .flatten()
        .map(str::to_string)
}

fn tool_detail(tool: &Value) -> Option<String> {
    if let Some(raw_input) = tool.get("rawInput") {
        return serde_json::to_string(raw_input)
            .ok()
            .map(|value| bounded(&value, MAX_ACTIVITY_BYTES));
    }
    tool.get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.pointer("/content/text").and_then(Value::as_str))
        .next_back()
        .map(|text| bounded(text, MAX_ACTIVITY_BYTES))
}

fn push_bounded(target: &mut String, value: &str, max_bytes: usize) {
    if target.len() >= max_bytes {
        return;
    }
    let remaining = max_bytes - target.len();
    let mut end = value.len().min(remaining);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    target.push_str(&value[..end]);
}

fn bounded(value: &str, max_bytes: usize) -> String {
    let mut end = value.len().min(max_bytes);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
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

pub async fn model_catalog() -> Result<Vec<ProviderModelOption>, String> {
    let executable = find_executable("kimi")
        .ok_or_else(|| "Kimi Code is not installed or was not found on PATH".to_string())?;
    let args = vec![
        "provider".to_string(),
        "list".to_string(),
        "--json".to_string(),
    ];
    let command = platform_command(&executable, &args);
    let mut process = JsonProcess::spawn(command, "Kimi model catalog", MAX_CATALOG_STREAM).await?;
    process.close_stdin().await?;

    let result = timeout(CATALOG_TIMEOUT, async {
        let mut stdout = String::new();
        loop {
            match process.next_stdout().await? {
                ProcessOutput::Stdout(line) => {
                    if stdout.len().saturating_add(line.len()).saturating_add(1)
                        > MAX_CATALOG_STREAM
                    {
                        return Err("Kimi model catalog exceeded the output limit".to_string());
                    }
                    stdout.push_str(&line);
                    stdout.push('\n');
                }
                ProcessOutput::Exited(status) if status.success() => return Ok(stdout),
                ProcessOutput::Exited(_) => {
                    // The provider table may contain credentials. Never include provider
                    // stdout or stderr in errors, logs, or frontend-visible messages.
                    return Err("Kimi could not list its configured models".to_string());
                }
            }
        }
    })
    .await;

    let stdout = match result {
        Ok(result) => result?,
        Err(_) => {
            process.shutdown().await;
            return Err("Kimi model catalog timed out".to_string());
        }
    };
    let value: Value = serde_json::from_str(&stdout)
        .map_err(|error| format!("Kimi returned an invalid model catalog: {error}"))?;
    parse_model_catalog(&value)
}

fn parse_model_catalog(value: &Value) -> Result<Vec<ProviderModelOption>, String> {
    let models = value
        .get("models")
        .and_then(Value::as_object)
        .ok_or_else(|| "Kimi model catalog did not include a models table".to_string())?;
    if models.len() > MAX_MODELS {
        return Err(format!(
            "Kimi model catalog exceeded the {MAX_MODELS}-model limit"
        ));
    }

    let default_model = string_field(value, &["defaultModel", "default_model"], MAX_ID_BYTES);
    let mut options = Vec::with_capacity(models.len());
    for (alias, model) in models {
        let Some(id) = bounded_string(alias, MAX_ID_BYTES) else {
            continue;
        };
        let Some(model) = model.as_object() else {
            continue;
        };
        let name = field(model, &["displayName", "display_name"])
            .and_then(Value::as_str)
            .and_then(|value| bounded_string(value, MAX_NAME_BYTES))
            .unwrap_or_else(|| id.clone());
        let description = field(model, &["description"])
            .and_then(Value::as_str)
            .and_then(|value| bounded_string(value, MAX_DESCRIPTION_BYTES));
        let mut reasoning = field(model, &["supportEfforts", "support_efforts"])
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(parse_reasoning)
            .collect::<Vec<_>>();
        reasoning.dedup();
        let default_reasoning = field(model, &["defaultEffort", "default_effort"])
            .and_then(Value::as_str)
            .and_then(parse_reasoning);
        let is_default = default_model.as_deref() == Some(id.as_str())
            || field(model, &["isDefault", "is_default"])
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let context_length = field(model, &["maxContextSize", "max_context_size"])
            .and_then(Value::as_u64)
            .filter(|value| *value > 0);

        options.push(ProviderModelOption {
            id,
            name,
            description,
            is_default,
            reasoning,
            default_reasoning,
            speeds: vec![SpeedMode::Standard],
            default_speed: SpeedMode::Standard,
            context_length,
        });
    }

    if options.is_empty() {
        return Err("Kimi returned an empty model catalog".to_string());
    }
    if !options.iter().any(|option| option.is_default) {
        // Kimi 0.29's provider JSON omits `default_model`. Keep a native
        // "configured default" choice instead of guessing one of the aliases:
        // ProviderSessionConfig intentionally omits --model for this id.
        options.push(ProviderModelOption {
            id: "default".to_string(),
            name: "Kimi configured default".to_string(),
            description: Some("Use the default model selected by Kimi Code.".to_string()),
            is_default: true,
            reasoning: Vec::new(),
            default_reasoning: None,
            speeds: vec![SpeedMode::Standard],
            default_speed: SpeedMode::Standard,
            context_length: None,
        });
    }
    options.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(options)
}

fn field<'a>(object: &'a serde_json::Map<String, Value>, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| object.get(*name))
}

fn string_field(value: &Value, names: &[&str], max_bytes: usize) -> Option<String> {
    let object = value.as_object()?;
    field(object, names)
        .and_then(Value::as_str)
        .and_then(|value| bounded_string(value, max_bytes))
}

fn bounded_string(value: &str, max_bytes: usize) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.len() <= max_bytes && !value.chars().any(char::is_control))
        .then(|| value.to_string())
}

fn parse_reasoning(value: &str) -> Option<ReasoningEffort> {
    match value {
        "none" | "off" => Some(ReasoningEffort::None),
        "minimal" => Some(ReasoningEffort::Minimal),
        "low" => Some(ReasoningEffort::Low),
        "medium" => Some(ReasoningEffort::Medium),
        "high" => Some(ReasoningEffort::High),
        "xhigh" => Some(ReasoningEffort::Xhigh),
        "max" => Some(ReasoningEffort::Max),
        "ultra" => Some(ReasoningEffort::Ultra),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_MODELS, desired_mode, find_config_option, handle_session_update, is_user_question,
        parse_model_catalog, permission_outcome, select_option_contains, tool_call_question,
    };
    use crate::{
        model::{AccessMode, ApprovalDecision, InteractionMode, MessageKind, ReasoningEffort},
        providers::driver::ProviderEvent,
    };
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn parses_configured_aliases_capabilities_and_default() {
        let catalog = parse_model_catalog(&json!({
            "defaultModel": "managed/k3",
            "providers": {
                "managed": {
                    "apiKey": "must-not-be-read",
                    "oauth": { "token": "must-not-be-read" }
                }
            },
            "models": {
                "managed/basic": {
                    "model": "basic",
                    "displayName": "Basic",
                    "maxContextSize": 131072
                },
                "managed/k3": {
                    "model": "k3",
                    "displayName": "K3",
                    "description": "Configured Kimi model",
                    "maxContextSize": 262144,
                    "supportEfforts": ["low", "high", "max", "future-value"],
                    "defaultEffort": "high"
                }
            }
        }))
        .expect("catalog should parse");

        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].id, "managed/k3");
        assert!(catalog[0].is_default);
        assert_eq!(catalog[0].context_length, Some(262_144));
        assert_eq!(
            catalog[0].reasoning,
            [
                ReasoningEffort::Low,
                ReasoningEffort::High,
                ReasoningEffort::Max
            ]
        );
        assert_eq!(catalog[0].default_reasoning, Some(ReasoningEffort::High));
        assert_eq!(catalog[1].id, "managed/basic");
        assert!(!catalog[1].is_default);
        assert!(catalog[1].reasoning.is_empty());
    }

    #[test]
    fn accepts_snake_case_catalog_fields() {
        let catalog = parse_model_catalog(&json!({
            "default_model": "custom/model",
            "models": {
                "custom/model": {
                    "display_name": "Custom",
                    "max_context_size": 4096,
                    "support_efforts": ["off", "medium"],
                    "default_effort": "medium"
                }
            }
        }))
        .expect("catalog should parse");

        assert!(catalog[0].is_default);
        assert_eq!(
            catalog[0].reasoning,
            [ReasoningEffort::None, ReasoningEffort::Medium]
        );
        assert_eq!(catalog[0].context_length, Some(4096));
    }

    #[test]
    fn preserves_the_cli_configured_default_when_json_omits_its_alias() {
        let catalog = parse_model_catalog(&json!({
            "models": {
                "managed/fast": {
                    "displayName": "Fast",
                    "maxContextSize": 8192
                }
            }
        }))
        .expect("catalog should parse");

        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].id, "default");
        assert!(catalog[0].is_default);
        assert_eq!(catalog[1].id, "managed/fast");
    }

    #[test]
    fn rejects_unbounded_model_tables() {
        let mut models = serde_json::Map::new();
        for index in 0..=MAX_MODELS {
            models.insert(format!("model-{index}"), json!({}));
        }
        let error = parse_model_catalog(&json!({ "models": models }))
            .expect_err("oversized catalog should fail");
        assert!(error.contains("model limit"));
    }

    #[test]
    fn uses_only_provider_advertised_config_values() {
        let fixture = json!([
            {
                "type": "select",
                "id": "model",
                "category": "model",
                "currentValue": "kimi/default",
                "options": [
                    { "value": "kimi/default", "name": "Default" },
                    { "value": "kimi/fast", "name": "Fast" }
                ]
            },
            {
                "type": "select",
                "id": "thinking",
                "category": "thought_level",
                "currentValue": "low",
                "options": [
                    { "value": "low", "name": "Low" },
                    { "value": "high", "name": "High" }
                ]
            }
        ]);
        let options = fixture.as_array().unwrap();
        let model = find_config_option(options, "model", "model").unwrap();
        assert!(select_option_contains(&model["options"], "kimi/fast"));
        assert!(!select_option_contains(&model["options"], "invented"));
        assert_eq!(
            find_config_option(options, "thought_level", "thinking").unwrap()["id"],
            "thinking"
        );
    }

    #[test]
    fn maps_onyx_modes_to_verified_kimi_modes() {
        assert_eq!(
            desired_mode(InteractionMode::Plan, AccessMode::FullAccess),
            "plan"
        );
        assert_eq!(
            desired_mode(InteractionMode::Build, AccessMode::ApprovalRequired),
            "default"
        );
        assert_eq!(
            desired_mode(InteractionMode::Build, AccessMode::AutoAcceptEdits),
            "yolo"
        );
        assert_eq!(
            desired_mode(InteractionMode::Build, AccessMode::FullAccess),
            "auto"
        );
    }

    #[tokio::test]
    async fn translates_official_message_tool_and_usage_updates() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
        let mut thoughts = String::new();
        let mut titles = HashMap::new();
        handle_session_update(
            &json!({
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "hello" }
            }),
            &sender,
            &mut thoughts,
            &mut titles,
        )
        .await
        .unwrap();
        assert!(matches!(
            receiver.recv().await,
            Some(ProviderEvent::TextDelta(text)) if text == "hello"
        ));

        handle_session_update(
            &json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "tool-1",
                "title": "Read file",
                "kind": "read",
                "status": "pending",
                "rawInput": { "path": "README.md" }
            }),
            &sender,
            &mut thoughts,
            &mut titles,
        )
        .await
        .unwrap();
        assert!(matches!(
            receiver.recv().await,
            Some(ProviderEvent::Activity(activity))
                if activity.id.as_deref() == Some("tool-1")
                    && activity.title == "Read file"
                    && activity.kind == MessageKind::Tool
        ));

        handle_session_update(
            &json!({
                "sessionUpdate": "usage_update",
                "used": 2048,
                "size": 262144
            }),
            &sender,
            &mut thoughts,
            &mut titles,
        )
        .await
        .unwrap();
        assert!(matches!(
            receiver.recv().await,
            Some(ProviderEvent::ContextUsage(usage))
                if usage.used_tokens == 2048 && usage.max_tokens == Some(262144)
        ));
    }

    #[test]
    fn maps_permission_options_without_guessing_option_ids() {
        let options = json!([
            { "optionId": "allow-1", "name": "Allow", "kind": "allow_once" },
            { "optionId": "allow-session", "name": "Always", "kind": "allow_always" },
            { "optionId": "deny-1", "name": "Deny", "kind": "reject_once" }
        ]);
        let options = options.as_array().unwrap();
        assert_eq!(
            permission_outcome(ApprovalDecision::Allow, options)["optionId"],
            "allow-1"
        );
        assert_eq!(
            permission_outcome(ApprovalDecision::AllowSession, options)["optionId"],
            "allow-session"
        );
        assert_eq!(
            permission_outcome(ApprovalDecision::Deny, options)["optionId"],
            "deny-1"
        );
    }

    #[test]
    fn recognizes_verified_kimi_user_question_shape() {
        let options = json!([
            { "optionId": "q0_a", "name": "A", "kind": "allow_once" },
            { "optionId": "q0_b", "name": "B", "kind": "allow_once" },
            { "optionId": "q0_skip", "name": "Skip", "kind": "reject_once" }
        ]);
        assert!(is_user_question(
            "AskUserQuestion",
            options.as_array().unwrap()
        ));
        assert_eq!(
            tool_call_question(&json!({
                "content": [{
                    "type": "content",
                    "content": { "type": "text", "text": "Choose A or B" }
                }]
            })),
            "Choose A or B"
        );
    }
}
