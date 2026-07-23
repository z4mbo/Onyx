mod active_app;
mod model;
mod modifier_hold;
mod openrouter;
mod providers;
mod storage;
mod terminal;
mod windowing;
mod workspace;

use crate::{
    model::{
        ActiveAppContext, AgentSession, ChatReply, ChatRequest, CreateSessionInput,
        MediaGenerationRequest, Message, MessageKind, MessageRole, OpenRouterModel,
        OpenRouterStatus, ProviderBrand, ProviderId, ProviderModelOption, ProviderStatus,
        ProviderUsage, SendMessageInput, SessionEvent, SessionStatus, TranscriptionReply,
        TranscriptionRequest, UpdateSessionOptionsInput, VideoJob, VoiceSettings, WorkspaceEntry,
    },
    openrouter::ApprovalRegistry,
    storage::SessionStore,
};
use chrono::Utc;
use enigo::{Enigo, Keyboard, Settings as EnigoSettings};
use parking_lot::{Mutex, RwLock};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{
    AppHandle, Emitter, Manager, RunEvent, State, WindowEvent,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use walkdir::WalkDir;

struct AppState {
    store: Arc<SessionStore>,
    running: Arc<Mutex<HashMap<String, CancellationToken>>>,
    approvals: ApprovalRegistry,
    providers: Arc<providers::ProviderRuntime>,
    terminals: terminal::TerminalRegistry,
    voice_settings: RwLock<VoiceSettings>,
    voice_settings_path: PathBuf,
    exiting: AtomicBool,
}

const VOICE_SETTINGS_MAX_BYTES: u64 = 64 * 1024;

fn load_voice_settings(path: &Path) -> VoiceSettings {
    let Ok(metadata) = std::fs::metadata(path) else {
        return VoiceSettings::default();
    };
    if !metadata.is_file() || metadata.len() > VOICE_SETTINGS_MAX_BYTES {
        return VoiceSettings::default();
    }
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn persist_voice_settings(path: &Path, settings: &VoiceSettings) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > VOICE_SETTINGS_MAX_BYTES {
        return Err("Voice settings exceed the storage limit".into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    std::fs::rename(temporary, path).map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_providers() -> Vec<ProviderStatus> {
    providers::probe_providers(openrouter::status().await.connected).await
}

#[tauri::command]
async fn list_provider_models(provider: ProviderId) -> Result<Vec<ProviderModelOption>, String> {
    match provider {
        ProviderId::Codex => providers::codex::model_catalog().await,
        _ => Ok(Vec::new()),
    }
}

#[tauri::command]
async fn provider_usage(provider: ProviderId) -> Result<Option<ProviderUsage>, String> {
    match provider {
        ProviderId::Codex => providers::codex::account_usage().await.map(Some),
        _ => Ok(None),
    }
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
        provider_brand: input
            .provider_brand
            .unwrap_or_else(|| ProviderBrand::for_provider(input.provider)),
        model: input.model.filter(|value| !value.trim().is_empty()),
        reasoning: input.reasoning,
        speed_mode: input.speed_mode,
        interaction_mode: input.interaction_mode,
        access_mode: input.access_mode,
        workspace,
        provider_session_id: None,
        status: SessionStatus::Idle,
        messages: Vec::new(),
        context_usage: None,
        created_at: now,
        updated_at: now,
    })
}

#[tauri::command]
async fn update_session_options(
    app: AppHandle,
    input: UpdateSessionOptionsInput,
    state: State<'_, AppState>,
) -> Result<AgentSession, String> {
    state.providers.remove_session(&input.session_id).await;
    let session = state.store.update_options(input)?;
    let _ = app.emit(
        "onyx://session",
        SessionEvent::Snapshot {
            session: session.clone(),
        },
    );
    Ok(session)
}

#[tauri::command]
async fn delete_session(
    app: AppHandle,
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(token) = state.running.lock().remove(&session_id) {
        token.cancel();
    }
    state.providers.remove_session(&session_id).await;
    if state.store.remove(&session_id)? {
        let _ = app.emit(
            "onyx://session",
            SessionEvent::Removed {
                session_id: session_id.clone(),
            },
        );
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
        "onyx://session",
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
    let provider_runtime = state.providers.clone();
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
            .map(|result| (result.content, None, result.activities, None))
        } else {
            provider_runtime
                .run_turn(
                    app_for_task.clone(),
                    session_for_task.provider,
                    session_id.clone(),
                    session_for_task.provider_session_id.clone(),
                    session_for_task.model.clone(),
                    session_for_task.workspace.clone(),
                    session_for_task.reasoning,
                    session_for_task.speed_mode,
                    session_for_task.interaction_mode,
                    session_for_task.access_mode,
                    content,
                    assistant_id.clone(),
                    cancellation,
                    approvals,
                )
                .await
                .map(|result| {
                    (
                        result.content,
                        result.provider_session_id,
                        result.activities,
                        result.context_usage,
                    )
                })
        };

        let snapshot = match result {
            Ok((content, provider_session_id, activities, context_usage)) => store.finish_turn(
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
                context_usage,
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
                    None,
                    !cancelled,
                )
            }
        };
        running.lock().remove(&session_id);
        if let Ok(session) = snapshot {
            let _ = app_for_task.emit("onyx://session", SessionEvent::Snapshot { session });
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
fn get_voice_settings(state: State<'_, AppState>) -> VoiceSettings {
    state.voice_settings.read().clone()
}

#[tauri::command]
fn apply_voice_settings(
    app: AppHandle,
    settings: VoiceSettings,
    state: State<'_, AppState>,
) -> Result<VoiceSettings, String> {
    if settings.overlay_margin > 160 || !(0.5..=2.0).contains(&settings.voice_rate) {
        return Err("Voice overlay margin or speech rate is outside the supported range".into());
    }
    if settings.transcription_model.len() > 256
        || settings.agent_model.len() > 256
        || settings.web_model.len() > 256
        || settings.files_model.len() > 256
        || settings.image_model.len() > 256
        || settings.voice_model.len() > 256
        || settings.voice_id.len() > 128
    {
        return Err("A voice model identifier is too long".into());
    }
    persist_voice_settings(&state.voice_settings_path, &settings)?;
    *state.voice_settings.write() = settings.clone();
    windowing::position_saved_windows(&app)?;
    Ok(settings)
}

#[tauri::command]
async fn transcribe_audio(
    request: TranscriptionRequest,
    state: State<'_, AppState>,
) -> Result<TranscriptionReply, String> {
    let settings = state.voice_settings.read().clone();
    if settings.transcription_provider != "openrouter" {
        return Err("This build currently routes transcription through your OpenRouter key".into());
    }
    openrouter::transcribe(
        &settings.transcription_model,
        settings.language.as_deref(),
        &request,
    )
    .await
}

#[tauri::command]
async fn speak_text(text: String, state: State<'_, AppState>) -> Result<String, String> {
    let settings = state.voice_settings.read().clone();
    if settings.voice_provider != "openrouter" {
        return Err("This build currently routes speech through your OpenRouter key".into());
    }
    openrouter::speak(
        &settings.voice_model,
        &settings.voice_id,
        settings.voice_rate,
        &text,
    )
    .await
}

#[tauri::command]
fn inject_text(text: String) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > 100_000 {
        return Err("Text is empty or exceeds the 100,000 character limit".into());
    }
    let mut enigo = Enigo::new(&EnigoSettings::default())
        .map_err(|error| format!("Native input is unavailable: {error}"))?;
    enigo.text(&text).map_err(|error| format!(
        "Unable to insert text. On macOS, enable Onyx under Privacy & Security → Accessibility. On Windows, the target cannot run as administrator. Details: {error}"
    ))
}

#[tauri::command]
fn active_app_context() -> ActiveAppContext {
    active_app::current()
}

#[tauri::command]
fn set_agent_expanded(app: AppHandle, expanded: bool) -> Result<(), String> {
    windowing::set_agent_expanded(&app, expanded)
}

#[tauri::command]
fn hide_window(app: AppHandle, label: String) -> Result<(), String> {
    if !matches!(label.as_str(), "main" | "hud" | "agent") {
        return Err("Unknown Onyx window".into());
    }
    app.get_webview_window(&label)
        .ok_or_else(|| "Onyx window is unavailable".to_string())?
        .hide()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn show_main_window(app: AppHandle) -> Result<(), String> {
    windowing::show_main(&app)
}

#[tauri::command]
fn platform() -> &'static str {
    std::env::consts::OS
}

#[tauri::command]
async fn chat_send(request: ChatRequest) -> Result<ChatReply, String> {
    if request.messages.is_empty() || request.messages.len() > 128 {
        return Err("Chat history is empty or too long".into());
    }
    if request.provider == ProviderId::Openrouter {
        return openrouter::chat_completion(&request.model, &request.messages, request.web_search)
            .await;
    }
    let prompt = request
        .messages
        .iter()
        .map(|message| format!("{}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n\n");
    let content = providers::cli::chat_once(
        request.provider,
        Some(request.model.clone()),
        prompt,
        request.web_search,
    )
    .await?;
    Ok(ChatReply {
        content,
        model: request.model,
        media: Vec::new(),
    })
}

#[tauri::command]
async fn generate_image(request: MediaGenerationRequest) -> Result<ChatReply, String> {
    openrouter::generate_image(
        &request.model,
        &request.prompt,
        request.aspect_ratio.as_deref(),
    )
    .await
}

#[tauri::command]
async fn start_video(request: MediaGenerationRequest) -> Result<VideoJob, String> {
    openrouter::start_video(
        &request.model,
        &request.prompt,
        request.aspect_ratio.as_deref(),
    )
    .await
}

#[tauri::command]
async fn poll_video(id: String) -> Result<VideoJob, String> {
    openrouter::poll_video(&id).await
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

#[tauri::command]
async fn workspace_repo_summary(workspace: String) -> Result<workspace::RepoSummary, String> {
    workspace::repo_summary(workspace).await
}

#[tauri::command]
async fn workspace_git_init(workspace: String) -> Result<workspace::GitActionResult, String> {
    workspace::init_repository(workspace).await
}

#[tauri::command]
async fn workspace_git_diff(workspace: String) -> Result<String, String> {
    workspace::git_diff(workspace).await
}

#[tauri::command]
fn workspace_read_file(
    workspace: String,
    path: String,
) -> Result<workspace::WorkspaceFile, String> {
    workspace::read_file(workspace, path)
}

#[tauri::command]
fn workspace_editors() -> Vec<workspace::EditorTarget> {
    workspace::editors()
}

#[tauri::command]
fn workspace_open(workspace: String, target: String) -> Result<(), String> {
    workspace::open_workspace(workspace, target)
}

#[tauri::command]
async fn workspace_commit(
    workspace: String,
    message: Option<String>,
) -> Result<workspace::GitActionResult, String> {
    workspace::commit(workspace, message).await
}

#[tauri::command]
async fn workspace_push(workspace: String) -> Result<workspace::GitActionResult, String> {
    workspace::push(workspace).await
}

#[tauri::command]
async fn workspace_create_pr(workspace: String) -> Result<workspace::GitActionResult, String> {
    workspace::create_pr(workspace).await
}

#[tauri::command]
async fn terminal_open(
    app: AppHandle,
    workspace: String,
    cols: u16,
    rows: u16,
    wsl_distribution: Option<String>,
    state: State<'_, AppState>,
) -> Result<terminal::TerminalSession, String> {
    state
        .terminals
        .open(app, workspace, cols, rows, wsl_distribution)
        .await
}

#[tauri::command]
fn list_wsl_distributions() -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        let output = std::process::Command::new("wsl.exe")
            .args(["--list", "--quiet"])
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| format!("Unable to query WSL: {error}"))?;
        if !output.status.success() {
            return Err("WSL is unavailable or has no installed distribution".into());
        }
        let likely_utf16 = output.stdout.starts_with(&[0xff, 0xfe])
            || output
                .stdout
                .iter()
                .skip(1)
                .step_by(2)
                .take(8)
                .all(|byte| *byte == 0);
        let text = if likely_utf16 {
            let words = output
                .stdout
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>();
            String::from_utf16_lossy(&words)
        } else {
            String::from_utf8_lossy(&output.stdout).into_owned()
        };
        Ok(text
            .lines()
            .map(|line| line.trim_matches(['\0', ' ', '\r', '\n']))
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

#[tauri::command]
async fn terminal_write(
    session_id: String,
    data: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.terminals.write(session_id, data).await
}

#[tauri::command]
async fn terminal_resize(
    session_id: String,
    cols: u16,
    rows: u16,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.terminals.resize(session_id, cols, rows).await
}

#[tauri::command]
async fn terminal_close(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.terminals.close(session_id).await
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            list_providers,
            list_provider_models,
            provider_usage,
            list_sessions,
            create_session,
            update_session_options,
            delete_session,
            send_message,
            cancel_turn,
            openrouter_status,
            openrouter_save_key,
            openrouter_clear_key,
            openrouter_models,
            get_voice_settings,
            apply_voice_settings,
            transcribe_audio,
            speak_text,
            inject_text,
            active_app_context,
            set_agent_expanded,
            hide_window,
            show_main_window,
            platform,
            chat_send,
            generate_image,
            start_video,
            poll_video,
            respond_approval,
            workspace_entries,
            workspace_repo_summary,
            workspace_git_init,
            workspace_git_diff,
            workspace_read_file,
            workspace_editors,
            workspace_open,
            workspace_commit,
            workspace_push,
            workspace_create_pr,
            terminal_open,
            list_wsl_distributions,
            terminal_write,
            terminal_resize,
            terminal_close,
        ])
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| error.to_string())?;
            let voice_settings_path = data_dir.join("voice-settings.json");
            let voice_settings = load_voice_settings(&voice_settings_path);
            app.manage(AppState {
                store: Arc::new(SessionStore::load(&data_dir)?),
                running: Arc::new(Mutex::new(HashMap::new())),
                approvals: Arc::new(Mutex::new(HashMap::new())),
                providers: Arc::new(providers::ProviderRuntime::new()),
                terminals: terminal::TerminalRegistry::default(),
                voice_settings: RwLock::new(voice_settings),
                voice_settings_path,
                exiting: AtomicBool::new(false),
            });
            modifier_hold::start(app.handle().clone());
            let _ = windowing::position_saved_windows(app.handle());
            setup_tray(app)?;
            if let Some(window) = app.get_webview_window("main") {
                let window_for_event = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_for_event.hide();
                    }
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Onyx");

    app.run(|handle, event| match event {
        RunEvent::ExitRequested { api, code, .. } => {
            if let Some(state) = handle.try_state::<AppState>() {
                let had_running_turns = !state.running.lock().is_empty();
                let had_terminals = state.terminals.has_sessions();
                if (had_running_turns || state.providers.has_sessions() || had_terminals)
                    && !state.exiting.swap(true, Ordering::SeqCst)
                {
                    api.prevent_exit();
                    for token in state.running.lock().values() {
                        token.cancel();
                    }
                    state.approvals.lock().clear();
                    let handle = handle.clone();
                    let providers = state.providers.clone();
                    let terminals = state.terminals.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = tokio::task::spawn_blocking(move || terminals.shutdown_all()).await;
                        providers.shutdown_all().await;
                        if had_running_turns {
                            tokio::time::sleep(Duration::from_millis(2_500)).await;
                        }
                        handle.exit(code.unwrap_or(0));
                    });
                }
            }
        }
        RunEvent::Exit => {
            if let Some(state) = handle.try_state::<AppState>() {
                for token in state.running.lock().values() {
                    token.cancel();
                }
                state.approvals.lock().clear();
                state.terminals.shutdown_all();
            }
        }
        _ => {}
    });
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Onyx", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Onyx", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::with_id("onyx-tray");
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.tooltip("Onyx")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => {
                let _ = windowing::show_main(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = windowing::show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
