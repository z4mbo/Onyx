use std::sync::atomic::{AtomicBool, Ordering};

use enigo::{Enigo, Keyboard, Settings as EnigoSettings};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::{
    active_app,
    models::{
        ActiveAppContext, AppSettings, ModelOption, SearchReply, SearchRequest, TranscriptionReply,
        TranscriptionRequest, TtsConfig, TtsSpeakReply, TtsVoiceOption,
    },
    oauth, provider, secrets, shortcuts,
    state::AppState,
    tts, windowing,
};

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .settings
        .read()
        .map(|settings| settings.clone())
        .map_err(|_| "Stato impostazioni non disponibile.".into())
}

#[tauri::command]
pub fn apply_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    shortcuts::apply(&app, settings)
}

#[tauri::command]
pub fn get_tts_config(state: State<'_, AppState>) -> Result<TtsConfig, String> {
    state
        .tts_config
        .read()
        .map(|config| config.clone())
        .map_err(|_| "Stato impostazioni voce non disponibile.".into())
}

#[tauri::command]
pub fn save_tts_config(state: State<'_, AppState>, config: TtsConfig) -> Result<TtsConfig, String> {
    let normalized = tts::normalize_config(config)?;
    let mut stored = state
        .tts_config
        .write()
        .map_err(|_| "Stato impostazioni voce non disponibile.".to_string())?;
    tts::save_config(&state.tts_config_path, &normalized)?;
    *stored = normalized.clone();
    Ok(normalized)
}

#[tauri::command]
pub async fn list_tts_voices(
    state: State<'_, AppState>,
    provider_name: Option<String>,
    model: Option<String>,
) -> Result<Vec<TtsVoiceOption>, String> {
    let config = state
        .tts_config
        .read()
        .map_err(|_| "Stato impostazioni voce non disponibile.".to_string())?
        .clone();
    let provider_name = provider_name
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| config.provider.clone());
    let model = model
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| config.model.clone());
    tts::list_voices(&provider_name, Some(&model), &config.voice).await
}

#[tauri::command]
pub async fn preview_tts(
    state: State<'_, AppState>,
    text: Option<String>,
) -> Result<TtsSpeakReply, String> {
    run_tts(state.inner(), tts::preview_text(text)).await
}

#[tauri::command]
pub async fn speak_tts(state: State<'_, AppState>, text: String) -> Result<TtsSpeakReply, String> {
    run_tts(state.inner(), text).await
}

#[tauri::command]
pub async fn transcribe_audio(
    state: State<'_, AppState>,
    request: TranscriptionRequest,
) -> Result<TranscriptionReply, String> {
    let settings = state
        .settings
        .read()
        .map_err(|_| "Stato impostazioni non disponibile.".to_string())?
        .clone();
    if !matches!(settings.stt_provider.as_str(), "openrouter" | "openai") {
        return Err(match settings.stt_provider.as_str() {
            "local" => "Configura un endpoint Whisper locale prima di usare questa route.".into(),
            "managed" => {
                "Onyx Managed richiede un abbonamento attivo e il backend configurato.".into()
            }
            _ => "Il provider selezionato non offre trascrizione in questa versione.".into(),
        });
    }
    let key = secrets::get_provider_key(&settings.stt_provider)?.ok_or_else(|| {
        format!(
            "Collega {} nella sezione Modelli prima di usare la dettatura.",
            provider_label(&settings.stt_provider)
        )
    })?;
    provider::transcribe(
        &state.client,
        &settings.stt_provider,
        &key,
        &settings.stt_model,
        settings.language.as_deref(),
        &request,
    )
    .await
}

#[tauri::command]
pub async fn search_web(
    state: State<'_, AppState>,
    request: SearchRequest,
) -> Result<SearchReply, String> {
    if request.provider == "chatgpt_codex" {
        return state.codex.search(&request).await;
    }
    if !matches!(
        request.provider.as_str(),
        "openrouter" | "openai" | "anthropic_api"
    ) {
        return Err(match request.provider.as_str() {
            "local" => {
                "Per la ricerca locale configura un endpoint LLM e un motore di ricerca.".into()
            }
            "managed" => {
                "Onyx Managed richiede un abbonamento attivo e il backend configurato.".into()
            }
            "claude_subscription_agent_sdk" => {
                "Il runtime Claude Agent SDK non è ancora installato in questa build.".into()
            }
            _ => "Provider di ricerca non supportato.".into(),
        });
    }
    let key = secrets::get_provider_key(&request.provider)?.ok_or_else(|| {
        format!(
            "Collega {} nella sezione Modelli prima di usare la ricerca.",
            provider_label(&request.provider)
        )
    })?;
    provider::search_web(&state.client, &key, &request).await
}

