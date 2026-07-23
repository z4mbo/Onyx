use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewWindow};

use crate::{AppState, model::OverlayPosition};

pub fn position_saved_windows(app: &AppHandle) -> Result<(), String> {
    position_hud(app)?;
    position_agent(app, 0)
}

pub fn position_hud(app: &AppHandle) -> Result<(), String> {
    let (position, margin) = {
        let state = app.state::<AppState>();
        let settings = state.voice_settings.read();
        (settings.overlay_position, settings.overlay_margin)
    };
    if let Some(window) = app.get_webview_window("hud") {
        position_window(&window, position, margin)?;
    }
    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn show_hud(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("hud")
        .ok_or_else(|| "Indicatore dettatura non disponibile.".to_string())?;
    position_hud(app)?;
    window
        .set_focusable(false)
        .map_err(|error| error.to_string())?;
    window
        .set_ignore_cursor_events(true)
        .map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn show_agent(app: &AppHandle) -> Result<(), String> {
    set_agent_mode(app, "inactive")?;
    app.get_webview_window("agent")
        .ok_or_else(|| "Overlay agente non disponibile.".to_string())?
        .show()
        .map_err(|error| error.to_string())
}

pub fn set_agent_expanded(app: &AppHandle, expanded: bool) -> Result<(), String> {
    set_agent_mode(app, if expanded { "expanded" } else { "inactive" })
}

pub fn set_agent_mode(app: &AppHandle, mode: &str) -> Result<(), String> {
    let window = app
        .get_webview_window("agent")
        .ok_or_else(|| "Overlay agente non disponibile.".to_string())?;
    let (size, focusable, ignore_cursor) = match mode {
        "inactive" => (LogicalSize::new(180.0, 18.0), false, false),
        "listening" => (LogicalSize::new(260.0, 50.0), false, true),
        "expanded" => (LogicalSize::new(410.0, 460.0), true, false),
        _ => return Err("Unknown agent overlay mode".to_string()),
    };
    window.set_size(size).map_err(|error| error.to_string())?;
    window
        .set_focusable(focusable)
        .map_err(|error| error.to_string())?;
    window
        .set_ignore_cursor_events(ignore_cursor)
        .map_err(|error| error.to_string())?;
    position_agent(app, 0)
}

fn position_agent(app: &AppHandle, margin: u32) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("agent") {
        position_window(&window, OverlayPosition::TopCenter, margin)?;
    }
    Ok(())
}

pub fn show_main(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Finestra Onyx non disponibile.".to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window
        .set_focusable(true)
        .map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

fn position_window(
    window: &WebviewWindow,
    position: OverlayPosition,
    logical_margin: u32,
) -> Result<(), String> {
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or_else(|| "Nessun monitor disponibile.".to_string())?;
    let area = monitor.work_area();
    let window_size = window.outer_size().map_err(|error| error.to_string())?;
    let margin = (f64::from(logical_margin) * monitor.scale_factor()).round() as i32;
    let left = area.position.x;
    let top = area.position.y;
    let right = left + area.size.width as i32;
    let bottom = top + area.size.height as i32;
    let width = window_size.width as i32;
    let height = window_size.height as i32;
    let center_x = left + (area.size.width as i32 - width) / 2;
    let center_y = top + (area.size.height as i32 - height) / 2;
    let (x, y) = match position {
        OverlayPosition::TopLeft => (left + margin, top + margin),
        OverlayPosition::TopCenter => (center_x, top + margin),
        OverlayPosition::TopRight => (right - width - margin, top + margin),
        OverlayPosition::Center => (center_x, center_y),
        OverlayPosition::BottomLeft => (left + margin, bottom - height - margin),
        OverlayPosition::BottomCenter => (center_x, bottom - height - margin),
        OverlayPosition::BottomRight => (right - width - margin, bottom - height - margin),
    };
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())
}
