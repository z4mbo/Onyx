use crate::model::{
    AgentSession, ApprovalRequest, Message, MessageKind, MessageRole, OpenRouterModel,
    OpenRouterStatus, SessionEvent,
};
use cap_std::{ambient_authority, fs::Dir};
use chrono::Utc;
use parking_lot::Mutex as ParkingMutex;
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
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
    let key = read_key().await?;
    let workspace = canonical_workspace(&session.workspace)?;
    let mut messages = vec![json!({
        "role": "system",
        "content": "You are zAI, a coding agent working in the user's selected workspace. Inspect files before editing. Use tools when needed, make focused changes, and explain the result. File writes and shell commands always require the user's explicit approval. Never claim a tool succeeded until its result is returned."
    })];
    for message in session.messages.iter().filter(|message| {
        message.kind == MessageKind::Text
            && matches!(message.role, MessageRole::User | MessageRole::Assistant)
    }) {
        messages.push(json!({
            "role": match message.role { MessageRole::User => "user", _ => "assistant" },
            "content": message.content,
        }));
    }
    if !messages
        .last()
        .and_then(|value| value.get("content"))
        .and_then(Value::as_str)
        .is_some_and(|value| value == prompt)
    {
        messages.push(json!({ "role": "user", "content": prompt }));
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|error| error.to_string())?;
    let mut final_content = String::new();
    let mut activities = Vec::new();

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
            response = request.send() => response.map_err(|error| safe_http_error(error))?,
        };
        if !response.status().is_success() {
            return Err(openrouter_error(
                response.status(),
                response.text().await.unwrap_or_default(),
            ));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| "OpenRouter returned invalid JSON".to_string())?;
        let assistant = body
            .pointer("/choices/0/message")
            .cloned()
            .ok_or_else(|| "OpenRouter returned no assistant message".to_string())?;
        let content = extract_content(assistant.get("content"));
        if !content.is_empty() {
            if !final_content.is_empty() {
                final_content.push_str("\n\n");
            }
            final_content.push_str(&content);
            let _ = app.emit(
                "zai://session",
                SessionEvent::Delta {
                    session_id: session.id.clone(),
                    message_id: message_id.clone(),
                    delta: content,
                },
            );
        }
        let tool_calls = assistant
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        messages.push(assistant);
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
            let call_id = call.get("id").and_then(Value::as_str).unwrap_or("tool");
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let arguments = call
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .and_then(|value| serde_json::from_str::<Value>(value).ok())
                .unwrap_or_else(|| json!({}));
            let activity = Message::new(
                MessageRole::Tool,
                MessageKind::Tool,
                format!(
                    "{name}\n{}",
                    serde_json::to_string_pretty(&arguments).unwrap_or_default()
                ),
            );
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
                name,
                &arguments,
                &cancellation,
                &approvals,
            )
            .await;
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": result.unwrap_or_else(|error| format!("Tool error: {error}"))
            }));
        }
    }
    Err("OpenRouter reached the tool-call safety limit for this turn".to_string())
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
    if !response.status().is_success() {
        return Err(openrouter_error(
            response.status(),
            response.text().await.unwrap_or_default(),
        ));
    }
    let body: Value = response
        .json()
        .await
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
    models.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
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
    if response.status().is_success() {
        Ok(())
    } else {
        Err(openrouter_error(
            response.status(),
            response.text().await.unwrap_or_default(),
        ))
    }
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
            let path = safe_path(workspace, required_argument(arguments, "path")?, false)?;
            let mut bytes = Vec::new();
            fs::File::open(path)
                .map_err(|error| error.to_string())?
                .take(256 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            if bytes.len() > 256 * 1024 {
                return Err("File is larger than the 256 KiB read limit".to_string());
            }
            String::from_utf8(bytes).map_err(|_| "File is not UTF-8 text".to_string())
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
            tokio::task::spawn_blocking(move || search_files(&workspace, &query))
                .await
                .map_err(|error| error.to_string())?
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

fn search_files(workspace: &Path, query: &str) -> Result<String, String> {
    if query.is_empty() || query.len() > 512 {
        return Err("Search query must be between 1 and 512 characters".to_string());
    }
    let mut matches = Vec::new();
    let mut scanned_bytes = 0_u64;
    for entry in WalkDir::new(workspace)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !ignored(entry.path()))
        .filter_map(Result::ok)
        .take(5_000)
        .filter(|entry| entry.file_type().is_file())
    {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.len() > 512 * 1024 {
            continue;
        }
        scanned_bytes = scanned_bytes.saturating_add(metadata.len());
        if scanned_bytes > 64 * 1024 * 1024 {
            break;
        }
        let Ok(content) = fs::read_to_string(entry.path()) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            if line.contains(query) {
                matches.push(format!(
                    "{}:{}:{}",
                    entry
                        .path()
                        .strip_prefix(workspace)
                        .unwrap_or(entry.path())
                        .display(),
                    index + 1,
                    line.trim()
                ));
                if matches.len() >= 100 {
                    return Ok(matches.join("\n"));
                }
            }
        }
    }
    Ok(if matches.is_empty() {
        "No matches".to_string()
    } else {
        matches.join("\n")
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
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Command stdout was unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Command stderr was unavailable".to_string())?;
    let process_id = child.id();
    let mut stdout_task = tokio::spawn(read_limited(stdout, 64 * 1024));
    let mut stderr_task = tokio::spawn(read_limited(stderr, 64 * 1024));
    let status = tokio::select! {
        _ = cancellation.cancelled() => {
            terminate_child(&mut child).await;
            return Err("Turn cancelled".to_string());
        }
        result = timeout(Duration::from_secs(120), child.wait()) => match result {
            Ok(Ok(status)) => status,
            Ok(Err(error)) => return Err(error.to_string()),
            Err(_) => {
                terminate_child(&mut child).await;
                return Err("Command timed out after 120 seconds".to_string());
            }
        }
    };
    let stdout = match timeout(Duration::from_secs(2), &mut stdout_task).await {
        Ok(result) => result.map_err(|error| error.to_string())??,
        Err(_) => {
            terminate_process_group(process_id, true).await;
            stdout_task.abort();
            stderr_task.abort();
            return Err("Command left background processes running".to_string());
        }
    };
    let stderr = match timeout(Duration::from_secs(2), &mut stderr_task).await {
        Ok(result) => result.map_err(|error| error.to_string())??,
        Err(_) => {
            terminate_process_group(process_id, true).await;
            stderr_task.abort();
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

fn extract_content(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
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

fn openrouter_error(status: StatusCode, body: String) -> String {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            "The OpenRouter API key is invalid or unauthorized".to_string()
        }
        StatusCode::TOO_MANY_REQUESTS => "OpenRouter is rate limiting this key".to_string(),
        _ if status.is_server_error() => "OpenRouter is temporarily unavailable".to_string(),
        _ => {
            let message = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "OpenRouter rejected the request".to_string());
            format!("{message} ({status})")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_content, safe_path, secure_write};
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
            extract_content(Some(&json!([
                { "type": "text", "text": "hello " },
                { "type": "text", "text": "world" }
            ]))),
            "hello world"
        );
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
