//! OpenCode driver: one `opencode serve` HTTP server per session plus the
//! global `/event` SSE stream. Turns are fired with `session/prompt_async` and
//! completion is inferred from `session.status` reaching `idle`, mirroring how
//! T3 Code drives OpenCode.

use super::{
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
    AccessMode, ApprovalDecision, ContextUsage, MessageKind, ProviderId, ProviderModelOption,
    SpeedMode,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot},
    time::{Instant, timeout, timeout_at},
};
use tokio_util::sync::CancellationToken;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const ABORT_TIMEOUT: Duration = Duration::from_secs(12);
const CATALOG_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_SERVE_STREAM: usize = 32 * 1024 * 1024;
const MAX_CATALOG_STREAM: usize = 8 * 1024 * 1024;
const MAX_SSE_EVENT: usize = 8 * 1024 * 1024;
const MAX_HTTP_BODY: usize = 8 * 1024 * 1024;
const EVENT_CHANNEL_CAPACITY: usize = 256;
const MAX_TRACKED_PARTS: usize = 8_192;
const MAX_ACTIVITY_BYTES: usize = 64 * 1024;
const MAX_MODELS: usize = 4_096;
const MAX_SLUG_BYTES: usize = 512;

/// Oldest OpenCode release with the `prompt_async` + permission ruleset API
/// this driver depends on.
const MIN_VERSION: (u64, u64, u64) = (1, 14, 19);

pub struct OpencodeDriver;

impl ProviderDriver for OpencodeDriver {
    fn provider(&self) -> ProviderId {
        ProviderId::Opencode
    }

    fn connect<'a>(
        &'a self,
        config: ProviderSessionConfig,
    ) -> DriverFuture<'a, Result<Box<dyn ProviderSession>, String>> {
        Box::pin(async move {
            let session = OpencodeSession::connect(config).await?;
            Ok(Box::new(session) as Box<dyn ProviderSession>)
        })
    }
}

struct OpencodeSession {
    process: JsonProcess,
    client: reqwest::Client,
    base_url: String,
    directory: String,
    session_id: String,
    events: mpsc::Receiver<Result<Value, String>>,
    sse_task: tokio::task::JoinHandle<()>,
    config: ProviderSessionConfig,
}

impl Drop for OpencodeSession {
    fn drop(&mut self) {
        self.sse_task.abort();
    }
}

/// Shared-only view of the session's HTTP transport. The turn loop's futures
/// hold `&self` of THIS type across awaits instead of `&OpencodeSession`,
/// which keeps them `Send` on Windows where the child-process handle inside
/// `JsonProcess` is not `Sync`.
#[derive(Clone, Copy)]
struct SessionHttp<'a> {
    client: &'a reqwest::Client,
    base_url: &'a str,
    directory: &'a str,
    session_id: &'a str,
    config: &'a ProviderSessionConfig,
}

impl SessionHttp<'_> {
    fn session_url(&self, suffix: &str) -> String {
        format!("{}/session/{}{suffix}", self.base_url, self.session_id)
    }

    async fn post_prompt(&self, text: &str) -> Result<(), String> {
        let body = prompt_body(text, self.config.model());
        http_json(
            self.client
                .post(self.session_url("/prompt_async"))
                .query(&[("directory", self.directory)])
                .json(&body),
            "OpenCode prompt",
        )
        .await
        .map(|_| ())
    }

    /// Ask the server to interrupt the running turn, bounded by the shared
    /// abort deadline so a wedged server cannot stall cancellation forever.
    async fn abort_turn(&self, deadline: Instant) -> Result<(), String> {
        timeout_at(
            deadline,
            http_json(
                self.client
                    .post(self.session_url("/abort"))
                    .query(&[("directory", self.directory)]),
                "OpenCode abort",
            ),
        )
        .await
        .map_err(|_| "OpenCode turn interrupt timed out".to_string())?
        .map_err(|error| format!("OpenCode could not abort the turn: {error}"))?;
        Ok(())
    }

    async fn handle_permission(
        &self,
        properties: &Value,
        cancelling: bool,
        cancellation: &CancellationToken,
        events: &mpsc::Sender<ProviderEvent>,
    ) -> Result<bool, String> {
        let request_id = properties
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| "OpenCode permission request did not include an id".to_string())?
            .to_string();
        if cancelling {
            // The turn is being interrupted; do not surface new prompts.
            self.reply_permission(&request_id, ApprovalDecision::Deny)
                .await?;
            return Ok(true);
        }
        let (title, detail, risk) = permission_request_parts(properties);
        let (sender, receiver) = oneshot::channel();
        send_event(
            events,
            ProviderEvent::Approval(ProviderApproval {
                title,
                detail,
                risk,
                responder: sender,
            }),
        )
        .await?;
        let decision = tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                self.reply_permission(&request_id, ApprovalDecision::Deny).await?;
                return Ok(true);
            }
            result = receiver => result.unwrap_or(ApprovalDecision::Deny),
        };
        self.reply_permission(&request_id, decision).await?;
        Ok(cancellation.is_cancelled())
    }

    async fn reply_permission(
        &self,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), String> {
        http_json(
            self.client
                .post(format!("{}/permission/{request_id}/reply", self.base_url))
                .query(&[("directory", self.directory)])
                .json(&permission_reply_body(decision)),
            "OpenCode permission reply",
        )
        .await
        .map(|_| ())
    }

    async fn handle_question(
        &self,
        properties: &Value,
        cancelling: bool,
        cancellation: &CancellationToken,
        events: &mpsc::Sender<ProviderEvent>,
    ) -> Result<bool, String> {
        let request_id = properties
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| "OpenCode question request did not include an id".to_string())?
            .to_string();
        if cancelling {
            self.reject_question(&request_id).await?;
            return Ok(true);
        }
        let prompt = question_prompt(properties)?;
        let question_ids = prompt
            .questions
            .iter()
            .map(|question| question.id.clone())
            .collect::<Vec<_>>();
        let (sender, receiver) = oneshot::channel();
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
            _ = cancellation.cancelled() => {
                self.reject_question(&request_id).await?;
                return Ok(true);
            }
            result = receiver => result.unwrap_or(ProviderUserInputResponse::Cancelled),
        };
        match response {
            ProviderUserInputResponse::Answered(answers) => {
                if cancellation.is_cancelled() {
                    self.reject_question(&request_id).await?;
                    return Ok(true);
                }
                http_json(
                    self.client
                        .post(format!("{}/question/{request_id}/reply", self.base_url))
                        .query(&[("directory", self.directory)])
                        .json(&question_answers_body(&question_ids, &answers)),
                    "OpenCode question reply",
                )
                .await?;
                Ok(false)
            }
            ProviderUserInputResponse::Cancelled => {
                self.reject_question(&request_id).await?;
                Ok(cancellation.is_cancelled())
            }
        }
    }

    async fn reject_question(&self, request_id: &str) -> Result<(), String> {
        http_json(
            self.client
                .post(format!("{}/question/{request_id}/reject", self.base_url))
                .query(&[("directory", self.directory)]),
            "OpenCode question reject",
        )
        .await
        .map(|_| ())
    }
}

