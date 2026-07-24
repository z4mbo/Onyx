use gloo_timers::future::TimeoutFuture;
use icondata::{
    LuChevronDown, LuEllipsis, LuImage, LuLoaderCircle, LuMenu, LuMessageSquare, LuPanelLeftClose,
    LuPanelLeftOpen, LuPlus, LuSearch, LuSend, LuStar, LuTrash2, LuVideo, LuX,
};
use leptos::ev::{KeyboardEvent, SubmitEvent};
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen_futures::spawn_local;

use crate::{
    bridge,
    catalog::{
        PROVIDER_BRANDS, ProviderCatalogs, models_for_brand, runtime_for_brand, selected_or_default,
    },
    model::{
        AccountProfile, ChatMedia, ChatMessage, ChatMessageInput, ChatMode, ChatRequest,
        ChatThread, ConnectionStatus, MediaGenerationRequest, OpenRouterModel, ProviderBrand,
        ProviderId, ProviderModelOption, ProviderStatus, SpeedMode,
    },
    storage,
};

use super::{InternalBrowser, ProviderBadge};

const MAX_THREADS: usize = 80;

#[derive(Clone, Copy, PartialEq, Eq)]
struct SubscriptionApp {
    id: &'static str,
    name: &'static str,
    brand: ProviderBrand,
    url: &'static str,
}

const SUBSCRIPTION_APPS: [SubscriptionApp; 4] = [
    SubscriptionApp {
        id: "chatgpt",
        name: "ChatGPT",
        brand: ProviderBrand::Openai,
        url: "https://chatgpt.com/",
    },
    SubscriptionApp {
        id: "claude",
        name: "Claude",
        brand: ProviderBrand::Anthropic,
        url: "https://claude.ai/new",
    },
    SubscriptionApp {
        id: "gemini",
        name: "Gemini",
        brand: ProviderBrand::Google,
        url: "https://gemini.google.com/app",
    },
    SubscriptionApp {
        id: "grok",
        name: "Grok",
        brand: ProviderBrand::Xai,
        url: "https://grok.com/",
    },
];

fn mode_icon(mode: ChatMode) -> icondata::Icon {
    match mode {
        ChatMode::Chat => LuMessageSquare,
        ChatMode::Image => LuImage,
        ChatMode::Video => LuVideo,
    }
}

fn make_thread(provider: ProviderId, model: String, mode: ChatMode) -> ChatThread {
    let now = storage::timestamp();
    ChatThread {
        id: storage::unique_id("chat"),
        title: "New chat".to_owned(),
        provider,
        model,
        mode,
        messages: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    }
}

fn persist_threads(threads: RwSignal<Vec<ChatThread>>, mut next: Vec<ChatThread>) {
    next.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    next.truncate(MAX_THREADS);
    storage::write_json(storage::CHAT_THREADS_KEY, &next);
    storage::dispatch("onyx:cloud-data-changed");
    threads.set(next);
}

fn image_model() -> ProviderModelOption {
    ProviderModelOption {
        id: "gpt-image-2".to_owned(),
        name: "GPT Image 2".to_owned(),
        description: Some("OpenAI native image generation".to_owned()),
        is_default: true,
        reasoning: Vec::new(),
        default_reasoning: None,
        speeds: vec![SpeedMode::Standard],
        default_speed: SpeedMode::Standard,
        context_length: None,
    }
}

fn available_models(
    brand: ProviderBrand,
    mode: ChatMode,
    catalogs: &ProviderCatalogs,
    openrouter_models: &[OpenRouterModel],
) -> Vec<ProviderModelOption> {
    if mode == ChatMode::Image && brand == ProviderBrand::Openai {
        return vec![image_model()];
    }
    let models = models_for_brand(brand, catalogs, openrouter_models);
    if mode == ChatMode::Chat {
        return models;
    }
    models
        .into_iter()
        .filter(|model| {
            openrouter_models
                .iter()
                .find(|source| source.id == model.id)
                .is_some_and(|source| {
                    source
                        .output_modalities
                        .iter()
                        .any(|item| item == mode.as_str())
                })
        })
        .collect()
}

