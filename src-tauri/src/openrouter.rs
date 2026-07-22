use crate::model::{
    AgentSession, ApprovalRequest, Message, MessageKind, MessageRole, OpenRouterModel,
    OpenRouterStatus, SessionEvent,
};
#[cfg(windows)]
use crate::providers::process::WindowsJob;
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use chrono::Utc;
use parking_lot::Mutex as ParkingMutex;
use reqwest::{Client, Response, StatusCode};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    sync::oneshot,
    time::timeout,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use walkdir::WalkDir;

const SERVICE: &str = "com.z4mbo.zai";
const ACCOUNT: &str = "openrouter";
const API_BASE: &str = "https://openrouter.ai/api/v1";
const MAX_TOOL_ROUNDS: usize = 12;
const MAX_HISTORY_MESSAGES: usize = 64;
const MAX_HISTORY_CONTENT_BYTES: usize = 512 * 1024;
const MAX_CURRENT_PROMPT_BYTES: usize = 256 * 1024;
const MAX_MODEL_ID_BYTES: usize = 512;
const MAX_CHAT_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_MODELS_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_AUTH_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_ERROR_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_ERROR_MESSAGE_BYTES: usize = 2 * 1024;
const MAX_ASSISTANT_CONTENT_BYTES: usize = 1024 * 1024;
const MAX_TOOL_CALLS_PER_ROUND: usize = 16;
const MAX_TOOL_CALLS_PER_TURN: usize = 64;
const MAX_TOOL_CALL_ID_BYTES: usize = 256;
const MAX_TOOL_NAME_BYTES: usize = 128;
// A 1 MiB write payload can expand to roughly 6 MiB when represented as JSON
// function arguments (for example, when every byte needs a Unicode escape).
const MAX_TOOL_ARGUMENTS_BYTES: usize = 7 * 1024 * 1024;
const MAX_ACTIVITY_DETAIL_BYTES: usize = 16 * 1024;
const MAX_TURN_ACTIVITY_BYTES: usize = 256 * 1024;
const MAX_TOOL_RESULT_BYTES: usize = 256 * 1024;
const MAX_TURN_TOOL_RESULT_BYTES: usize = 2 * 1024 * 1024;
const MAX_SEARCH_RESULT_BYTES: usize = 256 * 1024;
const MAX_SEARCH_FILE_BYTES: usize = 512 * 1024;
const MAX_SEARCH_SCANNED_BYTES: usize = 64 * 1024 * 1024;
const MAX_REQUEST_CONTEXT_BYTES: usize = 12 * 1024 * 1024;
const MAX_TURN_CONTEXT_GROWTH_BYTES: usize = 10 * 1024 * 1024;
const FILE_READ_TIMEOUT: Duration = Duration::from_secs(15);
const FILE_SEARCH_TIMEOUT: Duration = Duration::from_secs(45);

const SYSTEM_PROMPT: &str = "You are zAI, a coding agent working in the user's selected workspace. Inspect files before editing. Use tools when needed, make focused changes, and explain the result. File writes and shell commands always require the user's explicit approval. Never claim a tool succeeded until its result is returned.";
const TRUNCATION_MARKER: &str = "\n...[truncated by zAI safety limit]";

#[derive(Debug)]
struct ParsedToolCall {
    id: String,
    name: String,
    arguments_text: String,
    arguments: Value,
}

pub type ApprovalRegistry = Arc<ParkingMutex<HashMap<String, oneshot::Sender<bool>>>>;

pub struct OpenRouterRunResult {
    pub content: String,
    pub activities: Vec<Message>,
}

pub async fn status() -> OpenRouterStatus {
    OpenRouterStatus {
        connected: read_key().await.is_ok_and(|value| !value.trim().is_empty()),
    }
}

