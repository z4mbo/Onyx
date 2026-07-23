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

    console_error_panic_hook::set_once();
    theme::apply_document_theme();
    mount_to_body(|| view! { <app::App /> });
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
