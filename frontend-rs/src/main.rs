mod model;

#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod bridge;
mod catalog;
#[cfg(target_arch = "wasm32")]
mod components;
mod highlight;
mod markdown;
#[cfg(target_arch = "wasm32")]
mod storage;
#[cfg(target_arch = "wasm32")]
mod theme;

#[cfg(target_arch = "wasm32")]
fn main() {
    use leptos::prelude::*;
    use wasm_bindgen::JsCast;

    std::panic::set_hook(Box::new(|panic| {
        console_error_panic_hook::hook(panic);
        let detail = panic.to_string();
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Some(root) = document.get_element_by_id("root") else {
            return;
        };
        let Ok(status) = document.create_element("main") else {
            return;
        };
        status.set_class_name("onyx-boot-status onyx-boot-status--error");
        let _ = status.set_attribute("role", "alert");
        let Ok(panel) = document.create_element("div") else {
            return;
        };
        let Ok(heading) = document.create_element("strong") else {
            return;
        };
        heading.set_text_content(Some("Onyx needs to reload"));
        let Ok(message) = document.create_element("p") else {
            return;
        };
        message.set_text_content(Some("The Rust interface stopped unexpectedly."));
        let Ok(diagnostic) = document.create_element("code") else {
            return;
        };
        diagnostic.set_text_content(Some(&detail));
        let _ = panel.append_with_node_3(&heading, &message, &diagnostic);
        let _ = status.append_child(&panel);
        root.replace_children_with_node_1(&status);
    }));
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
