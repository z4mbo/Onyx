use super::{
    cli::CliSession,
    driver::{
        DriverFuture, MAX_USER_INPUT_OPTIONS, MAX_USER_INPUT_QUESTIONS, ProviderActivity,
        ProviderApproval, ProviderDriver, ProviderEvent, ProviderSession, ProviderSessionConfig,
        ProviderUserInput, ProviderUserInputAnswers, ProviderUserInputOption,
        ProviderUserInputPrompt, ProviderUserInputQuestion, ProviderUserInputResponse,
        TURN_CANCELLED_ERROR,
    },
    process::{JsonProcess, ProcessOutput, find_executable, platform_command},
};
use crate::model::{
    AccessMode, ApprovalDecision, ContextUsage, InteractionMode, ProviderId, ProviderModelOption,
    ProviderUsage, ReasoningEffort, SpeedMode, UsageWindow,
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::mpsc,
    time::{Instant, timeout, timeout_at},
};
use tokio_util::sync::CancellationToken;

const MAX_PERSISTENT_STREAM: usize = 256 * 1024 * 1024;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(12);
const TURN_INTERRUPT_TIMEOUT: Duration = Duration::from_secs(12);

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
    config: ProviderSessionConfig,
}

impl CodexSession {
    async fn connect(mut config: ProviderSessionConfig) -> Result<Self, String> {
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

        let mut next_request_id = 2_u64;
        if config.model().is_none() {
            let request_id = next_request_id;
            next_request_id = next_request_id.saturating_add(1);
            process
                .send_json(&config_read_request(
                    request_id,
                    config.workspace.to_string_lossy().as_ref(),
                ))
                .await?;
            let response = timeout(STARTUP_TIMEOUT, wait_for_response(&mut process, request_id))
                .await
                .map_err(|_| "Codex configured model lookup timed out".to_string())??;
            config.model = configured_model(&response)?;
        }

        let request_id = next_request_id;
        next_request_id = next_request_id.saturating_add(1);
        let thread_request = thread_request(request_id, &config);
        process.send_json(&thread_request).await?;
        let thread_response = timeout(STARTUP_TIMEOUT, wait_for_response(&mut process, request_id))
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
            next_request_id,
            config,
        })
    }

    async fn run_turn_inner(
        &mut self,
        prompt: &str,
        cancellation: &CancellationToken,
        events: &mpsc::Sender<ProviderEvent>,
        mut steer: mpsc::UnboundedReceiver<String>,
    ) -> Result<(), String> {
        let request_id = self.take_request_id();
        self.process
            .send_json(&turn_start_request(
                request_id,
                &self.thread_id,
                prompt,
                &self.config,
            ))
            .await?;
        let mut turn_id: Option<String> = None;
        let mut streamed_items = HashSet::new();
        let mut pending_error = None;
        let mut steer_open = true;
        // `turn/steer` requires the active turn id, which only arrives with the
        // `turn/start` response; messages sent before that are queued here.
        let mut queued_steers: Vec<String> = Vec::new();
        let mut steer_requests: HashSet<u64> = HashSet::new();
        let mut cancellation_requested = false;
        let mut interrupt_request_id = None;
        let mut interrupt_deadline = None;
        let mut interrupt_error = None;

        loop {
            if cancellation_requested
                && interrupt_request_id.is_none()
                && let Some(turn) = turn_id.clone()
            {
                let id = self.take_request_id();
                let request = turn_interrupt_request(id, &self.thread_id, &turn);
                let deadline = *interrupt_deadline
                    .get_or_insert_with(|| Instant::now() + TURN_INTERRUPT_TIMEOUT);
                timeout_at(deadline, self.process.send_json(&request))
                    .await
                    .map_err(|_| "Codex turn interrupt timed out".to_string())?
                    .map_err(|error| format!("Codex could not interrupt the turn: {error}"))?;
                interrupt_request_id = Some(id);
            }

            enum TurnInput {
                Cancel,
                Steer(Option<String>),
                Output(ProcessOutput),
            }
            let input = if cancellation_requested {
                let deadline = *interrupt_deadline
                    .get_or_insert_with(|| Instant::now() + TURN_INTERRUPT_TIMEOUT);
                let output = timeout_at(deadline, self.process.next_stdout())
                    .await
                    .map_err(|_| interrupt_timeout_error(interrupt_error.as_deref()))??;
                TurnInput::Output(output)
            } else {
                tokio::select! {
                    _ = cancellation.cancelled() => TurnInput::Cancel,
                    message = steer.recv(), if steer_open => TurnInput::Steer(message),
                    output = self.process.next_stdout() => TurnInput::Output(output?),
                }
            };
            let value = match input {
                TurnInput::Cancel => {
                    mark_turn_cancelling(&mut cancellation_requested, &mut interrupt_deadline);
                    queued_steers.clear();
                    continue;
                }
                TurnInput::Steer(Some(text)) => {
                    match turn_id.as_deref() {
                        Some(turn) => {
                            let id = self.take_request_id();
                            steer_requests.insert(id);
                            let request = turn_steer_request(id, &self.thread_id, turn, &text);
                            self.process.send_json(&request).await?;
                        }
                        None => queued_steers.push(text),
                    }
                    continue;
                }
                TurnInput::Steer(None) => {
                    steer_open = false;
                    continue;
                }
                TurnInput::Output(ProcessOutput::Stdout(line)) => {
                    serde_json::from_str::<Value>(&line).map_err(|error| {
                        format!("Codex app-server returned invalid JSON: {error}")
                    })?
                }
                TurnInput::Output(ProcessOutput::Exited(status)) => {
                    let detail = self.process.stderr_tail();
                    return Err(if detail.is_empty() {
                        format!("Codex app-server exited with {status}")
                    } else {
                        format!("Codex app-server exited with {status}: {detail}")
                    });
                }
            };

            if let Some(interrupt_id) = interrupt_request_id
                && is_client_response(&value, interrupt_id)
            {
                if let Err(error) = response_result(&value) {
                    // The turn can win the completion race immediately before
                    // app-server handles the interrupt. Keep draining for the
                    // matching terminal notification before deciding whether
                    // the persistent transport is safe to retain.
                    interrupt_error = Some(error);
                }
                continue;
            }

            if let Some(steer_id) = steer_requests
                .iter()
                .copied()
                .find(|id| is_client_response(&value, *id))
            {
                steer_requests.remove(&steer_id);
                if let Err(error) = response_result(&value) {
                    send_event(
                        events,
                        ProviderEvent::Activity(ProviderActivity::error(format!(
                            "Codex could not apply the steering message: {error}"
                        ))),
                    )
                    .await?;
                }
                continue;
            }

            if is_client_response(&value, request_id) {
                let result = response_result(&value)?;
                let response_turn_id = result
                    .pointer("/turn/id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if response_turn_id.is_some() {
                    turn_id = response_turn_id;
                }
                if cancellation_requested {
                    queued_steers.clear();
                } else if let Some(turn) = turn_id.clone() {
                    for text in queued_steers.drain(..) {
                        let id = self.take_request_id();
                        steer_requests.insert(id);
                        let request = turn_steer_request(id, &self.thread_id, &turn, &text);
                        self.process.send_json(&request).await?;
                    }
                }
                continue;
            }

            let method = value.get("method").and_then(Value::as_str);
            if cancellation_requested && value.get("id").is_some() {
                // The native interrupt resolves outstanding server requests.
                // Do not synthesize answers to structured prompts while the
                // turn is being cancelled.
                continue;
            }
            match method {
                Some("turn/started") => {
                    turn_id = turn_id.or_else(|| {
                        value
                            .pointer("/params/turn/id")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    });
                }
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
                Some("item/commandExecution/outputDelta") => {
                    if let (Some(id), Some(delta)) = (
                        value.pointer("/params/itemId").and_then(Value::as_str),
                        value.pointer("/params/delta").and_then(Value::as_str),
                    ) {
                        send_event(
                            events,
                            ProviderEvent::ActivityDelta {
                                id: id.to_string(),
                                delta: delta.to_string(),
                            },
                        )
                        .await?;
                    }
                }
                Some("item/started") => {
                    if let Some(activity) = item_activity(&value, false) {
                        send_event(events, ProviderEvent::Activity(activity)).await?;
                    }
                }
                Some("thread/tokenUsage/updated") => {
                    if let Some(usage) = parse_context_usage(&value) {
                        send_event(events, ProviderEvent::ContextUsage(usage)).await?;
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
                    record_prompt_cancellation(
                        self.handle_approval(&value, method, cancellation, events)
                            .await?,
                        &mut cancellation_requested,
                        &mut interrupt_deadline,
                        &mut queued_steers,
                    );
                }
                Some("item/tool/requestUserInput") => {
                    record_prompt_cancellation(
                        self.handle_user_input(&value, cancellation, events).await?,
                        &mut cancellation_requested,
                        &mut interrupt_deadline,
                        &mut queued_steers,
                    );
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
                    if !turn_completion_matches(&value, &self.thread_id, turn_id.as_deref()) {
                        continue;
                    }
                    return completed_turn_result(
                        completed_turn,
                        pending_error.take(),
                        cancellation_requested,
                    );
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
    ) -> Result<bool, String> {
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
        let decision = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Ok(true),
            result = receiver => result.unwrap_or(ApprovalDecision::Deny),
        };
        if cancellation.is_cancelled() {
            return Ok(true);
        }
        self.process
            .send_json(&approval_response(method, response_id, decision, &params))
            .await?;
        Ok(false)
    }

    async fn handle_user_input(
        &mut self,
        value: &Value,
        cancellation: &CancellationToken,
        events: &mpsc::Sender<ProviderEvent>,
    ) -> Result<bool, String> {
        let prompt = codex_user_input_prompt(value)?;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        send_event(
            events,
            ProviderEvent::UserInput(ProviderUserInput {
                prompt,
                responder: sender,
            }),
        )
        .await?;
        let response = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Ok(true),
            result = receiver => result.unwrap_or(ProviderUserInputResponse::Cancelled),
        };
        match response {
            ProviderUserInputResponse::Answered(answers) => {
                if cancellation.is_cancelled() {
                    return Ok(true);
                }
                self.respond_to_server_request(value, codex_user_input_response(&answers))
                    .await?;
                Ok(false)
            }
            ProviderUserInputResponse::Cancelled => Ok(true),
        }
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
                    "message": format!("Onyx does not implement Codex server request {method}")
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

    fn supports_steering(&self) -> bool {
        true
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
        Box::pin(async move {
            send_event(&events, ProviderEvent::Continuation(self.thread_id.clone())).await?;
            self.run_turn_inner(prompt, cancellation, &events, steer)
                .await
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
            "clientInfo": { "name": "onyx", "title": "Onyx", "version": env!("CARGO_PKG_VERSION") },
            "capabilities": { "experimentalApi": true }
        }
    })
}

fn config_read_request(id: u64, cwd: &str) -> Value {
    json!({
        "id": id,
        "method": "config/read",
        "params": {
            "cwd": cwd,
            "includeLayers": false
        }
    })
}

fn configured_model(response: &Value) -> Result<Option<String>, String> {
    let result = response_result(response)?;
    Ok(result
        .pointer("/config/model")
        .and_then(Value::as_str)
        .filter(|model| !model.trim().is_empty())
        .map(str::to_string))
}

fn thread_request(id: u64, config: &ProviderSessionConfig) -> Value {
    let mut params = json!({
        "cwd": config.workspace,
        "approvalPolicy": config.approval_policy(),
        "sandbox": config.sandbox_name()
    });
    if let Some(model) = config.model() {
        params["model"] = json!(model);
    }
    if let Some(service_tier) = config.speed_mode.as_service_tier() {
        params["serviceTier"] = json!(service_tier);
    }
    if let Some(thread_id) = config.continuation.as_deref() {
        params["threadId"] = json!(thread_id);
        json!({ "id": id, "method": "thread/resume", "params": params })
    } else {
        json!({ "id": id, "method": "thread/start", "params": params })
    }
}

fn turn_start_request(
    id: u64,
    thread_id: &str,
    prompt: &str,
    config: &ProviderSessionConfig,
) -> Value {
    let mut params = json!({
        "threadId": thread_id,
        "input": [{ "type": "text", "text": prompt }],
        "approvalPolicy": config.approval_policy(),
        "sandboxPolicy": match config.access_mode {
            AccessMode::ApprovalRequired | AccessMode::AutoAcceptEdits => {
                json!({ "type": "workspaceWrite" })
            }
            AccessMode::FullAccess => json!({ "type": "dangerFullAccess" }),
        }
    });
    if let Some(model) = config.model() {
        params["model"] = json!(model);
    }
    if let Some(reasoning) = config.reasoning {
        params["effort"] = json!(reasoning.clamped_str());
    }
    if let Some(service_tier) = config.speed_mode.as_service_tier() {
        params["serviceTier"] = json!(service_tier);
    }
    let mut collaboration_settings = serde_json::Map::new();
    collaboration_settings.insert("developer_instructions".to_string(), Value::Null);
    if let Some(model) = config.model() {
        collaboration_settings.insert("model".to_string(), json!(model));
    }
    if let Some(reasoning) = config.reasoning {
        collaboration_settings.insert(
            "reasoning_effort".to_string(),
            json!(reasoning.clamped_str()),
        );
    }
    if collaboration_settings.contains_key("model") {
        params["collaborationMode"] = json!({
            "mode": if config.interaction_mode == InteractionMode::Plan { "plan" } else { "default" },
            // A null developer-instructions field explicitly asks app-server for
            // Codex's own built-in mode behavior instead of an Onyx-authored prompt.
            "settings": Value::Object(collaboration_settings)
        });
    }
    json!({
        "id": id,
        "method": "turn/start",
        "params": params
    })
}

fn turn_steer_request(id: u64, thread_id: &str, turn_id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "method": "turn/steer",
        "params": {
            "threadId": thread_id,
            "expectedTurnId": turn_id,
            "input": [{ "type": "text", "text": text }]
        }
    })
}

