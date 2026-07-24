use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Position, Rect, Size, Url, Webview,
    WebviewUrl,
    webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder},
};

const BROWSER_LABEL_PREFIX: &str = "internal-browser-";
const MAX_BROWSER_LABEL_BYTES: usize = 96;
const MAX_BROWSER_URL_BYTES: usize = 8 * 1024;
const MAX_BROWSER_WEBVIEWS: usize = 8;
const MAX_LOGICAL_COORDINATE: f64 = 32_768.0;
const MIN_LOGICAL_SIZE: f64 = 1.0;
const MAX_LOGICAL_SIZE: f64 = 16_384.0;
static BROWSERS_VISIBLE: AtomicBool = AtomicBool::new(true);

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalBrowserBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InternalBrowserNavigation {
    label: String,
    url: String,
}

impl InternalBrowserBounds {
    fn checked(self) -> Result<Rect, String> {
        let values = [self.x, self.y, self.width, self.height];
        if values.iter().any(|value| !value.is_finite()) {
            return Err("Internal browser bounds must be finite".into());
        }
        if self.x < 0.0
            || self.y < 0.0
            || self.x > MAX_LOGICAL_COORDINATE
            || self.y > MAX_LOGICAL_COORDINATE
        {
            return Err("Internal browser position is outside the supported range".into());
        }
        if !(MIN_LOGICAL_SIZE..=MAX_LOGICAL_SIZE).contains(&self.width)
            || !(MIN_LOGICAL_SIZE..=MAX_LOGICAL_SIZE).contains(&self.height)
        {
            return Err("Internal browser size is outside the supported range".into());
        }
        if self.x + self.width > MAX_LOGICAL_COORDINATE
            || self.y + self.height > MAX_LOGICAL_COORDINATE
        {
            return Err("Internal browser bounds exceed the supported area".into());
        }
        Ok(Rect {
            position: Position::Logical(LogicalPosition::new(self.x, self.y)),
            size: Size::Logical(LogicalSize::new(self.width, self.height)),
        })
    }
}

fn checked_label(label: &str) -> Result<&str, String> {
    if !label.starts_with(BROWSER_LABEL_PREFIX)
        || label.len() <= BROWSER_LABEL_PREFIX.len()
        || label.len() > MAX_BROWSER_LABEL_BYTES
        || !label.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        return Err("Invalid internal browser identifier".into());
    }
    Ok(label)
}

fn checked_url(raw: &str) -> Result<Url, String> {
    if raw.is_empty() || raw.len() > MAX_BROWSER_URL_BYTES || raw.chars().any(char::is_control) {
        return Err("Invalid internal browser URL".into());
    }
    let parsed = Url::parse(raw).map_err(|_| "Enter a valid HTTP or HTTPS URL".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("Only credential-free HTTP and HTTPS URLs are supported".into());
    }
    Ok(parsed)
}

fn is_allowed_navigation(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.as_str().len() <= MAX_BROWSER_URL_BYTES
}

fn browser_webview(app: &AppHandle, label: &str) -> Result<Webview, String> {
    checked_label(label)?;
    let webview = app
        .get_webview(label)
        .ok_or_else(|| "Internal browser is not open".to_string())?;
    if webview.window().label() != "main" {
        return Err("Internal browser is attached to an unexpected window".into());
    }
    Ok(webview)
}

fn browser_count(app: &AppHandle) -> usize {
    app.webviews()
        .values()
        .filter(|webview| {
            webview.label().starts_with(BROWSER_LABEL_PREFIX) && webview.window().label() == "main"
        })
        .count()
}

