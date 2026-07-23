use js_sys::{Function, Promise, Reflect};
use serde::{Serialize, de::DeserializeOwned};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;

use crate::model::{
    AccountProfile, ActiveAppContext, AgentSession, CapturedAudio, ChatReply, ChatRequest,
    ConnectionStatus, CreateSessionInput, EditorTarget, GitActionResult, NativeVoicePermissions,
    OAuthStart, OpenRouterModel, ProviderId, ProviderModelOption, ProviderStatus, ProviderUsage,
    RepoSummary, TerminalSession, TranscriptionReply, UpdateInfo, UpdateSessionOptionsInput,
    VoiceSettings, WorkspaceEntry, WorkspaceFile, demo_providers,
};

#[wasm_bindgen(inline_js = r#"
export function onyxInvoke(command, args) {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) return Promise.reject(new Error("Tauri IPC is unavailable."));
  return invoke(command, args);
}

export function onyxListen(eventName, callback) {
  const listen = window.__TAURI__?.event?.listen;
  if (!listen) return Promise.reject(new Error("Tauri events are unavailable."));
  return listen(eventName, event => callback(event.payload));
}

export function onyxOpenDirectory() {
  return window.__ONYX_RUNTIME__?.openDirectory()
    ?? Promise.resolve("/Users/you/Developer/project");
}

export function onyxRuntime(name, args) {
  const runtime = window.__ONYX_RUNTIME__;
  const fn = runtime?.[name];
  if (typeof fn !== "function") {
    return Promise.reject(new Error(`Onyx runtime function ${name} is unavailable.`));
  }
  return Promise.resolve(fn(...(Array.isArray(args) ? args : [])));
}

export function onyxAudioStart(callback) {
  const fn = window.__ONYX_RUNTIME__?.startAudioCapture;
  if (!fn) return Promise.reject(new Error("Audio capture runtime is unavailable."));
  return fn(level => callback(level));
}

export function onyxTerminalMount(element, sessionId, onData, onResize, autofocus) {
  const fn = window.__ONYX_RUNTIME__?.mountTerminal;
  if (!fn) throw new Error("Terminal runtime is unavailable.");
  return fn(
    element,
    sessionId,
    data => onData(data),
    (cols, rows) => onResize(cols, rows),
    autofocus,
  );
}

export function onyxUpdateInstall(callback) {
  const fn = window.__ONYX_RUNTIME__?.installUpdate;
  if (!fn) return Promise.reject(new Error("Updater runtime is unavailable."));
  return fn((downloaded, total) => callback(downloaded, total));
}

export function onyxCloudStart(callback) {
  const fn = window.__ONYX_RUNTIME__?.startCloudAuth;
  if (!fn) return false;
  return fn(authenticated => callback(authenticated));
}
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = onyxInvoke)]
    fn raw_invoke(command: &str, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(catch, js_name = onyxListen)]
    fn raw_listen(event_name: &str, callback: &Function) -> Result<Promise, JsValue>;

    #[wasm_bindgen(catch, js_name = onyxOpenDirectory)]
    fn raw_open_directory() -> Result<Promise, JsValue>;

    #[wasm_bindgen(catch, js_name = onyxRuntime)]
    fn raw_runtime(name: &str, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(catch, js_name = onyxAudioStart)]
    fn raw_audio_start(callback: &Function) -> Result<Promise, JsValue>;

    #[wasm_bindgen(catch, js_name = onyxTerminalMount)]
    fn raw_terminal_mount(
        element: &web_sys::HtmlElement,
        session_id: &str,
        on_data: &Function,
        on_resize: &Function,
        autofocus: bool,
    ) -> Result<u32, JsValue>;

    #[wasm_bindgen(catch, js_name = onyxUpdateInstall)]
    fn raw_update_install(callback: &Function) -> Result<Promise, JsValue>;

    #[wasm_bindgen(catch, js_name = onyxCloudStart)]
    fn raw_cloud_start(callback: &Function) -> Result<bool, JsValue>;
}