impl OpencodeSession {
    async fn connect(config: ProviderSessionConfig) -> Result<Self, String> {
        let executable = find_executable("opencode")
            .ok_or_else(|| "OpenCode is not installed or was not found on PATH".to_string())?;
        let args = vec![
            "serve".to_string(),
            "--hostname=127.0.0.1".to_string(),
            "--port=0".to_string(),
        ];
        let mut command = platform_command(&executable, &args);
        command.current_dir(&config.workspace);
        // An empty inline config isolates the embedded server from unrelated
        // opencode.json project files while keeping the user's stored auth.
        command.env("OPENCODE_CONFIG_CONTENT", "{}");
        let mut process = JsonProcess::spawn(command, "OpenCode server", MAX_SERVE_STREAM).await?;

        let base_url = match timeout(STARTUP_TIMEOUT, wait_for_listen_url(&mut process)).await {
            Ok(result) => result?,
            Err(_) => {
                process.shutdown().await;
                return Err("OpenCode server did not report a listening address".to_string());
            }
        };

        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| format!("OpenCode HTTP client could not be created: {error}"))?;
        let directory = config.workspace.to_string_lossy().into_owned();

        // Subscribe to the global event stream before any prompt so no
        // session event can be missed.
        let (sender, events) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let subscription = match timeout(
            STARTUP_TIMEOUT,
            open_event_stream(&client, &base_url, &directory),
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                process.shutdown().await;
                return Err(error);
            }
            Err(_) => {
                process.shutdown().await;
                return Err("OpenCode event subscription timed out".to_string());
            }
        };
        let sse_task = tokio::spawn(pump_event_stream(subscription, sender));

        let session_id = match establish_session(&client, &base_url, &directory, &config).await {
            Ok(session_id) => session_id,
            Err(error) => {
                sse_task.abort();
                process.shutdown().await;
                return Err(error);
            }
        };

        Ok(Self {
            process,
            client,
            base_url,
            directory,
            session_id,
            events,
            sse_task,
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
        // Field-level borrows keep the shared HTTP view disjoint from the
        // `&mut self.events` receiver used in the select loop below.
        let http = SessionHttp {
            client: &self.client,
            base_url: &self.base_url,
            directory: &self.directory,
            session_id: &self.session_id,
            config: &self.config,
        };
        http.post_prompt(prompt).await?;

        let mut steer_open = true;
        let mut cancelling = false;
        let mut abort_deadline: Option<Instant> = None;
        let mut saw_busy = false;
        let mut pending_error: Option<String> = None;
        let mut text_streamed = false;
        let mut turn = TurnState::new();

        loop {
            enum TurnInput {
                Cancel,
                Steer(Option<String>),
                Event(Option<Result<Value, String>>),
            }
            let input = if cancelling {
                let deadline =
                    *abort_deadline.get_or_insert_with(|| Instant::now() + ABORT_TIMEOUT);
                let event = timeout_at(deadline, self.events.recv())
                    .await
                    .map_err(|_| "OpenCode turn interrupt timed out".to_string())?;
                TurnInput::Event(event)
            } else {
                tokio::select! {
                    _ = cancellation.cancelled() => TurnInput::Cancel,
                    message = steer.recv(), if steer_open => TurnInput::Steer(message),
                    event = self.events.recv() => TurnInput::Event(event),
                }
            };
            let value = match input {
                TurnInput::Cancel => {
                    cancelling = true;
                    let deadline =
                        *abort_deadline.get_or_insert_with(|| Instant::now() + ABORT_TIMEOUT);
                    http.abort_turn(deadline).await?;
                    continue;
                }
                TurnInput::Steer(Some(text)) => {
                    // While the session is busy another prompt_async queues the
                    // message into the running turn.
                    http.post_prompt(&text).await?;
                    continue;
                }
                TurnInput::Steer(None) => {
                    steer_open = false;
                    continue;
                }
                TurnInput::Event(Some(Ok(value))) => value,
                TurnInput::Event(Some(Err(error))) => return Err(error),
                TurnInput::Event(None) => {
                    return Err("OpenCode event stream closed before the turn completed".into());
                }
            };

            if event_session_id(&value) != Some(self.session_id.as_str()) {
                continue;
            }
            let properties = value.pointer("/properties").unwrap_or(&Value::Null);
            match value.get("type").and_then(Value::as_str) {
                Some("message.updated") => turn.record_message(properties),
                Some("message.part.updated") => {
                    let part = properties.get("part").unwrap_or(&Value::Null);
                    turn.record_part(part);
                    match part.get("type").and_then(Value::as_str) {
                        Some("tool") => {
                            if let Some(activity) = tool_activity(part) {
                                send_event(events, ProviderEvent::Activity(activity)).await?;
                            }
                        }
                        Some("step-finish") => {
                            if let Some(usage) = step_finish_usage(part) {
                                send_event(events, ProviderEvent::ContextUsage(usage)).await?;
                            }
                        }
                        Some("text") => {
                            if let Some(text) = turn.unstreamed_assistant_text(part) {
                                text_streamed = true;
                                send_event(events, ProviderEvent::Text(text)).await?;
                            }
                        }
                        _ => {}
                    }
                }
                Some("message.part.delta") => {
                    if properties.get("field").and_then(Value::as_str) != Some("text") {
                        continue;
                    }
                    let Some(delta) = properties.get("delta").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(part_id) = properties.get("partID").and_then(Value::as_str) else {
                        continue;
                    };
                    if turn.is_reasoning_part(part_id) {
                        if let Some(detail) = turn.append_reasoning(delta) {
                            send_event(
                                events,
                                ProviderEvent::Activity(ProviderActivity {
                                    id: Some("opencode-reasoning".to_string()),
                                    title: "Reasoning".to_string(),
                                    detail: Some(detail),
                                    kind: MessageKind::Reasoning,
                                }),
                            )
                            .await?;
                        }
                    } else {
                        turn.mark_streamed(part_id);
                        text_streamed = true;
                        send_event(events, ProviderEvent::TextDelta(delta.to_string())).await?;
                    }
                }
                Some("session.error") => {
                    if let Some(message) = session_error_message(properties) {
                        pending_error = Some(message.clone());
                        send_event(
                            events,
                            ProviderEvent::Activity(ProviderActivity::error(message)),
                        )
                        .await?;
                    }
                }
                Some("permission.asked") => {
                    let was_cancelled = http
                        .handle_permission(properties, cancelling, cancellation, events)
                        .await?;
                    if was_cancelled && !cancelling {
                        cancelling = true;
                        let deadline =
                            *abort_deadline.get_or_insert_with(|| Instant::now() + ABORT_TIMEOUT);
                        http.abort_turn(deadline).await?;
                    }
                }
                Some("question.asked") => {
                    let was_cancelled = http
                        .handle_question(properties, cancelling, cancellation, events)
                        .await?;
                    if was_cancelled && !cancelling {
                        cancelling = true;
                        let deadline =
                            *abort_deadline.get_or_insert_with(|| Instant::now() + ABORT_TIMEOUT);
                        http.abort_turn(deadline).await?;
                    }
                }
                Some("session.status") => {
                    match properties.pointer("/status/type").and_then(Value::as_str) {
                        Some("busy") | Some("retry") => saw_busy = true,
                        // Only the canonical status event is terminal. The
                        // deprecated duplicate `session.idle` event is ignored
                        // so it can never terminate the following turn.
                        Some("idle") if saw_busy || cancelling => {
                            if cancelling {
                                return Err(TURN_CANCELLED_ERROR.to_string());
                            }
                            return match pending_error.take() {
                                Some(error) if !text_streamed => Err(error),
                                _ => Ok(()),
                            };
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

impl ProviderSession for OpencodeSession {
    fn provider(&self) -> ProviderId {
        ProviderId::Opencode
    }

    fn continuation(&self) -> Option<String> {
        Some(self.session_id.clone())
    }

    fn supports_steering(&self) -> bool {
        true
    }

    /// `session/abort` is a native server-side interrupt: the HTTP transport
    /// and the SSE stream stay connected and the session reliably publishes a
    /// terminal `session.status: idle`. `run_turn` only reports
    /// `TURN_CANCELLED_ERROR` after observing that terminal state (an abort
    /// that never reaches idle times out and surfaces as a hard error, which
    /// discards the session), so retaining the transport is safe.
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
            send_event(
                &events,
                ProviderEvent::Continuation(self.session_id.clone()),
            )
            .await?;
            self.run_turn_inner(prompt, cancellation, &events, steer)
                .await
        })
    }

    fn shutdown<'a>(&'a mut self) -> DriverFuture<'a, ()> {
        Box::pin(async move {
            self.sse_task.abort();
            self.process.shutdown().await;
        })
    }
}

/// Tracks the streaming state the SSE protocol spreads across events: message
/// roles, part kinds, which parts already streamed deltas, and accumulated
/// reasoning text.
struct TurnState {
    roles: HashMap<String, String>,
    reasoning_parts: HashSet<String>,
    streamed_parts: HashSet<String>,
    emitted_text_parts: HashSet<String>,
    reasoning: String,
}

impl TurnState {
    fn new() -> Self {
        Self {
            roles: HashMap::new(),
            reasoning_parts: HashSet::new(),
            streamed_parts: HashSet::new(),
            emitted_text_parts: HashSet::new(),
            reasoning: String::new(),
        }
    }

    fn record_message(&mut self, properties: &Value) {
        if self.roles.len() >= MAX_TRACKED_PARTS {
            return;
        }
        if let (Some(id), Some(role)) = (
            properties.pointer("/info/id").and_then(Value::as_str),
            properties.pointer("/info/role").and_then(Value::as_str),
        ) {
            self.roles.insert(id.to_string(), role.to_string());
        }
    }

    fn record_part(&mut self, part: &Value) {
        if self.reasoning_parts.len() >= MAX_TRACKED_PARTS {
            return;
        }
        if part.get("type").and_then(Value::as_str) == Some("reasoning")
            && let Some(id) = part.get("id").and_then(Value::as_str)
        {
            self.reasoning_parts.insert(id.to_string());
        }
    }

    fn is_reasoning_part(&self, part_id: &str) -> bool {
        self.reasoning_parts.contains(part_id)
    }

    fn mark_streamed(&mut self, part_id: &str) {
        if self.streamed_parts.len() < MAX_TRACKED_PARTS {
            self.streamed_parts.insert(part_id.to_string());
        }
    }

    fn append_reasoning(&mut self, delta: &str) -> Option<String> {
        if self.reasoning.len() >= MAX_ACTIVITY_BYTES {
            return None;
        }
        let remaining = MAX_ACTIVITY_BYTES - self.reasoning.len();
        let mut end = delta.len().min(remaining);
        while end > 0 && !delta.is_char_boundary(end) {
            end -= 1;
        }
        self.reasoning.push_str(&delta[..end]);
        Some(self.reasoning.clone())
    }

    /// Completed assistant text that never streamed as deltas (providers
    /// without streaming emit the full text on the final part update).
    fn unstreamed_assistant_text(&mut self, part: &Value) -> Option<String> {
        let part_id = part.get("id").and_then(Value::as_str)?;
        if part.pointer("/time/end").is_none()
            || self.streamed_parts.contains(part_id)
            || self.emitted_text_parts.contains(part_id)
        {
            return None;
        }
        let message_id = part.get("messageID").and_then(Value::as_str)?;
        if self.roles.get(message_id).map(String::as_str) != Some("assistant") {
            return None;
        }
        let text = part.get("text").and_then(Value::as_str)?;
        if text.trim().is_empty() {
            return None;
        }
        if self.emitted_text_parts.len() < MAX_TRACKED_PARTS {
            self.emitted_text_parts.insert(part_id.to_string());
        }
        Some(text.to_string())
    }
}

async fn wait_for_listen_url(process: &mut JsonProcess) -> Result<String, String> {
    loop {
        match process.next_stdout().await? {
            ProcessOutput::Stdout(line) => {
                if let Some(url) = parse_listen_url(&line) {
                    return Ok(url);
                }
            }
            ProcessOutput::Exited(status) => {
                let detail = process.stderr_tail();
                return Err(if detail.is_empty() {
                    format!("OpenCode server exited with {status}")
                } else {
                    format!("OpenCode server exited with {status}: {detail}")
                });
            }
        }
    }
}

fn parse_listen_url(line: &str) -> Option<String> {
    if !line.contains("opencode server listening") {
        return None;
    }
    line.split_whitespace()
        .find(|token| token.starts_with("http://") || token.starts_with("https://"))
        .map(|token| token.trim_end_matches('/').to_string())
}

async fn open_event_stream(
    client: &reqwest::Client,
    base_url: &str,
    directory: &str,
) -> Result<reqwest::Response, String> {
    let response = client
        .get(format!("{base_url}/event"))
        .query(&[("directory", directory)])
        .send()
        .await
        .map_err(|error| format!("OpenCode event subscription failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "OpenCode event subscription failed with HTTP {}",
            response.status()
        ));
    }
    Ok(response)
}

/// Minimal SSE reader: `data:` payload lines are collected until the blank
/// line that terminates an event, then parsed as one JSON value.
async fn pump_event_stream(
    mut response: reqwest::Response,
    sender: mpsc::Sender<Result<Value, String>>,
) {
    let mut buffer: Vec<u8> = Vec::new();
    let mut data = String::new();
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => {
                let _ = sender
                    .send(Err("OpenCode event stream ended".to_string()))
                    .await;
                return;
            }
            Err(error) => {
                let _ = sender
                    .send(Err(format!("OpenCode event stream failed: {error}")))
                    .await;
                return;
            }
        };
        if buffer.len().saturating_add(chunk.len()) > MAX_SSE_EVENT {
            let _ = sender
                .send(Err("OpenCode event exceeded the size limit".to_string()))
                .await;
            return;
        }
        buffer.extend_from_slice(&chunk);
        while let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = buffer.drain(..=index).collect();
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line).into_owned();
            if line.is_empty() {
                if !data.is_empty() {
                    let payload = std::mem::take(&mut data);
                    if let Ok(value) = serde_json::from_str::<Value>(&payload)
                        && sender.send(Ok(value)).await.is_err()
                    {
                        return;
                    }
                }
                continue;
            }
            if let Some(payload) = line.strip_prefix("data:") {
                if data.len().saturating_add(payload.len()) > MAX_SSE_EVENT {
                    let _ = sender
                        .send(Err("OpenCode event exceeded the size limit".to_string()))
                        .await;
                    return;
                }
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(payload.strip_prefix(' ').unwrap_or(payload));
            }
        }
    }
}

