use icondata::{
    LuCheck, LuChevronDown, LuCpu, LuExternalLink, LuKeyRound, LuKeyboard, LuLoaderCircle, LuMic,
    LuRefreshCw, LuSlidersHorizontal, LuSparkles, LuUserRound, LuX,
};
use leptos::prelude::*;
use leptos_icons::Icon;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;

use crate::{
    bridge,
    catalog::ProviderCatalogs,
    model::{
        AccountProfile, AgentSession, ConnectionStatus, NativeVoicePermissions, OpenRouterModel,
        OverlayPosition, ProviderBrand, ProviderId, ProviderStatus, UpdateProgress, VoiceSettings,
    },
    storage, theme,
};

use super::ProviderBadge;

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
    Account,
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
        SettingsPage::Account => "Account & cloud",
    }
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
    sessions: Signal<Vec<AgentSession>>,
    providers: RwSignal<Vec<ProviderStatus>>,
    catalogs: Signal<ProviderCatalogs>,
    openrouter: RwSignal<ConnectionStatus>,
    openai: RwSignal<ConnectionStatus>,
    openrouter_models: RwSignal<Vec<OpenRouterModel>>,
    color_scheme: RwSignal<ColorScheme>,
    profile: Signal<Option<AccountProfile>>,
    cloud_configured: Signal<bool>,
    cloud_authenticated: Signal<bool>,
    on_close: Callback<()>,
    on_sign_out: Callback<()>,
) -> impl IntoView {
    let _ = catalogs;
    let page = RwSignal::new(SettingsPage::General);
    let router_key = RwSignal::new(String::new());
    let openai_key = RwSignal::new(String::new());
    let saving = RwSignal::new(false);
    let message = RwSignal::new(None::<String>);
    let voice = RwSignal::new(None::<VoiceSettings>);
    let microphone_status = RwSignal::new("idle".to_owned());
    let microphone_message = RwSignal::new(String::new());
    let native_permissions = RwSignal::new(None::<NativeVoicePermissions>);
    let native_message = RwSignal::new(String::new());
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
                }
                Ok(None) => {
                    update_state.set("current".to_owned());
                    update_message.set("You are on the latest version.".to_owned());
                }
                Err(cause) => {
                    update_state.set("failed".to_owned());
                    update_message.set(cause);
                }
            }
        });
    });
    let install_update = Callback::new(move |_: ()| {
        update_state.set("installing".to_owned());
        update_message.set("Downloading update…".to_owned());
        spawn_local(async move {
            if let Err(cause) = bridge::install_update(move |downloaded, total| {
                update_progress.set(Some(UpdateProgress { downloaded, total }));
            })
            .await
            {
                update_state.set("failed".to_owned());
                update_message.set(cause);
                update_progress.set(None);
            }
        });
    });
    let sync_now = Callback::new(move |_: ()| {
        saving.set(true);
        message.set(None);
        spawn_local(async move {
            let snapshot = serde_json::json!({
                "version": 1,
                "exportedAt": storage::timestamp(),
                "sessions": sessions.get(),
                "chats": storage::read_json::<serde_json::Value>(
                    storage::CHAT_THREADS_KEY,
                    serde_json::json!([]),
                ),
                "voiceHistory": storage::read_json::<serde_json::Value>(
                    storage::VOICE_HISTORY_KEY,
                    serde_json::json!([]),
                ),
                "preferences": {
                    "colorScheme": storage::get(storage::COLOR_SCHEME_KEY),
                    "desktop": storage::read_json::<serde_json::Value>(
                        storage::DESKTOP_PREFERENCES_KEY,
                        serde_json::Value::Null,
                    ),
                },
            });
            let result = bridge::push_cloud(&snapshot.to_string()).await;
            message.set(Some(match result {
                Ok(()) => "Sessions, chats, voice history, and preferences synced.".to_owned(),
                Err(cause) => cause,
            }));
            saving.set(false);
        });
    });

    let provider_row = move |provider: ProviderStatus| {
        let brand = ProviderBrand::for_provider(provider.id);
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
                        <span class="zai-settings-ready">
                            <Icon icon=LuCheck width="13px" height="13px" />
                            "Ready"
                        </span>
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
                                <NavButton target=SettingsPage::Account page=page icon=LuUserRound />
                            </nav>
                        </div>
                        <div class="zai-settings-version">
                            <strong>"Onyx Desktop"</strong>
                            <span>"v0.2.0 · Rust"</span>
                        </div>
                    </aside>

                    <div class="zai-settings-content">
                        <button
                            class="zai-settings-close"
                            on:click=move |_| on_close.run(())
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
                                        <input
                                            class="zai-settings-inline-input"
                                            prop:value=move || voice.get().map(|value| value.transcription_model).unwrap_or_default()
                                            on:input=move |event| voice.update(|value| if let Some(value) = value { value.transcription_model = event_target_value(&event) })
                                        />
                                    </div>
                                    <div class="zai-setting-row">
                                        <div><strong>"Agent model"</strong><span>"Model used for general voice questions"</span></div>
                                        <input
                                            class="zai-settings-inline-input"
                                            prop:value=move || voice.get().map(|value| value.agent_model).unwrap_or_default()
                                            on:input=move |event| voice.update(|value| if let Some(value) = value { value.agent_model = event_target_value(&event) })
                                        />
                                    </div>
                                    <div class="zai-setting-row">
                                        <div><strong>"Speech model"</strong><span>"Voice used to read agent answers"</span></div>
                                        <input
                                            class="zai-settings-inline-input"
                                            prop:value=move || voice.get().map(|value| value.voice_model).unwrap_or_default()
                                            on:input=move |event| voice.update(|value| if let Some(value) = value { value.voice_model = event_target_value(&event) })
                                        />
                                    </div>
                                    <div class="zai-setting-row">
                                        <div><strong>"Voice"</strong><span>"Voice identifier supported by the selected speech model"</span></div>
                                        <input
                                            class="zai-settings-inline-input"
                                            prop:value=move || voice.get().map(|value| value.voice_id).unwrap_or_else(|| "alloy".to_owned())
                                            on:input=move |event| voice.update(|value| if let Some(value) = value { value.voice_id = event_target_value(&event) })
                                        />
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

                        <Show when=move || page.get() == SettingsPage::Account>
                            <div class="zai-settings-page">
                                <h1 id="zai-settings-page-title">"Account & cloud"</h1>
                                <p class="zai-settings-intro">"Onyx is local-first. Signing in enables optional sync through your Clerk and Convex deployment."</p>
                                <section class="zai-settings-card">
                                    <Show when=move || profile.get().is_some()>
                                        <div class="zai-setting-row">
                                            <div><strong>{move || profile.get().map(|value| value.name).unwrap_or_default()}</strong><span>{move || profile.get().map(|value| value.email).unwrap_or_default()}</span></div>
                                            <span class="zai-settings-ready"><Icon icon=LuCheck width="13px" height="13px" />"Signed in"</span>
                                        </div>
                                        <div class="zai-setting-row">
                                            <div><strong>"Cloud sync"</strong><span>{move || if cloud_configured.get() { "Convex is configured for this build" } else { "Add VITE_CONVEX_URL to enable sync" }}</span></div>
                                            <button class="zai-neutral-button" disabled=move || !cloud_authenticated.get() || saving.get() on:click=move |_| sync_now.run(())>"Sync now"</button>
                                        </div>
                                        <div class="zai-setting-row">
                                            <div><strong>"Account session"</strong><span>"Sign out of Onyx on this device"</span></div>
                                            <button class="zai-danger-link" on:click=move |_| on_sign_out.run(())>"Sign out"</button>
                                        </div>
                                    </Show>
                                </section>
                                <Show when=move || message.get().is_some()><p class="zai-settings-message">{move || message.get().unwrap_or_default()}</p></Show>
                            </div>
                        </Show>
                    </div>
                </section>
            </div>
        </Show>
    }
}