pub fn is_tauri() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    Reflect::has(window.as_ref(), &JsValue::from_str("__TAURI_INTERNALS__")).unwrap_or(false)
        || Reflect::has(window.as_ref(), &JsValue::from_str("__TAURI__")).unwrap_or(false)
}

fn error_text(value: JsValue) -> String {
    value
        .as_string()
        .or_else(|| {
            Reflect::get(&value, &JsValue::from_str("message"))
                .ok()
                .and_then(|message| message.as_string())
        })
        .unwrap_or_else(|| format!("{value:?}"))
}

pub async fn invoke<T, A>(command: &str, args: &A) -> Result<T, String>
where
    T: DeserializeOwned,
    A: Serialize + ?Sized,
{
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    let args = args
        .serialize(&serializer)
        .map_err(|error| error.to_string())?;
    let promise = raw_invoke(command, args).map_err(error_text)?;
    let value = JsFuture::from(promise).await.map_err(error_text)?;
    serde_wasm_bindgen::from_value(value).map_err(|error| error.to_string())
}

pub async fn runtime<T, A>(name: &str, args: &A) -> Result<T, String>
where
    T: DeserializeOwned,
    A: Serialize + ?Sized,
{
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    let args = args
        .serialize(&serializer)
        .map_err(|error| error.to_string())?;
    let promise = raw_runtime(name, args).map_err(error_text)?;
    let value = JsFuture::from(promise).await.map_err(error_text)?;
    serde_wasm_bindgen::from_value(value).map_err(|error| error.to_string())
}

pub async fn list_providers() -> Result<Vec<ProviderStatus>, String> {
    if !is_tauri() {
        return Ok(demo_providers());
    }
    invoke("list_providers", &serde_json::json!({})).await
}

pub async fn list_sessions() -> Result<Vec<AgentSession>, String> {
    if !is_tauri() {
        return Ok(Vec::new());
    }
    invoke("list_sessions", &serde_json::json!({})).await
}

pub async fn provider_models(provider: ProviderId) -> Result<Vec<ProviderModelOption>, String> {
    if !is_tauri() {
        return Ok(Vec::new());
    }
    invoke(
        "list_provider_models",
        &serde_json::json!({ "provider": provider }),
    )
    .await
}

pub async fn provider_usage(provider: ProviderId) -> Result<Option<ProviderUsage>, String> {
    if !is_tauri() {
        return Ok(None);
    }
    invoke(
        "provider_usage",
        &serde_json::json!({ "provider": provider }),
    )
    .await
}

pub async fn create_session(input: CreateSessionInput) -> Result<AgentSession, String> {
    if !is_tauri() {
        let now = js_sys::Date::new_0()
            .to_iso_string()
            .as_string()
            .unwrap_or_default();
        return Ok(AgentSession {
            id: format!("browser-session-{}", js_sys::Date::now()),
            title: "New session".to_owned(),
            provider: input.provider,
            provider_brand: input.provider_brand,
            model: input.model,
            reasoning: input.reasoning,
            speed_mode: input.speed_mode,
            interaction_mode: input.interaction_mode,
            access_mode: input.access_mode,
            workspace: input.workspace,
            provider_session_id: None,
            status: crate::model::SessionStatus::Idle,
            messages: Vec::new(),
            context_usage: None,
            created_at: now.clone(),
            updated_at: now,
        });
    }
    invoke("create_session", &serde_json::json!({ "input": input })).await
}

pub async fn delete_session(session_id: &str) -> Result<(), String> {
    if !is_tauri() {
        return Ok(());
    }
    invoke(
        "delete_session",
        &serde_json::json!({ "sessionId": session_id }),
    )
    .await
}

pub async fn update_session_options(
    input: UpdateSessionOptionsInput,
) -> Result<AgentSession, String> {
    invoke(
        "update_session_options",
        &serde_json::json!({ "input": input }),
    )
    .await
}

pub async fn send_message_id(session_id: &str, content: &str) -> Result<AgentSession, String> {
    invoke(
        "send_message",
        &serde_json::json!({
            "input": {
                "sessionId": session_id,
                "content": content,
            },
        }),
    )
    .await
}