pub async fn save_key(value: String) -> Result<OpenRouterStatus, String> {
    let key = value.trim().to_string();
    if key.is_empty() || key.len() > 8192 {
        return Err("Enter a valid OpenRouter API key".to_string());
    }
    validate_key(&key).await?;
    tokio::task::spawn_blocking(move || {
        keyring::Entry::new(SERVICE, ACCOUNT)
            .map_err(|error| error.to_string())?
            .set_password(&key)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(OpenRouterStatus { connected: true })
}

pub async fn clear_key() -> Result<OpenRouterStatus, String> {
    tokio::task::spawn_blocking(|| {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|error| error.to_string())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(OpenRouterStatus { connected: false })
}

pub async fn models() -> Result<Vec<OpenRouterModel>, String> {
    let key = read_key().await?;
    fetch_models(&key).await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_turn(
    app: AppHandle,
    session: AgentSession,
    prompt: String,
    message_id: String,
    cancellation: CancellationToken,
    approvals: ApprovalRegistry,
) -> Result<OpenRouterRunResult, String> {
    let model = session
        .model
        .as_deref()
        .filter(|value| !value.is_empty() && *value != "default")
        .ok_or_else(|| "Choose an OpenRouter model before starting this session".to_string())?;
    if model.len() > MAX_MODEL_ID_BYTES || model.chars().any(char::is_control) {
        return Err("The selected OpenRouter model identifier is invalid".to_string());
    }
    let key = read_key().await?;
    let workspace = canonical_workspace(&session.workspace)?;
    let mut messages = build_initial_messages(&session.messages, &prompt)?;
    let mut request_context_bytes = encoded_messages_len(&messages)?;
    let mut turn_context_growth_bytes = 0_usize;

    let client = Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|error| error.to_string())?;
    let mut final_content = String::new();
    let mut activities = Vec::new();
    let mut tool_call_count = 0_usize;
    let mut activity_bytes = 0_usize;
    let mut tool_result_bytes = 0_usize;

    for _ in 0..MAX_TOOL_ROUNDS {
        let request = client
            .post(format!("{API_BASE}/chat/completions"))
            .bearer_auth(&key)
            .header("HTTP-Referer", "https://github.com/z4mbo/zAI")
            .header("X-OpenRouter-Title", "zAI")
            .json(&json!({
                "model": model,
                "messages": messages,
                "tools": tool_definitions(),
                "tool_choice": "auto"
            }));
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err("Turn cancelled".to_string()),
            response = request.send() => response.map_err(safe_http_error)?,
        };
        let status = response.status();
        let body_limit = if status.is_success() {
            MAX_CHAT_RESPONSE_BYTES
        } else {
            MAX_ERROR_RESPONSE_BYTES
        };
        let response_body =
            read_http_body_limited(response, body_limit)
                .await
                .map_err(|error| {
                    if status.is_success() {
                        format!("OpenRouter response {error}")
                    } else {
                        format!("{}; response body {error}", openrouter_error(status, ""))
                    }
                })?;
        if !status.is_success() {
            return Err(openrouter_error(
                status,
                String::from_utf8_lossy(&response_body),
            ));
        }
        let body: Value = serde_json::from_slice(&response_body)
            .map_err(|_| "OpenRouter returned invalid JSON".to_string())?;
        let assistant = body
            .pointer("/choices/0/message")
            .ok_or_else(|| "OpenRouter returned no assistant message".to_string())?;
        let content =
            extract_content_limited(assistant.get("content"), MAX_ASSISTANT_CONTENT_BYTES)?;
        let tool_calls = parse_tool_calls(assistant.get("tool_calls"))?;
        tool_call_count = tool_call_count
            .checked_add(tool_calls.len())
            .ok_or_else(|| "OpenRouter returned too many tool calls".to_string())?;
        if tool_call_count > MAX_TOOL_CALLS_PER_TURN {
            return Err(format!(
                "OpenRouter exceeded the {MAX_TOOL_CALLS_PER_TURN}-tool-call per-turn safety limit"
            ));
        }
        let assistant_message = assistant_context_message(&content, &tool_calls);
        push_turn_context_message(
            &mut messages,
            assistant_message,
            &mut request_context_bytes,
            &mut turn_context_growth_bytes,
        )?;
        if !content.is_empty() {
            append_assistant_content(&mut final_content, &content)?;
            let _ = app.emit(
                "zai://session",
                SessionEvent::Delta {
                    session_id: session.id.clone(),
                    message_id: message_id.clone(),
                    delta: content.clone(),
                },
            );
        }
        if tool_calls.is_empty() {
            let content = final_content.trim().to_string();
            if content.is_empty() {
                return Err("OpenRouter completed without returning a message".to_string());
            }
            return Ok(OpenRouterRunResult {
                content,
                activities,
            });
        }

        for call in tool_calls {
            let activity_remaining = MAX_TURN_ACTIVITY_BYTES.saturating_sub(activity_bytes);
            if activity_remaining == 0 {
                return Err(format!(
                    "OpenRouter exceeded the {MAX_TURN_ACTIVITY_BYTES}-byte activity safety limit"
                ));
            }
            let activity_detail = bounded_activity_detail(
                &call.name,
                &call.arguments,
                activity_remaining.min(MAX_ACTIVITY_DETAIL_BYTES),
            );
            activity_bytes = activity_bytes.saturating_add(activity_detail.len());
            let activity = Message::new(MessageRole::Tool, MessageKind::Tool, activity_detail);
            let _ = app.emit(
                "zai://session",
                SessionEvent::Activity {
                    session_id: session.id.clone(),
                    message: activity.clone(),
                },
            );
            activities.push(activity);
            let result = execute_tool(
                &app,
                &session.id,
                &workspace,
                &call.name,
                &call.arguments,
                &cancellation,
                &approvals,
            )
            .await;
            let raw_result = result.unwrap_or_else(|error| format!("Tool error: {error}"));
            let result_remaining = MAX_TURN_TOOL_RESULT_BYTES.saturating_sub(tool_result_bytes);
            if result_remaining == 0 {
                return Err(format!(
                    "OpenRouter exceeded the {MAX_TURN_TOOL_RESULT_BYTES}-byte tool-result safety limit"
                ));
            }
            let tool_result = truncate_utf8_with_marker(
                &raw_result,
                result_remaining.min(MAX_TOOL_RESULT_BYTES),
                TRUNCATION_MARKER,
            );
            tool_result_bytes = tool_result_bytes.saturating_add(tool_result.len());
            let tool_message = json!({
                "role": "tool",
                "tool_call_id": call.id,
                "content": tool_result
            });
            push_turn_context_message(
                &mut messages,
                tool_message,
                &mut request_context_bytes,
                &mut turn_context_growth_bytes,
            )?;
        }
    }
    Err("OpenRouter reached the tool-call safety limit for this turn".to_string())
}

fn build_initial_messages(history: &[Message], prompt: &str) -> Result<Vec<Value>, String> {
    if prompt.len() > MAX_CURRENT_PROMPT_BYTES {
        return Err(format!(
            "OpenRouter prompts are limited to {MAX_CURRENT_PROMPT_BYTES} bytes"
        ));
    }

    let mut eligible = history
        .iter()
        .filter(|message| {
            message.kind == MessageKind::Text
                && matches!(message.role, MessageRole::User | MessageRole::Assistant)
        })
        .collect::<Vec<_>>();
    if eligible
        .last()
        .is_some_and(|message| message.role == MessageRole::User && message.content == prompt)
    {
        eligible.pop();
    }

    let mut remaining_bytes = MAX_HISTORY_CONTENT_BYTES;
    let mut selected = Vec::new();
    for message in eligible.iter().rev().take(MAX_HISTORY_MESSAGES) {
        if remaining_bytes == 0 {
            break;
        }
        let content =
            truncate_utf8_with_marker(&message.content, remaining_bytes, TRUNCATION_MARKER);
        if content.is_empty() && !message.content.is_empty() {
            break;
        }
        remaining_bytes = remaining_bytes.saturating_sub(content.len());
        selected.push(json!({
            "role": if message.role == MessageRole::User { "user" } else { "assistant" },
            "content": content,
        }));
    }
    selected.reverse();

    let mut messages = Vec::with_capacity(selected.len() + 2);
    messages.push(json!({ "role": "system", "content": SYSTEM_PROMPT }));
    messages.extend(selected);
    messages.push(json!({ "role": "user", "content": prompt }));
    let encoded_len = encoded_messages_len(&messages)?;
    if encoded_len > MAX_REQUEST_CONTEXT_BYTES {
        return Err(format!(
            "OpenRouter request context exceeds the {MAX_REQUEST_CONTEXT_BYTES}-byte safety limit"
        ));
    }
    Ok(messages)
}

fn encoded_messages_len(messages: &[Value]) -> Result<usize, String> {
    serde_json::to_vec(messages)
        .map(|encoded| encoded.len())
        .map_err(|_| "Could not encode the OpenRouter request context".to_string())
}

fn push_turn_context_message(
    messages: &mut Vec<Value>,
    message: Value,
    request_context_bytes: &mut usize,
    turn_context_growth_bytes: &mut usize,
) -> Result<(), String> {
    let encoded_len = serde_json::to_vec(&message)
        .map_err(|_| "Could not encode an OpenRouter context message".to_string())?
        .len();
    let growth = encoded_len.saturating_add(usize::from(!messages.is_empty()));
    let next_context = request_context_bytes
        .checked_add(growth)
        .ok_or_else(|| "OpenRouter request context size overflowed".to_string())?;
    let next_growth = turn_context_growth_bytes
        .checked_add(growth)
        .ok_or_else(|| "OpenRouter per-turn context growth overflowed".to_string())?;
    if next_context > MAX_REQUEST_CONTEXT_BYTES {
        return Err(format!(
            "OpenRouter request context exceeded the {MAX_REQUEST_CONTEXT_BYTES}-byte safety limit"
        ));
    }
    if next_growth > MAX_TURN_CONTEXT_GROWTH_BYTES {
        return Err(format!(
            "OpenRouter exceeded the {MAX_TURN_CONTEXT_GROWTH_BYTES}-byte per-turn context growth limit"
        ));
    }
    messages.push(message);
    *request_context_bytes = next_context;
    *turn_context_growth_bytes = next_growth;
    Ok(())
}

fn parse_tool_calls(value: Option<&Value>) -> Result<Vec<ParsedToolCall>, String> {
    let calls = match value {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(calls)) => calls,
        _ => return Err("OpenRouter returned invalid tool-call data".to_string()),
    };
    if calls.len() > MAX_TOOL_CALLS_PER_ROUND {
        return Err(format!(
            "OpenRouter exceeded the {MAX_TOOL_CALLS_PER_ROUND}-tool-call per-round safety limit"
        ));
    }

    calls
        .iter()
        .map(|call| {
            if call
                .get("type")
                .is_some_and(|kind| kind.as_str() != Some("function"))
            {
                return Err("OpenRouter returned an unsupported tool-call type".to_string());
            }
            let id =
                bounded_protocol_string(call.get("id"), "tool-call id", MAX_TOOL_CALL_ID_BYTES)?;
            let name = bounded_protocol_string(
                call.pointer("/function/name"),
                "tool name",
                MAX_TOOL_NAME_BYTES,
            )?;
            if !name.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
            }) {
                return Err("OpenRouter returned an invalid tool name".to_string());
            }
            let arguments_text = bounded_protocol_string(
                call.pointer("/function/arguments"),
                "tool arguments",
                MAX_TOOL_ARGUMENTS_BYTES,
            )?;
            let arguments = serde_json::from_str::<Value>(&arguments_text)
                .map_err(|_| "OpenRouter returned invalid JSON tool arguments".to_string())?;
            if !arguments.is_object() {
                return Err("OpenRouter tool arguments must be a JSON object".to_string());
            }
            Ok(ParsedToolCall {
                id,
                name,
                arguments_text,
                arguments,
            })
        })
        .collect()
}

