use web_sys::UrlSearchParams;

const COLOR_SCHEME_KEY: &str = "onyx.color-scheme";
const LEGACY_COLOR_SCHEME_KEY: &str = "zai.color-scheme";

pub fn apply_document_theme() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let preference = window
        .local_storage()
        .ok()
        .flatten()
        .and_then(|storage| {
            storage
                .get_item(COLOR_SCHEME_KEY)
                .ok()
                .flatten()
                .or_else(|| storage.get_item(LEGACY_COLOR_SCHEME_KEY).ok().flatten())
        })
        .filter(|value| value == "light" || value == "dark")
        .unwrap_or_else(|| "system".to_owned());
    let color_scheme = if preference == "system" {
        let dark = window
            .match_media("(prefers-color-scheme: dark)")
            .ok()
            .flatten()
            .is_some_and(|query| query.matches());
        if dark { "dark" } else { "light" }
    } else {
        preference.as_str()
    };

    if let Some(root) = document.document_element() {
        let _ = root.set_attribute("data-color-scheme", color_scheme);
        let _ = root.set_attribute("data-theme", "oc-2");
    }
    if let Some(body) = document.body() {
        let _ = body.set_attribute("data-new-layout", "");
    }
}

pub fn window_name() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    UrlSearchParams::new_with_str(&search).ok()?.get("window")
}

pub fn mark_window(name: &str) {
    let root = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element());
    if let Some(root) = root {
        let _ = root.set_attribute("data-window", name);
    }
}