pub async fn steer_turn(session_id: &str, content: &str) -> Result<AgentSession, String> {
    invoke(
        "steer_turn",
        &serde_json::json!({
            "input": {
                "sessionId": session_id,
                "content": content,
            },
        }),
    )
    .await
}

pub async fn cancel_turn(session_id: &str) -> Result<(), String> {
    invoke(
        "cancel_turn",
        &serde_json::json!({ "sessionId": session_id }),
    )
    .await
}

pub async fn choose_workspace() -> Result<Option<String>, String> {
    if !is_tauri() {
        return Ok(Some("/Users/you/Developer/project".to_owned()));
    }
    let value = JsFuture::from(raw_open_directory().map_err(error_text)?)
        .await
        .map_err(error_text)?;
    if value.is_null() || value.is_undefined() {
        Ok(None)
    } else {
        value
            .as_string()
            .map(Some)
            .ok_or_else(|| "The workspace picker returned an invalid path.".to_owned())
    }
}

pub async fn choose_files() -> Result<Vec<String>, String> {
    let value: serde_json::Value = runtime("openFiles", &serde_json::json!([])).await?;
    match value {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::String(path) => Ok(vec![path]),
        serde_json::Value::Array(paths) => paths
            .into_iter()
            .map(|path| {
                path.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "The file picker returned invalid paths.".to_owned())
            })
            .collect(),
        _ => Err("The file picker returned invalid paths.".to_owned()),
    }
}

pub async fn workspace_entries(workspace: &str) -> Result<Vec<WorkspaceEntry>, String> {
    invoke(
        "workspace_entries",
        &serde_json::json!({ "workspace": workspace }),
    )
    .await
}

pub async fn repo_summary(workspace: &str) -> Result<RepoSummary, String> {
    invoke(
        "workspace_repo_summary",
        &serde_json::json!({ "workspace": workspace }),
    )
    .await
}

pub async fn init_git(workspace: &str) -> Result<GitActionResult, String> {
    invoke(
        "workspace_git_init",
        &serde_json::json!({ "workspace": workspace }),
    )
    .await
}

pub async fn git_diff(workspace: &str) -> Result<String, String> {
    invoke(
        "workspace_git_diff",
        &serde_json::json!({ "workspace": workspace }),
    )
    .await
}

pub async fn read_workspace_file(workspace: &str, path: &str) -> Result<WorkspaceFile, String> {
    invoke(
        "workspace_read_file",
        &serde_json::json!({ "workspace": workspace, "path": path }),
    )
    .await
}

pub async fn workspace_editors() -> Result<Vec<EditorTarget>, String> {
    invoke("workspace_editors", &serde_json::json!({})).await
}

pub async fn open_workspace(workspace: &str, target: &str) -> Result<(), String> {
    invoke(
        "workspace_open",
        &serde_json::json!({ "workspace": workspace, "target": target }),
    )
    .await
}

pub async fn commit_workspace(
    workspace: &str,
    message: Option<String>,
) -> Result<GitActionResult, String> {
    invoke(
        "workspace_commit",
        &serde_json::json!({ "workspace": workspace, "message": message }),
    )
    .await
}

pub async fn push_workspace(workspace: &str) -> Result<GitActionResult, String> {
    invoke(
        "workspace_push",
        &serde_json::json!({ "workspace": workspace }),
    )
    .await
}

pub async fn create_pull_request(workspace: &str) -> Result<GitActionResult, String> {
    invoke(
        "workspace_create_pr",
        &serde_json::json!({ "workspace": workspace }),
    )
    .await
}

pub async fn terminal_open(
    workspace: &str,
    cols: u16,
    rows: u16,
    wsl_distribution: Option<String>,
) -> Result<TerminalSession, String> {
    invoke(
        "terminal_open",
        &serde_json::json!({
            "workspace": workspace,
            "cols": cols,
            "rows": rows,
            "wslDistribution": wsl_distribution,
        }),
    )
    .await
}

pub async fn list_wsl_distributions() -> Result<Vec<String>, String> {
    invoke("list_wsl_distributions", &serde_json::json!({})).await
}

pub async fn terminal_write(session_id: &str, data: &str) -> Result<(), String> {
    invoke(
        "terminal_write",
        &serde_json::json!({ "sessionId": session_id, "data": data }),
    )
    .await
}

