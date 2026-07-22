mod model;
mod openrouter;
mod providers;
mod storage;

use crate::{
    model::{
        AgentSession, CreateSessionInput, Message, MessageKind, MessageRole, OpenRouterModel,
        OpenRouterStatus, ProviderId, ProviderStatus, SendMessageInput, SessionEvent,
        SessionStatus, WorkspaceEntry,
    },
    openrouter::ApprovalRegistry,
    storage::SessionStore,
};
use chrono::Utc;
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use walkdir::WalkDir;

struct AppState {
    store: Arc<SessionStore>,
    running: Arc<Mutex<HashMap<String, CancellationToken>>>,
    approvals: ApprovalRegistry,
    exiting: AtomicBool,
}

#[tauri::command]
async fn list_providers() -> Vec<ProviderStatus> {
    providers::probe_providers(openrouter::status().await.connected).await
}

#[tauri::command]
fn list_sessions(state: State<'_, AppState>) -> Vec<AgentSession> {
    state.store.list()
}

#[tauri::command]
fn create_session(
    input: CreateSessionInput,
    state: State<'_, AppState>,
) -> Result<AgentSession, String> {
    let workspace = Path::new(input.workspace.trim())
        .canonicalize()
        .map_err(|error| format!("Workspace is unavailable: {error}"))?;
    if !workspace.is_dir() {
        return Err("Workspace must be a directory".to_string());
    }
    let now = Utc::now();
    state.store.insert(AgentSession {
        id: Uuid::new_v4().to_string(),
        title: "New session".to_string(),
        provider: input.provider,
        model: input.model.filter(|value| !value.trim().is_empty()),
        workspace,
        provider_session_id: None,
        status: SessionStatus::Idle,
        messages: Vec::new(),
        created_at: now,
        updated_at: now,
    })
}

#[tauri::command]
fn delete_session(
    app: AppHandle,
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(token) = state.running.lock().remove(&session_id) {
        token.cancel();
    }
    if state.store.remove(&session_id)? {
        let _ = app.emit("zai://session", SessionEvent::Removed { session_id });
    }
    Ok(())
}

#[tauri::command]
fn cancel_turn(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let token = state
        .running
        .lock()
        .get(&session_id)
        .cloned()
        .ok_or_else(|| "No turn is running for this session".to_string())?;
    token.cancel();
    Ok(())
}

#[tauri::command]
async fn send_message(
    app: AppHandle,
    input: SendMessageInput,
    state: State<'_, AppState>,
) -> Result<AgentSession, String> {
    let content = input.content.trim().to_string();
    if content.is_empty() {
        return Err("Write a message first".to_string());
    }
    if content.len() > 256 * 1024 {
        return Err("Message exceeds the 256 KiB limit".to_string());
    }
    let existing = state
        .store
        .get(&input.session_id)
        .ok_or_else(|| "Session not found".to_string())?;
    if existing.provider != ProviderId::Openrouter
        && (content.len() > 24 * 1024 || content.encode_utf16().count() > 24 * 1024)
    {
        return Err("Local CLI prompts are limited to 24 KiB".to_string());
    }
    let user_message = Message::new(MessageRole::User, MessageKind::Text, content.clone());
    let snapshot = state.store.begin_turn(&input.session_id, user_message)?;
    let _ = app.emit(
        "zai://session",
        SessionEvent::Snapshot {
            session: snapshot.clone(),
        },
    );

    let cancellation = CancellationToken::new();
    state
        .running
        .lock()
        .insert(input.session_id.clone(), cancellation.clone());
    let store = state.store.clone();
    let running = state.running.clone();
    let approvals = state.approvals.clone();
    let session_id = input.session_id;
    let assistant_id = Uuid::new_v4().to_string();
    let app_for_task = app.clone();
    let session_for_task = snapshot.clone();

    tauri::async_runtime::spawn(async move {
        let result = if session_for_task.provider == ProviderId::Openrouter {
            openrouter::run_turn(
                app_for_task.clone(),
                session_for_task.clone(),
                content,
                assistant_id.clone(),
                cancellation,
                approvals,
            )
            .await
            .map(|result| (result.content, None, result.activities))
        } else {
            providers::run_cli_turn(
                app_for_task.clone(),
                session_for_task.provider,
                session_id.clone(),
                session_for_task.provider_session_id.clone(),
                session_for_task.model.clone(),
                session_for_task.workspace.clone(),
                content,
                assistant_id.clone(),
                cancellation,
            )
            .await
            .map(|result| {
                (
                    result.content,
                    result.provider_session_id,
                    result.activities,
                )
            })
        };

        let snapshot = match result {
            Ok((content, provider_session_id, activities)) => store.finish_turn(
                &session_id,
                activities,
                Message {
                    id: assistant_id,
                    role: MessageRole::Assistant,
                    kind: MessageKind::Text,
                    content,
                    created_at: Utc::now(),
                },
                provider_session_id,
                false,
            ),
            Err(error) => {
                let cancelled = error == "Turn cancelled";
                store.finish_turn(
                    &session_id,
                    Vec::new(),
                    Message {
                        id: assistant_id,
                        role: MessageRole::System,
                        kind: if cancelled {
                            MessageKind::Text
                        } else {
                            MessageKind::Error
                        },
                        content: if cancelled {
                            "Turn cancelled".to_string()
                        } else {
                            error
                        },
                        created_at: Utc::now(),
                    },
                    None,
                    !cancelled,
                )
            }
        };
        running.lock().remove(&session_id);
        if let Ok(session) = snapshot {
            let _ = app_for_task.emit("zai://session", SessionEvent::Snapshot { session });
        }
    });
    Ok(snapshot)
}