fn bounded_protocol_string(
    value: Option<&Value>,
    label: &str,
    max_bytes: usize,
) -> Result<String, String> {
    let value = value
        .and_then(Value::as_str)
        .ok_or_else(|| format!("OpenRouter returned a missing or invalid {label}"))?;
    if value.is_empty() || value.len() > max_bytes {
        return Err(format!(
            "OpenRouter {label} exceeds its {max_bytes}-byte safety limit"
        ));
    }
    if value.chars().any(char::is_control) && label != "tool arguments" {
        return Err(format!("OpenRouter returned an invalid {label}"));
    }
    Ok(value.to_string())
}

fn assistant_context_message(content: &str, calls: &[ParsedToolCall]) -> Value {
    let mut message = json!({
        "role": "assistant",
        "content": if content.is_empty() { Value::Null } else { Value::String(content.to_string()) },
    });
    if !calls.is_empty() {
        message["tool_calls"] = Value::Array(
            calls
                .iter()
                .map(|call| {
                    json!({
                        "id": call.id,
                        "type": "function",
                        "function": {
                            "name": call.name,
                            "arguments": call.arguments_text,
                        }
                    })
                })
                .collect(),
        );
    }
    message
}

fn append_assistant_content(output: &mut String, content: &str) -> Result<(), String> {
    let separator_bytes = if output.is_empty() { 0 } else { 2 };
    let next_len = output
        .len()
        .checked_add(separator_bytes)
        .and_then(|length| length.checked_add(content.len()))
        .ok_or_else(|| "OpenRouter assistant content size overflowed".to_string())?;
    if next_len > MAX_ASSISTANT_CONTENT_BYTES {
        return Err(format!(
            "OpenRouter assistant content exceeded the {MAX_ASSISTANT_CONTENT_BYTES}-byte safety limit"
        ));
    }
    if separator_bytes != 0 {
        output.push_str("\n\n");
    }
    output.push_str(content);
    Ok(())
}

fn bounded_activity_detail(name: &str, arguments: &Value, limit: usize) -> String {
    let arguments = serde_json::to_string_pretty(arguments).unwrap_or_else(|_| "{}".to_string());
    truncate_utf8_with_marker(&format!("{name}\n{arguments}"), limit, TRUNCATION_MARKER)
}

fn truncate_utf8_with_marker(value: &str, limit: usize, marker: &str) -> String {
    if value.len() <= limit {
        return value.to_string();
    }
    if limit == 0 {
        return String::new();
    }
    if marker.len() >= limit {
        return utf8_prefix(value, limit).to_string();
    }
    let prefix = utf8_prefix(value, limit - marker.len());
    format!("{prefix}{marker}")
}