async fn establish_session(
    client: &reqwest::Client,
    base_url: &str,
    directory: &str,
    config: &ProviderSessionConfig,
) -> Result<String, String> {
    let ruleset = permission_ruleset(config.access_mode);
    if let Some(existing) = config.continuation.as_deref() {
        let response = client
            .get(format!("{base_url}/session/{existing}"))
            .query(&[("directory", directory)])
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| format!("OpenCode session lookup failed: {error}"))?;
        if response.status().is_success() {
            let session = read_json_body(response, "OpenCode session lookup").await?;
            let session_directory = session.get("directory").and_then(Value::as_str);
            let session_id = if session_directory.is_some_and(|current| current != directory) {
                // The stored session lives in another directory; fork it into
                // the current workspace instead of mutating the original.
                let forked = http_json(
                    client
                        .post(format!("{base_url}/session/{existing}/fork"))
                        .query(&[("directory", directory)])
                        .json(&json!({})),
                    "OpenCode session fork",
                )
                .await?;
                forked
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "OpenCode fork did not return a session id".to_string())?
                    .to_string()
            } else {
                existing.to_string()
            };
            // Re-apply the ruleset so a changed Onyx access mode reaches the
            // resumed session.
            http_json(
                client
                    .patch(format!("{base_url}/session/{session_id}"))
                    .query(&[("directory", directory)])
                    .json(&json!({ "permission": ruleset })),
                "OpenCode session update",
            )
            .await?;
            return Ok(session_id);
        }
        // 404 (or any other failure) falls through to a fresh session.
    }
    let created = http_json(
        client
            .post(format!("{base_url}/session"))
            .query(&[("directory", directory)])
            .json(&json!({ "permission": ruleset })),
        "OpenCode session create",
    )
    .await?;
    created
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "OpenCode did not return a session id".to_string())
}