#[tauri::command]
pub async fn internal_browser_open(
    app: AppHandle,
    label: String,
    url: String,
    bounds: InternalBrowserBounds,
) -> Result<String, String> {
    checked_label(&label)?;
    let url = checked_url(&url)?;
    let bounds = bounds.checked()?;

    if let Some(webview) = app.get_webview(&label) {
        if webview.window().label() != "main" {
            return Err("Internal browser is attached to an unexpected window".into());
        }
        webview
            .set_bounds(bounds)
            .map_err(|error| error.to_string())?;
        webview
            .navigate(url.clone())
            .map_err(|error| error.to_string())?;
        if BROWSERS_VISIBLE.load(Ordering::Relaxed) {
            webview.show().map_err(|error| error.to_string())?;
        } else {
            webview.hide().map_err(|error| error.to_string())?;
        }
        return Ok(url.into());
    }
    if browser_count(&app) >= MAX_BROWSER_WEBVIEWS {
        return Err(format!(
            "At most {MAX_BROWSER_WEBVIEWS} internal browsers may be open"
        ));
    }

    let window = app
        .get_window("main")
        .ok_or_else(|| "The main Onyx window is unavailable".to_string())?;
    let popup_label = label.clone();
    let popup_app = app.clone();
    let builder = WebviewBuilder::new(label, WebviewUrl::External(url.clone()))
        .on_navigation(is_allowed_navigation)
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Finished && is_allowed_navigation(payload.url()) {
                let _ = webview.app_handle().emit(
                    "onyx://internal-browser-navigation",
                    InternalBrowserNavigation {
                        label: webview.label().to_owned(),
                        url: payload.url().to_string(),
                    },
                );
            }
        })
        .on_new_window(move |candidate, _features| {
            if is_allowed_navigation(&candidate) {
                // Keep provider OAuth and target=_blank flows inside Onyx. Navigating
                // the same child avoids silently handing authenticated URLs to the
                // system browser and keeps the browser count bounded.
                let app = popup_app.clone();
                let label = popup_label.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(webview) = app.get_webview(&label) {
                        let _ = webview.navigate(candidate);
                    }
                });
            }
            NewWindowResponse::Deny
        });
    let webview = window
        .add_child(builder, bounds.position, bounds.size)
        .map_err(|error| error.to_string())?;
    if !BROWSERS_VISIBLE.load(Ordering::Relaxed) {
        webview.hide().map_err(|error| error.to_string())?;
    }
    Ok(url.into())
}

#[tauri::command]
pub fn internal_browsers_set_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    BROWSERS_VISIBLE.store(visible, Ordering::Relaxed);
    for webview in app.webviews().values().filter(|webview| {
        webview.label().starts_with(BROWSER_LABEL_PREFIX) && webview.window().label() == "main"
    }) {
        if visible {
            webview.show()
        } else {
            webview.hide()
        }
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn internal_browser_navigate(
    app: AppHandle,
    label: String,
    url: String,
) -> Result<String, String> {
    let url = checked_url(&url)?;
    browser_webview(&app, &label)?
        .navigate(url.clone())
        .map_err(|error| error.to_string())?;
    Ok(url.into())
}

#[tauri::command]
pub async fn internal_browser_back(app: AppHandle, label: String) -> Result<(), String> {
    browser_webview(&app, &label)?
        .eval("window.history.back()")
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn internal_browser_forward(app: AppHandle, label: String) -> Result<(), String> {
    browser_webview(&app, &label)?
        .eval("window.history.forward()")
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn internal_browser_reload(app: AppHandle, label: String) -> Result<(), String> {
    browser_webview(&app, &label)?
        .reload()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn internal_browser_set_bounds(
    app: AppHandle,
    label: String,
    bounds: InternalBrowserBounds,
) -> Result<(), String> {
    browser_webview(&app, &label)?
        .set_bounds(bounds.checked()?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn internal_browser_close(app: AppHandle, label: String) -> Result<(), String> {
    checked_label(&label)?;
    let Some(webview) = app.get_webview(&label) else {
        return Ok(());
    };
    if webview.window().label() != "main" {
        return Err("Internal browser is attached to an unexpected window".into());
    }
    webview.close().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_cannot_target_existing_application_webviews() {
        assert!(checked_label("internal-browser-session_42").is_ok());
        assert!(checked_label("main").is_err());
        assert!(checked_label("internal-browser-").is_err());
        assert!(checked_label("internal-browser-UPPER").is_err());
        assert!(checked_label("internal-browser-bad/slash").is_err());
    }

    #[test]
    fn navigation_is_limited_to_credential_free_http_urls() {
        assert!(checked_url("https://chatgpt.com/").is_ok());
        assert!(checked_url("http://127.0.0.1:3000/path").is_ok());
        assert!(checked_url("javascript:alert(1)").is_err());
        assert!(checked_url("file:///etc/passwd").is_err());
        assert!(checked_url("https://user:secret@example.com/").is_err());
    }

    #[test]
    fn bounds_are_finite_positive_and_bounded() {
        assert!(
            InternalBrowserBounds {
                x: 12.0,
                y: 44.0,
                width: 800.0,
                height: 600.0,
            }
            .checked()
            .is_ok()
        );
        assert!(
            InternalBrowserBounds {
                x: -1.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            }
            .checked()
            .is_err()
        );
        assert!(
            InternalBrowserBounds {
                x: 0.0,
                y: 0.0,
                width: f64::NAN,
                height: 100.0,
            }
            .checked()
            .is_err()
        );
    }
}
