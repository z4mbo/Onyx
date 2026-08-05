use super::{
    cli::CliSession,
    driver::{
        DriverFuture, MAX_USER_INPUT_OPTIONS, MAX_USER_INPUT_QUESTIONS, ProviderActivity,
        ProviderApproval, ProviderDriver, ProviderEvent, ProviderSession, ProviderSessionConfig,
        ProviderUserInput, ProviderUserInputAnswers, ProviderUserInputOption,
        ProviderUserInputPrompt, ProviderUserInputQuestion, ProviderUserInputResponse,
    },
    normalize::{NormalizedEvent, StreamNormalizer},
    process::{JsonProcess, ProcessOutput, find_executable, platform_command},
};
use crate::model::{
    AccessMode, ApprovalDecision, ContextUsage, InteractionMode, MessageKind, ProviderId,
    ProviderModelOption, ReasoningEffort, SpeedMode,
};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{sync::mpsc, time::timeout};
use tokio_util::sync::CancellationToken;

const MAX_PERSISTENT_STREAM: usize = 256 * 1024 * 1024;
const MAX_CATALOG_STREAM: usize = 16 * 1024 * 1024;
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(60);
const CATALOG_TIMEOUT: Duration = Duration::from_secs(15);
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

fn persistent_args(config: &ProviderSessionConfig) -> Vec<String> {
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
            (_, AccessMode::ApprovalRequired) => "default",
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
    if let Some(reasoning) = config.reasoning.and_then(claude_effort) {
        args.extend(["--effort".to_string(), reasoning.to_string()]);
    }
    let mut settings = serde_json::Map::new();
    if config.speed_mode == SpeedMode::Fast {
        settings.insert("fastMode".into(), Value::Bool(true));
    }
    if !settings.is_empty() {
        args.extend([
            "--settings".to_string(),
            Value::Object(settings).to_string(),
        ]);
    }
    if let Some(id) = config.continuation.as_deref() {
        args.extend(["--resume".to_string(), id.to_string()]);
    }
    if let Some(model) = config.model() {
        args.extend(["--model".to_string(), model.to_string()]);
    }
    args
}

fn catalog_probe_args() -> Vec<String> {
    vec![
        "--print".to_string(),
        "--verbose".to_string(),
        "--input-format".to_string(),
        "stream-json".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--permission-prompt-tool".to_string(),
        "stdio".to_string(),
        "--permission-mode".to_string(),
        "default".to_string(),
    ]
}