async fn http_json(request: reqwest::RequestBuilder, label: &str) -> Result<Value, String> {
    let response = request
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("{label} failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = bounded_body(response).await;
        return Err(if body.is_empty() {
            format!("{label} failed with HTTP {status}")
        } else {
            format!("{label} failed with HTTP {status}: {body}")
        });
    }
    read_json_body(response, label).await
}

async fn read_json_body(response: reqwest::Response, label: &str) -> Result<Value, String> {
    let body = bounded_body(response).await;
    if body.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&body).map_err(|error| format!("{label} returned invalid JSON: {error}"))
}

async fn bounded_body(mut response: reqwest::Response) -> String {
    let mut body = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        if body.len().saturating_add(chunk.len()) > MAX_HTTP_BODY {
            break;
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8_lossy(&body).into_owned()
}

fn event_session_id(value: &Value) -> Option<&str> {
    value
        .pointer("/properties/sessionID")
        .or_else(|| value.pointer("/properties/part/sessionID"))
        .or_else(|| value.pointer("/properties/info/sessionID"))
        .and_then(Value::as_str)
}

/// OpenCode evaluates rulesets with last-match-wins wildcard semantics, so the
/// `question` allow rule must come after the broad `ask` rules.
fn permission_ruleset(access_mode: AccessMode) -> Value {
    match access_mode {
        AccessMode::FullAccess => json!([
            { "permission": "*", "pattern": "*", "action": "allow" }
        ]),
        AccessMode::ApprovalRequired | AccessMode::AutoAcceptEdits => {
            let edit_action = if access_mode == AccessMode::AutoAcceptEdits {
                "allow"
            } else {
                "ask"
            };
            json!([
                { "permission": "*", "pattern": "*", "action": "ask" },
                { "permission": "bash", "pattern": "*", "action": "ask" },
                { "permission": "edit", "pattern": "*", "action": edit_action },
                { "permission": "webfetch", "pattern": "*", "action": "ask" },
                { "permission": "websearch", "pattern": "*", "action": "ask" },
                { "permission": "question", "pattern": "*", "action": "allow" },
            ])
        }
    }
}

/// Model slugs are `provider/model`, split on the FIRST slash; the model part
/// itself may contain slashes.
fn parse_model_slug(model: &str) -> Option<(&str, &str)> {
    let (provider, model) = model.split_once('/')?;
    (!provider.trim().is_empty() && !model.trim().is_empty()).then_some((provider, model))
}

fn prompt_body(text: &str, model: Option<&str>) -> Value {
    let mut body = json!({
        "parts": [{ "type": "text", "text": text }]
    });
    if let Some((provider_id, model_id)) = model.and_then(parse_model_slug) {
        body["model"] = json!({ "providerID": provider_id, "modelID": model_id });
    }
    body
}

fn permission_reply_body(decision: ApprovalDecision) -> Value {
    let reply = match decision {
        ApprovalDecision::Allow => "once",
        ApprovalDecision::AllowSession => "always",
        ApprovalDecision::Deny => "reject",
    };
    json!({ "reply": reply })
}

fn permission_request_parts(properties: &Value) -> (String, String, String) {
    let permission = properties
        .get("permission")
        .and_then(Value::as_str)
        .unwrap_or("tool");
    let patterns = properties
        .get("patterns")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let title = if patterns.is_empty() {
        format!("Allow OpenCode to use {permission}?")
    } else {
        format!("Allow OpenCode to use {permission} for {patterns}?")
    };
    let metadata = properties.get("metadata").unwrap_or(&Value::Null);
    let detail = if metadata.as_object().is_some_and(|map| !map.is_empty()) {
        serde_json::to_string_pretty(metadata).unwrap_or_else(|_| metadata.to_string())
    } else if patterns.is_empty() {
        permission.to_string()
    } else {
        patterns
    };
    let risk = match permission {
        "bash" => "executes a command requested by OpenCode".to_string(),
        "edit" => "writes files requested by OpenCode".to_string(),
        "webfetch" | "websearch" => "performs network access requested by OpenCode".to_string(),
        other => format!("runs the {other} action requested by OpenCode"),
    };
    (title, detail, risk)
}

fn question_prompt(properties: &Value) -> Result<ProviderUserInputPrompt, String> {
    let raw_questions = properties
        .get("questions")
        .and_then(Value::as_array)
        .ok_or_else(|| "OpenCode question request did not include questions".to_string())?;
    if raw_questions.len() > MAX_USER_INPUT_QUESTIONS {
        return Err(format!(
            "OpenCode question request exceeded the {MAX_USER_INPUT_QUESTIONS}-question limit"
        ));
    }
    let mut questions = Vec::with_capacity(raw_questions.len());
    for (index, raw) in raw_questions.iter().enumerate() {
        let raw_options = raw
            .get("options")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if raw_options.len() > MAX_USER_INPUT_OPTIONS {
            return Err(format!(
                "OpenCode question exceeded the {MAX_USER_INPUT_OPTIONS}-option limit"
            ));
        }
        let options = raw_options
            .iter()
            .filter_map(|option| {
                Some(ProviderUserInputOption {
                    label: option.get("label").and_then(Value::as_str)?.to_string(),
                    description: option
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                })
            })
            .collect::<Vec<_>>();
        questions.push(ProviderUserInputQuestion {
            // The reply endpoint expects answers in question order; the ids
            // encode that order.
            id: format!("q{index}"),
            header: raw
                .get("header")
                .and_then(Value::as_str)
                .unwrap_or("Question")
                .to_string(),
            question: raw
                .get("question")
                .and_then(Value::as_str)
                .unwrap_or("Choose an answer")
                .to_string(),
            multi_select: raw
                .get("multiple")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            allow_other: raw
                .get("custom")
                .and_then(Value::as_bool)
                .unwrap_or(options.is_empty()),
            secret: false,
            options,
        });
    }
    ProviderUserInputPrompt::new("OpenCode needs your input", questions, None)
}

/// Answers are sent as an array of arrays of selected labels, in question
/// order.
fn question_answers_body(question_ids: &[String], answers: &ProviderUserInputAnswers) -> Value {
    let ordered = question_ids
        .iter()
        .map(|id| json!(answers.get(id).cloned().unwrap_or_default()))
        .collect::<Vec<_>>();
    json!({ "answers": ordered })
}

fn session_error_message(properties: &Value) -> Option<String> {
    let error = properties.get("error")?;
    let name = error.get("name").and_then(Value::as_str).unwrap_or("");
    if name == "MessageAbortedError" {
        // Expected while a turn is being interrupted.
        return None;
    }
    let message = error
        .pointer("/data/message")
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty());
    Some(match (name.is_empty(), message) {
        (false, Some(message)) => format!("OpenCode error ({name}): {message}"),
        (false, None) => format!("OpenCode error: {name}"),
        (true, Some(message)) => format!("OpenCode error: {message}"),
        (true, None) => "OpenCode reported an error".to_string(),
    })
}