fn utf8_prefix(value: &str, max_bytes: usize) -> &str {
    let mut end = value.len().min(max_bytes);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

pub fn respond_to_approval(
    registry: &ApprovalRegistry,
    id: &str,
    allow: bool,
) -> Result<(), String> {
    let sender = registry
        .lock()
        .remove(id)
        .ok_or_else(|| "Approval request is no longer active".to_string())?;
    sender
        .send(allow)
        .map_err(|_| "Approval request was cancelled".to_string())
}

async fn read_key() -> Result<String, String> {
    tokio::task::spawn_blocking(|| {
        keyring::Entry::new(SERVICE, ACCOUNT)
            .map_err(|error| error.to_string())?
            .get_password()
            .map_err(|error| match error {
                keyring::Error::NoEntry => "Connect an OpenRouter API key in Settings".to_string(),
                other => other.to_string(),
            })
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn fetch_models(key: &str) -> Result<Vec<OpenRouterModel>, String> {
    let response = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?
        .get(format!("{API_BASE}/models"))
        .bearer_auth(key)
        .header("HTTP-Referer", "https://github.com/z4mbo/zAI")
        .header("X-OpenRouter-Title", "zAI")
        .send()
        .await
        .map_err(safe_http_error)?;
    let status = response.status();
    let limit = if status.is_success() {
        MAX_MODELS_RESPONSE_BYTES
    } else {
        MAX_ERROR_RESPONSE_BYTES
    };
    let response_body = read_http_body_limited(response, limit)
        .await
        .map_err(|error| {
            if status.is_success() {
                format!("OpenRouter model response {error}")
            } else {
                format!("{}; response body {error}", openrouter_error(status, ""))
            }
        })?;
    if !status.is_success() {
        return Err(openrouter_error(
            status,
            String::from_utf8_lossy(&response_body),
        ));
    }
    let body: Value = serde_json::from_slice(&response_body)
        .map_err(|_| "OpenRouter returned invalid model data".to_string())?;
    let mut models = body
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let id = model.get("id")?.as_str()?.to_string();
            Some(OpenRouterModel {
                name: model
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_string(),
                id,
                context_length: model.get("context_length").and_then(Value::as_u64),
                prompt_price: model
                    .pointer("/pricing/prompt")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                completion_price: model
                    .pointer("/pricing/completion")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect::<Vec<_>>();
    models.sort_by_key(|model| model.name.to_lowercase());
    if models.is_empty() {
        return Err("No OpenRouter models were returned for this key".to_string());
    }
    Ok(models)
}

async fn validate_key(key: &str) -> Result<(), String> {
    let response = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?
        .get(format!("{API_BASE}/auth/key"))
        .bearer_auth(key)
        .send()
        .await
        .map_err(safe_http_error)?;
    let status = response.status();
    let limit = if status.is_success() {
        MAX_AUTH_RESPONSE_BYTES
    } else {
        MAX_ERROR_RESPONSE_BYTES
    };
    let response_body = read_http_body_limited(response, limit)
        .await
        .map_err(|error| {
            if status.is_success() {
                format!("OpenRouter authentication response {error}")
            } else {
                format!("{}; response body {error}", openrouter_error(status, ""))
            }
        })?;
    if status.is_success() {
        Ok(())
    } else {
        Err(openrouter_error(
            status,
            String::from_utf8_lossy(&response_body),
        ))
    }
}

async fn read_http_body_limited(mut response: Response, limit: usize) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|content_length| content_length > limit as u64)
    {
        return Err(format!("exceeded the {limit}-byte safety limit"));
    }
    let mut body =
        Vec::with_capacity(response.content_length().unwrap_or(0).min(limit as u64) as usize);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "could not be read".to_string())?
    {
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(format!("exceeded the {limit}-byte safety limit"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[allow(clippy::too_many_arguments)]
async fn execute_tool(
    app: &AppHandle,
    session_id: &str,
    workspace: &Path,
    name: &str,
    arguments: &Value,
    cancellation: &CancellationToken,
    approvals: &ApprovalRegistry,
) -> Result<String, String> {
    match name {
        "read_file" => {
            let workspace = workspace.to_path_buf();
            let relative = required_argument(arguments, "path")?.to_string();
            // `spawn_blocking` abort cannot interrupt an OS read that has already
            // started. The capability-anchored regular-file open, Unix
            // O_NONBLOCK flag, and byte cap keep local filesystem reads finite;
            // the timeout still releases the turn if a pathological remote
            // filesystem stalls a worker thread.
            let mut task = tokio::task::spawn_blocking(move || {
                read_workspace_text(&workspace, Path::new(&relative), 256 * 1024)
            });
            tokio::select! {
                _ = cancellation.cancelled() => {
                    task.abort();
                    Err("Turn cancelled".to_string())
                }
                result = timeout(FILE_READ_TIMEOUT, &mut task) => match result {
                    Ok(result) => result.map_err(|error| error.to_string())?,
                    Err(_) => {
                        task.abort();
                        Err("File read timed out".to_string())
                    }
                }
            }
        }
        "list_files" => {
            let workspace = workspace.to_path_buf();
            tokio::task::spawn_blocking(move || list_files(&workspace))
                .await
                .map_err(|error| error.to_string())?
        }
        "search_files" => {
            let workspace = workspace.to_path_buf();
            let query = required_argument(arguments, "query")?.to_string();
            let worker_cancellation = CancellationToken::new();
            let worker_token = worker_cancellation.clone();
            let mut task = tokio::task::spawn_blocking(move || {
                search_files(&workspace, &query, &worker_token)
            });
            tokio::select! {
                _ = cancellation.cancelled() => {
                    worker_cancellation.cancel();
                    task.abort();
                    Err("Turn cancelled".to_string())
                }
                result = timeout(FILE_SEARCH_TIMEOUT, &mut task) => match result {
                    Ok(result) => result.map_err(|error| error.to_string())?,
                    Err(_) => {
                        worker_cancellation.cancel();
                        task.abort();
                        Err("File search timed out".to_string())
                    }
                }
            }
        }
        "write_file" => {
            let relative = required_argument(arguments, "path")?;
            let content = required_argument(arguments, "content")?;
            if content.len() > 1024 * 1024 {
                return Err("Write exceeds the 1 MiB safety limit".to_string());
            }
            safe_path(workspace, relative, true)?;
            let approved = request_approval(
                app,
                session_id,
                "Allow OpenRouter to write a file?",
                &format!("Write {} bytes to {relative}", content.len()),
                "writes files in the selected workspace",
                cancellation,
                approvals,
            )
            .await?;
            if !approved {
                return Ok("User denied the file write".to_string());
            }
            secure_write(workspace, relative, content)?;
            Ok(format!("Wrote {relative}"))
        }
        "run_command" => {
            let command = required_argument(arguments, "command")?;
            let approved = request_approval(
                app,
                session_id,
                "Allow OpenRouter to run this command?",
                command,
                "executes a shell command in the selected workspace",
                cancellation,
                approvals,
            )
            .await?;
            if !approved {
                return Ok("User denied the command".to_string());
            }
            run_shell_command(workspace, command, cancellation).await
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}

async fn request_approval(
    app: &AppHandle,
    session_id: &str,
    title: &str,
    detail: &str,
    risk: &str,
    cancellation: &CancellationToken,
    approvals: &ApprovalRegistry,
) -> Result<bool, String> {
    let id = Uuid::new_v4().to_string();
    let (sender, receiver) = oneshot::channel();
    approvals.lock().insert(id.clone(), sender);
    let request = ApprovalRequest {
        id: id.clone(),
        session_id: session_id.to_string(),
        title: title.to_string(),
        detail: detail.to_string(),
        risk: risk.to_string(),
        created_at: Utc::now(),
    };
    if let Err(error) = app.emit("zai://approval", request) {
        approvals.lock().remove(&id);
        return Err(error.to_string());
    }
    tokio::select! {
        _ = cancellation.cancelled() => {
            approvals.lock().remove(&id);
            Err("Turn cancelled".to_string())
        }
        result = timeout(Duration::from_secs(600), receiver) => {
            approvals.lock().remove(&id);
            match result {
                Ok(Ok(allow)) => Ok(allow),
                Ok(Err(_)) => Err("Approval request was closed".to_string()),
                Err(_) => Err("Approval request timed out".to_string()),
            }
        }
    }
}

fn tool_definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a UTF-8 text file inside the selected workspace.",
                "parameters": { "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"], "additionalProperties": false }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_files",
                "description": "List files inside the selected workspace.",
                "parameters": { "type": "object", "properties": {}, "additionalProperties": false }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "search_files",
                "description": "Search UTF-8 workspace files for a literal string.",
                "parameters": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"], "additionalProperties": false }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Write a UTF-8 file inside the workspace after user approval.",
                "parameters": { "type": "object", "properties": { "path": { "type": "string" }, "content": { "type": "string" } }, "required": ["path", "content"], "additionalProperties": false }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "run_command",
                "description": "Run a shell command in the workspace after user approval.",
                "parameters": { "type": "object", "properties": { "command": { "type": "string" } }, "required": ["command"], "additionalProperties": false }
            }
        }
    ])
}

fn canonical_workspace(path: &Path) -> Result<PathBuf, String> {
    let workspace = path
        .canonicalize()
        .map_err(|error| format!("Workspace is unavailable: {error}"))?;
    if !workspace.is_dir() {
        return Err("Workspace must be a directory".to_string());
    }
    Ok(workspace)
}

fn safe_path(workspace: &Path, value: &str, allow_missing: bool) -> Result<PathBuf, String> {
    let relative = Path::new(value);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("Path must stay inside the selected workspace".to_string());
    }
    let candidate = workspace.join(relative);
    let resolved = if allow_missing && !candidate.exists() {
        let parent = candidate
            .parent()
            .ok_or_else(|| "Invalid path".to_string())?;
        let existing = nearest_existing(parent)?;
        let canonical = existing.canonicalize().map_err(|error| error.to_string())?;
        if !canonical.starts_with(workspace) {
            return Err("Path escapes the selected workspace".to_string());
        }
        candidate
    } else {
        candidate
            .canonicalize()
            .map_err(|error| error.to_string())?
    };
    if !resolved.starts_with(workspace) {
        return Err("Path escapes the selected workspace".to_string());
    }
    Ok(resolved)
}

fn nearest_existing(path: &Path) -> Result<PathBuf, String> {
    let mut current = path.to_path_buf();
    while !current.exists() {
        current = current
            .parent()
            .ok_or_else(|| "Invalid destination path".to_string())?
            .to_path_buf();
    }
    Ok(current)
}

fn secure_write(workspace: &Path, value: &str, content: &str) -> Result<(), String> {
    let relative = Path::new(value);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err("Path must stay inside the selected workspace".to_string());
    }
    let root =
        Dir::open_ambient_dir(workspace, ambient_authority()).map_err(|error| error.to_string())?;
    if let Some(parent) = relative
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        root.create_dir_all(parent)
            .map_err(|error| error.to_string())?;
    }
    root.write(relative, content.as_bytes())
        .map_err(|error| error.to_string())
}

