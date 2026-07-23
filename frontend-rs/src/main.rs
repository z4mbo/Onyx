mod model;

#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod bridge;
#[cfg(target_arch = "wasm32")]
mod components;
#[cfg(target_arch = "wasm32")]
mod theme;

#[cfg(target_arch = "wasm32")]
fn main() {
    use leptos::prelude::*;
    use wasm_bindgen::JsCast;

    console_error_panic_hook::set_once();
    theme::apply_document_theme();
    let root = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("root"))
        .expect("Onyx requires a #root mount element")
        .unchecked_into::<web_sys::HtmlElement>();
    mount_to(root, || view! { <app::App /> }).forget();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
