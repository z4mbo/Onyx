use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

use icondata::{LuArrowLeft, LuArrowRight, LuGlobe, LuRefreshCw};
use js_sys::{Array, Function, Reflect};
use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use leptos_icons::Icon;
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::spawn_local;

use crate::bridge;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserNavigation {
    label: String,
    url: String,
}

fn error_text(value: JsValue) -> String {
    value
        .as_string()
        .or_else(|| {
            Reflect::get(&value, &JsValue::from_str("message"))
                .ok()
                .and_then(|message| message.as_string())
        })
        .unwrap_or_else(|| "The internal browser request failed".to_owned())
}

fn normalize_address(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Enter a URL".into());
    }
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return Ok(raw.to_owned());
    }
    if raw.contains("://") {
        return Err("Only HTTP and HTTPS URLs are supported".into());
    }
    if raw.starts_with("localhost") || raw.starts_with("127.") || raw.starts_with("[::1]") {
        Ok(format!("http://{raw}"))
    } else {
        Ok(format!("https://{raw}"))
    }
}

async fn open_browser(label: &str, url: &str, bounds: BrowserBounds) -> Result<String, String> {
    bridge::invoke(
        "internal_browser_open",
        &serde_json::json!({ "label": label, "url": url, "bounds": bounds }),
    )
    .await
}

async fn navigate_browser(label: &str, url: &str) -> Result<String, String> {
    bridge::invoke(
        "internal_browser_navigate",
        &serde_json::json!({ "label": label, "url": url }),
    )
    .await
}

async fn browser_action(command: &str, label: &str) -> Result<(), String> {
    bridge::invoke(command, &serde_json::json!({ "label": label })).await
}

async fn set_browser_bounds(label: &str, bounds: BrowserBounds) -> Result<(), String> {
    bridge::invoke(
        "internal_browser_set_bounds",
        &serde_json::json!({ "label": label, "bounds": bounds }),
    )
    .await
}

struct BoundsObserver {
    observer: Option<JsValue>,
    window: web_sys::Window,
    _resize_callback: Closure<dyn FnMut()>,
    window_callback: Closure<dyn FnMut(web_sys::Event)>,
}

thread_local! {
    static BOUNDS_OBSERVERS: RefCell<HashMap<String, BoundsObserver>> =
        RefCell::new(HashMap::new());
    static NAVIGATION_LISTENERS: RefCell<HashMap<String, bridge::EventListener>> =
        RefCell::new(HashMap::new());
    static NEXT_BROWSER_INSTANCE: Cell<u32> = const { Cell::new(1) };
}

fn instance_label(base: &str) -> String {
    let instance = NEXT_BROWSER_INSTANCE.with(|next| {
        let instance = next.get();
        next.set(instance.wrapping_add(1).max(1));
        instance
    });
    let suffix = format!("-{instance:08x}");
    let base = base
        .chars()
        .take(MAX_BROWSER_LABEL_BYTES.saturating_sub(suffix.len()))
        .collect::<String>();
    format!("{base}{suffix}")
}

const MAX_BROWSER_LABEL_BYTES: usize = 96;

impl BoundsObserver {
    fn new(element: &web_sys::HtmlElement, measure: Rc<dyn Fn()>) -> Result<Self, String> {
        let window =
            web_sys::window().ok_or_else(|| "The application window is unavailable".to_string())?;
        let resize_measure = measure.clone();
        let resize_callback = Closure::wrap(Box::new(move || resize_measure()) as Box<dyn FnMut()>);
        let constructor = Reflect::get(&js_sys::global(), &JsValue::from_str("ResizeObserver"))
            .ok()
            .and_then(|value| value.dyn_into::<Function>().ok());
        let observer = if let Some(constructor) = constructor {
            let observer = Reflect::construct(&constructor, &Array::of1(resize_callback.as_ref()))
                .map_err(error_text)?;
            let observe = Reflect::get(&observer, &JsValue::from_str("observe"))
                .map_err(error_text)?
                .dyn_into::<Function>()
                .map_err(error_text)?;
            observe
                .call1(&observer, element.as_ref())
                .map_err(error_text)?;
            Some(observer)
        } else {
            None
        };

        let window_measure = measure.clone();
        let window_callback = Closure::wrap(Box::new(move |_event: web_sys::Event| {
            window_measure();
        }) as Box<dyn FnMut(web_sys::Event)>);
        window
            .add_event_listener_with_callback("resize", window_callback.as_ref().unchecked_ref())
            .map_err(error_text)?;
        window
            .add_event_listener_with_callback_and_bool(
                "scroll",
                window_callback.as_ref().unchecked_ref(),
                true,
            )
            .map_err(error_text)?;
        measure();

        Ok(Self {
            observer,
            window,
            _resize_callback: resize_callback,
            window_callback,
        })
    }
}

