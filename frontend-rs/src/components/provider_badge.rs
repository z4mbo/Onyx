use leptos::prelude::*;

use crate::{catalog::brand_meta, model::ProviderBrand};

const ICONS_BASE: &str = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons@46b860c70e866212311aef2f98da3775c17f5068/svg";

fn icon_slugs(brand: ProviderBrand) -> (&'static str, &'static str) {
    match brand {
        ProviderBrand::Openai => ("openai", "openai-light"),
        ProviderBrand::Anthropic => ("anthropic", "anthropic-dark"),
        ProviderBrand::Google => ("google-gemini", "google-gemini"),
        ProviderBrand::Xai => ("grok", "grok-dark"),
        ProviderBrand::Moonshot => ("kimi-ai", "kimi-ai"),
        ProviderBrand::Openrouter => ("open-router", "open-router-dark"),
    }
}

#[component]
pub fn ProviderBadge(
    brand: Signal<ProviderBrand>,
    #[prop(default = false)] small: bool,
) -> impl IntoView {
    view! {
        <span
            class="provider-badge"
            class:provider-badge-sm=small
            data-brand=move || brand.get().as_str()
            style=move || format!("--provider-color:{}", brand_meta(brand.get()).color)
            aria-label=move || brand.get().display_name()
        >
            <img
                class="provider-badge__icon provider-badge__icon--light"
                src=move || format!("{ICONS_BASE}/{}.svg", icon_slugs(brand.get()).0)
                alt=""
                draggable="false"
            />
            <img
                class="provider-badge__icon provider-badge__icon--dark"
                src=move || format!("{ICONS_BASE}/{}.svg", icon_slugs(brand.get()).1)
                alt=""
                draggable="false"
            />
        </span>
    }
}
