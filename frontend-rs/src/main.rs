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
    root.replace_children_with_node_0();
    let mount_root = root.clone();
    mount_to(mount_root, || view! { <app::App /> }).forget();
    let _ = root.set_attribute("data-onyx-mounted", "true");
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