fn turn_interrupt_request(id: u64, thread_id: &str, turn_id: &str) -> Value {
    json!({
        "id": id,
        "method": "turn/interrupt",
        "params": {
            "threadId": thread_id,
            "turnId": turn_id
        }
    })
}

fn mark_turn_cancelling(cancelling: &mut bool, deadline: &mut Option<Instant>) {
    if !*cancelling {
        *cancelling = true;
        *deadline = Some(Instant::now() + TURN_INTERRUPT_TIMEOUT);
    }
}

fn record_prompt_cancellation(
    cancelled: bool,
    cancelling: &mut bool,
    deadline: &mut Option<Instant>,
    queued_steers: &mut Vec<String>,
) {
    if cancelled {
        mark_turn_cancelling(cancelling, deadline);
        queued_steers.clear();
    }
}

fn interrupt_timeout_error(interrupt_error: Option<&str>) -> String {
    match interrupt_error {
        Some(error) => {
            format!("Codex rejected the turn interrupt and did not reach a terminal state: {error}")
        }
        None => "Codex turn interrupt timed out".to_string(),
    }
}

fn turn_completion_matches(value: &Value, thread_id: &str, turn_id: Option<&str>) -> bool {
    if let Some(completed_thread) = value.pointer("/params/threadId").and_then(Value::as_str)
        && completed_thread != thread_id
    {
        return false;
    }
    match turn_id {
        Some(expected) => {
            value.pointer("/params/turn/id").and_then(Value::as_str) == Some(expected)
        }
        None => true,
    }
}