#[tauri::command]
pub async fn provider_connection_status(
    state: State<'_, AppState>,
    provider_name: String,
) -> Result<bool, String> {
    if provider_name == "chatgpt_codex" {
        return state
            .codex
            .account_status()
            .await
            .map(|status| status.connected);
    }
    if !matches!(
        provider_name.as_str(),
        "openrouter" | "openai" | "anthropic_api"
    ) {
        return Ok(false);
    }
    let Some(api_key) = secrets::get_provider_key(&provider_name)? else {
        return Ok(false);
    };
    provider::validate_provider_key(&state.client, &provider_name, &api_key).await?;
    Ok(true)
}

#[tauri::command]
pub async fn save_provider_api_key(
    state: State<'_, AppState>,
    provider_name: String,
    api_key: String,
) -> Result<(), String> {
    let candidate = api_key.trim();
    provider::validate_provider_key(&state.client, &provider_name, candidate).await?;
    secrets::set_provider_key(&provider_name, candidate)
}

#[tauri::command]
pub fn disconnect_provider(provider_name: String) -> Result<(), String> {
    secrets::delete_provider_key(&provider_name)
}

#[tauri::command]
pub async fn list_models(
    state: State<'_, AppState>,
    provider_name: String,
    capability: String,
) -> Result<Vec<ModelOption>, String> {
    if provider_name == "chatgpt_codex" {
        if capability != "web_search" {
            return Err("ChatGPT/Codex Ã¨ disponibile soltanto per l'agente di ricerca.".into());
        }
        return state.codex.list_models().await;
    }
    let key = secrets::get_provider_key(&provider_name).ok().flatten();
    provider::list_models(&state.client, &provider_name, &capability, key.as_deref()).await
}

// Compatibility commands for the 0.8 frontend and OAuth callback.
#[tauri::command]
pub async fn openrouter_connection_status(state: State<'_, AppState>) -> Result<bool, String> {
    provider_connection_status(state, "openrouter".into()).await
}

#[tauri::command]
pub async fn save_openrouter_api_key(
    state: State<'_, AppState>,
    api_key: String,
) -> Result<(), String> {
    save_provider_api_key(state, "openrouter".into(), api_key).await
}

#[tauri::command]
pub fn disconnect_openrouter() -> Result<(), String> {
    secrets::delete_openrouter_key()
}

#[tauri::command]
pub async fn begin_openrouter_oauth(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    oauth::start(app, state.client.clone(), state.oauth_in_progress.clone()).await
}

#[tauri::command]
pub async fn list_openrouter_transcription_models(
    state: State<'_, AppState>,
) -> Result<Vec<ModelOption>, String> {
    let key = secrets::get_openrouter_key()?;
    provider::list_transcription_models(&state.client, key.as_deref()).await
}

#[tauri::command]
pub fn inject_text(text: String) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > 100_000 {
        return Err("Il testo da inserire è vuoto o troppo lungo.".into());
    }
    let mut enigo = Enigo::new(&EnigoSettings::default())
        .map_err(|error| format!("Input nativo non disponibile: {error}"))?;
    enigo.text(&text).map_err(|error| format!(
        "Non riesco a inserire il testo. Su macOS abilita Onyx in Privacy e sicurezza → Accessibilità; su Windows il target non deve essere eseguito come amministratore. Dettagli: {error}"
    ))
}

#[tauri::command]
pub fn active_app_context() -> ActiveAppContext {
    active_app::current()
}

#[tauri::command]
pub fn set_agent_expanded(app: AppHandle, expanded: bool) -> Result<(), String> {
    windowing::set_agent_expanded(&app, expanded)
}

#[tauri::command]
pub fn open_external(app: AppHandle, target: String) -> Result<(), String> {
    let url = url::Url::parse(&target).map_err(|_| "Link non valido.".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Onyx può aprire soltanto link web http/https.".into());
    }
    app.opener()
        .open_url(target, None::<&str>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) -> Result<(), String> {
    windowing::show_main(&app)
}

#[tauri::command]
pub fn hide_window(app: AppHandle, label: String) -> Result<(), String> {
    if !matches!(label.as_str(), "main" | "hud" | "agent") {
        return Err("Finestra non valida.".into());
    }
    app.get_webview_window(&label)
        .ok_or_else(|| "Finestra non disponibile.".to_string())?
        .hide()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn platform() -> &'static str {
    std::env::consts::OS
}

fn provider_label(value: &str) -> &str {
    match value {
        "openrouter" => "OpenRouter",
        "openai" => "OpenAI",
        "anthropic_api" => "Anthropic",
        _ => "il provider selezionato",
    }
}

async fn run_tts(state: &AppState, text: String) -> Result<TtsSpeakReply, String> {
    let _guard = TtsInProgressGuard::acquire(&state.tts_in_progress)?;
    let config = state
        .tts_config
        .read()
        .map_err(|_| "Stato impostazioni voce non disponibile.".to_string())?
        .clone();
    tts::speak(&state.client, &config, &text).await
}

struct TtsInProgressGuard<'a>(&'a AtomicBool);

impl<'a> TtsInProgressGuard<'a> {
    fn acquire(flag: &'a AtomicBool) -> Result<Self, String> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self(flag))
            .map_err(|_| "Onyx sta già riproducendo una risposta.".into())
    }
}

impl Drop for TtsInProgressGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