fn open_regular_workspace_file(root: &Dir, relative: &Path) -> Result<cap_std::fs::File, String> {
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err("Path must stay inside the selected workspace".to_string());
    }

    // Reject known special files before opening them. This prevents a FIFO from
    // blocking on platforms without a non-blocking filesystem-open flag.
    let metadata = root.metadata(relative).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("Path is not a regular file".to_string());
    }

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = root
        .open_with(relative, &options)
        .map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Path is not a regular file".to_string());
    }
    Ok(file)
}

fn read_regular_workspace_file(
    root: &Dir,
    relative: &Path,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let file = open_regular_workspace_file(root, relative)?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if metadata.len() > limit as u64 {
        return Err(format!("File is larger than the {limit}-byte read limit"));
    }
    let mut bytes = Vec::with_capacity(metadata.len().min(limit as u64) as usize);
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err(format!("File is larger than the {limit}-byte read limit"));
    }
    Ok(bytes)
}

fn read_workspace_text(workspace: &Path, relative: &Path, limit: usize) -> Result<String, String> {
    let root =
        Dir::open_ambient_dir(workspace, ambient_authority()).map_err(|error| error.to_string())?;
    let bytes = read_regular_workspace_file(&root, relative, limit)?;
    String::from_utf8(bytes).map_err(|_| "File is not UTF-8 text".to_string())
}

fn list_files(workspace: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    for entry in WalkDir::new(workspace)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !ignored(entry.path()))
        .filter_map(Result::ok)
        .take(5_000)
        .filter(|entry| entry.file_type().is_file())
        .take(600)
    {
        files.push(
            entry
                .path()
                .strip_prefix(workspace)
                .unwrap_or(entry.path())
                .display()
                .to_string(),
        );
    }
    Ok(files.join("\n"))
}