fn tool_activity(part: &Value) -> Option<ProviderActivity> {
    let id = part.get("id").and_then(Value::as_str).map(str::to_string);
    let tool = part.get("tool").and_then(Value::as_str).unwrap_or("tool");
    let state = part.get("state")?;
    match state.get("status").and_then(Value::as_str)? {
        "running" => {
            let title = state
                .get("title")
                .and_then(Value::as_str)
                .filter(|title| !title.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("Running {tool}"));
            let mut activity = ProviderActivity::tool(title, None);
            activity.id = id;
            Some(activity)
        }
        "completed" => {
            let title = state
                .get("title")
                .and_then(Value::as_str)
                .filter(|title| !title.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("Completed {tool}"));
            let detail = state
                .get("output")
                .and_then(Value::as_str)
                .filter(|output| !output.trim().is_empty())
                .map(|output| bounded(output, MAX_ACTIVITY_BYTES));
            let mut activity = ProviderActivity::tool(title, detail);
            activity.id = id;
            Some(activity)
        }
        "error" => {
            let mut activity = ProviderActivity::error(format!(
                "{tool} failed: {}",
                bounded(
                    state
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("error"),
                    MAX_ACTIVITY_BYTES,
                )
            ));
            activity.id = id;
            Some(activity)
        }
        _ => None,
    }
}