fn completed_turn_result(
    turn: &Value,
    pending_error: Option<String>,
    cancellation_requested: bool,
) -> Result<(), String> {
    match turn.get("status").and_then(Value::as_str) {
        Some("completed") if cancellation_requested => Err(TURN_CANCELLED_ERROR.to_string()),
        Some("completed") => Ok(()),
        Some("interrupted") => Err(TURN_CANCELLED_ERROR.to_string()),
        Some(status) => Err(turn
            .pointer("/error/message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or(pending_error)
            .unwrap_or_else(|| format!("Codex turn ended with status {status}"))),
        None => {
            Err(pending_error
                .unwrap_or_else(|| "Codex turn completed without a status".to_string()))
        }
    }
}

fn parse_context_usage(value: &Value) -> Option<ContextUsage> {
    let usage = value.pointer("/params/tokenUsage")?;
    let last = usage.get("last")?;
    Some(ContextUsage {
        used_tokens: last.get("totalTokens")?.as_u64()?,
        max_tokens: usage.get("modelContextWindow").and_then(Value::as_u64),
        input_tokens: last.get("inputTokens").and_then(Value::as_u64),
        cached_input_tokens: last.get("cachedInputTokens").and_then(Value::as_u64),
        output_tokens: last.get("outputTokens").and_then(Value::as_u64),
        reasoning_output_tokens: last.get("reasoningOutputTokens").and_then(Value::as_u64),
    })
}

fn parse_reasoning(value: &str) -> Option<ReasoningEffort> {
    match value {
        "none" => Some(ReasoningEffort::None),
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

fn advertised_reasoning(model: &Value) -> Vec<ReasoningEffort> {
    let mut efforts = Vec::new();
    for effort in model
        .get("supportedReasoningEfforts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            entry
                .get("reasoningEffort")
                .and_then(Value::as_str)
                .and_then(parse_reasoning)
        })
    {
        if !efforts.contains(&effort) {
            efforts.push(effort);
        }
    }
    efforts
}

fn configured_default_model_option() -> ProviderModelOption {
    ProviderModelOption {
        id: "default".to_string(),
        name: "Codex CLI configured default".to_string(),
        description: Some(
            "Use the model and defaults from the active Codex configuration.".to_string(),
        ),
        is_default: true,
        reasoning: Vec::new(),
        default_reasoning: None,
        speeds: vec![SpeedMode::Standard],
        default_speed: SpeedMode::Standard,
        context_length: None,
    }
}

async fn connect_probe() -> Result<JsonProcess, String> {
    let executable = find_executable("codex")
        .ok_or_else(|| "Codex is not installed or was not found on PATH".to_string())?;
    let args = vec!["app-server".to_string(), "--stdio".to_string()];
    let command = platform_command(&executable, &args);
    let mut process =
        JsonProcess::spawn(command, "Codex app-server probe", 32 * 1024 * 1024).await?;
    process.send_json(&initialize_request(1)).await?;
    let initialized = timeout(STARTUP_TIMEOUT, wait_for_response(&mut process, 1))
        .await
        .map_err(|_| "Codex app-server initialization timed out".to_string())??;
    response_result(&initialized)?;
    process
        .send_json(&json!({ "method": "initialized" }))
        .await?;
    Ok(process)
}

pub async fn model_catalog() -> Result<Vec<ProviderModelOption>, String> {
    let mut process = connect_probe().await?;
    let mut id = 2_u64;
    let mut cursor: Option<String> = None;
    let mut models = Vec::new();
    loop {
        let params = cursor
            .as_ref()
            .map_or_else(|| json!({}), |cursor| json!({ "cursor": cursor }));
        process
            .send_json(&json!({ "id": id, "method": "model/list", "params": params }))
            .await?;
        let response = timeout(STARTUP_TIMEOUT, wait_for_response(&mut process, id))
            .await
            .map_err(|_| "Codex model catalog timed out".to_string())??;
        let result = response_result(&response)?;
        for model in result
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if model
                .get("hidden")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                continue;
            }
            let Some(model_id) = model.get("model").and_then(Value::as_str) else {
                continue;
            };
            let reasoning = advertised_reasoning(model);
            let default_reasoning = model
                .get("defaultReasoningEffort")
                .and_then(Value::as_str)
                .and_then(parse_reasoning);
            let has_fast = model
                .get("serviceTiers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|tier| tier.get("id").and_then(Value::as_str) == Some("fast"))
                || model
                    .get("additionalSpeedTiers")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|tier| tier.as_str() == Some("fast"));
            models.push(ProviderModelOption {
                id: model_id.to_string(),
                name: model
                    .get("displayName")
                    .and_then(Value::as_str)
                    .unwrap_or(model_id)
                    .to_string(),
                description: model
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                // The explicit "configured default" entry is the only automatic
                // selection. Advertised models remain available as explicit choices.
                is_default: false,
                reasoning,
                default_reasoning,
                speeds: if has_fast {
                    vec![SpeedMode::Standard, SpeedMode::Fast]
                } else {
                    vec![SpeedMode::Standard]
                },
                default_speed: if model.get("defaultServiceTier").and_then(Value::as_str)
                    == Some("fast")
                {
                    SpeedMode::Fast
                } else {
                    SpeedMode::Standard
                },
                context_length: None,
            });
        }
        cursor = result
            .get("nextCursor")
            .and_then(Value::as_str)
            .map(str::to_string);
        if cursor.is_none() {
            break;
        }
        id = id.saturating_add(1);
    }
    process.shutdown().await;
    if models.is_empty() {
        Err("Codex returned an empty model catalog".into())
    } else {
        models.insert(0, configured_default_model_option());
        Ok(models)
    }
}