#[tauri::command]
async fn openrouter_status() -> OpenRouterStatus {
    openrouter::status().await
}

#[tauri::command]
async fn openrouter_save_key(key: String) -> Result<OpenRouterStatus, String> {
    openrouter::save_key(key).await
}

#[tauri::command]
async fn openrouter_clear_key() -> Result<OpenRouterStatus, String> {
    openrouter::clear_key().await
}

#[tauri::command]
async fn openrouter_models() -> Result<Vec<OpenRouterModel>, String> {
    openrouter::models().await
}

#[tauri::command]
fn respond_approval(id: String, allow: bool, state: State<'_, AppState>) -> Result<(), String> {
    openrouter::respond_to_approval(&state.approvals, &id, allow)
}

#[tauri::command]
fn workspace_entries(workspace: String) -> Result<Vec<WorkspaceEntry>, String> {
    let root = Path::new(&workspace)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !root.is_dir() {
        return Err("Workspace must be a directory".to_string());
    }
    let mut entries = Vec::new();
    for entry in WalkDir::new(&root)
        .max_depth(4)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            !entry.file_name().to_str().is_some_and(|name| {
                matches!(
                    name,
                    ".git" | "node_modules" | "target" | "dist" | ".next" | ".cache"
                )
            })
        })
        .filter_map(Result::ok)
        .skip(1)
        .take(500)
    {
        entries.push(WorkspaceEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: entry.path().to_string_lossy().into_owned(),
            is_directory: entry.file_type().is_dir(),
            depth: entry.depth().saturating_sub(1),
        });
    }
    Ok(entries)
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            list_providers,
            list_sessions,
            create_session,
            delete_session,
            send_message,
            cancel_turn,
            openrouter_status,
            openrouter_save_key,
            openrouter_clear_key,
            openrouter_models,
            respond_approval,
            workspace_entries,
        ])
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| error.to_string())?;
            app.manage(AppState {
                store: Arc::new(SessionStore::load(&data_dir)?),
                running: Arc::new(Mutex::new(HashMap::new())),
                approvals: Arc::new(Mutex::new(HashMap::new())),
                exiting: AtomicBool::new(false),
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build zAI");

    app.run(|handle, event| match event {
        RunEvent::ExitRequested { api, code, .. } => {
            if let Some(state) = handle.try_state::<AppState>()
                && !state.running.lock().is_empty()
                && !state.exiting.swap(true, Ordering::SeqCst)
            {
                api.prevent_exit();
                for token in state.running.lock().values() {
                    token.cancel();
                }
                state.approvals.lock().clear();
                let handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(2_500)).await;
                    handle.exit(code.unwrap_or(0));
                });
            }
        }
        RunEvent::Exit => {
            if let Some(state) = handle.try_state::<AppState>() {
                for token in state.running.lock().values() {
                    token.cancel();
                }
                state.approvals.lock().clear();
            }
        }
        _ => {}
    });
}
