use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::{
    codex::{CodexAccountStatus, CodexDeviceLoginStart, CodexLoginStart, CodexRateLimits},
    state::AppState,
};

#[tauri::command]
pub async fn chatgpt_account_status(
    state: State<'_, AppState>,
) -> Result<CodexAccountStatus, String> {
    state.codex.account_status().await
}

#[tauri::command]
pub async fn begin_chatgpt_login(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CodexLoginStart, String> {
    let login = state.codex.begin_login().await?;
    app.opener()
        .open_url(login.auth_url.clone(), None::<&str>)
        .map_err(|error| format!("Non riesco ad aprire il login ChatGPT: {error}"))?;
    Ok(login)
}

#[tauri::command]
pub async fn begin_chatgpt_device_login(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CodexDeviceLoginStart, String> {
    let login = state.codex.begin_device_login().await?;
    app.opener()
        .open_url(login.verification_url.clone(), None::<&str>)
        .map_err(|error| format!("Non riesco ad aprire il device login ChatGPT: {error}"))?;
    Ok(login)
}

#[tauri::command]
pub async fn disconnect_chatgpt(state: State<'_, AppState>) -> Result<(), String> {
    state.codex.logout().await
}

#[tauri::command]
pub async fn chatgpt_rate_limits(state: State<'_, AppState>) -> Result<CodexRateLimits, String> {
    state.codex.rate_limits().await
}