pub async fn terminal_resize(session_id: &str, cols: u16, rows: u16) -> Result<(), String> {
    invoke(
        "terminal_resize",
        &serde_json::json!({ "sessionId": session_id, "cols": cols, "rows": rows }),
    )
    .await
}

pub async fn terminal_close(session_id: &str) -> Result<(), String> {
    invoke(
        "terminal_close",
        &serde_json::json!({ "sessionId": session_id }),
    )
    .await
}

pub async fn openrouter_status() -> Result<ConnectionStatus, String> {
    invoke("openrouter_status", &serde_json::json!({})).await
}

pub async fn save_openrouter_key(key: &str) -> Result<ConnectionStatus, String> {
    invoke("openrouter_save_key", &serde_json::json!({ "key": key })).await
}

pub async fn clear_openrouter_key() -> Result<ConnectionStatus, String> {
    invoke("openrouter_clear_key", &serde_json::json!({})).await
}

pub async fn openrouter_models() -> Result<Vec<OpenRouterModel>, String> {
    invoke("openrouter_models", &serde_json::json!({})).await
}

pub async fn openai_status() -> Result<ConnectionStatus, String> {
    invoke("openai_status", &serde_json::json!({})).await
}

pub async fn save_openai_key(key: &str) -> Result<ConnectionStatus, String> {
    invoke("openai_save_key", &serde_json::json!({ "key": key })).await
}

pub async fn clear_openai_key() -> Result<ConnectionStatus, String> {
    invoke("openai_clear_key", &serde_json::json!({})).await
}

pub async fn get_voice_settings() -> Result<VoiceSettings, String> {
    invoke("get_voice_settings", &serde_json::json!({})).await
}

pub async fn apply_voice_settings(settings: VoiceSettings) -> Result<VoiceSettings, String> {
    invoke(
        "apply_voice_settings",
        &serde_json::json!({ "settings": settings }),
    )
    .await
}

pub async fn request_microphone_permission() -> Result<String, String> {
    invoke("request_microphone_permission", &serde_json::json!({})).await
}

pub async fn native_voice_permissions() -> Result<NativeVoicePermissions, String> {
    invoke("native_voice_permissions", &serde_json::json!({})).await
}

pub async fn request_native_voice_permissions() -> Result<NativeVoicePermissions, String> {
    invoke("request_native_voice_permissions", &serde_json::json!({})).await
}

pub async fn transcribe_audio(
    audio_base64: &str,
    format: &str,
) -> Result<TranscriptionReply, String> {
    invoke(
        "transcribe_audio",
        &serde_json::json!({
            "request": {
                "audioBase64": audio_base64,
                "format": format,
            },
        }),
    )
    .await
}

pub async fn speak_text(text: &str) -> Result<String, String> {
    invoke("speak_text", &serde_json::json!({ "text": text })).await
}

pub async fn inject_text(text: &str) -> Result<(), String> {
    invoke("inject_text", &serde_json::json!({ "text": text })).await
}

pub async fn active_app_context() -> Result<ActiveAppContext, String> {
    invoke("active_app_context", &serde_json::json!({})).await
}

pub async fn set_agent_overlay_mode(mode: &str) -> Result<(), String> {
    invoke(
        "set_agent_overlay_mode",
        &serde_json::json!({ "mode": mode }),
    )
    .await
}

pub async fn hide_window(label: &str) -> Result<(), String> {
    invoke("hide_window", &serde_json::json!({ "label": label })).await
}

pub async fn platform() -> Result<String, String> {
    invoke("platform", &serde_json::json!({})).await
}

pub async fn chat_send(request: ChatRequest) -> Result<ChatReply, String> {
    invoke("chat_send", &serde_json::json!({ "request": request })).await
}

pub async fn respond_approval(id: &str, allow: bool, for_session: bool) -> Result<(), String> {
    invoke(
        "respond_approval",
        &serde_json::json!({
            "id": id,
            "allow": allow,
            "forSession": for_session,
        }),
    )
    .await
}