impl ClaudeSession {
    async fn connect(config: ProviderSessionConfig) -> Result<Self, String> {
        let executable = find_executable("claude")
            .ok_or_else(|| "Claude Code is not installed or was not found on PATH".to_string())?;
        let args = persistent_args(&config);
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
            continuation: discovered_continuation.continuation.or(config.continuation),
        })
    }

    async fn run_turn_inner(
        &mut self,
        prompt: &str,
        cancellation: &CancellationToken,
        events: &mpsc::Sender<ProviderEvent>,
        mut steer: mpsc::UnboundedReceiver<String>,
    ) -> Result<(), String> {
        self.process.send_json(&user_message(prompt)).await?;
        let mut normalizer = StreamNormalizer::new(ProviderId::Claude);
        let mut steer_open = true;

        loop {
            // `next_stdout` is a cancel-safe channel read, so losing the race
            // against a steering message never drops a stdout line.
            enum TurnInput {
                Steer(Option<String>),
                Output(ProcessOutput),
            }
            let input = tokio::select! {
                _ = cancellation.cancelled() => {
                    self.process.shutdown().await;
                    return Err("Turn cancelled".to_string());
                }
                message = steer.recv(), if steer_open => TurnInput::Steer(message),
                output = self.process.next_stdout() => TurnInput::Output(output?),
            };
            let value = match input {
                TurnInput::Steer(Some(text)) => {
                    self.process.send_json(&user_message(&text)).await?;
                    continue;
                }
                TurnInput::Steer(None) => {
                    steer_open = false;
                    continue;
                }
                TurnInput::Output(ProcessOutput::Stdout(line)) => {
                    serde_json::from_str::<Value>(&line).map_err(|error| {
                        format!("Claude Code returned invalid stream JSON: {error}")
                    })?
                }
                TurnInput::Output(ProcessOutput::Exited(status)) => {
                    let detail = self.process.stderr_tail();
                    return Err(if detail.is_empty() {
                        format!("Claude Code stream transport exited with {status}")
                    } else {
                        format!("Claude Code stream transport exited with {status}: {detail}")
                    });
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
                                id: None,
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
        let input = request.get("input").cloned().unwrap_or_else(|| json!({}));

        // Plan mode: capture the proposed plan for review instead of asking the
        // user to "approve a tool". Denying ends the turn cleanly and the plan
        // stays visible in the transcript until they switch to Build.
        if tool_name == "ExitPlanMode" || tool_name == "exit_plan_mode" {
            if let Some(plan) = input
                .get("plan")
                .and_then(Value::as_str)
                .filter(|plan| !plan.trim().is_empty())
            {
                send_event(
                    events,
                    ProviderEvent::Text(format!("## Proposed plan\n\n{plan}")),
                )
                .await?;
            }
            send_event(
                events,
                ProviderEvent::Activity(ProviderActivity {
                    id: None,
                    title: "Plan ready for review".to_string(),
                    detail: Some("Switch the session to Build to run it".to_string()),
                    kind: MessageKind::Text,
                }),
            )
            .await?;
            return self
                .process
                .send_json(&control_response(
                    request_id,
                    permission_deny(
                        "Onyx captured your proposed plan for the user to review. Stop here and wait for their decision before taking any further action.",
                    ),
                ))
                .await;
        }

        if tool_name == "AskUserQuestion" {
            return self
                .handle_ask_user_question(request_id, input, cancellation, events)
                .await;
        }

        let title = request
            .get("title")
            .or_else(|| request.get("display_name"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("Allow Claude Code to use {tool_name}?"));
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
        let decision = tokio::select! {
            _ = cancellation.cancelled() => {
                self.process.shutdown().await;
                return Err("Turn cancelled".to_string());
            }
            result = receiver => result.unwrap_or(ApprovalDecision::Deny),
        };
        let permission = match decision {
            ApprovalDecision::Deny => permission_deny("Denied by user"),
            ApprovalDecision::Allow => permission_allow(input, None),
            ApprovalDecision::AllowSession => {
                permission_allow(input, request.get("suggestions").cloned())
            }
        };
        self.process
            .send_json(&control_response(request_id, permission))
            .await
    }

    async fn handle_ask_user_question(
        &mut self,
        request_id: &str,
        input: Value,
        cancellation: &CancellationToken,
        events: &mpsc::Sender<ProviderEvent>,
    ) -> Result<(), String> {
        let prompt = claude_user_input_prompt(&input)?;
        let prompt_for_response = prompt.clone();
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
            _ = cancellation.cancelled() => {
                self.process.shutdown().await;
                return Err("Turn cancelled".to_string());
            }
            response = receiver => response.unwrap_or(ProviderUserInputResponse::Cancelled),
        };
        let permission = match response {
            ProviderUserInputResponse::Answered(answers) => {
                claude_user_input_permission(input, &prompt_for_response, &answers)?
            }
            ProviderUserInputResponse::Cancelled => {
                permission_deny("User cancelled the interactive question")
            }
        };
        self.process
            .send_json(&control_response(request_id, permission))
            .await
    }
}

fn claude_user_input_prompt(input: &Value) -> Result<ProviderUserInputPrompt, String> {
    let raw_questions = input
        .get("questions")
        .and_then(Value::as_array)
        .ok_or_else(|| "Claude AskUserQuestion did not include questions".to_string())?;
    if raw_questions.len() > MAX_USER_INPUT_QUESTIONS {
        return Err(format!(
            "Claude AskUserQuestion exceeded the {MAX_USER_INPUT_QUESTIONS}-question limit"
        ));
    }

    let mut questions = Vec::with_capacity(raw_questions.len());
    for (index, raw) in raw_questions.iter().enumerate() {
        let question = required_field(raw, "question", "Claude question text")?;
        let raw_options = match raw.get("options") {
            Some(Value::Array(options)) => options.as_slice(),
            Some(Value::Null) | None => &[],
            Some(_) => return Err("Claude question options were not an array".to_string()),
        };
        if raw_options.len() > MAX_USER_INPUT_OPTIONS {
            return Err(format!(
                "Claude question exceeded the {MAX_USER_INPUT_OPTIONS}-option limit"
            ));
        }
        let options = raw_options
            .iter()
            .map(|option| {
                Ok(ProviderUserInputOption {
                    label: required_field(option, "label", "Claude question option label")?
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
            // Claude Code indexes the returned answer map by the full question
            // text rather than an independent tool-provided id.
            id: question.to_string(),
            header: raw
                .get("header")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("Question {}", index + 1)),
            question: question.to_string(),
            options,
            multi_select: raw
                .get("multiSelect")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            // Claude Code always offers an "Other" free-text answer.
            allow_other: true,
            secret: false,
        });
    }
    ProviderUserInputPrompt::new("Claude needs your input", questions, None)
}

fn claude_user_input_permission(
    mut input: Value,
    prompt: &ProviderUserInputPrompt,
    answers: &ProviderUserInputAnswers,
) -> Result<Value, String> {
    let input = input
        .as_object_mut()
        .ok_or_else(|| "Claude AskUserQuestion input was not an object".to_string())?;
    let mut encoded = serde_json::Map::new();
    for question in &prompt.questions {
        let values = answers
            .get(&question.id)
            .ok_or_else(|| format!("Missing answer for {}", question.header))?;
        let value = if question.multi_select {
            json!(values)
        } else {
            json!(
                values
                    .first()
                    .ok_or_else(|| format!("Missing answer for {}", question.header))?
            )
        };
        encoded.insert(question.id.clone(), value);
    }
    input.insert("answers".to_string(), Value::Object(encoded));
    Ok(permission_allow(Value::Object(input.clone()), None))
}

fn required_field<'a>(value: &'a Value, field: &str, label: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{label} is missing"))
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

    fn supports_steering(&self) -> bool {
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
            if let Some(id) = self.continuation.clone() {
                send_event(&events, ProviderEvent::Continuation(id)).await?;
            }
            self.run_turn_inner(prompt, cancellation, &events, steer)
                .await
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

#[derive(Debug)]
struct ClaudeInitialize {
    continuation: Option<String>,
    models: Vec<ProviderModelOption>,
}

pub(super) fn claude_effort(effort: ReasoningEffort) -> Option<&'static str> {
    match effort {
        ReasoningEffort::Low => Some("low"),
        ReasoningEffort::Medium => Some("medium"),
        ReasoningEffort::High => Some("high"),
        ReasoningEffort::Xhigh => Some("xhigh"),
        ReasoningEffort::Max => Some("max"),
        // Auto is Onyx's name for leaving the choice to Claude Code, which is
        // why the CLI itself offers no such level. It omits `--effort` rather
        // than sending a value Claude Code does not accept.
        ReasoningEffort::Auto
        | ReasoningEffort::None
        | ReasoningEffort::Minimal
        | ReasoningEffort::Ultra => None,
    }
}

fn parse_claude_effort(value: &str) -> Option<ReasoningEffort> {
    match value {
        "low" => Some(ReasoningEffort::Low),
        "medium" => Some(ReasoningEffort::Medium),
        "high" => Some(ReasoningEffort::High),
        "xhigh" => Some(ReasoningEffort::Xhigh),
        "max" => Some(ReasoningEffort::Max),
        _ => None,
    }
}

fn parse_initialize_models(value: &Value) -> Vec<ProviderModelOption> {
    let Some(models) = value
        .pointer("/response/response/models")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    models
        .iter()
        .filter_map(|model| {
            let id = model
                .get("value")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            let supports_effort = model
                .get("supportsEffort")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| model.get("supportedEffortLevels").is_some());
            let mut reasoning = Vec::new();
            if supports_effort {
                // `None` means "let Claude Code choose" in Onyx. It is never
                // emitted as an `--effort` value.
                reasoning.push(ReasoningEffort::Auto);
                for effort in model
                    .get("supportedEffortLevels")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter_map(parse_claude_effort)
                {
                    if !reasoning.contains(&effort) {
                        reasoning.push(effort);
                    }
                }
            }
            let supports_fast = model
                .get("supportsFastMode")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Some(ProviderModelOption {
                id: id.to_string(),
                name: model
                    .get("displayName")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(id)
                    .to_string(),
                description: model
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                is_default: id == "default",
                legacy: is_legacy_claude_model(id),
                reasoning,
                default_reasoning: supports_effort.then_some(ReasoningEffort::Auto),
                speeds: if supports_fast {
                    vec![SpeedMode::Standard, SpeedMode::Fast]
                } else {
                    vec![SpeedMode::Standard]
                },
                default_speed: SpeedMode::Standard,
                context_length: None,
            })
        })
        .collect()
}

/// Follows T3 Code's convention: only the current model families stay at the
/// top level; superseded versions fold behind a "Legacy models" group.
/// Versionless ids ("sonnet", "opusplan") are rolling aliases and never legacy.
fn is_legacy_claude_model(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    if id == "default" || !id.chars().any(|character| character.is_ascii_digit()) {
        return false;
    }
    !(id.contains("fable-5")
        || id.contains("opus-5")
        || id.contains("sonnet-5")
        || id.contains("haiku-5"))
}

fn configured_default_model_option() -> ProviderModelOption {
    ProviderModelOption {
        id: "default".to_string(),
        name: "Use Claude Code setting".to_string(),
        description: Some(
            "Do not pass --model; use the model selected by Claude Code's account and configuration."
                .to_string(),
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

async fn wait_for_initialize(process: &mut JsonProcess) -> Result<ClaudeInitialize, String> {
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
                        Some("success") => Ok(ClaudeInitialize {
                            continuation,
                            models: parse_initialize_models(&value),
                        }),
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

pub async fn model_catalog() -> Result<Vec<ProviderModelOption>, String> {
    let executable = find_executable("claude")
        .ok_or_else(|| "Claude Code is not installed or was not found on PATH".to_string())?;
    let args = catalog_probe_args();
    let mut command = platform_command(&executable, &args);
    command.env_remove("CLAUDECODE");
    let mut process = JsonProcess::spawn(
        command,
        "Claude Code model catalog probe",
        MAX_CATALOG_STREAM,
    )
    .await?;
    let result = async {
        process.send_json(&initialize_request()).await?;
        timeout(CATALOG_TIMEOUT, wait_for_initialize(&mut process))
            .await
            .map_err(|_| "Claude Code model catalog probe timed out".to_string())?
            .map(|initialize| {
                let mut models = initialize.models;
                if !models.iter().any(|model| model.is_default) {
                    models.insert(0, configured_default_model_option());
                }
                models
            })
    }
    .await;
    process.shutdown().await;
    result
}

fn permission_allow(input: Value, suggestions: Option<Value>) -> Value {
    let mut permission = json!({ "behavior": "allow", "updatedInput": input });
    // Claude Code persists these permission-rule suggestions for the session,
    // mirroring the SDK's "accept for session" flow.
    if let Some(suggestions) = suggestions.filter(|value| {
        value
            .as_array()
            .is_some_and(|suggestions| !suggestions.is_empty())
    }) {
        permission["updatedPermissions"] = suggestions;
    }
    permission
}

fn permission_deny(message: &str) -> Value {
    json!({ "behavior": "deny", "message": message, "interrupt": false })
}

fn control_response(request_id: &str, permission: Value) -> Value {
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
    use super::{
        catalog_probe_args, claude_user_input_permission, claude_user_input_prompt,
        configured_default_model_option, control_error, control_response, initialize_request,
        parse_initialize_models, permission_allow, permission_deny, persistent_args, user_message,
    };
    use crate::{
        model::{AccessMode, InteractionMode, ProviderId, ReasoningEffort, SpeedMode},
        providers::driver::{ProviderSessionConfig, ProviderUserInputAnswers},
    };
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn persistent_transport_maps_session_options_to_claude_code() {
        let args = persistent_args(&ProviderSessionConfig {
            provider: ProviderId::Claude,
            model: Some("opus".to_string()),
            workspace: PathBuf::from("/tmp/project"),
            continuation: Some("session-1".to_string()),
            reasoning: Some(ReasoningEffort::High),
            speed_mode: SpeedMode::Fast,
            interaction_mode: InteractionMode::Plan,
            access_mode: AccessMode::FullAccess,
        });
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--input-format", "stream-json"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--permission-mode", "plan"])
        );
        assert!(args.windows(2).any(|pair| pair == ["--effort", "high"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--settings", r#"{"fastMode":true}"#])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--resume", "session-1"])
        );
        assert!(args.windows(2).any(|pair| pair == ["--model", "opus"]));
        assert!(
            !args
                .iter()
                .any(|arg| arg == "--dangerously-skip-permissions")
        );
    }

    #[test]
    fn persistent_transport_omits_non_claude_effort_values() {
        for reasoning in [
            // Auto is Onyx's "let Claude Code decide", not a CLI effort level.
            ReasoningEffort::Auto,
            ReasoningEffort::None,
            ReasoningEffort::Minimal,
            ReasoningEffort::Ultra,
        ] {
            let args = persistent_args(&ProviderSessionConfig {
                provider: ProviderId::Claude,
                model: None,
                workspace: PathBuf::from("/tmp/project"),
                continuation: None,
                reasoning: Some(reasoning),
                speed_mode: SpeedMode::Standard,
                interaction_mode: InteractionMode::Build,
                access_mode: AccessMode::AutoAcceptEdits,
            });
            assert!(!args.iter().any(|arg| arg == "--effort"));
            assert!(!args.iter().any(|arg| arg == "--settings"));
            assert!(!args.iter().any(|arg| arg.contains("ultracode")));
        }
    }

    #[test]
    fn catalog_probe_uses_control_stream_without_a_prompt() {
        let args = catalog_probe_args();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--input-format", "stream-json"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--output-format", "stream-json"])
        );
        assert!(!args.iter().any(|arg| arg == "--resume" || arg == "--model"));
        assert_eq!(args.last().map(String::as_str), Some("default"));
    }

    #[test]
    fn initialize_fixture_maps_per_model_capabilities() {
        let response = json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": "onyx-initialize",
                "response": {
                    "models": [
                        {
                            "value": "claude-opus-example",
                            "displayName": "Opus Example",
                            "description": "Most capable",
                            "supportsEffort": true,
                            "supportedEffortLevels": [
                                "low", "medium", "high", "xhigh", "max",
                                "minimal", "ultracode", "high"
                            ],
                            "supportsFastMode": true
                        },
                        {
                            "value": "claude-haiku-example",
                            "displayName": "Haiku Example",
                            "supportsEffort": false,
                            "supportedEffortLevels": ["high"],
                            "supportsFastMode": false
                        }
                    ]
                }
            }
        });
        let models = parse_initialize_models(&response);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "claude-opus-example");
        assert_eq!(models[0].name, "Opus Example");
        assert_eq!(models[0].description.as_deref(), Some("Most capable"));
        assert_eq!(
            models[0].reasoning,
            vec![
                ReasoningEffort::Auto,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::Xhigh,
                ReasoningEffort::Max,
            ]
        );
        assert_eq!(models[0].default_reasoning, Some(ReasoningEffort::Auto));
        assert_eq!(models[0].speeds, vec![SpeedMode::Standard, SpeedMode::Fast]);
        assert!(models[1].reasoning.is_empty());
        assert_eq!(models[1].default_reasoning, None);
        assert_eq!(models[1].speeds, vec![SpeedMode::Standard]);
    }

    #[test]
    fn initialize_fixture_skips_invalid_models_without_inventing_fallbacks() {
        let response = json!({
            "response": {
                "response": {
                    "models": [
                        {"value": ""},
                        {"displayName": "Missing value"},
                        {"value": "configured", "supportedEffortLevels": ["future-level"]}
                    ]
                }
            }
        });
        let models = parse_initialize_models(&response);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "configured");
        assert_eq!(models[0].name, "configured");
        assert_eq!(models[0].reasoning, vec![ReasoningEffort::Auto]);
        assert!(!models[0].is_default);

        let configured = configured_default_model_option();
        assert_eq!(configured.id, "default");
        assert!(configured.is_default);
        assert!(configured.reasoning.is_empty());
    }

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
        let response = control_response(
            "request-1",
            permission_allow(json!({"command":"cargo test"}), None),
        );
        assert_eq!(response["response"]["subtype"], "success");
        assert_eq!(
            response["response"]["response"]["updatedInput"]["command"],
            "cargo test"
        );
        assert!(
            response["response"]["response"]
                .get("updatedPermissions")
                .is_none()
        );
    }

    #[test]
    fn session_scope_forwards_permission_suggestions() {
        let suggestions = json!([{ "type": "addRules", "rules": [{ "toolName": "Bash" }] }]);
        let allow = permission_allow(json!({}), Some(suggestions.clone()));
        assert_eq!(allow["updatedPermissions"], suggestions);
        assert!(
            permission_allow(json!({}), Some(json!([])))
                .get("updatedPermissions")
                .is_none()
        );
        let deny = permission_deny("no");
        assert_eq!(deny["behavior"], "deny");
        assert_eq!(deny["message"], "no");
    }

    #[test]
    fn unsupported_control_requests_receive_protocol_error() {
        let response = control_error("request-2", "unsupported");
        assert_eq!(response["response"]["subtype"], "error");
        assert_eq!(response["response"]["request_id"], "request-2");
    }

    #[test]
    fn ask_user_question_round_trips_structured_answers() {
        let input = json!({
            "questions": [{
                "header": "Scope",
                "question": "Which scope should I use?",
                "options": [
                    {"label": "Focused", "description": "Only this module"},
                    {"label": "Broad", "description": "The whole workspace"}
                ],
                "multiSelect": false
            }]
        });
        let prompt = claude_user_input_prompt(&input).expect("valid AskUserQuestion");
        assert_eq!(prompt.questions[0].id, "Which scope should I use?");
        assert!(prompt.questions[0].allow_other);
        let answers = ProviderUserInputAnswers::from([(
            "Which scope should I use?".to_string(),
            vec!["Focused".to_string()],
        )]);
        let permission =
            claude_user_input_permission(input, &prompt, &answers).expect("permission");
        assert_eq!(permission["behavior"], "allow");
        assert_eq!(
            permission["updatedInput"]["answers"]["Which scope should I use?"],
            "Focused"
        );
    }

    #[test]
    fn malformed_ask_user_question_is_not_auto_denied_or_answered() {
        assert!(claude_user_input_prompt(&json!({"questions": []})).is_err());
        assert!(
            claude_user_input_prompt(&json!({
                "questions": [{"header": "Missing prompt"}]
            }))
            .is_err()
        );
    }
}