impl Drop for BoundsObserver {
    fn drop(&mut self) {
        if let Some(observer) = &self.observer
            && let Ok(disconnect) = Reflect::get(observer, &JsValue::from_str("disconnect"))
                .and_then(|value| value.dyn_into::<Function>())
        {
            let _ = disconnect.call0(observer);
        }
        let _ = self.window.remove_event_listener_with_callback(
            "resize",
            self.window_callback.as_ref().unchecked_ref(),
        );
        let _ = self.window.remove_event_listener_with_callback_and_bool(
            "scroll",
            self.window_callback.as_ref().unchecked_ref(),
            true,
        );
    }
}

#[derive(Clone)]
struct BrowserController {
    label: Rc<String>,
    address: RwSignal<String>,
    current: RwSignal<String>,
    open_error: RwSignal<Option<String>>,
    bounds: RwSignal<Option<BrowserBounds>>,
    opened: RwSignal<bool>,
    opening: RwSignal<bool>,
    disposed: RwSignal<bool>,
    on_error: Callback<String>,
}

impl BrowserController {
    fn report(&self, cause: String) {
        self.on_error.run(cause);
    }

    fn report_open_error(&self, cause: String) {
        self.open_error.set(Some(cause.clone()));
        self.report(cause);
    }

    fn open_at(&self, bounds: BrowserBounds) {
        if self.disposed.get_untracked() {
            return;
        }
        let raw = self.current.get_untracked();
        if raw.is_empty() || self.opened.get_untracked() || self.opening.get_untracked() {
            return;
        }
        let url = match normalize_address(&raw) {
            Ok(url) => url,
            Err(cause) => {
                self.report_open_error(cause);
                return;
            }
        };
        self.opening.set(true);
        let controller = self.clone();
        spawn_local(async move {
            match open_browser(&controller.label, &url, bounds).await {
                Ok(canonical) => {
                    if controller.disposed.get_untracked() {
                        let _ = browser_action("internal_browser_close", &controller.label).await;
                        return;
                    }
                    controller.opened.set(true);
                    controller.open_error.set(None);
                    controller.address.set(canonical.clone());
                    controller.opening.set(false);
                    let latest = controller.current.get_untracked();
                    if let Ok(latest) = normalize_address(&latest)
                        && latest != canonical
                    {
                        match navigate_browser(&controller.label, &latest).await {
                            Ok(canonical) => {
                                controller.current.set(canonical.clone());
                                controller.address.set(canonical);
                            }
                            Err(cause) => controller.report(cause),
                        }
                    } else {
                        controller.current.set(canonical);
                    }
                }
                Err(cause) => {
                    controller.opening.set(false);
                    controller.report_open_error(cause);
                }
            }
        });
    }

    fn sync_bounds(&self, bounds: BrowserBounds) {
        if self.disposed.get_untracked() {
            return;
        }
        if self.bounds.get_untracked() == Some(bounds) {
            return;
        }
        self.bounds.set(Some(bounds));
        if self.opened.get_untracked() {
            let label = self.label.clone();
            let on_error = self.on_error;
            spawn_local(async move {
                if let Err(cause) = set_browser_bounds(&label, bounds).await {
                    on_error.run(cause);
                }
            });
        } else {
            self.open_at(bounds);
        }
    }