pub async fn account_usage() -> Result<ProviderUsage, String> {
    let mut process = connect_probe().await?;
    process
        .send_json(&json!({ "id": 2, "method": "account/rateLimits/read", "params": {} }))
        .await?;
    let response = timeout(STARTUP_TIMEOUT, wait_for_response(&mut process, 2))
        .await
        .map_err(|_| "Codex usage limits timed out".to_string())??;
    let result = response_result(&response)?;
    let limits = result.get("rateLimits").unwrap_or(&Value::Null);
    let mut windows = Vec::new();
    for (key, label) in [("primary", "5 hour"), ("secondary", "Weekly")] {
        let Some(window) = limits.get(key).filter(|value| value.is_object()) else {
            continue;
        };
        windows.push(UsageWindow {
            label: label.into(),
            used_percent: window
                .get("usedPercent")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            window_minutes: window.get("windowDurationMins").and_then(Value::as_u64),
            resets_at: window.get("resetsAt").and_then(Value::as_i64),
        });
    }
    let usage = ProviderUsage {
        provider: ProviderId::Codex,
        plan: limits
            .get("planType")
            .and_then(Value::as_str)
            .map(str::to_string),
        windows,
        updated_at: chrono::Utc::now(),
    };
    process.shutdown().await;
    Ok(usage)
}