fn search_files(
    workspace: &Path,
    query: &str,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    if query.is_empty() || query.len() > 512 {
        return Err("Search query must be between 1 and 512 characters".to_string());
    }
    let root =
        Dir::open_ambient_dir(workspace, ambient_authority()).map_err(|error| error.to_string())?;
    let mut output = String::new();
    let mut match_count = 0_usize;
    let mut scanned_bytes = 0_usize;
    for entry in WalkDir::new(workspace)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !ignored(entry.path()))
        .filter_map(Result::ok)
        .take(5_000)
        .filter(|entry| entry.file_type().is_file())
    {
        if cancellation.is_cancelled() {
            return Err("Turn cancelled".to_string());
        }
        let remaining = MAX_SEARCH_SCANNED_BYTES.saturating_sub(scanned_bytes);
        if remaining == 0 {
            break;
        }
        let Ok(relative) = entry.path().strip_prefix(workspace) else {
            continue;
        };
        let per_file_limit = remaining.min(MAX_SEARCH_FILE_BYTES);
        let Ok(bytes) = read_regular_workspace_file(&root, relative, per_file_limit) else {
            if per_file_limit < MAX_SEARCH_FILE_BYTES {
                break;
            }
            continue;
        };
        scanned_bytes = scanned_bytes.saturating_add(bytes.len());
        let Ok(content) = String::from_utf8(bytes) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            if cancellation.is_cancelled() {
                return Err("Turn cancelled".to_string());
            }
            if line.contains(query) {
                let matched_line = format!(
                    "{}:{}:{}",
                    entry
                        .path()
                        .strip_prefix(workspace)
                        .unwrap_or(entry.path())
                        .display(),
                    index + 1,
                    line.trim()
                );
                let separator_bytes = usize::from(!output.is_empty());
                let remaining = MAX_SEARCH_RESULT_BYTES.saturating_sub(output.len());
                if separator_bytes.saturating_add(matched_line.len()) > remaining {
                    if separator_bytes != 0 && remaining > 0 {
                        output.push('\n');
                    }
                    let remaining = MAX_SEARCH_RESULT_BYTES.saturating_sub(output.len());
                    output.push_str(&truncate_utf8_with_marker(
                        &matched_line,
                        remaining,
                        TRUNCATION_MARKER,
                    ));
                    return Ok(output);
                }
                if separator_bytes != 0 {
                    output.push('\n');
                }
                output.push_str(&matched_line);
                match_count += 1;
                if match_count >= 100 {
                    return Ok(output);
                }
            }
        }
    }
    Ok(if output.is_empty() {
        "No matches".to_string()
    } else {
        output
    })
}

fn ignored(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                ".git" | "node_modules" | "target" | "dist" | ".next" | ".cache"
            )
        })
}

async fn run_shell_command(
    workspace: &Path,
    value: &str,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    if value.is_empty() || value.len() > 16 * 1024 {
        return Err("Command is empty or too large".to_string());
    }
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            value,
        ]);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut command = Command::new(shell);
        command.args(["-lc", value]);
        command
    };
    command
        .current_dir(workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    configure_process_group(&mut command);
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    #[cfg(windows)]
    let mut windows_job = match WindowsJob::assign(&child) {
        Ok(job) => job,
        Err(error) => {
            terminate_child(&mut child).await;
            return Err(format!("Failed to contain command: {error}"));
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            terminate_child(&mut child).await;
            return Err("Command stdout was unavailable".to_string());
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            terminate_child(&mut child).await;
            return Err("Command stderr was unavailable".to_string());
        }
    };
    #[cfg(unix)]
    let process_id = child.id();
    let mut stdout_task = tokio::spawn(read_limited(stdout, 64 * 1024));
    let mut stderr_task = tokio::spawn(read_limited(stderr, 64 * 1024));
    let status = tokio::select! {
        _ = cancellation.cancelled() => {
            terminate_child(&mut child).await;
            abort_output_task(&mut stdout_task).await;
            abort_output_task(&mut stderr_task).await;
            return Err("Turn cancelled".to_string());
        }
        result = timeout(Duration::from_secs(120), child.wait()) => match result {
            Ok(Ok(status)) => status,
            Ok(Err(error)) => {
                terminate_child(&mut child).await;
                abort_output_task(&mut stdout_task).await;
                abort_output_task(&mut stderr_task).await;
                return Err(error.to_string());
            }
            Err(_) => {
                terminate_child(&mut child).await;
                abort_output_task(&mut stdout_task).await;
                abort_output_task(&mut stderr_task).await;
                return Err("Command timed out after 120 seconds".to_string());
            }
        }
    };
    let stdout = match timeout(Duration::from_secs(2), &mut stdout_task).await {
        Ok(Ok(Ok(output))) => output,
        Ok(Ok(Err(error))) => {
            #[cfg(unix)]
            terminate_process_group(process_id, true).await;
            #[cfg(windows)]
            windows_job.terminate(false);
            abort_output_task(&mut stderr_task).await;
            return Err(error);
        }
        Ok(Err(error)) => {
            #[cfg(unix)]
            terminate_process_group(process_id, true).await;
            #[cfg(windows)]
            windows_job.terminate(false);
            abort_output_task(&mut stderr_task).await;
            return Err(error.to_string());
        }
        Err(_) => {
            #[cfg(unix)]
            terminate_process_group(process_id, true).await;
            #[cfg(windows)]
            windows_job.terminate(false);
            stdout_task.abort();
            stderr_task.abort();
            let _ = (&mut stdout_task).await;
            let _ = (&mut stderr_task).await;
            return Err("Command left background processes running".to_string());
        }
    };
    let stderr = match timeout(Duration::from_secs(2), &mut stderr_task).await {
        Ok(Ok(Ok(output))) => output,
        Ok(Ok(Err(error))) => {
            #[cfg(unix)]
            terminate_process_group(process_id, true).await;
            #[cfg(windows)]
            windows_job.terminate(false);
            return Err(error);
        }
        Ok(Err(error)) => {
            #[cfg(unix)]
            terminate_process_group(process_id, true).await;
            #[cfg(windows)]
            windows_job.terminate(false);
            return Err(error.to_string());
        }
        Err(_) => {
            #[cfg(unix)]
            terminate_process_group(process_id, true).await;
            #[cfg(windows)]
            windows_job.terminate(false);
            stderr_task.abort();
            let _ = (&mut stderr_task).await;
            return Err("Command left background processes running".to_string());
        }
    };
    let mut text = String::from_utf8_lossy(&stdout).into_owned();
    if !stderr.is_empty() {
        text.push_str("\n[stderr]\n");
        text.push_str(&String::from_utf8_lossy(&stderr));
    }
    if !status.success() {
        text.push_str(&format!("\n[exit status: {status}]"));
    }
    Ok(text)
}