    fn navigate(&self, raw: &str) {
        if self.disposed.get_untracked() {
            return;
        }
        let url = match normalize_address(raw) {
            Ok(url) => url,
            Err(cause) => {
                self.report(cause);
                return;
            }
        };
        self.current.set(url.clone());
        self.address.set(url.clone());
        if self.opened.get_untracked() {
            let controller = self.clone();
            spawn_local(async move {
                match navigate_browser(&controller.label, &url).await {
                    Ok(canonical) => {
                        controller.current.set(canonical.clone());
                        controller.address.set(canonical);
                    }
                    Err(cause) => controller.report(cause),
                }
            });
        } else if let Some(bounds) = self.bounds.get_untracked() {
            self.open_at(bounds);
        }
    }

    fn action(&self, command: &'static str) {
        if self.disposed.get_untracked() || !self.opened.get_untracked() {
            return;
        }
        let label = self.label.clone();
        let on_error = self.on_error;
        spawn_local(async move {
            if let Err(cause) = browser_action(command, &label).await {
                on_error.run(cause);
            }
        });
    }
}

#[component]
pub fn InternalBrowser(
    label: String,
    #[prop(default = String::new())] initial_url: String,
    #[prop(default = true)] show_toolbar: bool,
    on_error: Callback<String>,
) -> impl IntoView {
    let mount = NodeRef::<leptos::html::Div>::new();
    let address = RwSignal::new(initial_url.clone());
    let current = RwSignal::new(initial_url);
    let open_error = RwSignal::new(None::<String>);
    let bounds = RwSignal::new(None::<BrowserBounds>);
    let opened = RwSignal::new(false);
    let opening = RwSignal::new(false);
    let disposed = RwSignal::new(false);
    let controller = BrowserController {
        // A child webview can finish closing after this Leptos component has
        // already remounted. A per-mount label keeps that stale close from
        // targeting the new browser instance.
        label: Rc::new(instance_label(&label)),
        address,
        current,
        open_error,
        bounds,
        opened,
        opening,
        disposed,
        on_error,
    };
    let navigation_key = controller.label.as_str().to_owned();
    let effect_navigation_key = navigation_key.clone();
    let listener_controller = controller.clone();
    Effect::new(move |_| {
        if NAVIGATION_LISTENERS.with(|items| items.borrow().contains_key(&effect_navigation_key)) {
            return;
        }
        let listener_key = effect_navigation_key.clone();
        let controller = listener_controller.clone();
        let lifecycle_controller = listener_controller.clone();
        spawn_local(async move {
            match bridge::listen::<BrowserNavigation, _>(
                "onyx://internal-browser-navigation",
                move |navigation| {
                    if navigation.label == *controller.label {
                        controller.current.set(navigation.url.clone());
                        controller.address.set(navigation.url);
                    }
                },
            )
            .await
            {
                Ok(listener) if !lifecycle_controller.disposed.get_untracked() => {
                    NAVIGATION_LISTENERS.with(|items| {
                        items.borrow_mut().insert(listener_key, listener);
                    });
                }
                Ok(_) => {}
                Err(cause) if !lifecycle_controller.disposed.get_untracked() => {
                    lifecycle_controller.report(cause);
                }
                Err(_) => {}
            }
        });
    });
    let observer_key = controller.label.as_str().to_owned();
    let effect_observer_key = observer_key.clone();
    let effect_controller = controller.clone();
    Effect::new(move |_| {
        if BOUNDS_OBSERVERS.with(|items| items.borrow().contains_key(&effect_observer_key)) {
            return;
        }
        let Some(element) = mount.get() else {
            return;
        };
        let measured_element = element.clone();
        let measured_controller = effect_controller.clone();
        let measure = Rc::new(move || {
            let rect = measured_element.get_bounding_client_rect();
            if rect.width() < 1.0 || rect.height() < 1.0 {
                return;
            }
            measured_controller.sync_bounds(BrowserBounds {
                x: rect.x().max(0.0),
                y: rect.y().max(0.0),
                width: rect.width(),
                height: rect.height(),
            });
        });
        match BoundsObserver::new(&element, measure) {
            Ok(handle) => BOUNDS_OBSERVERS.with(|items| {
                items
                    .borrow_mut()
                    .insert(effect_observer_key.clone(), handle);
            }),
            Err(cause) => effect_controller.report(cause),
        }
    });

    let cleanup_label = controller.label.as_str().to_owned();
    on_cleanup(move || {
        disposed.set(true);
        BOUNDS_OBSERVERS.with(|items| {
            items.borrow_mut().remove(&observer_key);
        });
        NAVIGATION_LISTENERS.with(|items| {
            items.borrow_mut().remove(&navigation_key);
        });
        spawn_local(async move {
            let _ = browser_action("internal_browser_close", &cleanup_label).await;
        });
    });

    let submit_controller = controller.clone();
    let back_controller = controller.clone();
    let forward_controller = controller.clone();
    let reload_controller = controller.clone();
    let retry_requested = RwSignal::new(0_u64);
    let retry_controller = controller.clone();
    Effect::new(move |_| {
        if retry_requested.get() == 0 {
            return;
        }
        if let Some(bounds) = retry_controller.bounds.get_untracked() {
            retry_controller.open_at(bounds);
        }
    });

    view! {
        <div
            class="zai-browser-surface onyx-internal-browser"
            style="position:relative;display:flex;min-width:0;min-height:0;height:100%;flex-direction:column"
        >
            <form
                class="zai-browser-toolbar"
                style:display=if show_toolbar { "" } else { "none" }
                on:submit=move |event: SubmitEvent| {
                    event.prevent_default();
                    submit_controller.navigate(&address.get_untracked());
                }
            >
                <button
                    type="button"
                    aria-label="Back"
                    title="Back"
                    on:click=move |_| back_controller.action("internal_browser_back")
                >
                    <Icon icon=LuArrowLeft width="15px" height="15px" />
                </button>
                <button
                    type="button"
                    aria-label="Forward"
                    title="Forward"
                    on:click=move |_| forward_controller.action("internal_browser_forward")
                >
                    <Icon icon=LuArrowRight width="15px" height="15px" />
                </button>
                <button
                    type="button"
                    aria-label="Reload"
                    title="Reload"
                    on:click=move |_| reload_controller.action("internal_browser_reload")
                >
                    <Icon icon=LuRefreshCw width="15px" height="15px" />
                </button>
                <label>
                    <Icon icon=LuGlobe width="15px" height="15px" />
                    <span class="sr-only">"URL"</span>
                    <input
                        prop:value=move || address.get()
                        spellcheck="false"
                        autocomplete="off"
                        on:input=move |event| address.set(event_target_value(&event))
                    />
                </label>
            </form>
            <div
                node_ref=mount
                class="onyx-internal-browser__viewport"
                data-browser-label=controller.label.as_str().to_owned()
                style="position:relative;min-width:0;min-height:0;flex:1;overflow:hidden"
                aria-label="Internal web browser"
            >
                <Show
                    when=move || open_error.get().is_some()
                    fallback=move || view! {
                        <Show when=move || current.get().is_empty()>
                            <div class="zai-browser-empty">
                                <Icon icon=LuGlobe width="23px" height="23px" />
                                <strong>"Open a page"</strong>
                                <span>"Enter an HTTP or HTTPS URL above."</span>
                            </div>
                        </Show>
                    }
                >
                    <div class="zai-browser-empty zai-browser-empty--error" role="alert">
                        <Icon icon=LuGlobe width="23px" height="23px" />
                        <strong>"The internal browser could not open this page"</strong>
                        <span>{move || open_error.get().unwrap_or_default()}</span>
                        <button
                            type="button"
                            disabled=move || opening.get()
                            on:click=move |_| retry_requested.update(|value| *value += 1)
                        >
                            {move || if opening.get() { "Retrying…" } else { "Retry" }}
                        </button>
                    </div>
                </Show>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_address;

    #[test]
    fn addresses_default_to_https_and_keep_local_development_http() {
        assert_eq!(
            normalize_address("chatgpt.com").unwrap(),
            "https://chatgpt.com"
        );
        assert_eq!(
            normalize_address("localhost:3000").unwrap(),
            "http://localhost:3000"
        );
        assert!(normalize_address("file:///tmp/example").is_err());
    }
}
