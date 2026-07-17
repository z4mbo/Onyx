use tauri::{AppHandle, Manager};

use crate::{models::AppSettings, provider, state::AppState, windowing};

/// Applies non-secret settings. Modifier-only hold gestures are handled by the
/// native listener and intentionally are not registered as ordinary accelerators.
pub fn apply(app: &AppHandle, mut next: AppSettings) -> Result<AppSettings, String> {
    normalize(&mut next)?;
    *app.state::<AppState>()
        .settings
        .write()
        .map_err(|_| "Stato impostazioni non disponibile.".to_string())? = next.clone();
    windowing::position_saved_windows(app)?;
    Ok(next)
}

fn normalize(settings: &mut AppSettings) -> Result<(), String> {
    settings.wispr_shortcut = "Ctrl+Shift (hold)".into();
    settings.dictation_shortcut = "Ctrl+Shift (hold)".into();
    settings.agent_shortcut = "Ctrl+Alt (hold)".into();
    settings.stt_provider = normalize_provider(&settings.stt_provider)?;
    settings.agent_provider = normalize_provider(&settings.agent_provider)?;
    settings.stt_model = settings.stt_model.trim().to_string();
    settings.agent_model = settings.agent_model.trim().to_string();
    settings.reasoning = match settings.reasoning.trim() {
        "none" | "low" | "medium" | "high" | "xhigh" => settings.reasoning.trim().into(),
        _ => "medium".into(),
    };
    settings.overlay_margin = settings.overlay_margin.clamp(8, 120);
    settings.language = settings
        .language
        .take()
        .map(|language| language.trim().to_ascii_lowercase())
        .filter(|language| !language.is_empty());
    provider::validate_model_id(&settings.stt_model)?;
    provider::validate_model_id(&settings.agent_model)?;
    if settings.language.as_ref().is_some_and(|language| {
        language.len() > 12
            || !language
                .bytes()
                .all(|byte| byte.is_ascii_alphabetic() || byte == b'-')
    }) {
        return Err("Codice lingua non valido.".into());
    }
    Ok(())
}

fn normalize_provider(value: &str) -> Result<String, String> {
    match value.trim() {
        "openrouter"
        | "openai"
        | "chatgpt_codex"
        | "local"
        | "managed"
        | "anthropic_api"
        | "claude_subscription_agent_sdk" => Ok(value.trim().to_string()),
        _ => Err("Provider non supportato.".into()),
    }
}
