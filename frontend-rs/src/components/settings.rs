use gloo_timers::future::TimeoutFuture;
use icondata::{
    LuCheck, LuChevronDown, LuCpu, LuExternalLink, LuKeyRound, LuKeyboard, LuLoaderCircle, LuMic,
    LuRefreshCw, LuSlidersHorizontal, LuSparkles, LuX,
};
use leptos::prelude::*;
use leptos_icons::Icon;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;

use crate::{
    bridge,
    catalog::ProviderCatalogs,
    model::{
        ConnectionStatus, NativeVoicePermissions, OpenRouterModel, OpenRouterVoiceCatalog,
        OpenRouterVoiceModel, OverlayPosition, ProviderBrand, ProviderId, ProviderStatus,
        TerminalSession, UpdateProgress, VoiceSettings, normalized_speech_voice,
        resolved_openrouter_speech_selection, supported_speech_voices,
    },
    storage, theme,
};

use super::{ProviderBadge, TerminalViewport};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorScheme {
    #[default]
    System,
    Light,
    Dark,
}

impl ColorScheme {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }

    pub fn stored() -> Self {
        Self::from_str(&storage::get(storage::COLOR_SCHEME_KEY).unwrap_or_default())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SettingsPage {
    #[default]
    General,
    Shortcuts,
    Voice,
    Providers,
    Models,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopPreferences {
    wsl_mode: String,
    wsl_distribution: String,
}

fn page_label(page: SettingsPage) -> &'static str {
    match page {
        SettingsPage::General => "General",
        SettingsPage::Shortcuts => "Shortcuts",
        SettingsPage::Voice => "Voice",
        SettingsPage::Providers => "Runtimes",
        SettingsPage::Models => "Models",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VoiceChoice {
    value: String,
    label: String,
    disabled: bool,
}

fn add_choice(choices: &mut Vec<VoiceChoice>, value: String, label: String) {
    add_choice_with_state(choices, value, label, false);
}

fn add_choice_with_state(
    choices: &mut Vec<VoiceChoice>,
    value: String,
    label: String,
    disabled: bool,
) {
    if !choices.iter().any(|choice| choice.value == value) {
        choices.push(VoiceChoice {
            value,
            label,
            disabled,
        });
    }
}

fn encoded_choice(provider: &str, model: &str) -> String {
    format!("{provider}|{model}")
}

fn dictation_choices(
    models: &[OpenRouterVoiceModel],
    current_provider: &str,
    current_model: &str,
) -> Vec<VoiceChoice> {
    let mut choices = Vec::new();
    for (id, name) in [
        ("gpt-4o-mini-transcribe", "OpenAI · GPT-4o mini Transcribe"),
        ("gpt-4o-transcribe", "OpenAI · GPT-4o Transcribe"),
        ("whisper-1", "OpenAI · Whisper"),
    ] {
        add_choice(&mut choices, encoded_choice("openai", id), name.to_owned());
    }
    for model in models {
        add_choice(
            &mut choices,
            encoded_choice("openrouter", &model.id),
            format!("OpenRouter · {}", model.name),
        );
    }
    let current_value = encoded_choice(current_provider, current_model);
    if !current_model.trim().is_empty()
        && !choices.iter().any(|choice| choice.value == current_value)
    {
        add_choice_with_state(
            &mut choices,
            current_value,
            format!("Saved · {current_model} · unavailable"),
            true,
        );
    }
    choices
}

fn agent_choices(
    catalogs: &ProviderCatalogs,
    router_models: &[OpenRouterModel],
    providers: &[ProviderStatus],
    current_provider: ProviderId,
    current_model: &str,
) -> Vec<VoiceChoice> {
    let mut choices = Vec::new();
    // OpenCode is intentionally absent: these choices feed the voice overlay's
    // chat_send/chat_once path, which needs a one-shot CLI transport that
    // OpenCode does not provide.
    for provider in [
        ProviderId::Claude,
        ProviderId::Codex,
        ProviderId::Gemini,
        ProviderId::Kimi,
    ]
    .into_iter()
    .filter(|provider| {
        providers
            .iter()
            .any(|status| status.id == *provider && status.available)
    }) {
        for model in catalogs.get(&provider).into_iter().flatten() {
            add_choice(
                &mut choices,
                encoded_choice(provider.as_str(), &model.id),
                format!("{} · {}", provider.display_name(), model.name),
            );
        }
    }
    let openrouter_available = providers
        .iter()
        .any(|status| status.id == ProviderId::Openrouter && status.available);
    if openrouter_available {
        for model in router_models.iter().filter(|model| {
            (model.input_modalities.is_empty()
                || model
                    .input_modalities
                    .iter()
                    .any(|modality| modality.eq_ignore_ascii_case("text")))
                && (model.output_modalities.is_empty()
                    || model
                        .output_modalities
                        .iter()
                        .any(|modality| modality.eq_ignore_ascii_case("text")))
        }) {
            add_choice(
                &mut choices,
                encoded_choice("openrouter", &model.id),
                format!("OpenRouter · {}", model.name),
            );
        }
    }
    if !current_model.trim().is_empty() {
        let value = encoded_choice(current_provider.as_str(), current_model);
        if !choices.iter().any(|choice| choice.value == value) {
            choices.push(VoiceChoice {
                value,
                label: format!(
                    "Saved · {} · {current_model} (unavailable)",
                    current_provider.display_name()
                ),
                disabled: true,
            });
        }
    }
    choices
}

fn speech_choices(
    models: &[OpenRouterVoiceModel],
    current_provider: &str,
    current_model: &str,
) -> Vec<VoiceChoice> {
    let mut choices = Vec::new();
    for (id, name) in [
        ("gpt-4o-mini-tts", "OpenAI · GPT-4o mini TTS"),
        ("tts-1", "OpenAI · TTS 1"),
        ("tts-1-hd", "OpenAI · TTS 1 HD"),
    ] {
        add_choice(&mut choices, encoded_choice("openai", id), name.to_owned());
    }
    for model in models {
        let value = encoded_choice("openrouter", &model.id);
        let is_current =
            current_provider == "openrouter" && current_model.trim() == model.id.as_str();
        let voices_unavailable = model.supported_voices.is_empty();
        add_choice_with_state(
            &mut choices,
            value,
            if voices_unavailable {
                format!("OpenRouter · {} · voice list unavailable", model.name)
            } else {
                format!("OpenRouter · {}", model.name)
            },
            voices_unavailable && !is_current,
        );
    }
    if !current_model.trim().is_empty() {
        add_choice(
            &mut choices,
            encoded_choice(current_provider, current_model),
            format!("Saved · {current_model}"),
        );
    }
    choices
}

fn speech_voice_choices(
    catalog: &OpenRouterVoiceCatalog,
    provider: &str,
    model: &str,
    current_voice: &str,
) -> Vec<VoiceChoice> {
    let mut choices = Vec::new();
    if let Some(voices) = supported_speech_voices(catalog, provider, model) {
        for voice in voices {
            add_choice(&mut choices, voice.clone(), voice);
        }
    } else if !current_voice.trim().is_empty() {
        add_choice(
            &mut choices,
            current_voice.trim().to_owned(),
            format!("Saved · {}", current_voice.trim()),
        );
    }
    choices
}

fn provider_id(value: &str) -> Option<ProviderId> {
    ProviderId::ALL
        .into_iter()
        .find(|provider| provider.as_str() == value)
}

#[component]
fn NavButton(
    target: SettingsPage,
    page: RwSignal<SettingsPage>,
    icon: icondata::Icon,
) -> impl IntoView {
    view! {
        <button
            class:active=move || page.get() == target
            on:click=move |_| page.set(target)
        >
            <Icon icon=icon width="17px" height="17px" />
            {page_label(target)}
        </button>
    }
}

#[component]
pub fn SettingsDialog(
    open: Signal<bool>,
    providers: RwSignal<Vec<ProviderStatus>>,
    catalogs: Signal<ProviderCatalogs>,
    openrouter: RwSignal<ConnectionStatus>,
    openai: RwSignal<ConnectionStatus>,
    openrouter_models: RwSignal<Vec<OpenRouterModel>>,
    color_scheme: RwSignal<ColorScheme>,
    on_close: Callback<()>,
) -> impl IntoView {
    let page = RwSignal::new(SettingsPage::General);
    let router_key = RwSignal::new(String::new());
    let openai_key = RwSignal::new(String::new());
    let saving = RwSignal::new(false);
    let message = RwSignal::new(None::<String>);
    let voice = RwSignal::new(None::<VoiceSettings>);
    let openrouter_voice_catalog = RwSignal::new(OpenRouterVoiceCatalog::default());
    let voice_catalog_loaded = RwSignal::new(false);
    let voice_catalog_loading = RwSignal::new(false);
    let microphone_status = RwSignal::new("idle".to_owned());
    let microphone_message = RwSignal::new(String::new());
    let native_permissions = RwSignal::new(None::<NativeVoicePermissions>);
    let native_message = RwSignal::new(String::new());
    let provider_terminal = RwSignal::new(None::<TerminalSession>);
    let provider_terminal_title = RwSignal::new(String::new());
    let platform = RwSignal::new("unknown".to_owned());
    let wsl_distributions = RwSignal::new(Vec::<String>::new());
    let desktop_preferences = RwSignal::new(storage::read_json(
        storage::DESKTOP_PREFERENCES_KEY,
        DesktopPreferences {
            wsl_mode: "off".to_owned(),
            wsl_distribution: String::new(),
        },
    ));
    let update_state = RwSignal::new("idle".to_owned());
    let update_message = RwSignal::new(String::new());
    let update_progress = RwSignal::new(None::<UpdateProgress>);
    let update_version = RwSignal::new(None::<String>);
    let dictation_models = Signal::derive(move || {
        let current = voice.get().unwrap_or_default();
        dictation_choices(
            &openrouter_voice_catalog.get().transcription,
            &current.transcription_provider,
            &current.transcription_model,
        )
    });
    let agent_models = Signal::derive(move || {
        let current = voice.get().unwrap_or_default();
        agent_choices(
            &catalogs.get(),
            &openrouter_models.get(),
            &providers.get(),
            current.agent_provider,
            &current.agent_model,
        )
    });
    let speech_models = Signal::derive(move || {
        let current = voice.get().unwrap_or_default();
        speech_choices(
            &openrouter_voice_catalog.get().speech,
            &current.voice_provider,
            &current.voice_model,
        )
    });
    let speech_voices = Signal::derive(move || {
        let current = voice.get().unwrap_or_default();
        speech_voice_choices(
            &openrouter_voice_catalog.get(),
            &current.voice_provider,
            &current.voice_model,
            &current.voice_id,
        )
    });

    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        if voice.get_untracked().is_none() {
            spawn_local(async move {
                match bridge::get_voice_settings().await {
                    Ok(settings) => voice.set(Some(settings)),
                    Err(cause) => message.set(Some(cause)),
                }
            });
        }
        spawn_local(async move {
            native_permissions.set(bridge::native_voice_permissions().await.ok());
        });
    });
    Effect::new(move |_| {
        if !open.get()
            || page.get() != SettingsPage::Voice
            || !openrouter.get().connected
            || voice_catalog_loaded.get()
            || voice_catalog_loading.get()
        {
            return;
        }
        voice_catalog_loading.set(true);
        spawn_local(async move {
            match bridge::openrouter_voice_models().await {
                Ok(catalog) => openrouter_voice_catalog.set(catalog),
                Err(cause) => message.set(Some(format!(
                    "Could not load OpenRouter voice models: {cause}"
                ))),
            }
            voice_catalog_loaded.set(true);
            voice_catalog_loading.set(false);
        });
    });
    Effect::new(move |_| {
        let Some(current) = voice.get() else {
            return;
        };
        let catalog = openrouter_voice_catalog.get();
        if current.voice_provider.eq_ignore_ascii_case("openrouter") {
            if !voice_catalog_loaded.get() {
                return;
            }
            let Some((model, selected_voice)) = resolved_openrouter_speech_selection(
                &catalog,
                &current.voice_model,
                &current.voice_id,
            ) else {
                return;
            };
            if model != current.voice_model || selected_voice != current.voice_id {
                voice.update(|settings| {
                    if let Some(settings) = settings {
                        settings.voice_model = model;
                        settings.voice_id = selected_voice;
                    }
                });
            }
            return;
        }
        let Some(next) = normalized_speech_voice(
            &catalog,
            &current.voice_provider,
            &current.voice_model,
            &current.voice_id,
        ) else {
            return;
        };
        voice.update(|settings| {
            if let Some(settings) = settings {
                settings.voice_id = next;
            }
        });
    });
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(value) = bridge::platform().await {
                platform.set(value.clone());
                if value == "windows" {
                    wsl_distributions
                        .set(bridge::list_wsl_distributions().await.unwrap_or_default());
                }
            }
        });
    });

    let refresh = Callback::new(move |_: ()| {
        saving.set(true);
        spawn_local(async move {
            match bridge::list_providers().await {
                Ok(value) => providers.set(value),
                Err(cause) => message.set(Some(cause)),
            }
            saving.set(false);
        });
    });
    let connect_router = Callback::new(move |_: ()| {
        let key = router_key.get().trim().to_owned();
        if key.is_empty() {
            return;
        }
        saving.set(true);
        message.set(None);
        spawn_local(async move {
            match bridge::save_openrouter_key(&key).await {
                Ok(status) => {
                    openrouter.set(status);
                    openrouter_voice_catalog.set(OpenRouterVoiceCatalog::default());
                    voice_catalog_loaded.set(false);
                    voice_catalog_loading.set(false);
                    if let Ok(value) = bridge::list_providers().await {
                        providers.set(value);
                    }
                    match bridge::openrouter_models().await {
                        Ok(models) => {
                            let count = models.len();
                            openrouter_models.set(models);
                            router_key.set(String::new());
                            message.set(Some(format!("Connected. {count} models available.")));
                        }
                        Err(cause) => message.set(Some(cause)),
                    }
                }
                Err(cause) => message.set(Some(cause)),
            }
            saving.set(false);
        });
    });
    let connect_openai = Callback::new(move |_: ()| {
        let key = openai_key.get().trim().to_owned();
        if key.is_empty() {
            return;
        }
        saving.set(true);
        spawn_local(async move {
            match bridge::save_openai_key(&key).await {
                Ok(status) => {
                    openai.set(status);
                    openai_key.set(String::new());
                    message.set(Some(
                        "OpenAI API connected. Native image and audio routes are ready.".to_owned(),
                    ));
                }
                Err(cause) => message.set(Some(cause)),
            }
            saving.set(false);
        });
    });
    let save_voice = Callback::new(move |_: ()| {
        let Some(settings) = voice.get() else {
            return;
        };
        saving.set(true);
        spawn_local(async move {
            match bridge::apply_voice_settings(settings).await {
                Ok(settings) => {
                    voice.set(Some(settings));
                    message.set(Some("Voice settings saved.".to_owned()));
                }
                Err(cause) => message.set(Some(cause)),
            }
            saving.set(false);
        });
    });
    let check_updates = Callback::new(move |_: ()| {
        update_state.set("checking".to_owned());
        update_message.set(String::new());
        spawn_local(async move {
            match bridge::check_update().await {
                Ok(Some(update)) => {
                    update_state.set("available".to_owned());
                    update_message.set(format!("Onyx {} is ready to install.", update.version));
                    update_version.set(Some(update.version));
                }
                Ok(None) => {
                    update_state.set("current".to_owned());
                    update_message.set("You are on the latest version.".to_owned());
                    update_version.set(None);
                }
                Err(cause) => {
                    update_state.set("failed".to_owned());
                    update_message.set(cause);
                }
            }
        });
    });
    let install_update = Callback::new(move |_: ()| {
        let Some(version) = update_version.get_untracked() else {
            return;
        };
        update_state.set("installing".to_owned());
        update_message.set("Downloading update…".to_owned());
        spawn_local(async move {
            let downloaded = bridge::download_update(&version, move |downloaded, total| {
                update_progress.set(Some(UpdateProgress {
                    downloaded,
                    total,
                    finished: false,
                }));
            })
            .await;
            let result = match downloaded {
                Ok(()) => {
                    update_message.set("Restarting to finish the update…".to_owned());
                    bridge::install_update(&version).await
                }
                Err(cause) => Err(cause),
            };
            match result {
                Ok(()) => {
                    // The backend restarts the app; if that never lands,
                    // release the panel instead of spinning forever.
                    TimeoutFuture::new(15_000).await;
                    if update_state.get_untracked() == "installing" {
                        update_state.set("failed".to_owned());
                        update_message.set(
                            "The update installed but Onyx could not restart itself. Quit and reopen Onyx to finish."
                                .to_owned(),
                        );
                        update_progress.set(None);
                    }
                }
                Err(cause) => {
                    update_state.set("failed".to_owned());
                    update_message.set(cause);
                    update_progress.set(None);
                }
            }
        });
    });
    let open_provider_terminal = Callback::new(move |(provider, action): (ProviderId, String)| {
        saving.set(true);
        message.set(None);
        spawn_local(async move {
            match bridge::provider_terminal_open(provider, &action, None, None).await {
                Ok(terminal) => {
                    provider_terminal_title.set(if action == "update" {
                        format!("Update {}", provider.display_name())
                    } else {
                        format!("{} CLI", provider.display_name())
                    });
                    provider_terminal.set(Some(terminal));
                }
                Err(cause) => message.set(Some(cause)),
            }
            saving.set(false);
        });
    });
    let close_provider_terminal = Callback::new(move |_: ()| {
        if let Some(terminal) = provider_terminal.get_untracked() {
            spawn_local(async move {
                let _ = bridge::terminal_close(&terminal.id).await;
            });
        }
        provider_terminal.set(None);
    });

    let provider_row = move |provider: ProviderStatus| {
        let brand = ProviderBrand::for_provider(provider.id);
        let provider_id = provider.id;
        view! {
            <div class="zai-settings-provider-row">
                <ProviderBadge brand=Signal::derive(move || brand) />
                <div class="zai-settings-provider-copy">
                    <strong>{provider.name}</strong>
                    <span>{provider.version.unwrap_or(provider.transport)}</span>
                    {provider.executable_path.map(|path| view! { <code>{path}</code> })}
                </div>
                {if provider.available {
                    view! {
                        <div class="zai-settings-status-actions">
                            <span class="zai-settings-ready">
                                <Icon icon=LuCheck width="13px" height="13px" />
                                "Ready"
                            </span>
                            <button
                                class="zai-neutral-button"
                                disabled=move || saving.get()
                                on:click=move |_| {
                                    open_provider_terminal.run((provider_id, "interactive".to_owned()));
                                }
                            >
                                "Open CLI"
                            </button>
                            <button
                                class="zai-neutral-button"
                                disabled=move || saving.get()
                                on:click=move |_| {
                                    open_provider_terminal.run((provider_id, "update".to_owned()));
                                }
                            >
                                "Update"
                            </button>
                        </div>
                    }
                    .into_any()
                } else {
                    let url = provider.install_url;
                    view! {
                        <button
                            class="zai-settings-connect"
                            on:click=move |_| {
                                let url = url.clone();
                                spawn_local(async move {
                                    let _ = bridge::open_url(&url).await;
                                });
                            }
                        >
                            "Install"
                            <Icon icon=LuExternalLink width="12px" height="12px" />
                        </button>
                    }
                    .into_any()
                }}
            </div>
        }
    };

    view! {
        <Show when=move || open.get()>
            <div class="zai-modal-scrim">
                <section
                    class="zai-settings-dialog"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="zai-settings-page-title"
                    tabindex="-1"
                >
                    <aside class="zai-settings-sidebar">
                        <div>
                            <h2>"Desktop"</h2>
                            <nav>
                                <NavButton target=SettingsPage::General page=page icon=LuSlidersHorizontal />
                                <NavButton target=SettingsPage::Shortcuts page=page icon=LuKeyboard />
                                <NavButton target=SettingsPage::Voice page=page icon=LuMic />
                            </nav>
                        </div>
                        <div>
                            <h2>"Agents"</h2>
                            <nav>
                                <NavButton target=SettingsPage::Providers page=page icon=LuCpu />
                                <NavButton target=SettingsPage::Models page=page icon=LuSparkles />
                            </nav>
                        </div>
                        <div class="zai-settings-version">
                            <strong>"Onyx Desktop"</strong>
                            <span>{concat!("v", env!("CARGO_PKG_VERSION"), " · Rust")}</span>
                        </div>
                    </aside>

                    <div class="zai-settings-content">
                        <button
                            class="zai-settings-close"
                            on:click=move |_| {
                                close_provider_terminal.run(());
                                on_close.run(());
                            }
                            aria-label="Close settings"
                        >
                            <Icon icon=LuX width="16px" height="16px" />
                        </button>

                        <Show when=move || page.get() == SettingsPage::General>
                            <div class="zai-settings-page">
                                <h1 id="zai-settings-page-title">"General"</h1>
                                <section class="zai-settings-card">
                                    <div class="zai-setting-row"><div><strong>"Language"</strong><span>"Onyx currently ships with an English interface"</span></div><span class="zai-setting-value">"English"</span></div>
                                    <div class="zai-setting-row"><div><strong>"Permission approvals"</strong><span>"Supported approval requests stay visible for review"</span></div><span class="zai-setting-value">"Review"</span></div>
                                    <div class="zai-setting-row"><div><strong>"Provider sessions"</strong><span>"Onyx selects the supported session mode for each provider"</span></div><span class="zai-setting-value">"Managed"</span></div>
                                    <div class="zai-setting-row"><div><strong>"Reasoning summaries"</strong><span>"Shown in the timeline when a provider supplies them"</span></div><span class="zai-setting-value">"When available"</span></div>
                                    <div class="zai-setting-row"><div><strong>"Tool details"</strong><span>"Open timeline activities to inspect their output"</span></div><span class="zai-setting-value">"Collapsed"</span></div>
                                </section>

                                <Show when=move || platform.get() == "windows">
                                    <h1>"Windows terminal"</h1>
                                    <section class="zai-settings-card">
                                        <div class="zai-setting-row">
                                            <div><strong>"Terminal environment"</strong><span>"Use native PowerShell or launch terminal tabs through WSL"</span></div>
                                            <label class="zai-setting-select">
                                                <select
                                                    prop:value=move || desktop_preferences.get().wsl_mode
                                                    on:change=move |event| {
                                                        desktop_preferences.update(|value| value.wsl_mode = event_target_value(&event));
                                                        storage::write_json(storage::DESKTOP_PREFERENCES_KEY, &desktop_preferences.get());
                                                    }
                                                >
                                                    <option value="off">"Windows native"</option>
                                                    <option value="default">"Default WSL distribution"</option>
                                                    <option value="distribution">"Specific WSL distribution"</option>
                                                </select>
                                                <Icon icon=LuChevronDown width="13px" height="13px" />
                                            </label>
                                        </div>
                                        <Show when=move || desktop_preferences.get().wsl_mode == "distribution">
                                            <div class="zai-setting-row">
                                                <div><strong>"WSL distribution"</strong><span>"Installed distributions reported by wsl.exe"</span></div>
                                                <label class="zai-setting-select">
                                                    <select
                                                        prop:value=move || desktop_preferences.get().wsl_distribution
                                                        on:change=move |event| {
                                                            desktop_preferences.update(|value| value.wsl_distribution = event_target_value(&event));
                                                            storage::write_json(storage::DESKTOP_PREFERENCES_KEY, &desktop_preferences.get());
                                                        }
                                                    >
                                                        <option value="">"Choose distribution"</option>
                                                        <For
                                                            each=move || wsl_distributions.get()
                                                            key=|value| value.clone()
                                                            children=|value| view! { <option value=value.clone()>{value.clone()}</option> }
                                                        />
                                                    </select>
                                                    <Icon icon=LuChevronDown width="13px" height="13px" />
                                                </label>
                                            </div>
                                        </Show>
                                    </section>
                                </Show>

                                <h1>"Updates"</h1>
                                <section class="zai-settings-card">
                                    <div class="zai-setting-row">
                                        <div><strong>"App version"</strong><span>{move || if update_message.get().is_empty() { "Onyx checks GitHub releases for signed updates".to_owned() } else { update_message.get() }}</span></div>
                                        <div class="zai-setting-route__controls">
                                            <Show when=move || update_state.get() == "available">
                                                <button class="zai-neutral-button zai-update-install" on:click=move |_| install_update.run(())>"Install & restart"</button>
                                            </Show>
                                            <Show
                                                when=move || update_state.get() == "installing"
                                                fallback=move || view! {
                                                    <button
                                                        class="zai-neutral-button"
                                                        disabled=move || update_state.get() == "checking"
                                                        on:click=move |_| check_updates.run(())
                                                    >
                                                        <Icon icon=LuRefreshCw width="13px" height="13px" />
                                                        {move || if update_state.get() == "checking" { "Checking…" } else { "Check for updates" }}
                                                    </button>
                                                }
                                            >
                                                <span class="zai-setting-value">
                                                    <Icon icon=LuLoaderCircle width="13px" height="13px" />
                                                    {move || update_progress.get().and_then(|progress| progress.total.map(|total| {
                                                        format!("{}%", ((progress.downloaded as f64 / total.max(1) as f64) * 100.0).round() as u32)
                                                    })).unwrap_or_else(|| "Downloading…".to_owned())}
                                                </span>
                                            </Show>
                                        </div>
                                    </div>
                                </section>

                                <h1>"Appearance"</h1>
                                <section class="zai-settings-card">
                                    <div class="zai-setting-row">
                                        <div><strong>"Color scheme"</strong><span>"Choose whether Onyx follows the system, light, or dark theme"</span></div>
                                        <label class="zai-setting-select">
                                            <select
                                                prop:value=move || color_scheme.get().as_str()
                                                on:change=move |event| {
                                                    let next = ColorScheme::from_str(&event_target_value(&event));
                                                    color_scheme.set(next);
                                                    storage::set(storage::COLOR_SCHEME_KEY, next.as_str());
                                                    theme::apply_document_theme();
                                                    spawn_local(async move {
                                                        let _ = bridge::set_window_theme(
                                                            (next != ColorScheme::System).then_some(next.as_str()),
                                                        ).await;
                                                    });
                                                }
                                            >
                                                <option value="system">"System"</option>
                                                <option value="light">"Light"</option>
                                                <option value="dark">"Dark"</option>
                                            </select>
                                            <Icon icon=LuChevronDown width="13px" height="13px" />
                                        </label>
                                    </div>
                                    <div class="zai-setting-row"><div><strong>"Interface style"</strong><span>"Onyx's OpenCode and T3-informed desktop visual language"</span></div><span class="zai-setting-value">"Onyx"</span></div>
                                </section>
                            </div>
                        </Show>

                        <Show when=move || page.get() == SettingsPage::Shortcuts>
                            <div class="zai-settings-page">
                                <h1 id="zai-settings-page-title">"Shortcuts"</h1>
                                <section class="zai-settings-card">
                                    <For
                                        each=move || {
                                            let command = if platform.get() == "macos" { "⌘" } else { "Ctrl" };
                                            vec![
                                                ("New session", format!("{command} N")),
                                                ("Settings", format!("{command} ,")),
                                                ("Bottom terminal", format!("{command} J")),
                                                ("Right panel", format!("{command} ⇧ J")),
                                                ("Send message", "↵".to_owned()),
                                                ("New line", "⇧ ↵".to_owned()),
                                                ("Stop agent", "Esc".to_owned()),
                                                ("Hold to dictate", "Control Shift".to_owned()),
                                                ("Hold for voice agent", "Control Option".to_owned()),
                                            ]
                                        }
                                        key=|item| item.0
                                        children=|item| view! { <div class="zai-setting-row"><strong>{item.0}</strong><kbd>{item.1}</kbd></div> }
                                    />
                                </section>
                            </div>
                        </Show>

                        <Show when=move || page.get() == SettingsPage::Providers>
                            <div class="zai-settings-page">
                                <div class="zai-settings-title-row">
                                    <h1 id="zai-settings-page-title">"Runtimes"</h1>
                                    <button class="zai-neutral-button" disabled=move || saving.get() on:click=move |_| refresh.run(())>
                                        <Icon icon=LuRefreshCw width="13px" height="13px" />
                                        "Refresh"
                                    </button>
                                </div>
                                <h3>"Local coding agents"</h3>
                                <section class="zai-settings-provider-card">
                                    <For
                                        each=move || {
                                            providers
                                                .get()
                                                .into_iter()
                                                .filter(|provider| provider.id != ProviderId::Openrouter)
                                                .collect::<Vec<_>>()
                                        }
                                        key=|provider| provider.id
                                        children=provider_row
                                    />
                                </section>

                                <h3>"OpenAI API"</h3>
                                <section class="zai-settings-provider-card">
                                    <div class="zai-settings-provider-row zai-openrouter-row">
                                        <ProviderBadge brand=Signal::derive(move || ProviderBrand::Openai) />
                                        <div class="zai-settings-provider-copy"><strong>"OpenAI API"</strong><span>"Native image, transcription, and speech routes"</span></div>
                                        <Show
                                            when=move || openai.get().connected
                                            fallback=move || view! { <span class="zai-settings-disconnected">"Not connected"</span> }
                                        >
                                            <div class="zai-settings-status-actions">
                                                <span class="zai-settings-ready"><Icon icon=LuCheck width="13px" height="13px" />"Connected"</span>
                                                <button class="zai-danger-link" on:click=move |_| {
                                                    spawn_local(async move {
                                                        match bridge::clear_openai_key().await {
                                                            Ok(status) => openai.set(status),
                                                            Err(cause) => message.set(Some(cause)),
                                                        }
                                                    });
                                                }>"Disconnect"</button>
                                            </div>
                                        </Show>
                                    </div>
                                    <Show when=move || !openai.get().connected>
                                        <div class="zai-openrouter-key">
                                            <Icon icon=LuKeyRound width="15px" height="15px" />
                                            <input type="password" autocomplete="off" prop:value=move || openai_key.get() on:input=move |event| openai_key.set(event_target_value(&event)) placeholder="sk-…" />
                                            <button class="zai-neutral-button" disabled=move || openai_key.get().trim().is_empty() || saving.get() on:click=move |_| connect_openai.run(())>"Connect"</button>
                                        </div>
                                    </Show>
                                </section>

                                <h3>"OpenRouter"</h3>
                                <section class="zai-settings-provider-card">
                                    <div class="zai-settings-provider-row zai-openrouter-row">
                                        <ProviderBadge brand=Signal::derive(move || ProviderBrand::Openrouter) />
                                        <div class="zai-settings-provider-copy"><strong>"OpenRouter"</strong><span>"Choose from models available to your API key"</span></div>
                                        <Show
                                            when=move || openrouter.get().connected
                                            fallback=move || view! { <span class="zai-settings-disconnected">"Not connected"</span> }
                                        >
                                            <div class="zai-settings-status-actions">
                                                <span class="zai-settings-ready"><Icon icon=LuCheck width="13px" height="13px" />"Connected"</span>
                                                <button class="zai-danger-link" on:click=move |_| {
                                                    spawn_local(async move {
                                                        match bridge::clear_openrouter_key().await {
                                                            Ok(status) => {
                                                                openrouter.set(status);
                                                                openrouter_models.set(Vec::new());
                                                                openrouter_voice_catalog.set(OpenRouterVoiceCatalog::default());
                                                                voice_catalog_loaded.set(false);
                                                                voice_catalog_loading.set(false);
                                                                if let Ok(value) = bridge::list_providers().await {
                                                                    providers.set(value);
                                                                }
                                                            }
                                                            Err(cause) => message.set(Some(cause)),
                                                        }
                                                    });
                                                }>"Disconnect"</button>
                                            </div>
                                        </Show>
                                    </div>
                                    <Show when=move || !openrouter.get().connected>
                                        <div class="zai-openrouter-key">
                                            <Icon icon=LuKeyRound width="15px" height="15px" />
                                            <input type="password" autocomplete="off" prop:value=move || router_key.get() on:input=move |event| router_key.set(event_target_value(&event)) placeholder="sk-or-v1-…" />
                                            <button class="zai-neutral-button" disabled=move || router_key.get().trim().is_empty() || saving.get() on:click=move |_| connect_router.run(())>"Connect"</button>
                                        </div>
                                    </Show>
                                </section>
                                <Show when=move || message.get().is_some()><p class="zai-settings-message">{move || message.get().unwrap_or_default()}</p></Show>
                            </div>
                        </Show>

                        <Show when=move || page.get() == SettingsPage::Models>
                            <div class="zai-settings-page">
                                <h1 id="zai-settings-page-title">"Models"</h1>
                                <p class="zai-settings-intro">"CLI agents use their own configured model catalogs. OpenRouter models are loaded from your account."</p>
                                <section class="zai-settings-card zai-model-list">
                                    <Show
                                        when=move || !openrouter_models.read().is_empty()
                                        fallback=move || view! { <div class="zai-model-empty">"Connect OpenRouter to load its model catalog."</div> }
                                    >
                                        <For
                                            each=move || {
                                                openrouter_models
                                                    .get()
                                                    .into_iter()
                                                    .take(100)
                                                    .collect::<Vec<_>>()
                                            }
                                            key=|model| model.id.clone()
                                            children=|model| view! {
                                                <div class="zai-setting-row">
                                                    <div><strong>{model.name}</strong><code>{model.id}</code></div>
                                                    <span class="zai-setting-value">{model.context_length.map(|value| format!("{}k", value / 1000)).unwrap_or_default()}</span>
                                                </div>
                                            }
                                        />
                                    </Show>
                                </section>
                            </div>
                        </Show>

                        <Show when=move || page.get() == SettingsPage::Voice>
                            <div class="zai-settings-page">
                                <div class="zai-settings-title-row">
                                    <h1 id="zai-settings-page-title">"Voice"</h1>
                                    <button class="zai-neutral-button" disabled=move || voice.get().is_none() || saving.get() on:click=move |_| save_voice.run(())>"Save"</button>
                                </div>
                                <p class="zai-settings-intro">"Dictation and the voice agent stay available from the tray even while the editor is closed."</p>
                                <section class="zai-settings-card">
                                    <div class="zai-setting-row">
                                        <div><strong>"Microphone access"</strong><span>{move || if microphone_message.get().is_empty() { "Grant and test access before using global hold shortcuts.".to_owned() } else { microphone_message.get() }}</span></div>
                                        <button class="zai-neutral-button" disabled=move || microphone_status.get() == "checking" on:click=move |_| {
                                            microphone_status.set("checking".to_owned());
                                            spawn_local(async move {
                                                let result = async {
                                                    bridge::request_microphone_permission().await?;
                                                    bridge::request_microphone_access().await
                                                }.await;
                                                match result {
                                                    Ok(()) => {
                                                        microphone_status.set("ready".to_owned());
                                                        microphone_message.set("Microphone access is ready in Onyx.".to_owned());
                                                    }
                                                    Err(cause) => {
                                                        microphone_status.set("blocked".to_owned());
                                                        microphone_message.set(cause);
                                                    }
                                                }
                                            });
                                        }>
                                            {move || if microphone_status.get() == "checking" { "Checking…" } else if microphone_status.get() == "ready" { "Ready" } else { "Enable & test" }}
                                        </button>
                                    </div>
                                    <div class="zai-setting-row">
                                        <div><strong>"Global shortcuts & text insertion"</strong><span>{move || if native_message.get().is_empty() { "macOS needs Input Monitoring and Accessibility permissions.".to_owned() } else { native_message.get() }}</span></div>
                                        <button class="zai-neutral-button" on:click=move |_| {
                                            spawn_local(async move {
                                                match bridge::request_native_voice_permissions().await {
                                                    Ok(value) => {
                                                        native_permissions.set(Some(value));
                                                        native_message.set(if value.accessibility && value.input_monitoring {
                                                            "Global shortcuts and text insertion are ready.".to_owned()
                                                        } else {
                                                            "Enable Onyx in both panes, then relaunch once.".to_owned()
                                                        });
                                                    }
                                                    Err(cause) => native_message.set(cause),
                                                }
                                            });
                                        }>
                                            {move || if native_permissions.get().is_some_and(|value| value.accessibility && value.input_monitoring) { "Ready" } else { "Enable" }}
                                        </button>
                                    </div>
                                    <div class="zai-setting-row"><div><strong>"Dictation"</strong><span>"Hold anywhere, release to transcribe and paste"</span></div><kbd>"Control Shift"</kbd></div>
                                    <div class="zai-setting-row"><div><strong>"Agentic voice"</strong><span>"Hold anywhere to ask Onyx about the active app"</span></div><kbd>"Control Option"</kbd></div>
                                    <div class="zai-setting-row">
                                        <div><strong>"Dictation model"</strong><span>"Fast multilingual transcription with automatic language detection"</span></div>
                                        <label class="zai-setting-select">
                                            <select
                                                aria-label="Dictation model"
                                                prop:value=move || voice.get().map(|value| encoded_choice(&value.transcription_provider, &value.transcription_model)).unwrap_or_default()
                                                on:change=move |event| {
                                                    if let Some((provider, model)) = event_target_value(&event).split_once('|') {
                                                        voice.update(|value| if let Some(value) = value {
                                                            value.transcription_provider = provider.to_owned();
                                                            value.transcription_model = model.to_owned();
                                                        });
                                                    }
                                                }
                                            >
                                                <For
                                                    each=move || dictation_models.get()
                                                    key=|choice| choice.value.clone()
                                                    children=|choice| view! {
                                                        <option value=choice.value disabled=choice.disabled>
                                                            {choice.label}
                                                        </option>
                                                    }
                                                />
                                            </select>
                                            <Icon icon=LuChevronDown width="13px" height="13px" />
                                        </label>
                                    </div>
                                    <div class="zai-setting-row">
                                        <div><strong>"Agent model"</strong><span>"Model used for general voice questions"</span></div>
                                        <label class="zai-setting-select">
                                            <select
                                                aria-label="Voice agent model"
                                                prop:value=move || voice.get().map(|value| encoded_choice(value.agent_provider.as_str(), &value.agent_model)).unwrap_or_default()
                                                on:change=move |event| {
                                                    if let Some((provider, model)) = event_target_value(&event).split_once('|')
                                                        && let Some(provider) = provider_id(provider)
                                                    {
                                                        voice.update(|value| if let Some(value) = value {
                                                            value.agent_provider = provider;
                                                            value.agent_model = model.to_owned();
                                                        });
                                                    }
                                                }
                                            >
                                                <For
                                                    each=move || agent_models.get()
                                                    key=|choice| choice.value.clone()
                                                    children=|choice| view! {
                                                        <option value=choice.value disabled=choice.disabled>
                                                            {choice.label}
                                                        </option>
                                                    }
                                                />
                                            </select>
                                            <Icon icon=LuChevronDown width="13px" height="13px" />
                                        </label>
                                    </div>
                                    <div class="zai-setting-row">
                                        <div><strong>"Speech model"</strong><span>"Voice used to read agent answers"</span></div>
                                        <label class="zai-setting-select">
                                            <select
                                                aria-label="Speech model"
                                                prop:value=move || voice.get().map(|value| encoded_choice(&value.voice_provider, &value.voice_model)).unwrap_or_default()
                                                on:change=move |event| {
                                                    if let Some((provider, model)) = event_target_value(&event).split_once('|') {
                                                        voice.update(|value| if let Some(value) = value {
                                                            value.voice_provider = provider.to_owned();
                                                            value.voice_model = model.to_owned();
                                                        });
                                                    }
                                                }
                                            >
                                                <For
                                                    each=move || speech_models.get()
                                                    key=|choice| choice.value.clone()
                                                    children=|choice| view! {
                                                        <option value=choice.value disabled=choice.disabled>
                                                            {choice.label}
                                                        </option>
                                                    }
                                                />
                                            </select>
                                            <Icon icon=LuChevronDown width="13px" height="13px" />
                                        </label>
                                    </div>
                                    <div class="zai-setting-row">
                                        <div><strong>"Voice"</strong><span>"Voice identifier supported by the selected speech model"</span></div>
                                        <label class="zai-setting-select">
                                            <select
                                                aria-label="Speech voice"
                                                prop:value=move || voice.get().map(|value| value.voice_id).unwrap_or_else(|| "alloy".to_owned())
                                                on:change=move |event| voice.update(|value| if let Some(value) = value { value.voice_id = event_target_value(&event) })
                                            >
                                                <For
                                                    each=move || speech_voices.get()
                                                    key=|choice| choice.value.clone()
                                                    children=|choice| view! {
                                                        <option value=choice.value>{choice.label}</option>
                                                    }
                                                />
                                            </select>
                                            <Icon icon=LuChevronDown width="13px" height="13px" />
                                        </label>
                                    </div>
                                    <div class="zai-setting-row">
                                        <div><strong>"Speech rate"</strong><span>"0.5× to 2×"</span></div>
                                        <input
                                            class="zai-settings-inline-input"
                                            type="number"
                                            min="0.5"
                                            max="2"
                                            step="0.1"
                                            prop:value=move || voice.get().map(|value| value.voice_rate.to_string()).unwrap_or_else(|| "1".to_owned())
                                            on:input=move |event| {
                                                if let Ok(next) = event_target_value(&event).parse::<f32>() {
                                                    voice.update(|value| if let Some(value) = value { value.voice_rate = next.clamp(0.5, 2.0) });
                                                }
                                            }
                                        />
                                    </div>
                                    <div class="zai-setting-row">
                                        <div><strong>"Overlay position"</strong><span>"Where dictation feedback appears"</span></div>
                                        <label class="zai-setting-select">
                                            <select
                                                prop:value=move || voice.get().map(|value| value.overlay_position.as_str()).unwrap_or("bottom_center")
                                                on:change=move |event| {
                                                    if let Some(next) = OverlayPosition::from_str(&event_target_value(&event)) {
                                                        voice.update(|value| if let Some(value) = value { value.overlay_position = next });
                                                    }
                                                }
                                            >
                                                <For
                                                    each=move || OverlayPosition::ALL
                                                    key=|value| *value
                                                    children=|value| view! { <option value=value.as_str()>{value.as_str().replace('_', " ")}</option> }
                                                />
                                            </select>
                                            <Icon icon=LuChevronDown width="13px" height="13px" />
                                        </label>
                                    </div>
                                    <div class="zai-setting-row">
                                        <div><strong>"Speak responses"</strong><span>"Read agent answers aloud when TTS is configured"</span></div>
                                        <input
                                            type="checkbox"
                                            prop:checked=move || voice.get().is_some_and(|value| value.speak_responses)
                                            on:change=move |_| voice.update(|value| if let Some(value) = value { value.speak_responses = !value.speak_responses })
                                        />
                                    </div>
                                </section>
                                <Show when=move || message.get().is_some()><p class="zai-settings-message">{move || message.get().unwrap_or_default()}</p></Show>
                            </div>
                        </Show>

                    </div>
                </section>
                <Show when=move || provider_terminal.get().is_some()>
                    <div class="zai-provider-terminal-scrim">
                        <section class="zai-provider-terminal" role="dialog" aria-modal="true">
                            <header>
                                <div>
                                    <strong>{move || provider_terminal_title.get()}</strong>
                                    <span>"Official CLI running inside Onyx"</span>
                                </div>
                                <button
                                    type="button"
                                    aria-label="Close CLI terminal"
                                    on:click=move |_| close_provider_terminal.run(())
                                >
                                    <Icon icon=LuX width="15px" height="15px" />
                                </button>
                            </header>
                            {move || provider_terminal.get().map(|terminal| view! {
                                <TerminalViewport session_id=terminal.id autofocus=true />
                            })}
                        </section>
                    </div>
                </Show>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::fallback_catalogs;

    fn provider_status(id: ProviderId, available: bool) -> ProviderStatus {
        ProviderStatus {
            id,
            name: id.display_name().to_owned(),
            available,
            executable_path: None,
            version: None,
            install_url: String::new(),
            transport: String::new(),
        }
    }

    fn voice_model(id: &str, voices: &[&str]) -> OpenRouterVoiceModel {
        OpenRouterVoiceModel {
            id: id.to_owned(),
            name: id.to_owned(),
            supported_voices: voices.iter().map(|voice| (*voice).to_owned()).collect(),
        }
    }

    #[test]
    fn unavailable_agent_provider_only_keeps_disabled_saved_choice() {
        let providers = vec![
            provider_status(ProviderId::Claude, false),
            provider_status(ProviderId::Codex, true),
            provider_status(ProviderId::Gemini, false),
            provider_status(ProviderId::Kimi, false),
            provider_status(ProviderId::Openrouter, false),
        ];

        let choices = agent_choices(
            &fallback_catalogs(),
            &[],
            &providers,
            ProviderId::Claude,
            "opus",
        );
        let claude_choices = choices
            .iter()
            .filter(|choice| choice.value.starts_with("claude|"))
            .collect::<Vec<_>>();

        assert_eq!(claude_choices.len(), 1);
        assert_eq!(claude_choices[0].value, "claude|opus");
        assert!(claude_choices[0].disabled);
        assert!(claude_choices[0].label.contains("(unavailable)"));
        assert!(
            choices
                .iter()
                .any(|choice| choice.value == "codex|default" && !choice.disabled)
        );
    }

    #[test]
    fn available_saved_agent_model_is_not_duplicated_or_disabled() {
        let providers = vec![provider_status(ProviderId::Claude, true)];
        let choices = agent_choices(
            &fallback_catalogs(),
            &[],
            &providers,
            ProviderId::Claude,
            "opus",
        );
        let saved = choices
            .iter()
            .filter(|choice| choice.value == "claude|opus")
            .collect::<Vec<_>>();

        assert_eq!(saved.len(), 1);
        assert!(!saved[0].disabled);
        assert!(!saved[0].label.contains("unavailable"));
    }

    #[test]
    fn saved_custom_speech_voice_remains_selectable() {
        let choices = speech_voice_choices(
            &OpenRouterVoiceCatalog::default(),
            "openrouter",
            "provider/custom-tts",
            "provider-specific-voice",
        );
        let saved = choices
            .iter()
            .find(|choice| choice.value == "provider-specific-voice")
            .expect("saved voice");

        assert_eq!(saved.label, "Saved · provider-specific-voice");
        assert!(!saved.disabled);
    }

    #[test]
    fn missing_saved_speech_model_is_not_left_enabled() {
        let choices = speech_choices(
            &[voice_model("deepgram/aura-2", &["aura-2-livia-it"])],
            "openrouter",
            "openai/gpt-4o-mini-tts-2025-12-15",
        );
        let saved = choices
            .iter()
            .find(|choice| choice.value == "openrouter|openai/gpt-4o-mini-tts-2025-12-15")
            .expect("missing saved model");

        assert!(saved.disabled);
        assert!(saved.label.contains("unavailable"));
    }

    #[test]
    fn voice_choices_follow_the_selected_openrouter_model() {
        let catalog = OpenRouterVoiceCatalog {
            transcription: Vec::new(),
            speech: vec![
                voice_model("provider/first", &["voice-a", "voice-b"]),
                voice_model("provider/second", &["voice-c"]),
            ],
        };

        let choices = speech_voice_choices(&catalog, "openrouter", "provider/second", "voice-c");
        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.value.as_str())
                .collect::<Vec<_>>(),
            vec!["voice-c"]
        );
        assert_eq!(
            normalized_speech_voice(&catalog, "openrouter", "provider/second", "voice-a"),
            Some("voice-c".to_owned())
        );
        assert_eq!(
            normalized_speech_voice(&catalog, "openrouter", "provider/second", "voice-c"),
            None
        );
    }

    #[test]
    fn dedicated_voice_catalogs_populate_the_correct_dropdowns() {
        let transcription = vec![voice_model("provider/stt", &[])];
        let speech = vec![
            voice_model("provider/tts", &["voice-a"]),
            voice_model("provider/unknown-voices", &[]),
        ];

        let dictation = dictation_choices(&transcription, "openrouter", "provider/stt");
        assert!(
            dictation
                .iter()
                .any(|choice| { choice.value == "openrouter|provider/stt" && !choice.disabled })
        );
        assert!(
            !dictation
                .iter()
                .any(|choice| choice.value == "openrouter|provider/tts")
        );

        let speech = speech_choices(&speech, "openrouter", "provider/tts");
        assert!(
            speech
                .iter()
                .any(|choice| { choice.value == "openrouter|provider/tts" && !choice.disabled })
        );
        assert!(speech.iter().any(|choice| {
            choice.value == "openrouter|provider/unknown-voices" && choice.disabled
        }));
        assert!(
            !speech
                .iter()
                .any(|choice| choice.value == "openrouter|provider/stt")
        );
    }
}
