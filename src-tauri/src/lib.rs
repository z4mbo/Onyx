mod active_app;
mod codex;
mod codex_commands;
mod commands;
mod models;
mod modifier_hold;
mod oauth;
mod provider;
mod secrets;
mod shortcuts;
mod state;
mod tts;
mod windowing;

use tauri::{
    Manager, WindowEvent,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::new(state::platform_config_dir())
        .expect("Onyx application state could not be initialized");
    tauri::Builder::default()
        .manage(app_state)
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = windowing::show_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::apply_settings,
            commands::get_tts_config,
            commands::save_tts_config,
            commands::list_tts_voices,
            commands::preview_tts,
            commands::speak_tts,
            commands::transcribe_audio,
            commands::openrouter_connection_status,
            commands::save_openrouter_api_key,
            commands::disconnect_openrouter,
            commands::begin_openrouter_oauth,
            commands::list_openrouter_transcription_models,
            commands::provider_connection_status,
            commands::save_provider_api_key,
            commands::disconnect_provider,
            commands::list_models,
            commands::search_web,
            commands::inject_text,
            commands::active_app_context,
            commands::set_agent_expanded,
            commands::open_external,
            commands::show_main_window,
            commands::hide_window,
            commands::quit_app,
            commands::platform,
            codex_commands::chatgpt_account_status,
            codex_commands::begin_chatgpt_login,
            codex_commands::begin_chatgpt_device_login,
            codex_commands::disconnect_chatgpt,
            codex_commands::chatgpt_rate_limits,
        ])
        .setup(|app| {
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
        .run(tauri::generate_context!())
        .expect("error while running Onyx");
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Apri Onyx", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Esci da Onyx", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::with_id("onyx-tray");
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.tooltip("Onyx · assistente vocale")
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