fn approval_response(method: &str, id: Value, decision: ApprovalDecision, params: &Value) -> Value {
    let approved = decision.allowed();
    let result = match method {
        "applyPatchApproval" | "execCommandApproval" => json!({
            "decision": match decision {
                ApprovalDecision::AllowSession => json!("approved_for_session"),
                ApprovalDecision::Allow => json!("approved"),
                ApprovalDecision::Deny => {
                    json!({ "denied": { "rejection": "User denied the request in Onyx" } })
                }
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

fn codex_user_input_prompt(value: &Value) -> Result<ProviderUserInputPrompt, String> {
    let params = value
        .get("params")
        .ok_or_else(|| "Codex user-input request did not include params".to_string())?;
    let raw_questions = params
        .get("questions")
        .and_then(Value::as_array)
        .ok_or_else(|| "Codex user-input request did not include questions".to_string())?;
    if raw_questions.len() > MAX_USER_INPUT_QUESTIONS {
        return Err(format!(
            "Codex user-input request exceeded the {MAX_USER_INPUT_QUESTIONS}-question limit"
        ));
    }
    let mut questions = Vec::with_capacity(raw_questions.len());
    for raw in raw_questions {
        let raw_options = match raw.get("options") {
            Some(Value::Array(options)) => options.as_slice(),
            Some(Value::Null) | None => &[],
            Some(_) => {
                return Err("Codex user-input options were not an array".to_string());
            }
        };
        if raw_options.len() > MAX_USER_INPUT_OPTIONS {
            return Err(format!(
                "Codex user-input question exceeded the {MAX_USER_INPUT_OPTIONS}-option limit"
            ));
        }
        let options = raw_options
            .iter()
            .map(|option| {
                Ok(ProviderUserInputOption {
                    label: required_field(option, "label", "Codex user-input option label")?
                        .to_string(),
                    description: option
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        questions.push(ProviderUserInputQuestion {
            id: required_field(raw, "id", "Codex user-input question id")?.to_string(),
            header: required_field(raw, "header", "Codex user-input question header")?.to_string(),
            question: required_field(raw, "question", "Codex user-input question text")?
                .to_string(),
            allow_other: raw
                .get("isOther")
                .and_then(Value::as_bool)
                .unwrap_or(options.is_empty()),
            secret: raw
                .get("isSecret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            options,
            multi_select: false,
        });
    }
    ProviderUserInputPrompt::new(
        "Codex needs your input",
        questions,
        params.get("autoResolutionMs").and_then(Value::as_u64),
    )
}

fn codex_user_input_response(answers: &ProviderUserInputAnswers) -> Value {
    json!({
        "answers": answers
            .iter()
            .map(|(id, values)| (id.clone(), json!({ "answers": values })))
            .collect::<serde_json::Map<String, Value>>()
    })
}

fn required_field<'a>(value: &'a Value, field: &str, label: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{label} is missing"))
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
    let id = item.get("id").and_then(Value::as_str).map(str::to_owned);
    match item.get("type")?.as_str()? {
        "commandExecution" => {
            let command = item
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("command");
            let mut activity = ProviderActivity::tool(
                if completed {
                    format!("Ran {command}")
                } else {
                    format!("Running {command}")
                },
                completed
                    .then(|| item.get("aggregatedOutput"))
                    .flatten()
                    .and_then(value_text),
            );
            activity.id = id;
            Some(activity)
        }
        "fileChange" if completed => {
            let mut activity = ProviderActivity::tool(
                "Applied file changes",
                item.get("changes").and_then(value_text),
            );
            activity.id = id;
            Some(activity)
        }
        "mcpToolCall" => {
            let tool = item
                .get("tool")
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("MCP tool");
            let mut activity = ProviderActivity::tool(
                if completed {
                    format!("Completed {tool}")
                } else {
                    format!("Running {tool}")
                },
                None,
            );
            activity.id = id;
            Some(activity)
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
        advertised_reasoning, approval_response, codex_user_input_prompt,
        codex_user_input_response, completed_turn_result, config_read_request,
        configured_default_model_option, configured_model, initialize_request,
        interrupt_timeout_error, is_client_response, item_activity, parse_reasoning,
        thread_request, turn_completion_matches, turn_interrupt_request, turn_start_request,
        turn_steer_request,
    };
    use crate::{
        model::{
            AccessMode, ApprovalDecision, InteractionMode, ProviderId, ReasoningEffort, SpeedMode,
        },
        providers::driver::{ProviderSessionConfig, ProviderUserInputAnswers},
    };
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
            reasoning: Some(ReasoningEffort::High),
            speed_mode: SpeedMode::Fast,
            interaction_mode: InteractionMode::Plan,
            access_mode: AccessMode::FullAccess,
        };
        let request = thread_request(2, &config);
        assert_eq!(request["method"], "thread/resume");
        assert_eq!(request["params"]["model"], "gpt-5.4");
        assert_eq!(request["params"]["approvalPolicy"], "never");
        assert_eq!(request["params"]["sandbox"], "danger-full-access");
        assert_eq!(request["params"]["serviceTier"], "fast");
        let turn = turn_start_request(3, "thread-1", "hello", &config);
        assert_eq!(turn["params"]["input"][0]["type"], "text");
        assert_eq!(turn["params"]["model"], "gpt-5.4");
        assert_eq!(turn["params"]["effort"], "high");
        assert_eq!(turn["params"]["serviceTier"], "fast");
        assert_eq!(turn["params"]["approvalPolicy"], "never");
        assert_eq!(turn["params"]["sandboxPolicy"]["type"], "dangerFullAccess");
        assert_eq!(turn["params"]["collaborationMode"]["mode"], "plan");
        assert_eq!(
            turn["params"]["collaborationMode"]["settings"]["reasoning_effort"],
            "high"
        );
        assert!(
            turn["params"]["collaborationMode"]["settings"]["developer_instructions"].is_null()
        );
    }

    #[test]
    fn ask_mode_keeps_workspace_writes_and_omits_invalid_collaboration_settings() {
        let config = ProviderSessionConfig {
            provider: ProviderId::Codex,
            model: None,
            workspace: PathBuf::from("/tmp/project"),
            continuation: None,
            reasoning: None,
            speed_mode: SpeedMode::Standard,
            interaction_mode: InteractionMode::Build,
            access_mode: AccessMode::ApprovalRequired,
        };
        let turn = turn_start_request(3, "thread-1", "edit the file", &config);
        assert_eq!(turn["params"]["approvalPolicy"], "untrusted");
        assert_eq!(turn["params"]["sandboxPolicy"]["type"], "workspaceWrite");
        assert!(turn["params"].get("collaborationMode").is_none());
    }

    #[test]
    fn configured_default_is_explicit_and_uses_config_read() {
        let request = config_read_request(2, "/tmp/project");
        assert_eq!(request["method"], "config/read");
        assert_eq!(request["params"]["cwd"], "/tmp/project");
        assert_eq!(request["params"]["includeLayers"], false);
        assert_eq!(
            configured_model(&json!({
                "id": 2,
                "result": {"config": {"model": "gpt-5.6-sol"}}
            }))
            .unwrap()
            .as_deref(),
            Some("gpt-5.6-sol")
        );

        let option = configured_default_model_option();
        assert_eq!(option.id, "default");
        assert!(option.is_default);
        assert!(option.reasoning.is_empty());
    }

    #[test]
    fn native_ultra_effort_is_distinct_and_catalog_efforts_are_deduplicated() {
        assert_eq!(parse_reasoning("max"), Some(ReasoningEffort::Max));
        assert_eq!(parse_reasoning("ultra"), Some(ReasoningEffort::Ultra));
        assert_eq!(ReasoningEffort::Ultra.clamped_str(), "ultra");

        let efforts = advertised_reasoning(&json!({
            "supportedReasoningEfforts": [
                {"reasoningEffort": "max"},
                {"reasoningEffort": "ultra"},
                {"reasoningEffort": "ultra"}
            ]
        }));
        assert_eq!(efforts, vec![ReasoningEffort::Max, ReasoningEffort::Ultra]);
    }

    #[test]
    fn steer_request_targets_the_active_turn() {
        let request = turn_steer_request(7, "thread-1", "turn-9", "focus on the tests");
        assert_eq!(request["method"], "turn/steer");
        assert_eq!(request["params"]["threadId"], "thread-1");
        assert_eq!(request["params"]["expectedTurnId"], "turn-9");
        assert_eq!(request["params"]["input"][0]["type"], "text");
        assert_eq!(request["params"]["input"][0]["text"], "focus on the tests");
    }

    #[test]
    fn interrupt_request_uses_the_official_active_turn_shape() {
        let request = turn_interrupt_request(8, "thread-1", "turn-9");
        assert_eq!(
            request,
            json!({
                "id": 8,
                "method": "turn/interrupt",
                "params": {
                    "threadId": "thread-1",
                    "turnId": "turn-9"
                }
            })
        );
    }

    #[test]
    fn interrupted_and_cancel_raced_terminal_turns_surface_as_cancelled() {
        assert_eq!(
            completed_turn_result(&json!({"status": "interrupted"}), None, true),
            Err("Turn cancelled".to_string())
        );
        assert_eq!(
            completed_turn_result(&json!({"status": "completed"}), None, true),
            Err("Turn cancelled".to_string())
        );
        assert_eq!(
            completed_turn_result(&json!({"status": "completed"}), None, false),
            Ok(())
        );
        assert_eq!(
            completed_turn_result(
                &json!({"status": "interrupted"}),
                Some("interrupt raced completion".to_string()),
                true
            ),
            Err("Turn cancelled".to_string())
        );
    }

    #[test]
    fn interrupt_terminal_notifications_are_scoped_to_thread_and_turn() {
        let completed = json!({
            "params": {
                "threadId": "thread-1",
                "turn": {"id": "turn-9", "status": "interrupted"}
            }
        });
        assert!(turn_completion_matches(
            &completed,
            "thread-1",
            Some("turn-9")
        ));
        assert!(!turn_completion_matches(
            &completed,
            "thread-2",
            Some("turn-9")
        ));
        assert!(!turn_completion_matches(
            &completed,
            "thread-1",
            Some("turn-8")
        ));
    }

    #[test]
    fn interrupt_rejection_is_not_reported_as_a_safe_cancellation_without_terminal_state() {
        let error = interrupt_timeout_error(Some("turn is no longer active"));
        assert!(error.contains("turn is no longer active"));
        assert_ne!(error, "Turn cancelled");
    }

    #[test]
    fn approval_response_preserves_server_id() {
        assert_eq!(
            approval_response(
                "item/commandExecution/requestApproval",
                json!("approval-7"),
                ApprovalDecision::Allow,
                &json!({})
            ),
            json!({"id":"approval-7","result":{"decision":"accept"}})
        );
        assert_eq!(
            approval_response(
                "item/fileChange/requestApproval",
                json!(8),
                ApprovalDecision::Deny,
                &json!({})
            )["result"]["decision"],
            "decline"
        );
    }

    #[test]
    fn legacy_approval_denial_matches_current_review_decision_schema() {
        let response = approval_response(
            "execCommandApproval",
            json!(3),
            ApprovalDecision::Deny,
            &json!({}),
        );
        assert_eq!(
            response["result"]["decision"],
            json!({ "denied": { "rejection": "User denied the request in Onyx" } })
        );
        assert_eq!(
            approval_response(
                "applyPatchApproval",
                json!(4),
                ApprovalDecision::Allow,
                &json!({})
            )["result"]["decision"],
            "approved"
        );
        assert_eq!(
            approval_response(
                "execCommandApproval",
                json!(5),
                ApprovalDecision::AllowSession,
                &json!({})
            )["result"]["decision"],
            "approved_for_session"
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
            ApprovalDecision::Allow,
            &json!({"permissions": permissions}),
        );
        assert_eq!(response["result"]["scope"], "turn");
        assert_eq!(response["result"]["permissions"], permissions);
    }

    #[test]
    fn mcp_elicitation_response_includes_required_metadata() {
        let response = approval_response(
            "mcpServer/elicitation/request",
            json!(10),
            ApprovalDecision::Allow,
            &json!({}),
        );
        assert_eq!(response["result"]["action"], "accept");
        assert!(response["result"]["content"].is_null());
        assert!(response["result"]["_meta"].is_null());
    }

    #[test]
    fn command_notification_becomes_canonical_activity() {
        let event = json!({
            "method": "item/completed",
            "params": {"item": {
                "id": "command-1",
                "type": "commandExecution",
                "command": "cargo test",
                "aggregatedOutput": "ok"
            }}
        });
        let activity = item_activity(&event, true).expect("activity");
        assert_eq!(activity.title, "Ran cargo test");
        assert_eq!(activity.id.as_deref(), Some("command-1"));
        assert_eq!(activity.detail.as_deref(), Some("ok"));
    }

    #[test]
    fn user_input_request_preserves_questions_and_encodes_answers() {
        let request = json!({
            "id": 12,
            "method": "item/tool/requestUserInput",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "item-1",
                "questions": [{
                    "id": "scope",
                    "header": "Scope",
                    "question": "Which scope?",
                    "isOther": true,
                    "isSecret": false,
                    "options": [
                        {"label": "Small", "description": "One module"},
                        {"label": "Large", "description": "Whole app"}
                    ]
                }]
            }
        });
        let prompt = codex_user_input_prompt(&request).expect("valid request");
        assert_eq!(prompt.questions[0].id, "scope");
        assert!(prompt.questions[0].allow_other);

        let answers =
            ProviderUserInputAnswers::from([("scope".to_string(), vec!["Small".to_string()])]);
        assert_eq!(
            codex_user_input_response(&answers),
            json!({"answers":{"scope":{"answers":["Small"]}}})
        );
    }

    #[test]
    fn malformed_user_input_request_is_rejected_instead_of_answered_empty() {
        assert!(
            codex_user_input_prompt(&json!({
                "id": 13,
                "method": "item/tool/requestUserInput",
                "params": {"questions":[]}
            }))
            .is_err()
        );
    }
}