fn inferred_brand(thread: &ChatThread) -> ProviderBrand {
    if thread.provider == ProviderId::Openrouter && thread.model.starts_with("x-ai/") {
        ProviderBrand::Xai
    } else {
        ProviderBrand::for_provider(thread.provider)
    }
}

#[component]
pub fn ChatView(
    providers: Signal<Vec<ProviderStatus>>,
    catalogs: Signal<ProviderCatalogs>,
    openrouter_models: Signal<Vec<OpenRouterModel>>,
    openai: Signal<ConnectionStatus>,
    profile: Signal<Option<AccountProfile>>,
    on_settings: Callback<()>,
) -> impl IntoView {
    let mut stored = storage::read_json::<Vec<ChatThread>>(storage::CHAT_THREADS_KEY, Vec::new());
    stored.truncate(MAX_THREADS);
    let restored = stored.first().cloned();
    let initial_active = restored.as_ref().map(|thread| thread.id.clone());
    let initial_brand = restored
        .as_ref()
        .map(inferred_brand)
        .unwrap_or(ProviderBrand::Anthropic);
    let initial_mode = restored
        .as_ref()
        .map(|thread| thread.mode)
        .unwrap_or(ChatMode::Chat);
    let initial = available_models(
        initial_brand,
        initial_mode,
        &catalogs.get_untracked(),
        &openrouter_models.get_untracked(),
    );

    let threads = RwSignal::new(stored);
    let active_id = RwSignal::new(initial_active);
    let draft = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let sidebar_open = RwSignal::new(true);
    let model_menu_open = RwSignal::new(false);
    let model_query = RwSignal::new(String::new());
    let chat_query = RwSignal::new(String::new());
    let favorites = RwSignal::new(storage::read_json::<Vec<String>>(
        storage::CHAT_FAVORITES_KEY,
        Vec::new(),
    ));
    let brand = RwSignal::new(initial_brand);
    let selected_model = RwSignal::new(
        restored
            .as_ref()
            .map(|thread| thread.model.clone())
            .or_else(|| selected_or_default(&initial).map(|model| model.id))
            .unwrap_or_else(|| "default".to_owned()),
    );
    let mode = RwSignal::new(initial_mode);
    let web_search = RwSignal::new(false);
    let web_app = RwSignal::new(None::<SubscriptionApp>);
    let error = RwSignal::new(None::<String>);

    let active = Signal::derive(move || {
        let id = active_id.get();
        threads
            .read()
            .iter()
            .find(|thread| Some(thread.id.as_str()) == id.as_deref())
            .cloned()
    });
    let model_options = Signal::derive(move || {
        available_models(
            brand.get(),
            mode.get(),
            &catalogs.get(),
            &openrouter_models.get(),
        )
    });
    let selected_model_name = Signal::derive(move || {
        model_options
            .read()
            .iter()
            .find(|model| model.id == selected_model.get())
            .map(|model| model.name.clone())
            .unwrap_or_else(|| {
                let selected = selected_model.get();
                if selected.is_empty() {
                    "Choose model".to_owned()
                } else {
                    selected
                }
            })
    });
    let filtered_brands = Signal::derive(move || {
        PROVIDER_BRANDS
            .into_iter()
            .filter(|item| match mode.get() {
                ChatMode::Image => {
                    item.id == ProviderBrand::Openrouter
                        || (item.id == ProviderBrand::Openai && openai.get().connected)
                }
                ChatMode::Video => item.id == ProviderBrand::Openrouter,
                ChatMode::Chat => providers
                    .read()
                    .iter()
                    .find(|status| status.id == item.runtime)
                    .is_some_and(|status| status.available),
            })
            .collect::<Vec<_>>()
    });
    let filtered_models = Signal::derive(move || {
        let needle = model_query.get().trim().to_lowercase();
        let mut models = model_options.get();
        if !needle.is_empty() {
            models.retain(|model| {
                format!("{} {}", model.name, model.id)
                    .to_lowercase()
                    .contains(&needle)
            });
        }
        models.sort_by_key(|model| !favorites.read().contains(&model.id));
        models
    });
    let filtered_threads = Signal::derive(move || {
        let needle = chat_query.get().trim().to_lowercase();
        threads
            .read()
            .iter()
            .filter(|thread| needle.is_empty() || thread.title.to_lowercase().contains(&needle))
            .cloned()
            .collect::<Vec<_>>()
    });

    let choose_brand = Callback::new(move |value: ProviderBrand| {
        brand.set(value);
        let models = available_models(
            value,
            mode.get_untracked(),
            &catalogs.get_untracked(),
            &openrouter_models.get_untracked(),
        );
        selected_model.set(
            selected_or_default(&models)
                .map(|model| model.id)
                .unwrap_or_default(),
        );
    });
    let choose_mode = Callback::new(move |value: ChatMode| {
        mode.set(value);
        match value {
            ChatMode::Image => choose_brand.run(if openai.get_untracked().connected {
                ProviderBrand::Openai
            } else {
                ProviderBrand::Openrouter
            }),
            ChatMode::Video => choose_brand.run(ProviderBrand::Openrouter),
            ChatMode::Chat => {
                let current_brand = brand.get_untracked();
                let available = providers.read_untracked().iter().any(|status| {
                    status.id == runtime_for_brand(current_brand) && status.available
                });
                if !available {
                    choose_brand.run(ProviderBrand::Anthropic);
                }
            }
        }
    });
    let new_chat = Callback::new(move |_: ()| {
        let thread = make_thread(
            runtime_for_brand(brand.get_untracked()),
            selected_model.get_untracked(),
            mode.get_untracked(),
        );
        let id = thread.id.clone();
        let mut next = vec![thread];
        next.extend(threads.get_untracked());
        persist_threads(threads, next);
        active_id.set(Some(id));
        draft.set(String::new());
        error.set(None);
    });
    let delete_thread = Callback::new(move |id: String| {
        let mut next = threads.get_untracked();
        next.retain(|thread| thread.id != id);
        let was_active = active_id.get_untracked().as_deref() == Some(id.as_str());
        persist_threads(threads, next.clone());
        if was_active {
            active_id.set(next.first().map(|thread| thread.id.clone()));
        }
    });
    let select_thread = Callback::new(move |thread: ChatThread| {
        active_id.set(Some(thread.id.clone()));
        selected_model.set(thread.model.clone());
        mode.set(thread.mode);
        brand.set(inferred_brand(&thread));
        error.set(None);
    });
    let toggle_favorite = Callback::new(move |id: String| {
        let mut next = favorites.get_untracked();
        if next.contains(&id) {
            next.retain(|item| item != &id);
        } else {
            next.push(id);
        }
        storage::write_json(storage::CHAT_FAVORITES_KEY, &next);
        favorites.set(next);
    });
    let submit = Callback::new(move |_: ()| {
        let prompt = draft.get_untracked().trim().to_owned();
        let model = selected_model.get_untracked();
        if prompt.is_empty() || model.is_empty() || busy.get_untracked() {
            return;
        }
        let selected_mode = mode.get_untracked();
        let selected_brand = brand.get_untracked();
        let thread = active.get_untracked().unwrap_or_else(|| {
            let thread = make_thread(
                runtime_for_brand(selected_brand),
                model.clone(),
                selected_mode,
            );
            active_id.set(Some(thread.id.clone()));
            thread
        });
        let now = storage::timestamp();
        let user = ChatMessage {
            id: storage::unique_id("message"),
            role: "user".to_owned(),
            content: prompt.clone(),
            media: Vec::new(),
            created_at: now.clone(),
        };
        let mut next_thread = thread.clone();
        if next_thread.messages.is_empty() {
            next_thread.title = prompt.chars().take(52).collect();
        }
        next_thread.provider = runtime_for_brand(selected_brand);
        next_thread.model = model.clone();
        next_thread.mode = selected_mode;
        next_thread.messages.push(user);
        next_thread.updated_at = now;
        let mut next = threads.get_untracked();
        if let Some(current) = next.iter_mut().find(|item| item.id == thread.id) {
            *current = next_thread.clone();
        } else {
            next.push(next_thread.clone());
        }
        persist_threads(threads, next);
        draft.set(String::new());
        busy.set(true);
        error.set(None);

        spawn_local(async move {
            let result = match selected_mode {
                ChatMode::Image => {
                    bridge::generate_image(MediaGenerationRequest {
                        model: model.clone(),
                        prompt,
                        aspect_ratio: Some("1:1".to_owned()),
                        source: if selected_brand == ProviderBrand::Openai {
                            "openai"
                        } else {
                            "openrouter"
                        }
                        .to_owned(),
                    })
                    .await
                }
                ChatMode::Video => {
                    let mut job = match bridge::start_video(MediaGenerationRequest {
                        model: model.clone(),
                        prompt,
                        aspect_ratio: Some("16:9".to_owned()),
                        source: "openrouter".to_owned(),
                    })
                    .await
                    {
                        Ok(job) => job,
                        Err(cause) => {
                            busy.set(false);
                            error.set(Some(cause));
                            return;
                        }
                    };
                    for _ in 0..120 {
                        if matches!(job.status.as_str(), "completed" | "failed") {
                            break;
                        }
                        TimeoutFuture::new(2_500).await;
                        match bridge::poll_video(&job.id).await {
                            Ok(next) => job = next,
                            Err(cause) => {
                                busy.set(false);
                                error.set(Some(cause));
                                return;
                            }
                        }
                    }
                    match (job.status.as_str(), job.content_url) {
                        ("completed", Some(url)) => Ok(crate::model::ChatReply {
                            content: "Video generated".to_owned(),
                            model: model.clone(),
                            media: vec![ChatMedia {
                                kind: "video".to_owned(),
                                url,
                                mime_type: Some("video/mp4".to_owned()),
                            }],
                        }),
                        _ => Err(job
                            .error
                            .unwrap_or_else(|| "Video generation did not complete.".to_owned())),
                    }
                }
                ChatMode::Chat => {
                    bridge::chat_send(ChatRequest {
                        provider: runtime_for_brand(selected_brand),
                        model: model.clone(),
                        messages: next_thread
                            .messages
                            .iter()
                            .map(|message| ChatMessageInput {
                                role: message.role.clone(),
                                content: message.content.clone(),
                            })
                            .collect(),
                        web_search: web_search.get_untracked(),
                    })
                    .await
                }
            };
            match result {
                Ok(reply) => {
                    let assistant = ChatMessage {
                        id: storage::unique_id("message"),
                        role: "assistant".to_owned(),
                        content: reply.content,
                        media: reply.media,
                        created_at: storage::timestamp(),
                    };
                    let mut next = threads.get_untracked();
                    if let Some(item) = next.iter_mut().find(|item| item.id == thread.id) {
                        item.updated_at = assistant.created_at.clone();
                        item.messages.push(assistant);
                    }
                    persist_threads(threads, next);
                }
                Err(cause) => error.set(Some(cause)),
            }
            busy.set(false);
        });
    });

    view! {
        <section class="onyx-chat" data-sidebar=move || if sidebar_open.get() { "open" } else { "closed" }>
            <aside class="onyx-chat__sidebar">
                <div class="onyx-chat__sidebar-head">
                    <button class="onyx-chat__new" on:click=move |_| new_chat.run(())>
                        <Icon icon=LuPlus width="16px" height="16px" />
                        "New chat"
                    </button>
                    <button class="onyx-chat__icon-button" on:click=move |_| sidebar_open.set(false) aria-label="Close chat history">
                        <Icon icon=LuPanelLeftClose width="17px" height="17px" />
                    </button>
                </div>
                <label class="onyx-chat__search">
                    <Icon icon=LuSearch width="14px" height="14px" />
                    <input
                        prop:value=move || chat_query.get()
                        on:input=move |event| chat_query.set(event_target_value(&event))
                        placeholder="Search chats"
                    />
                </label>
                <div class="onyx-chat__history">
                    <Show
                        when=move || !filtered_threads.read().is_empty()
                        fallback=move || view! {
                            <p class="onyx-chat__empty-history">
                                {move || if chat_query.read().is_empty() {
                                    "Your conversations stay local unless cloud sync is enabled."
                                } else {
                                    "No chats match your search."
                                }}
                            </p>
                        }
                    >
                        <For
                            each=move || filtered_threads.get()
                            key=|thread| format!("{}:{}", thread.id, thread.updated_at)
                            children=move |thread| {
                                let selected = thread.clone();
                                let id = thread.id.clone();
                                let title = thread.title.clone();
                                let delete_label = format!("Delete {}", thread.title);
                                view! {
                                    <div
                                        class="onyx-chat__history-row"
                                        class:active=move || active_id.read().as_deref() == Some(id.as_str())
                                    >
                                        <button on:click=move |_| select_thread.run(selected.clone())>
                                            <Icon icon=mode_icon(thread.mode) width="14px" height="14px" />
                                            <span>{title}</span>
                                        </button>
                                        <button on:click=move |_| delete_thread.run(thread.id.clone()) aria-label=delete_label>
                                            <Icon icon=LuTrash2 width="13px" height="13px" />
                                        </button>
                                    </div>
                                }
                            }
                        />
                    </Show>
                </div>
                <button class="onyx-chat__account" on:click=move |_| on_settings.run(())>
                    {move || profile.get().map(|profile| {
                        if let Some(image) = profile.image_url {
                            view! { <img src=image alt="" referrerpolicy="no-referrer" /> }.into_any()
                        } else {
                            view! {
                                <span class="onyx-chat__account-fallback">
                                    {profile.name.chars().next().unwrap_or('O').to_uppercase().to_string()}
                                </span>
                            }.into_any()
                        }
                    }).unwrap_or_else(|| view! {
                        <span class="onyx-chat__account-fallback">"O"</span>
                    }.into_any())}
                    <div>
                        <strong>{move || profile.get().map(|item| item.name).unwrap_or_else(|| "Onyx account".to_owned())}</strong>
                        <small>{move || profile.get().map(|item| item.email).filter(|email| !email.is_empty()).unwrap_or_else(|| "Account & cloud".to_owned())}</small>
                    </div>
                    <Icon icon=LuEllipsis width="16px" height="16px" />
                </button>
            </aside>

            <main class="onyx-chat__main">
                <header class="onyx-chat__topbar">
                    <Show when=move || !sidebar_open.get()>
                        <button class="onyx-chat__icon-button" on:click=move |_| sidebar_open.set(true) aria-label="Open chat history">
                            <Icon icon=LuPanelLeftOpen width="18px" height="18px" />
                        </button>
                    </Show>
                    <div>
                        <strong>{move || web_app
                            .get()
                            .map(|app| app.name.to_owned())
                            .or_else(|| active.get().map(|thread| thread.title))
                            .unwrap_or_else(|| "New chat".to_owned())}</strong>
                        <span>{move || if web_app.get().is_some() {
                            "Official web app"
                        } else {
                            "Onyx Chat"
                        }}</span>
                    </div>
                    <Show
                        when=move || web_app.get().is_some()
                        fallback=move || view! {
                            <button class="onyx-chat__icon-button" on:click=move |_| on_settings.run(()) aria-label="Open chat settings">
                                <Icon icon=LuMenu width="17px" height="17px" />
                            </button>
                        }
                    >
                        <button
                            class="onyx-chat__icon-button"
                            on:click=move |_| web_app.set(None)
                            aria-label="Close official web app"
                        >
                            <Icon icon=LuX width="17px" height="17px" />
                        </button>
                    </Show>
                </header>

                <div class="onyx-chat__scroll">
                    <Show
                        when=move || active.get().is_some_and(|thread| !thread.messages.is_empty())
                        fallback=move || view! {
                            <div class="onyx-chat__welcome">
                                <h1>"Onyx"</h1>
                                <p>"How can I help?"</p>
                                <div class="onyx-chat__web-apps" aria-label="Subscription web apps">
                                    <For
                                        each=move || SUBSCRIPTION_APPS
                                        key=|app| app.name
                                        children=move |app| view! {
                                            <button on:click=move |_| web_app.set(Some(app))>
                                                <ProviderBadge brand=Signal::derive(move || app.brand) small=true />
                                                <span>{app.name}</span>
                                            </button>
                                        }
                                    />
                                </div>
                                <small class="onyx-chat__web-note">
                                    "Open the provider’s signed-in web app inside Onyx."
                                </small>
                            </div>
                        }
                    >
                        <div class="onyx-chat__messages">
                            <For
                                each=move || active.get().map(|thread| thread.messages).unwrap_or_default()
                                key=|message| message.id.clone()
                                children=move |message| view! {
                                    <article class="onyx-chat__message" class:user=message.role == "user">
                                        <div class="onyx-chat__message-copy">{message.content}</div>
                                        <For
                                            each=move || message.media.clone()
                                            key=|media| media.url.clone()
                                            children=move |media| if media.kind == "image" {
                                                view! { <img src=media.url alt="Generated media" /> }.into_any()
                                            } else {
                                                view! { <video src=media.url controls=true /> }.into_any()
                                            }
                                        />
                                    </article>
                                }
                            />
                            <Show when=move || busy.get()>
                                <div class="onyx-chat__thinking">
                                    <Icon icon=LuLoaderCircle width="15px" height="15px" attr:class="spin" />
                                    "Onyx is thinking…"
                                </div>
                            </Show>
                        </div>
                    </Show>
                </div>

                <div class="onyx-chat__dock">
                    <Show when=move || error.get().is_some()>
                        <button class="onyx-chat__error" on:click=move |_| error.set(None)>
                            {move || error.get().unwrap_or_default()}
                        </button>
                    </Show>
                    <form
                        class="onyx-chat__composer"
                        on:submit=move |event: SubmitEvent| {
                            event.prevent_default();
                            submit.run(());
                        }
                    >
                        <textarea
                            rows="1"
                            prop:value=move || draft.get()
                            on:input=move |event| draft.set(event_target_value(&event))
                            placeholder=move || if mode.get() == ChatMode::Chat {
                                "Message Onyx…".to_owned()
                            } else {
                                format!("Describe the {} you want…", mode.get().as_str())
                            }
                            on:keydown=move |event: KeyboardEvent| {
                                if event.key() == "Enter"
                                    && !event.shift_key()
                                    && !event.is_composing()
                                    && !event.repeat()
                                {
                                    event.prevent_default();
                                    submit.run(());
                                }
                            }
                        />
                        <div class="onyx-chat__composer-row">
                            <div class="onyx-chat__modes">
                                <For
                                    each=move || ChatMode::ALL
                                    key=|item| *item
                                    children=move |item| view! {
                                        <button
                                            type="button"
                                            class:active=move || mode.get() == item
                                            on:click=move |_| choose_mode.run(item)
                                        >
                                            <Icon icon=mode_icon(item) width="14px" height="14px" />
                                            {item.as_str()}
                                        </button>
                                    }
                                />
                                <button
                                    type="button"
                                    class:active=move || web_search.get()
                                    disabled=move || mode.get() != ChatMode::Chat
                                    on:click=move |_| web_search.update(|value| *value = !*value)
                                    title="Let the selected provider search the web"
                                >
                                    <Icon icon=LuSearch width="14px" height="14px" />
                                    "web"
                                </button>
                            </div>
                            <div class="onyx-chat__composer-actions">
                                <button
                                    type="button"
                                    class="onyx-chat__model-trigger"
                                    on:click=move |_| model_menu_open.update(|value| *value = !*value)
                                >
                                    <ProviderBadge brand=Signal::derive(move || brand.get()) small=true />
                                    <span>{move || selected_model_name.get()}</span>
                                    <Icon icon=LuChevronDown width="12px" height="12px" />
                                </button>
                                <button
                                    class="onyx-chat__send"
                                    type="submit"
                                    disabled=move || draft.read().trim().is_empty() || busy.get()
                                >
                                    {move || if busy.get() {
                                        view! { <Icon icon=LuLoaderCircle width="15px" height="15px" attr:class="spin" /> }.into_any()
                                    } else {
                                        view! { <Icon icon=LuSend width="15px" height="15px" /> }.into_any()
                                    }}
                                </button>
                            </div>
                        </div>
                    </form>

                    <Show when=move || model_menu_open.get()>
                        <div class="onyx-chat__model-menu">
                            <label>
                                <Icon icon=LuSearch width="14px" height="14px" />
                                <input
                                    autofocus=true
                                    prop:value=move || model_query.get()
                                    on:input=move |event| model_query.set(event_target_value(&event))
                                    placeholder="Search models"
                                />
                            </label>
                            <div class="onyx-chat__provider-tabs">
                                <For
                                    each=move || filtered_brands.get()
                                    key=|item| item.id
                                    children=move |item| view! {
                                        <button class:active=move || brand.get() == item.id on:click=move |_| choose_brand.run(item.id)>
                                            <ProviderBadge brand=Signal::derive(move || item.id) small=true />
                                            {item.name}
                                        </button>
                                    }
                                />
                            </div>
                            <div class="onyx-chat__model-list">
                                <Show
                                    when=move || !filtered_models.read().is_empty()
                                    fallback=move || view! { <p>"No compatible models were reported for this mode."</p> }
                                >
                                    <For
                                        each=move || filtered_models.get()
                                        key=|model| model.id.clone()
                                        children=move |model| {
                                            let choose_id = model.id.clone();
                                            let selected_check_id = model.id.clone();
                                            let favorite_id = model.id.clone();
                                            let favorite_check_id = model.id.clone();
                                            let description =
                                                model.description.clone().unwrap_or_else(|| model.id.clone());
                                            view! {
                                                <button
                                                    class:selected=move || selected_model.get() == selected_check_id
                                                    on:click=move |_| {
                                                        selected_model.set(choose_id.clone());
                                                        model_menu_open.set(false);
                                                    }
                                                >
                                                    <ProviderBadge brand=Signal::derive(move || brand.get()) small=true />
                                                    <span>
                                                        <strong>{model.name}</strong>
                                                        <small>{description}</small>
                                                    </span>
                                                    <i
                                                        role="button"
                                                        tabindex="0"
                                                        on:click=move |event| {
                                                            event.stop_propagation();
                                                            toggle_favorite.run(favorite_id.clone());
                                                        }
                                                    >
                                                        <Icon
                                                            icon=LuStar
                                                            width="14px"
                                                            height="14px"
                                                            attr:class=move || if favorites.read().contains(&favorite_check_id) { "is-favorite" } else { "" }
                                                        />
                                                    </i>
                                                </button>
                                            }
                                        }
                                    />
                                </Show>
                            </div>
                        </div>
                    </Show>
                    <p class="onyx-chat__disclaimer">"Models can make mistakes. Verify important information."</p>
                </div>
                <Show when=move || web_app.get().is_some()>
                    <For
                        each=move || web_app.get()
                        key=|app| app.id
                        children=move |app| view! {
                            <div class="onyx-chat__internal-browser">
                                <InternalBrowser
                                    label=format!("internal-browser-chat-{}", app.id)
                                    initial_url=app.url.to_owned()
                                    show_toolbar=true
                                    on_error=Callback::new(move |cause| error.set(Some(cause)))
                                />
                            </div>
                        }
                    />
                </Show>
            </main>
        </section>
    }
}