pub async fn account_profile() -> Result<Option<AccountProfile>, String> {
    invoke("clerk_account_profile", &serde_json::json!({})).await
}

pub async fn start_clerk_oauth(login_hint: Option<&str>) -> Result<OAuthStart, String> {
    invoke(
        "start_clerk_oauth",
        &serde_json::json!({ "loginHint": login_hint }),
    )
    .await
}

pub async fn clerk_sign_out() -> Result<(), String> {
    invoke("clerk_sign_out", &serde_json::json!({})).await
}

pub async fn open_url(url: &str) -> Result<(), String> {
    runtime("openUrl", &serde_json::json!([url])).await
}

pub async fn set_window_theme(theme: Option<&str>) -> Result<(), String> {
    runtime("setWindowTheme", &serde_json::json!([theme])).await
}

pub async fn request_microphone_access() -> Result<(), String> {
    runtime("requestMicrophoneAccess", &serde_json::json!([])).await
}

pub async fn stop_audio_capture() -> Result<CapturedAudio, String> {
    runtime("stopAudioCapture", &serde_json::json!([])).await
}

pub async fn cancel_audio_capture() -> Result<(), String> {
    runtime("cancelAudioCapture", &serde_json::json!([])).await
}

pub async fn play_audio(source: &str) -> Result<(), String> {
    runtime("playAudio", &serde_json::json!([source])).await
}

pub async fn copy_text(text: &str) -> Result<(), String> {
    runtime("copyText", &serde_json::json!([text])).await
}

pub async fn check_update() -> Result<Option<UpdateInfo>, String> {
    runtime("checkUpdate", &serde_json::json!([])).await
}

pub async fn cloud_configured() -> Result<bool, String> {
    runtime("cloudConfigured", &serde_json::json!([])).await
}

pub async fn push_cloud(payload: &str) -> Result<(), String> {
    runtime("pushCloud", &serde_json::json!([payload])).await
}

