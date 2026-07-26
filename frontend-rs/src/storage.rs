use serde::{Serialize, de::DeserializeOwned};

use crate::model::VoiceHistoryItem;

pub const LAST_WORKSPACE_KEY: &str = "onyx.last-workspace";
pub const PREFERRED_EDITOR_KEY: &str = "onyx.preferred-editor";
pub const DESKTOP_PREFERENCES_KEY: &str = "onyx.desktop-preferences.v1";
pub const COLOR_SCHEME_KEY: &str = "onyx.color-scheme";
pub const VOICE_HISTORY_KEY: &str = "onyx.voice-history.v1";
pub const RELEASE_NOTES_SEEN_KEY: &str = "onyx.release-notes.seen";

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()
        .and_then(|window| window.local_storage().ok())
        .flatten()
}

pub fn get(key: &str) -> Option<String> {
    local_storage()?.get_item(key).ok().flatten()
}

pub fn set(key: &str, value: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(key, value);
    }
}

pub fn remove(key: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(key);
    }
}

pub fn read_json<T>(key: &str, fallback: T) -> T
where
    T: DeserializeOwned,
{
    get(key)
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or(fallback)
}

pub fn write_json<T>(key: &str, value: &T)
where
    T: Serialize + ?Sized,
{
    if let Ok(value) = serde_json::to_string(value) {
        set(key, &value);
    }
}

pub fn timestamp() -> String {
    js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default()
}

pub fn display_time(timestamp: &str) -> String {
    let milliseconds = js_sys::Date::parse(timestamp);
    if !milliseconds.is_finite() {
        return String::new();
    }
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(milliseconds));
    format!("{:02}:{:02}", date.get_hours(), date.get_minutes())
}

pub fn unique_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{:016x}",
        js_sys::Date::now() as u64,
        (js_sys::Math::random() * u64::MAX as f64) as u64,
    )
}

pub fn dispatch(name: &str) {
    if let (Some(window), Ok(event)) = (web_sys::window(), web_sys::Event::new(name)) {
        let _ = window.dispatch_event(&event);
    }
}

pub fn load_voice_history() -> Vec<VoiceHistoryItem> {
    read_json::<Vec<VoiceHistoryItem>>(VOICE_HISTORY_KEY, Vec::new())
        .into_iter()
        .filter(|item| !item.text.trim().is_empty())
        .take(120)
        .collect()
}

pub fn append_voice_history(item: VoiceHistoryItem) {
    let mut next = vec![item.clone()];
    next.extend(
        load_voice_history()
            .into_iter()
            .filter(|entry| entry.id != item.id),
    );
    next.truncate(120);
    write_json(VOICE_HISTORY_KEY, &next);
    dispatch("onyx:voice-history");
    dispatch("onyx:cloud-data-changed");
}

pub fn clear_voice_history() {
    remove(VOICE_HISTORY_KEY);
    dispatch("onyx:voice-history");
    dispatch("onyx:cloud-data-changed");
}