async fn read_limited<R>(mut reader: R, limit: usize) -> Result<Vec<u8>, String>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(read) > limit {
            return Err(format!(
                "Command output exceeded the {limit}-byte safety limit"
            ));
        }
        output.extend_from_slice(&buffer[..read]);
    }
}

async fn abort_output_task(task: &mut tokio::task::JoinHandle<Result<Vec<u8>, String>>) {
    task.abort();
    let _ = task.await;
}

fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command
            .as_std_mut()
            .creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
}

async fn terminate_child(child: &mut Child) {
    let pid = child.id();
    terminate_process_group(pid, false).await;
    #[cfg(unix)]
    {
        tokio::time::sleep(Duration::from_millis(400)).await;
        terminate_process_group(pid, true).await;
    }
    if timeout(Duration::from_secs(2), child.wait()).await.is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}

async fn terminate_process_group(pid: Option<u32>, force: bool) {
    if let Some(pid) = pid {
        #[cfg(unix)]
        unsafe {
            libc::kill(
                -(pid as i32),
                if force { libc::SIGKILL } else { libc::SIGTERM },
            );
        }
        #[cfg(windows)]
        {
            let _ = force;
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }
    }
}

fn required_argument<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing {key}"))
}

fn extract_content_limited(value: Option<&Value>, limit: usize) -> Result<String, String> {
    match value {
        Some(Value::String(value)) => {
            if value.len() > limit {
                Err(format!(
                    "OpenRouter assistant content exceeded the {limit}-byte safety limit"
                ))
            } else {
                Ok(value.clone())
            }
        }
        Some(Value::Array(parts)) => {
            let mut content = String::new();
            for text in parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
            {
                let next_len = content
                    .len()
                    .checked_add(text.len())
                    .ok_or_else(|| "OpenRouter assistant content size overflowed".to_string())?;
                if next_len > limit {
                    return Err(format!(
                        "OpenRouter assistant content exceeded the {limit}-byte safety limit"
                    ));
                }
                content.push_str(text);
            }
            Ok(content)
        }
        None | Some(Value::Null) => Ok(String::new()),
        _ => Err("OpenRouter returned invalid assistant content".to_string()),
    }
}

fn safe_http_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "OpenRouter request timed out".to_string()
    } else if error.is_connect() {
        "Could not connect to OpenRouter".to_string()
    } else {
        "OpenRouter request failed".to_string()
    }
}