fn step_finish_usage(part: &Value) -> Option<ContextUsage> {
    let tokens = part.get("tokens")?;
    let field = |name: &str| tokens.get(name).and_then(Value::as_u64);
    let input = field("input");
    let output = field("output");
    let reasoning = field("reasoning");
    let cache_read = tokens.pointer("/cache/read").and_then(Value::as_u64);
    let used = field("total").unwrap_or(
        input
            .unwrap_or(0)
            .saturating_add(output.unwrap_or(0))
            .saturating_add(reasoning.unwrap_or(0))
            .saturating_add(cache_read.unwrap_or(0)),
    );
    (used > 0).then_some(ContextUsage {
        used_tokens: used,
        max_tokens: None,
        input_tokens: input,
        cached_input_tokens: cache_read,
        output_tokens: output,
        reasoning_output_tokens: reasoning,
    })
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

/// Availability gate for the versions this driver's HTTP API requires. An
/// unparseable or missing version does not block the provider.
pub fn version_supported(version: Option<&str>) -> bool {
    match version.and_then(parse_version) {
        Some(version) => version >= MIN_VERSION,
        None => true,
    }
}

fn parse_version(line: &str) -> Option<(u64, u64, u64)> {
    line.split_whitespace().find_map(|token| {
        let mut numbers = token
            .trim_start_matches('v')
            .split('.')
            .map(|piece| piece.parse::<u64>().ok());
        match (numbers.next(), numbers.next(), numbers.next()) {
            (Some(Some(major)), Some(Some(minor)), patch) => {
                Some((major, minor, patch.flatten().unwrap_or(0)))
            }
            _ => None,
        }
    })
}

pub async fn model_catalog() -> Result<Vec<ProviderModelOption>, String> {
    let executable = find_executable("opencode")
        .ok_or_else(|| "OpenCode is not installed or was not found on PATH".to_string())?;
    let args = vec!["models".to_string()];
    let mut command = platform_command(&executable, &args);
    command.env("OPENCODE_CONFIG_CONTENT", "{}");
    let mut process =
        JsonProcess::spawn(command, "OpenCode model catalog", MAX_CATALOG_STREAM).await?;
    process.close_stdin().await?;

    let result = timeout(CATALOG_TIMEOUT, async {
        let mut stdout = String::new();
        loop {
            match process.next_stdout().await? {
                ProcessOutput::Stdout(line) => {
                    if stdout.len().saturating_add(line.len()).saturating_add(1)
                        > MAX_CATALOG_STREAM
                    {
                        return Err("OpenCode model catalog exceeded the output limit".to_string());
                    }
                    stdout.push_str(&line);
                    stdout.push('\n');
                }
                ProcessOutput::Exited(status) if status.success() => return Ok(stdout),
                ProcessOutput::Exited(status) => {
                    return Err(format!("OpenCode could not list models ({status})"));
                }
            }
        }
    })
    .await;

    let stdout = match result {
        Ok(result) => result?,
        Err(_) => {
            process.shutdown().await;
            return Err("OpenCode model catalog timed out".to_string());
        }
    };
    let models = parse_models_output(&stdout);
    if models.len() <= 1 {
        return Err("OpenCode returned an empty model catalog".to_string());
    }
    Ok(models)
}

/// `opencode models` prints one `provider/model` slug per line. Lines that are
/// not slugs (blank lines, verbose JSON blocks) are ignored.
fn parse_models_output(stdout: &str) -> Vec<ProviderModelOption> {
    let mut options = vec![configured_default_model_option()];
    let mut seen = HashSet::new();
    for line in stdout.lines() {
        let slug = line.trim();
        if slug.is_empty()
            || slug.len() > MAX_SLUG_BYTES
            || slug.chars().any(|character| {
                character.is_whitespace() || character.is_control() || "{}\"[],".contains(character)
            })
        {
            continue;
        }
        if parse_model_slug(slug).is_none() || !seen.insert(slug.to_string()) {
            continue;
        }
        if options.len() >= MAX_MODELS {
            break;
        }
        options.push(ProviderModelOption {
            id: slug.to_string(),
            name: slug.to_string(),
            description: None,
            is_default: false,
            legacy: false,
            reasoning: Vec::new(),
            default_reasoning: None,
            speeds: vec![SpeedMode::Standard],
            default_speed: SpeedMode::Standard,
            context_length: None,
        });
    }
    options
}

fn configured_default_model_option() -> ProviderModelOption {
    ProviderModelOption {
        id: "default".to_string(),
        name: "OpenCode configured default".to_string(),
        description: Some(
            "Use the model selected by the active OpenCode configuration.".to_string(),
        ),
        is_default: true,
        legacy: false,
        reasoning: Vec::new(),
        default_reasoning: None,
        speeds: vec![SpeedMode::Standard],
        default_speed: SpeedMode::Standard,
        context_length: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TurnState, parse_listen_url, parse_model_slug, parse_models_output, parse_version,
        permission_reply_body, permission_request_parts, permission_ruleset, prompt_body,
        question_answers_body, question_prompt, session_error_message, step_finish_usage,
        tool_activity, version_supported,
    };
    use crate::{
        model::{AccessMode, ApprovalDecision, MessageKind},
        providers::driver::ProviderUserInputAnswers,
    };
    use serde_json::json;

    #[test]
    fn extracts_the_listen_url_from_serve_stdout() {
        assert_eq!(
            parse_listen_url("opencode server listening on http://127.0.0.1:53422").as_deref(),
            Some("http://127.0.0.1:53422")
        );
        assert_eq!(parse_listen_url("starting instance...").as_deref(), None);
        assert_eq!(
            parse_listen_url("opencode server listening somewhere").as_deref(),
            None
        );
    }

    #[test]
    fn model_slugs_split_on_the_first_slash_only() {
        assert_eq!(
            parse_model_slug("anthropic/claude-sonnet-4-5"),
            Some(("anthropic", "claude-sonnet-4-5"))
        );
        assert_eq!(
            parse_model_slug("openrouter/deepseek/deepseek-chat"),
            Some(("openrouter", "deepseek/deepseek-chat"))
        );
        assert_eq!(parse_model_slug("no-slash"), None);
        assert_eq!(parse_model_slug("/model"), None);
        assert_eq!(parse_model_slug("provider/"), None);

        let body = prompt_body("hello", Some("openrouter/deepseek/deepseek-chat"));
        assert_eq!(body["model"]["providerID"], "openrouter");
        assert_eq!(body["model"]["modelID"], "deepseek/deepseek-chat");
        assert_eq!(body["parts"][0]["type"], "text");
        assert_eq!(body["parts"][0]["text"], "hello");
        assert!(prompt_body("hello", None).get("model").is_none());
    }

    #[test]
    fn ruleset_matches_the_access_mode_with_question_allow_last() {
        assert_eq!(
            permission_ruleset(AccessMode::FullAccess),
            json!([{ "permission": "*", "pattern": "*", "action": "allow" }])
        );

        let ask = permission_ruleset(AccessMode::ApprovalRequired);
        let rules = ask.as_array().unwrap();
        assert_eq!(rules.len(), 6);
        assert_eq!(rules[0]["permission"], "*");
        assert_eq!(rules[0]["action"], "ask");
        assert_eq!(rules[2]["permission"], "edit");
        assert_eq!(rules[2]["action"], "ask");
        // Last-match-wins evaluation: the question rule must come last.
        assert_eq!(rules[5]["permission"], "question");
        assert_eq!(rules[5]["action"], "allow");

        let auto_edit = permission_ruleset(AccessMode::AutoAcceptEdits);
        assert_eq!(auto_edit[2]["permission"], "edit");
        assert_eq!(auto_edit[2]["action"], "allow");
    }

    #[test]
    fn parses_the_models_cli_output_and_keeps_a_configured_default() {
        let models = parse_models_output(
            "opencode/grok-code\nanthropic/claude-sonnet-4-5\nanthropic/claude-sonnet-4-5\n\n{\n  \"cost\": {}\n}\nnot a slug line\n",
        );
        assert_eq!(models[0].id, "default");
        assert!(models[0].is_default);
        assert_eq!(
            models
                .iter()
                .skip(1)
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["opencode/grok-code", "anthropic/claude-sonnet-4-5"]
        );
        assert!(models.iter().skip(1).all(|model| !model.is_default));
    }

    #[test]
    fn permission_replies_map_onyx_decisions_to_opencode_verbs() {
        assert_eq!(
            permission_reply_body(ApprovalDecision::Allow),
            json!({ "reply": "once" })
        );
        assert_eq!(
            permission_reply_body(ApprovalDecision::AllowSession),
            json!({ "reply": "always" })
        );
        assert_eq!(
            permission_reply_body(ApprovalDecision::Deny),
            json!({ "reply": "reject" })
        );
    }

    #[test]
    fn permission_requests_surface_patterns_and_metadata() {
        let (title, detail, risk) = permission_request_parts(&json!({
            "id": "per_1",
            "sessionID": "ses_1",
            "permission": "bash",
            "patterns": ["git push*"],
            "metadata": { "command": "git push origin main" },
            "always": ["git push*"]
        }));
        assert_eq!(title, "Allow OpenCode to use bash for git push*?");
        assert!(detail.contains("git push origin main"));
        assert!(risk.contains("command"));
    }

    #[test]
    fn question_events_become_ordered_prompts_and_ordered_answers() {
        let properties = json!({
            "id": "que_1",
            "sessionID": "ses_1",
            "questions": [
                {
                    "question": "Which storage layer should we use?",
                    "header": "Storage",
                    "options": [
                        { "label": "SQLite", "description": "Embedded" },
                        { "label": "Postgres", "description": "Server" }
                    ],
                    "multiple": false,
                    "custom": false
                },
                {
                    "question": "Anything else?",
                    "header": "Notes",
                    "options": [],
                    "custom": true
                }
            ]
        });
        let prompt = question_prompt(&properties).expect("valid question event");
        assert_eq!(prompt.questions.len(), 2);
        assert_eq!(prompt.questions[0].id, "q0");
        assert_eq!(prompt.questions[0].options[1].label, "Postgres");
        assert!(!prompt.questions[0].allow_other);
        assert!(prompt.questions[1].allow_other);

        let answers = ProviderUserInputAnswers::from([
            ("q1".to_string(), vec!["ship it".to_string()]),
            ("q0".to_string(), vec!["SQLite".to_string()]),
        ]);
        let ids = prompt
            .questions
            .iter()
            .map(|question| question.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            question_answers_body(&ids, &answers),
            json!({ "answers": [["SQLite"], ["ship it"]] })
        );
    }

    #[test]
    fn session_errors_are_reported_except_native_aborts() {
        assert_eq!(
            session_error_message(&json!({
                "sessionID": "ses_1",
                "error": { "name": "ProviderAuthError", "data": { "message": "no credentials" } }
            }))
            .as_deref(),
            Some("OpenCode error (ProviderAuthError): no credentials")
        );
        assert_eq!(
            session_error_message(&json!({
                "sessionID": "ses_1",
                "error": { "name": "MessageAbortedError", "data": {} }
            })),
            None
        );
        assert_eq!(session_error_message(&json!({})), None);
    }

    #[test]
    fn completed_and_failed_tool_parts_become_activities() {
        let completed = tool_activity(&json!({
            "id": "prt_1",
            "type": "tool",
            "tool": "bash",
            "state": {
                "status": "completed",
                "input": {},
                "output": "42 files",
                "title": "ls -la",
                "metadata": {},
                "time": { "start": 1, "end": 2 }
            }
        }))
        .expect("completed tool activity");
        assert_eq!(completed.title, "ls -la");
        assert_eq!(completed.detail.as_deref(), Some("42 files"));
        assert_eq!(completed.id.as_deref(), Some("prt_1"));
        assert_eq!(completed.kind, MessageKind::Tool);

        let failed = tool_activity(&json!({
            "id": "prt_2",
            "type": "tool",
            "tool": "webfetch",
            "state": { "status": "error", "input": {}, "error": "connection refused", "time": { "start": 1, "end": 2 } }
        }))
        .expect("failed tool activity");
        assert_eq!(failed.kind, MessageKind::Error);
        assert!(failed.title.contains("connection refused"));

        assert!(
            tool_activity(&json!({
                "id": "prt_3",
                "type": "tool",
                "tool": "bash",
                "state": { "status": "pending", "input": {} }
            }))
            .is_none()
        );
    }

    #[test]
    fn step_finish_tokens_become_context_usage() {
        let usage = step_finish_usage(&json!({
            "type": "step-finish",
            "tokens": {
                "input": 1200,
                "output": 300,
                "reasoning": 50,
                "cache": { "read": 4000, "write": 10 }
            }
        }))
        .expect("usage");
        assert_eq!(usage.used_tokens, 1200 + 300 + 50 + 4000);
        assert_eq!(usage.input_tokens, Some(1200));
        assert_eq!(usage.cached_input_tokens, Some(4000));
        assert!(step_finish_usage(&json!({ "type": "step-finish" })).is_none());
    }

    #[test]
    fn text_deltas_are_routed_by_part_kind_and_full_text_is_deduplicated() {
        let mut turn = TurnState::new();
        turn.record_message(&json!({
            "info": { "id": "msg_a", "sessionID": "ses_1", "role": "assistant" }
        }));
        turn.record_message(&json!({
            "info": { "id": "msg_u", "sessionID": "ses_1", "role": "user" }
        }));
        turn.record_part(&json!({ "id": "prt_r", "type": "reasoning" }));
        assert!(turn.is_reasoning_part("prt_r"));
        assert!(!turn.is_reasoning_part("prt_t"));

        // A streamed text part must not be re-emitted as full text.
        turn.mark_streamed("prt_t");
        let streamed_part = json!({
            "id": "prt_t",
            "messageID": "msg_a",
            "type": "text",
            "text": "hello world",
            "time": { "start": 1, "end": 2 }
        });
        assert_eq!(turn.unstreamed_assistant_text(&streamed_part), None);

        // Unstreamed assistant text is emitted exactly once.
        let quiet_part = json!({
            "id": "prt_q",
            "messageID": "msg_a",
            "type": "text",
            "text": "full text",
            "time": { "start": 1, "end": 2 }
        });
        assert_eq!(
            turn.unstreamed_assistant_text(&quiet_part).as_deref(),
            Some("full text")
        );
        assert_eq!(turn.unstreamed_assistant_text(&quiet_part), None);

        // The user's own prompt part is never echoed back.
        let user_part = json!({
            "id": "prt_u",
            "messageID": "msg_u",
            "type": "text",
            "text": "user prompt",
            "time": { "start": 1, "end": 2 }
        });
        assert_eq!(turn.unstreamed_assistant_text(&user_part), None);

        // Incomplete parts wait for their final update.
        let open_part = json!({
            "id": "prt_o",
            "messageID": "msg_a",
            "type": "text",
            "text": "partial",
            "time": { "start": 1 }
        });
        assert_eq!(turn.unstreamed_assistant_text(&open_part), None);
    }

    #[test]
    fn version_gate_accepts_supported_and_unknown_versions() {
        assert!(version_supported(Some("1.14.19")));
        assert!(version_supported(Some("opencode 1.14.30")));
        assert!(version_supported(Some("2.0.0")));
        assert!(!version_supported(Some("1.14.18")));
        assert!(!version_supported(Some("0.9.4")));
        assert!(version_supported(Some("development")));
        assert!(version_supported(None));
        assert_eq!(parse_version("v1.14.20"), Some((1, 14, 20)));
        assert_eq!(parse_version("garbage"), None);
    }
}