pub async fn pull_cloud() -> Result<Option<String>, String> {
    runtime("pullCloud", &serde_json::json!([])).await
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct RuntimeBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub async fn show_provider_view(provider: &str, bounds: RuntimeBounds) -> Result<(), String> {
    runtime("showProvider", &serde_json::json!([provider, bounds])).await
}

pub async fn focus_provider_view(provider: &str) -> Result<(), String> {
    runtime("focusProvider", &serde_json::json!([provider])).await
}

pub async fn hide_provider_view(provider: Option<&str>) -> Result<(), String> {
    runtime("hideProvider", &serde_json::json!([provider])).await
}

pub struct AudioCaptureHandle {
    _callback: Closure<dyn FnMut(f64)>,
}

pub async fn start_audio_capture<F>(mut on_level: F) -> Result<AudioCaptureHandle, String>
where
    F: FnMut(f64) + 'static,
{
    let callback = Closure::wrap(Box::new(move |level: f64| {
        on_level(level);
    }) as Box<dyn FnMut(f64)>);
    let promise = raw_audio_start(callback.as_ref().unchecked_ref()).map_err(error_text)?;
    JsFuture::from(promise).await.map_err(error_text)?;
    Ok(AudioCaptureHandle {
        _callback: callback,
    })
}

pub struct MountedTerminal {
    handle_id: u32,
    session_id: String,
    _on_data: Closure<dyn FnMut(String)>,
    _on_resize: Closure<dyn FnMut(u32, u32)>,
}

impl Drop for MountedTerminal {
    fn drop(&mut self) {
        let serializer = serde_wasm_bindgen::Serializer::json_compatible();
        let Ok(args) = serde_json::json!([self.handle_id, &self.session_id]).serialize(&serializer)
        else {
            return;
        };
        let _ = raw_runtime("unmountTerminal", args);
    }
}

pub fn mount_terminal<F, R>(
    element: &web_sys::HtmlElement,
    session_id: &str,
    mut on_data: F,
    mut on_resize: R,
    autofocus: bool,
) -> Result<MountedTerminal, String>
where
    F: FnMut(String) + 'static,
    R: FnMut(u16, u16) + 'static,
{
    let data_callback = Closure::wrap(Box::new(move |data: String| {
        on_data(data);
    }) as Box<dyn FnMut(String)>);
    let resize_callback = Closure::wrap(Box::new(move |cols: u32, rows: u32| {
        on_resize(
            cols.min(u32::from(u16::MAX)) as u16,
            rows.min(u32::from(u16::MAX)) as u16,
        );
    }) as Box<dyn FnMut(u32, u32)>);
    let handle_id = raw_terminal_mount(
        element,
        session_id,
        data_callback.as_ref().unchecked_ref(),
        resize_callback.as_ref().unchecked_ref(),
        autofocus,
    )
    .map_err(error_text)?;
    Ok(MountedTerminal {
        handle_id,
        session_id: session_id.to_owned(),
        _on_data: data_callback,
        _on_resize: resize_callback,
    })
}

pub async fn terminal_runtime_write(session_id: &str, data: &str) -> Result<(), String> {
    runtime("writeTerminal", &serde_json::json!([session_id, data])).await
}

pub async fn terminal_runtime_exit(
    session_id: &str,
    exit_code: Option<u32>,
    error: Option<&str>,
) -> Result<(), String> {
    runtime(
        "exitTerminal",
        &serde_json::json!([session_id, exit_code, error]),
    )
    .await
}

pub async fn terminal_runtime_clear(session_id: &str) -> Result<(), String> {
    runtime("clearTerminal", &serde_json::json!([session_id])).await
}

pub async fn terminal_runtime_forget(session_id: &str) -> Result<(), String> {
    runtime("forgetTerminal", &serde_json::json!([session_id])).await
}

pub async fn install_update<F>(mut on_progress: F) -> Result<(), String>
where
    F: FnMut(u64, Option<u64>) + 'static,
{
    let callback = Closure::wrap(Box::new(move |downloaded: f64, total: JsValue| {
        let total = if total.is_null() || total.is_undefined() {
            None
        } else {
            total.as_f64().map(|value| value as u64)
        };
        on_progress(downloaded as u64, total);
    }) as Box<dyn FnMut(f64, JsValue)>);
    let promise = raw_update_install(callback.as_ref().unchecked_ref()).map_err(error_text)?;
    JsFuture::from(promise).await.map_err(error_text)?;
    Ok(())
}

pub struct CloudAuthHandle {
    _callback: Closure<dyn FnMut(bool)>,
}

pub fn start_cloud_auth<F>(mut on_authenticated: F) -> Result<Option<CloudAuthHandle>, String>
where
    F: FnMut(bool) + 'static,
{
    let callback = Closure::wrap(Box::new(move |authenticated: bool| {
        on_authenticated(authenticated);
    }) as Box<dyn FnMut(bool)>);
    let configured = raw_cloud_start(callback.as_ref().unchecked_ref()).map_err(error_text)?;
    if configured {
        Ok(Some(CloudAuthHandle {
            _callback: callback,
        }))
    } else {
        Ok(None)
    }
}

pub struct EventListener {
    _callback: Closure<dyn FnMut(JsValue)>,
    unlisten: Option<Function>,
}

impl EventListener {
    pub fn forget(self) {
        std::mem::forget(self);
    }
}

impl Drop for EventListener {
    fn drop(&mut self) {
        if let Some(unlisten) = self.unlisten.take() {
            let _ = unlisten.call0(&JsValue::UNDEFINED);
        }
    }
}

pub async fn listen<T, F>(event_name: &str, mut handler: F) -> Result<EventListener, String>
where
    T: DeserializeOwned + 'static,
    F: FnMut(T) + 'static,
{
    let callback = Closure::wrap(Box::new(move |payload: JsValue| {
        if let Ok(payload) = serde_wasm_bindgen::from_value(payload) {
            handler(payload);
        }
    }) as Box<dyn FnMut(JsValue)>);
    let promise = raw_listen(event_name, callback.as_ref().unchecked_ref()).map_err(error_text)?;
    let unlisten = JsFuture::from(promise)
        .await
        .map_err(error_text)?
        .dyn_into::<Function>()
        .ok();
    Ok(EventListener {
        _callback: callback,
        unlisten,
    })
}