fn openrouter_error(status: StatusCode, body: impl AsRef<str>) -> String {
    let body = body.as_ref();
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            "The OpenRouter API key is invalid or unauthorized".to_string()
        }
        StatusCode::TOO_MANY_REQUESTS => "OpenRouter is rate limiting this key".to_string(),
        _ if status.is_server_error() => "OpenRouter is temporarily unavailable".to_string(),
        _ => {
            let message = serde_json::from_str::<Value>(body)
                .ok()
                .and_then(|value| {
                    value
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "OpenRouter rejected the request".to_string());
            let message =
                truncate_utf8_with_marker(&message, MAX_ERROR_MESSAGE_BYTES, TRUNCATION_MARKER);
            format!("{message} ({status})")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_ACTIVITY_DETAIL_BYTES, MAX_ASSISTANT_CONTENT_BYTES, MAX_CURRENT_PROMPT_BYTES,
        MAX_HISTORY_CONTENT_BYTES, MAX_HISTORY_MESSAGES, MAX_REQUEST_CONTEXT_BYTES,
        MAX_TOOL_CALLS_PER_ROUND, MAX_TOOL_NAME_BYTES, MAX_TURN_CONTEXT_GROWTH_BYTES,
        TRUNCATION_MARKER, bounded_activity_detail, build_initial_messages,
        extract_content_limited, parse_tool_calls, push_turn_context_message, read_workspace_text,
        run_shell_command, safe_path, secure_write, truncate_utf8_with_marker,
    };
    use crate::model::{Message, MessageKind, MessageRole};
    use serde_json::json;
    use std::fs;

    #[test]
    fn workspace_paths_reject_parent_traversal() {
        let root = std::env::temp_dir().join(format!("zai-path-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        assert!(safe_path(&root.canonicalize().unwrap(), "../secret", true).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn openrouter_content_parts_are_normalized() {
        assert_eq!(
            extract_content_limited(
                Some(&json!([
                    { "type": "text", "text": "hello " },
                    { "type": "text", "text": "world" }
                ])),
                32,
            )
            .unwrap(),
            "hello world"
        );
    }

    #[test]
    fn assistant_content_fails_closed_at_the_byte_limit() {
        let content = "x".repeat(MAX_ASSISTANT_CONTENT_BYTES + 1);
        assert!(
            extract_content_limited(Some(&json!(content)), MAX_ASSISTANT_CONTENT_BYTES).is_err()
        );
    }

    #[test]
    fn history_keeps_newest_messages_in_chronological_order_and_prompt_once() {
        let mut history = (0..MAX_HISTORY_MESSAGES + 5)
            .map(|index| {
                Message::new(
                    if index % 2 == 0 {
                        MessageRole::User
                    } else {
                        MessageRole::Assistant
                    },
                    MessageKind::Text,
                    format!("history-{index}"),
                )
            })
            .collect::<Vec<_>>();
        history.push(Message::new(
            MessageRole::User,
            MessageKind::Text,
            "current".to_string(),
        ));

        let messages = build_initial_messages(&history, "current").unwrap();
        assert_eq!(messages.len(), MAX_HISTORY_MESSAGES + 2);
        assert_eq!(messages[1]["content"], json!("history-5"));
        assert_eq!(
            messages[MAX_HISTORY_MESSAGES]["content"],
            json!(format!("history-{}", MAX_HISTORY_MESSAGES + 4))
        );
        assert_eq!(messages.last().unwrap()["content"], json!("current"));
        assert_eq!(
            messages
                .iter()
                .filter(|message| message["content"] == json!("current"))
                .count(),
            1
        );
    }

    #[test]
    fn history_byte_cap_truncates_on_a_utf8_boundary() {
        let history = vec![Message::new(
            MessageRole::Assistant,
            MessageKind::Text,
            "🙂".repeat(MAX_HISTORY_CONTENT_BYTES / 4 + 32),
        )];
        let messages = build_initial_messages(&history, "next").unwrap();
        let content = messages[1]["content"].as_str().unwrap();
        assert!(content.len() <= MAX_HISTORY_CONTENT_BYTES);
        assert!(content.ends_with(TRUNCATION_MARKER));
        assert!(content.is_char_boundary(content.len()));
    }

    #[test]
    fn current_prompt_is_never_silently_truncated() {
        let prompt = "x".repeat(MAX_CURRENT_PROMPT_BYTES + 1);
        assert!(build_initial_messages(&[], &prompt).is_err());
    }

    #[test]
    fn tool_call_parser_rejects_excess_and_invalid_protocol_fields() {
        let too_many = json!(
            (0..MAX_TOOL_CALLS_PER_ROUND + 1)
                .map(|index| json!({
                    "id": format!("call-{index}"),
                    "type": "function",
                    "function": { "name": "read_file", "arguments": "{}" }
                }))
                .collect::<Vec<_>>()
        );
        assert!(parse_tool_calls(Some(&too_many)).is_err());

        let long_name = "x".repeat(MAX_TOOL_NAME_BYTES + 1);
        let invalid = json!([{
            "id": "call-1",
            "type": "function",
            "function": { "name": long_name, "arguments": "not json" }
        }]);
        assert!(parse_tool_calls(Some(&invalid)).is_err());

        let invalid_arguments = json!([{
            "id": "call-1",
            "type": "function",
            "function": { "name": "read_file", "arguments": "[]" }
        }]);
        assert!(parse_tool_calls(Some(&invalid_arguments)).is_err());
    }

    #[test]
    fn activity_and_generic_truncation_stay_utf8_safe_and_bounded() {
        let arguments = json!({ "content": "🙂".repeat(MAX_ACTIVITY_DETAIL_BYTES) });
        let detail = bounded_activity_detail("write_file", &arguments, MAX_ACTIVITY_DETAIL_BYTES);
        assert!(detail.len() <= MAX_ACTIVITY_DETAIL_BYTES);
        assert!(detail.ends_with(TRUNCATION_MARKER));

        let truncated = truncate_utf8_with_marker("🙂🙂🙂", 9, "...");
        assert_eq!(truncated, "🙂...");
        assert!(truncated.len() <= 9);
    }

    #[test]
    fn turn_context_limits_fail_before_mutating_messages() {
        let mut messages = vec![json!({ "role": "system", "content": "system" })];
        let original_len = messages.len();
        let mut total = MAX_REQUEST_CONTEXT_BYTES;
        let mut growth = 0;
        assert!(
            push_turn_context_message(
                &mut messages,
                json!({ "role": "assistant", "content": "extra" }),
                &mut total,
                &mut growth,
            )
            .is_err()
        );
        assert_eq!(messages.len(), original_len);

        total = 2;
        growth = MAX_TURN_CONTEXT_GROWTH_BYTES;
        assert!(
            push_turn_context_message(
                &mut messages,
                json!({ "role": "assistant", "content": "extra" }),
                &mut total,
                &mut growth,
            )
            .is_err()
        );
        assert_eq!(messages.len(), original_len);
    }

    #[test]
    fn workspace_reads_enforce_the_open_handle_byte_limit() {
        let root = std::env::temp_dir().join(format!("zai-read-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("large.txt"), vec![b'x'; 1025]).unwrap();

        let error = read_workspace_text(&root, std::path::Path::new("large.txt"), 1024)
            .expect_err("oversized reads must fail closed");
        assert!(error.contains("larger than"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn workspace_reads_reject_symlink_escape() {
        use std::os::unix::fs::symlink;

        let base = std::env::temp_dir().join(format!("zai-read-test-{}", uuid::Uuid::new_v4()));
        let root = base.join("workspace");
        let outside = base.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "not available").unwrap();
        symlink(&outside, root.join("escape")).unwrap();

        assert!(
            read_workspace_text(&root, std::path::Path::new("escape/secret.txt"), 1024).is_err()
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn workspace_reads_reject_fifos_without_waiting_for_a_writer() {
        use std::{ffi::CString, os::unix::ffi::OsStrExt, sync::mpsc, time::Duration};

        let root = std::env::temp_dir().join(format!("zai-fifo-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let fifo = root.join("input.pipe");
        let fifo_path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);

        let worker_root = root.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result =
                read_workspace_text(&worker_root, std::path::Path::new("input.pipe"), 1024);
            let _ = sender.send(result);
        });
        let result = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("FIFO validation must not block in open");
        assert!(result.is_err());
        worker.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn command_output_limit_terminates_background_process_group() {
        let root = std::env::temp_dir().join(format!("zai-command-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let cancellation = tokio_util::sync::CancellationToken::new();
        let error = run_shell_command(
            &root,
            "/bin/sh -c '(sleep 1; printf survived > leaked.txt) & /usr/bin/head -c 70000 /dev/zero'",
            &cancellation,
        )
        .await
        .expect_err("output over the limit must fail");
        assert!(error.contains("output exceeded"));

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        assert!(
            !root.join("leaked.txt").exists(),
            "the background process survived output-limit cleanup"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn workspace_writes_reject_symlink_escape() {
        use std::os::unix::fs::symlink;

        let base = std::env::temp_dir().join(format!("zai-write-test-{}", uuid::Uuid::new_v4()));
        let root = base.join("workspace");
        let outside = base.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, root.join("escape")).unwrap();
        assert!(secure_write(&root, "escape/secret.txt", "blocked").is_err());
        assert!(!outside.join("secret.txt").exists());
        fs::remove_dir_all(base).unwrap();
    }
}
