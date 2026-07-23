use js_sys::{Function, Promise, Reflect};
use serde::{Serialize, de::DeserializeOwned};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;

use crate::model::{
    AgentSession, CreateSessionInput, Message, MessageKind, MessageRole, ProviderStatus,
    SessionStatus, demo_providers,
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
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) return Promise.resolve("/Users/you/Developer/project");
  return invoke("plugin:dialog|open", {
    options: {
      directory: true,
      multiple: false,
      title: "Choose a workspace",
    },
  });
}
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = onyxInvoke)]
    fn raw_invoke(command: &str, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(catch, js_name = onyxListen)]
    fn raw_listen(event_name: &str, callback: &Function) -> Result<Promise, JsValue>;

    #[wasm_bindgen(catch, js_name = onyxOpenDirectory)]
    fn raw_open_directory() -> Result<Promise, JsValue>;
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

pub async fn create_session(input: CreateSessionInput) -> Result<AgentSession, String> {
    if !is_tauri() {
        let now = js_sys::Date::new_0()
            .to_iso_string()
            .as_string()
            .unwrap_or_default();
        return Ok(AgentSession {
            id: format!("rust-preview-{}", js_sys::Date::now()),
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

pub async fn send_message(session: &AgentSession, content: &str) -> Result<AgentSession, String> {
    if !is_tauri() {
        let now = js_sys::Date::new_0()
            .to_iso_string()
            .as_string()
            .unwrap_or_default();
        let mut next = session.clone();
        next.title = if next.messages.is_empty() {
            content.chars().take(56).collect()
        } else {
            next.title
        };
        let base_id = js_sys::Date::now();
        next.messages.push(Message {
            id: format!("rust-user-{base_id}"),
            role: MessageRole::User,
            kind: MessageKind::Text,
            content: content.to_owned(),
            created_at: now.clone(),
        });
        next.messages.push(Message {
            id: format!("rust-assistant-{base_id}"),
            role: MessageRole::Assistant,
            kind: MessageKind::Text,
            content: "This is the Rust frontend preview using the same Onyx session contract."
                .to_owned(),
            created_at: now.clone(),
        });
        next.status = SessionStatus::Idle;
        next.updated_at = now;
        return Ok(next);
    }
    invoke(
        "send_message",
        &serde_json::json!({
            "input": {
                "sessionId": session.id,
                "content": content,
            },
        }),
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
