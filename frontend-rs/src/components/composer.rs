use std::collections::HashMap;

use icondata::{
    LuBot, LuBrainCircuit, LuCheck, LuChevronDown, LuCornerDownLeft, LuListPlus, LuLoaderCircle,
    LuLock, LuLockOpen, LuPenLine, LuPencilRuler, LuPlus, LuShieldAlert, LuStar, LuTrash2, LuZap,
};
use leptos::ev::{KeyboardEvent, SubmitEvent};
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen_futures::spawn_local;

use crate::{
    bridge,
    commands::{self, CommandAction},
    model::{
        AccessMode, ApprovalRequest, InteractionMode, ProviderBrand, ProviderId,
        ProviderModelOption, ProviderStatus, ReasoningEffort, SpeedMode,
    },
    storage,
};

use super::ProviderBadge;

fn access_name(provider: ProviderId, mode: AccessMode) -> &'static str {
    match (provider, mode) {
        (ProviderId::Kimi, AccessMode::ApprovalRequired) => "Default",
        (ProviderId::Kimi, AccessMode::AutoAcceptEdits) => "YOLO",
        (ProviderId::Kimi, AccessMode::FullAccess) => "Auto",
        (_, AccessMode::ApprovalRequired) => "Ask",
        (_, AccessMode::AutoAcceptEdits) => "Auto edits",
        (_, AccessMode::FullAccess) => "Full access",
    }
}

fn access_description(provider: ProviderId, mode: AccessMode) -> &'static str {
    match (provider, mode) {
        (ProviderId::Kimi, AccessMode::ApprovalRequired) => {
            "Kimi Default: manual approvals; tools execute normally"
        }
        (ProviderId::Kimi, AccessMode::AutoAcceptEdits) => {
            "Kimi YOLO: auto-approve tool actions; the agent may still ask questions"
        }
        (ProviderId::Kimi, AccessMode::FullAccess) => {
            "Kimi Auto: fully autonomous; the agent decides without asking"
        }
        (_, AccessMode::ApprovalRequired) => "Ask before edits, commands, and external actions",
        (_, AccessMode::AutoAcceptEdits) => {
            "Apply workspace edits automatically; ask for other actions"
        }
        (_, AccessMode::FullAccess) => "Allow provider actions without interactive approval",
    }
}

#[component]
pub fn Composer(
    provider: Signal<ProviderId>,
    brand: Signal<ProviderBrand>,
    model: Signal<Option<String>>,
    reasoning: Signal<Option<ReasoningEffort>>,
    speed_mode: Signal<SpeedMode>,
    interaction_mode: Signal<InteractionMode>,
    access_mode: Signal<AccessMode>,
    workspace: Signal<String>,
    providers: Signal<Vec<ProviderStatus>>,
    models: Signal<Vec<ProviderModelOption>>,
    locked: Signal<bool>,
    running: Signal<bool>,
    approval: Signal<Option<ApprovalRequest>>,
    approval_busy: Signal<bool>,
    queued_messages: Signal<Vec<String>>,
    /// The draft lives with the caller so a re-render of the surrounding page
    /// can never discard what the user is typing.
    value: Signal<String>,
    on_value: Callback<String>,
    steerable: Signal<bool>,
    #[prop(default = false)] hero: bool,
    #[prop(default = false)] autofocus: bool,
    on_brand: Callback<ProviderBrand>,
    on_model: Callback<String>,
    on_reasoning: Callback<ReasoningEffort>,
    on_speed_mode: Callback<SpeedMode>,
    on_interaction_mode: Callback<InteractionMode>,
    on_access_mode: Callback<AccessMode>,
    on_submit: Callback<String>,
    on_queue: Callback<String>,
    on_steer: Callback<String>,
    on_steer_queued: Callback<()>,
    on_drop_queued: Callback<()>,
    /// Commands the shell owns rather than the composer: rename, new session,
    /// settings, and the read-outs that live in the status bar.
    on_command: Callback<CommandAction>,
    on_cancel: Callback<()>,
    on_approval: Callback<(bool, bool)>,
    on_error: Callback<String>,
) -> impl IntoView {
    let content = value;
    let submitting = RwSignal::new(false);
    let responding_approval = RwSignal::new(false);

    let selected_model = Signal::derive(move || {
        let options = models.get();
        let selected = model.get();
        options
            .iter()
            .find(|item| Some(item.id.as_str()) == selected.as_deref())
            .or_else(|| options.first())
            .cloned()
    });

    // T3-style picker structure over a native select: starred models surface
    // in a Favorites group, superseded models fold behind Legacy models.
    let favorites = RwSignal::new(storage::read_json::<HashMap<ProviderBrand, Vec<String>>>(
        storage::FAVORITE_MODELS_KEY,
        HashMap::new(),
    ));
    let brand_favorites = Signal::derive(move || {
        favorites
            .get()
            .get(&brand.get())
            .cloned()
            .unwrap_or_default()
    });
    let favorite_models = Signal::derive(move || {
        let ids = brand_favorites.get();
        models
            .get()
            .into_iter()
            .filter(|item| ids.contains(&item.id))
            .collect::<Vec<_>>()
    });
    let primary_models = Signal::derive(move || {
        models
            .get()
            .into_iter()
            .filter(|item| !item.legacy)
            .collect::<Vec<_>>()
    });
    let legacy_models = Signal::derive(move || {
        models
            .get()
            .into_iter()
            .filter(|item| item.legacy)
            .collect::<Vec<_>>()
    });
    let is_favorite = Signal::derive(move || {
        selected_model
            .get()
            .is_some_and(|item| brand_favorites.get().contains(&item.id))
    });
    // Un-starring removes the Favorites <option> the browser had selected,
    // which silently moves selection without firing change; re-assert the
    // real value whenever the option set shifts.
    let model_select = NodeRef::<leptos::html::Select>::new();
    Effect::new(move |_| {
        favorites.track();
        let value = model.get().unwrap_or_default();
        if let Some(select) = model_select.get_untracked() {
            select.set_value(&value);
        }
    });
    let toggle_favorite = move |_| {
        let Some(selected) = selected_model.get_untracked() else {
            return;
        };
        favorites.update(|map| {
            let list = map.entry(brand.get_untracked()).or_default();
            if let Some(position) = list.iter().position(|id| *id == selected.id) {
                list.remove(position);
            } else {
                list.push(selected.id);
            }
        });
        storage::write_json(storage::FAVORITE_MODELS_KEY, &favorites.get_untracked());
    };
    let can_steer =
        Signal::derive(move || running.get() && steerable.get() && approval.get().is_none());
    let next_queued = Signal::derive(move || {
        queued_messages
            .read()
            .first()
            .map(|message| message.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
    });
    let provider_available = Signal::derive(move || {
        providers
            .read()
            .iter()
            .find(|status| status.id == provider.get())
            .is_none_or(|status| status.available)
    });
    let send_disabled = Signal::derive(move || {
        content.get().trim().is_empty()
            || submitting.get()
            || approval.get().is_some()
            || !provider_available.get()
            || model.get().as_deref().is_none_or(str::is_empty)
    });
    let modifier_label = if web_sys::window()
        .and_then(|window| window.navigator().platform().ok())
        .is_some_and(|platform| platform.to_ascii_lowercase().contains("mac"))
    {
        "⌘"
    } else {
        "Ctrl+"
    };
    let placeholder = Signal::derive(move || {
        if running.get() && steerable.get() {
            format!("Queue a follow-up · {modifier_label}↵ steers the running turn…")
        } else if running.get() {
            "Queue a follow-up for this agent…".to_owned()
        } else if workspace.get().trim().is_empty() {
            "Choose a project, then tell Onyx what to build…".to_owned()
        } else {
            "Tell Onyx what to build…".to_owned()
        }
    });

    let submit = Callback::new(move |_: ()| {
        if send_disabled.get() {
            return;
        }
        let value = content.get().trim().to_owned();
        submitting.set(true);
        if running.get() {
            on_queue.run(value);
        } else {
            on_submit.run(value);
        }
        on_value.run(String::new());
        submitting.set(false);
    });

    // Steering skips the queue: the text reaches the turn that is already
    // running instead of waiting for the next one.
    let steer = Callback::new(move |_: ()| {
        if send_disabled.get() || !can_steer.get() {
            return;
        }
        let value = content.get().trim().to_owned();
        submitting.set(true);
        on_steer.run(value);
        on_value.run(String::new());
        submitting.set(false);
    });

    let attach = Callback::new(move |_: ()| {
        // Attaching stays available during a turn, because the prompt being
        // written is a follow-up that has not been sent yet.
        if submitting.get() {
            return;
        }
        spawn_local(async move {
            match bridge::choose_files().await {
                Ok(paths) if !paths.is_empty() => {
                    let references = paths
                        .into_iter()
                        .map(|path| {
                            if path.contains(' ') {
                                format!("@\"{path}\"")
                            } else {
                                format!("@{path}")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    let mut next = content.get_untracked();
                    if !next.trim().is_empty() {
                        next.push('\n');
                    }
                    next.push_str(&references);
                    on_value.run(next);
                }
                Ok(_) => {}
                Err(message) => on_error.run(message),
            }
        });
    });

    // The palette follows a `/` prompt the way a terminal does, and the `+`
    // button pins it open so it can also be browsed without typing.
    let palette_open = RwSignal::new(false);
    let palette_pinned = RwSignal::new(false);
    let palette_values = RwSignal::new(None::<CommandAction>);
    let close_palette = Callback::new(move |_: ()| {
        palette_open.set(false);
        palette_pinned.set(false);
        palette_values.set(None);
    });
    Effect::new(move |_| {
        if commands::is_command_query(&content.get()) {
            palette_open.set(true);
        } else if !palette_pinned.get_untracked() {
            palette_open.set(false);
            palette_values.set(None);
        }
    });
    let matching_commands = Signal::derive(move || {
        let all = commands::commands_for(provider.get());
        let value = content.get();
        let query = if commands::is_command_query(&value) {
            value
        } else {
            String::new()
        };
        commands::filter(&all, &query)
    });

    let run_command = Callback::new(move |action: CommandAction| {
        if action.opens_values() {
            palette_values.set(Some(action));
            return;
        }
        // The typed command token is an instruction, never part of the prompt.
        if commands::is_command_query(&content.get_untracked()) {
            on_value.run(String::new());
        }
        close_palette.run(());
        match action {
            CommandAction::AttachFiles => attach.run(()),
            CommandAction::ToggleMode => {
                on_interaction_mode.run(
                    if interaction_mode.get_untracked() == InteractionMode::Plan {
                        InteractionMode::Build
                    } else {
                        InteractionMode::Plan
                    },
                );
            }
            CommandAction::Prompt(text) => {
                if running.get_untracked() {
                    on_queue.run(text.to_owned());
                } else {
                    on_submit.run(text.to_owned());
                }
            }
            other => on_command.run(other),
        }
    });

    view! {
        <div
            class="zai-composer t3-composer"
            class:zai-composer--hero=hero
            class:zai-composer--docked=!hero
            class:zai-composer--running=move || running.get()
            class:zai-composer--approval=move || approval.get().is_some()
            data-component="onyx-composer"
            data-layout=if hero { "hero" } else { "docked" }
            data-provider=move || provider.get().as_str()
        >
            <Show when=move || palette_open.get()>
                <button
                    type="button"
                    class="zai-command-palette__backdrop"
                    aria-label="Close commands"
                    on:click=move |_| close_palette.run(())
                />
                <div class="zai-command-palette" role="listbox" aria-label="Commands">
                    {move || match palette_values.get() {
                        Some(CommandAction::PickModel) => view! {
                            <For
                                each=move || models.get()
                                key=|item| item.id.clone()
                                children=move |item| {
                                    let id = item.id.clone();
                                    let selected = model.get().as_deref() == Some(item.id.as_str());
                                    view! {
                                        <button
                                            type="button"
                                            role="option"
                                            aria-selected=selected
                                            data-selected=if selected { "true" } else { "false" }
                                            on:click=move |_| {
                                                on_model.run(id.clone());
                                                close_palette.run(());
                                            }
                                        >
                                            <strong>{item.name}</strong>
                                            <small>{item.description.unwrap_or_default()}</small>
                                        </button>
                                    }
                                }
                            />
                        }.into_any(),
                        Some(CommandAction::PickReasoning) => view! {
                            <For
                                each=move || selected_model
                                    .get()
                                    .map(|item| item.reasoning)
                                    .unwrap_or_default()
                                key=|item| *item
                                children=move |item| {
                                    let selected = reasoning.get() == Some(item);
                                    view! {
                                        <button
                                            type="button"
                                            role="option"
                                            aria-selected=selected
                                            data-selected=if selected { "true" } else { "false" }
                                            on:click=move |_| {
                                                on_reasoning.run(item);
                                                close_palette.run(());
                                            }
                                        >
                                            <strong>{item.display_name()}</strong>
                                        </button>
                                    }
                                }
                            />
                        }.into_any(),
                        Some(CommandAction::PickAccess) => view! {
                            <For
                                each=move || [
                                    AccessMode::ApprovalRequired,
                                    AccessMode::AutoAcceptEdits,
                                    AccessMode::FullAccess,
                                ]
                                key=|item| item.as_str()
                                children=move |item| {
                                    let selected = access_mode.get() == item;
                                    view! {
                                        <button
                                            type="button"
                                            role="option"
                                            aria-selected=selected
                                            data-selected=if selected { "true" } else { "false" }
                                            on:click=move |_| {
                                                on_access_mode.run(item);
                                                close_palette.run(());
                                            }
                                        >
                                            <strong>{access_name(provider.get(), item)}</strong>
                                            <small>{access_description(provider.get(), item)}</small>
                                        </button>
                                    }
                                }
                            />
                        }.into_any(),
                        Some(CommandAction::PickSpeed) => view! {
                            <For
                                each=move || [SpeedMode::Standard, SpeedMode::Fast]
                                key=|item| item.as_str()
                                children=move |item| {
                                    let selected = speed_mode.get() == item;
                                    view! {
                                        <button
                                            type="button"
                                            role="option"
                                            aria-selected=selected
                                            data-selected=if selected { "true" } else { "false" }
                                            on:click=move |_| {
                                                on_speed_mode.run(item);
                                                close_palette.run(());
                                            }
                                        >
                                            <strong>{if item == SpeedMode::Fast { "Fast" } else { "Standard" }}</strong>
                                        </button>
                                    }
                                }
                            />
                        }.into_any(),
                        _ => view! {
                            <Show
                                when=move || !matching_commands.read().is_empty()
                                fallback=move || view! {
                                    <p class="zai-command-palette__empty">"No matching command"</p>
                                }
                            >
                                <For
                                    each=move || matching_commands.get()
                                    key=|command| command.name
                                    children=move |command| view! {
                                        <button
                                            type="button"
                                            role="option"
                                            aria-selected="false"
                                            on:click=move |_| run_command.run(command.action)
                                        >
                                            <strong>{command.name}</strong>
                                            <small>{command.summary}</small>
                                        </button>
                                    }
                                />
                            </Show>
                        }.into_any(),
                    }}
                </div>
            </Show>

            <Show when=move || !hero && !queued_messages.read().is_empty()>
                <div
                    class="zai-session-statusbar zai-queue-bar"
                    data-slot="queued-follow-up"
                    role="status"
                >
                    <span class="zai-queue-bar__label">
                        <Icon icon=LuListPlus width="14px" height="14px" />
                        {move || {
                            let count = queued_messages.read().len();
                            if count > 1 {
                                format!("Queued · {count}")
                            } else {
                                "Queued".to_owned()
                            }
                        }}
                    </span>
                    <span class="zai-workspace-divider">"/"</span>
                    <span
                        class="zai-queue-bar__preview"
                        title=move || next_queued.get()
                    >
                        {move || next_queued.get()}
                    </span>
                    <span class="zai-workspace-divider">"/"</span>
                    <Show when=move || can_steer.get()>
                        <button
                            type="button"
                            class="zai-queue-bar__action"
                            on:click=move |_| on_steer_queued.run(())
                            title="Send this message into the running turn now"
                        >
                            <Icon icon=LuCornerDownLeft width="14px" height="14px" />
                            "Steer now"
                        </button>
                        <span class="zai-workspace-divider">"/"</span>
                    </Show>
                    <button
                        type="button"
                        class="zai-queue-bar__action"
                        on:click=move |_| on_drop_queued.run(())
                        title="Remove the next queued message"
                    >
                        <Icon icon=LuTrash2 width="14px" height="14px" />
                        "Remove"
                    </button>
                </div>
            </Show>
                    <form
                        class="zai-composer__frame"
                        data-component="prompt-input-v2"
                        on:submit=move |event: SubmitEvent| {
                            event.prevent_default();
                            submit.run(());
                        }
                    >
                        <div class="zai-composer__surface">
                            <div class="zai-composer__editor-region">
                                <textarea
                                    class="zai-composer__editor"
                                    rows="1"
                                    prop:value=move || content.get()
                                    aria-label="Prompt"
                                    autocomplete="off"
                                    spellcheck="true"
                                    autofocus=autofocus
                                    placeholder=move || placeholder.get()
                                    on:input=move |event| on_value.run(event_target_value(&event))
                                    on:keydown=move |event: KeyboardEvent| {
                                        if event.key() == "Escape" {
                                            if palette_open.get() {
                                                event.prevent_default();
                                                close_palette.run(());
                                                return;
                                            }
                                            if running.get() {
                                                event.prevent_default();
                                                on_cancel.run(());
                                            }
                                            return;
                                        }
                                        if event.key() != "Enter"
                                            || event.shift_key()
                                            || event.is_composing()
                                            || event.repeat()
                                        {
                                            return;
                                        }
                                        event.prevent_default();
                                        // A command prompt runs the best match, the way
                                        // pressing Enter does in the CLIs themselves.
                                        if palette_open.get()
                                            && commands::is_command_query(&content.get())
                                            && let Some(command) = matching_commands.read().first()
                                        {
                                            run_command.run(command.action);
                                            return;
                                        }
                                        if (event.meta_key() || event.ctrl_key()) && can_steer.get() {
                                            steer.run(());
                                        } else {
                                            submit.run(());
                                        }
                                    }
                                />
                            </div>

                            <div class="zai-composer__footer">
                                <div class="zai-composer__controls">
                                    <button
                                        type="button"
                                        class="zai-composer__control zai-composer__attach"
                                        data-active=move || if palette_open.get() { "true" } else { "false" }
                                        on:click=move |_| {
                                            if palette_open.get_untracked() {
                                                close_palette.run(());
                                            } else {
                                                palette_values.set(None);
                                                palette_pinned.set(true);
                                                palette_open.set(true);
                                            }
                                        }
                                        aria-label="Attach files or run a command"
                                        aria-expanded=move || palette_open.get()
                                        title="Attach files or run a command (/)"
                                    >
                                        <Icon icon=LuPlus width="18px" height="18px" />
                                    </button>

                                    <label
                                        class="zai-composer__control zai-composer__select-control zai-composer__provider-tile"
                                        title=move || brand.get().display_name()
                                    >
                                        <ProviderBadge brand=brand small=true />
                                        <span class="zai-composer__control-label">
                                            {move || brand.get().display_name()}
                                        </span>
                                        <Icon icon=LuChevronDown width="12px" height="12px" />
                                        <select
                                            class="zai-composer__native-select"
                                            aria-label="Provider"
                                            prop:value=move || brand.get().as_str()
                                            disabled=move || locked.get()
                                            on:change=move |event| {
                                                if let Some(next) =
                                                    ProviderBrand::from_str(&event_target_value(&event))
                                                {
                                                    on_brand.run(next);
                                                }
                                            }
                                        >
                                            <For
                                                each=move || crate::catalog::PROVIDER_BRANDS
                                                key=|item| item.id
                                                children=move |item| {
                                                    let available = providers
                                                        .read()
                                                        .iter()
                                                        .find(|status| status.id == item.runtime)
                                                        .is_some_and(|status| status.available);
                                                    view! {
                                                        <option value=item.id.as_str() disabled=!available>
                                                            {format!(
                                                                "{}{}",
                                                                item.name,
                                                                if available { "" } else { " — unavailable" },
                                                            )}
                                                        </option>
                                                    }
                                                }
                                            />
                                        </select>
                                    </label>

                                    <label
                                        class="zai-composer__control zai-composer__select-control zai-composer__model-tile"
                                        title=move || selected_model
                                            .get()
                                            .and_then(|item| item.description.or(Some(item.name)))
                                            .unwrap_or_default()
                                    >
                                        <span class="zai-composer__control-label">
                                            {move || selected_model
                                                .get()
                                                .map(|item| item.name)
                                                .unwrap_or_else(|| "Choose model".to_owned())}
                                        </span>
                                        <Icon icon=LuChevronDown width="12px" height="12px" />
                                        <select
                                            node_ref=model_select
                                            class="zai-composer__native-select"
                                            aria-label="Model"
                                            prop:value=move || model.get().unwrap_or_default()
                                            disabled=move || locked.get() || models.read().is_empty()
                                            on:change=move |event| on_model.run(event_target_value(&event))
                                        >
                                            <Show when=move || !favorite_models.read().is_empty()>
                                                <optgroup label="Favorites">
                                                    <For
                                                        each=move || favorite_models.get()
                                                        key=|item| item.id.clone()
                                                        children=|item| view! {
                                                            <option value=item.id>{item.name}</option>
                                                        }
                                                    />
                                                </optgroup>
                                            </Show>
                                            <For
                                                each=move || primary_models.get()
                                                key=|item| item.id.clone()
                                                children=|item| view! {
                                                    <option value=item.id>{item.name}</option>
                                                }
                                            />
                                            <Show when=move || !legacy_models.read().is_empty()>
                                                <optgroup label="Legacy models">
                                                    <For
                                                        each=move || legacy_models.get()
                                                        key=|item| item.id.clone()
                                                        children=|item| view! {
                                                            <option value=item.id>{item.name}</option>
                                                        }
                                                    />
                                                </optgroup>
                                            </Show>
                                        </select>
                                    </label>

                                    <button
                                        type="button"
                                        class="zai-composer__control zai-composer__control--icon zai-composer__favorite"
                                        data-active=move || if is_favorite.get() { "true" } else { "false" }
                                        title=move || if is_favorite.get() {
                                            "Remove this model from favorites"
                                        } else {
                                            "Add this model to favorites"
                                        }
                                        aria-pressed=move || is_favorite.get()
                                        disabled=move || locked.get() || selected_model.get().is_none()
                                        on:click=toggle_favorite
                                    >
                                        <Icon icon=LuStar width="14px" height="14px" />
                                    </button>

                                    <Show when=move || selected_model.get().is_some_and(|item| !item.reasoning.is_empty())>
                                        <label
                                            class="zai-composer__control zai-composer__select-control"
                                            title="Reasoning effort advertised for this model"
                                        >
                                            <Icon icon=LuBrainCircuit width="15px" height="15px" />
                                            <span>{move || reasoning
                                                .get()
                                                .or_else(|| selected_model.get().and_then(|item| item.default_reasoning))
                                                .unwrap_or(ReasoningEffort::Medium)
                                                .display_name()}</span>
                                            <Icon icon=LuChevronDown width="12px" height="12px" />
                                            <select
                                                class="zai-composer__native-select"
                                                aria-label="Reasoning"
                                                prop:value=move || reasoning
                                                    .get()
                                                    .unwrap_or(ReasoningEffort::Medium)
                                                    .as_str()
                                                disabled=move || locked.get()
                                                on:change=move |event| {
                                                    if let Some(next) =
                                                        ReasoningEffort::from_str(&event_target_value(&event))
                                                    {
                                                        on_reasoning.run(next);
                                                    }
                                                }
                                            >
                                                <For
                                                    each=move || selected_model
                                                        .get()
                                                        .map(|item| item.reasoning)
                                                        .unwrap_or_default()
                                                    key=|item| *item
                                                    children=|item| view! {
                                                        <option value=item.as_str()>{item.display_name()}</option>
                                                    }
                                                />
                                            </select>
                                        </label>
                                    </Show>

                                    <Show when=move || selected_model.get().is_some_and(|item| item.speeds.len() > 1)>
                                        <label
                                            class="zai-composer__control zai-composer__select-control zai-composer__control--icon"
                                            data-active=move || if speed_mode.get() == SpeedMode::Fast { "true" } else { "false" }
                                            title=move || if speed_mode.get() == SpeedMode::Fast {
                                                "Service tier: Fast"
                                            } else {
                                                "Service tier: Standard"
                                            }
                                        >
                                            <Icon icon=LuZap width="15px" height="15px" />
                                            <select
                                                class="zai-composer__native-select"
                                                aria-label="Speed"
                                                prop:value=move || speed_mode.get().as_str()
                                                disabled=move || locked.get()
                                                on:change=move |event| {
                                                    on_speed_mode.run(if event_target_value(&event) == "fast" {
                                                        SpeedMode::Fast
                                                    } else {
                                                        SpeedMode::Standard
                                                    });
                                                }
                                            >
                                                <option value="standard">"Standard"</option>
                                                <option value="fast">"Fast"</option>
                                            </select>
                                        </label>
                                    </Show>

                                    <button
                                        type="button"
                                        class="zai-composer__control zai-composer__mode-toggle zai-composer__control--icon"
                                        class:active=move || interaction_mode.get() == InteractionMode::Plan
                                        disabled=move || locked.get()
                                        aria-pressed=move || interaction_mode.get() == InteractionMode::Plan
                                        aria-label=move || if interaction_mode.get() == InteractionMode::Plan { "Plan mode" } else { "Build mode" }
                                        title=move || if interaction_mode.get() == InteractionMode::Plan {
                                            "Plan mode — research and propose, no changes"
                                        } else {
                                            "Build mode — make the changes"
                                        }
                                        on:click=move |_| {
                                            on_interaction_mode.run(if interaction_mode.get() == InteractionMode::Plan {
                                                InteractionMode::Build
                                            } else {
                                                InteractionMode::Plan
                                            });
                                        }
                                    >
                                        {move || if interaction_mode.get() == InteractionMode::Plan {
                                            view! { <Icon icon=LuPencilRuler width="15px" height="15px" /> }.into_any()
                                        } else {
                                            view! { <Icon icon=LuBot width="15px" height="15px" /> }.into_any()
                                        }}
                                    </button>

                                    <label
                                        class="zai-composer__control zai-composer__select-control zai-composer__control--icon"
                                        data-access=move || access_mode.get().as_str()
                                        title=move || format!(
                                            "{} — {}",
                                            access_name(provider.get(), access_mode.get()),
                                            access_description(provider.get(), access_mode.get()),
                                        )
                                    >
                                        {move || match access_mode.get() {
                                            AccessMode::ApprovalRequired => view! { <Icon icon=LuLock width="15px" height="15px" /> }.into_any(),
                                            AccessMode::AutoAcceptEdits => view! { <Icon icon=LuPenLine width="15px" height="15px" /> }.into_any(),
                                            AccessMode::FullAccess => view! { <Icon icon=LuLockOpen width="15px" height="15px" /> }.into_any(),
                                        }}
                                        <select
                                            class="zai-composer__native-select"
                                            aria-label="Access"
                                            prop:value=move || access_mode.get().as_str()
                                            disabled=move || locked.get()
                                            on:change=move |event| {
                                                on_access_mode.run(match event_target_value(&event).as_str() {
                                                    "auto_accept_edits" => AccessMode::AutoAcceptEdits,
                                                    "full_access" => AccessMode::FullAccess,
                                                    _ => AccessMode::ApprovalRequired,
                                                });
                                            }
                                        >
                                            <option value="approval_required">
                                                {move || if provider.get() == ProviderId::Kimi { "Default" } else { "Ask" }}
                                            </option>
                                            <option value="auto_accept_edits">
                                                {move || if provider.get() == ProviderId::Kimi { "YOLO" } else { "Auto edits" }}
                                            </option>
                                            <option value="full_access">
                                                {move || if provider.get() == ProviderId::Kimi { "Auto" } else { "Full access" }}
                                            </option>
                                        </select>
                                    </label>
                                </div>

                                <div class="zai-composer__primary-actions">
                                    <Show
                                        when=move || running.get()
                                        fallback=move || view! {
                                            <button
                                                type="submit"
                                                class="zai-composer__submit"
                                                disabled=move || send_disabled.get()
                                                aria-label="Send message"
                                                title="Send (Enter)"
                                            >
                                                <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
                                                    <path d="M7 11.5V2.5M7 2.5L3 6.5M7 2.5L11 6.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
                                                </svg>
                                            </button>
                                        }
                                    >
                                        // Sending during a turn needs no separate control:
                                        // the message lands in the bar above the composer.
                                        <button
                                            type="submit"
                                            class="zai-composer__submit"
                                            disabled=move || send_disabled.get()
                                            aria-label="Send message"
                                            title=move || if can_steer.get() {
                                                format!("Send (Enter) · steer now ({modifier_label}↵)")
                                            } else {
                                                "Send (Enter)".to_owned()
                                            }
                                        >
                                            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
                                                <path d="M7 11.5V2.5M7 2.5L3 6.5M7 2.5L11 6.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
                                            </svg>
                                        </button>
                                        <button
                                            type="button"
                                            class="zai-composer__submit zai-composer__stop"
                                            on:click=move |_| on_cancel.run(())
                                            aria-label="Stop generation"
                                            title="Stop (Esc)"
                                        >
                                            <svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
                                                <rect x="2" y="2" width="8" height="8" rx="1.5" />
                                            </svg>
                                        </button>
                                    </Show>
                                </div>
                            </div>
                        </div>
                    </form>
                // The approval dock stacks above the composer (flex `order`)
                // instead of replacing it, so the draft and controls stay
                // visible and editable while a decision is pending.
                {move || approval.get().map(|request: ApprovalRequest| {
                    let id = request.id.clone();
                    view! {
                        <div class="zai-composer__frame zai-composer__frame--permission">
                            <div
                                class="zai-composer__permission"
                                role="group"
                                aria-labelledby=format!("onyx-approval-{id}")
                                aria-busy=move || approval_busy.get() || responding_approval.get()
                            >
                                <div class="zai-composer__permission-body">
                                    <div class="zai-composer__permission-header">
                                        <span class="zai-composer__permission-icon" aria-hidden="true">
                                            <Icon icon=LuShieldAlert width="17px" height="17px" />
                                        </span>
                                        <div>
                                            <span class="zai-composer__eyebrow">"Permission required"</span>
                                            <strong id=format!("onyx-approval-{id}")>{request.title}</strong>
                                        </div>
                                    </div>
                                    <pre class="zai-composer__permission-detail">{request.detail}</pre>
                                </div>
                                <div class="zai-composer__permission-tray">
                                    <span class="zai-composer__permission-risk">{request.risk}</span>
                                    <div class="zai-composer__permission-actions">
                                        <button
                                            type="button"
                                            class="zai-composer__permission-button zai-composer__permission-button--ghost"
                                            disabled=move || approval_busy.get() || responding_approval.get()
                                            title="Stop the running turn entirely"
                                            on:click=move |_| on_cancel.run(())
                                        >
                                            "Cancel turn"
                                        </button>
                                        <button
                                            type="button"
                                            class="zai-composer__permission-button"
                                            disabled=move || approval_busy.get() || responding_approval.get()
                                            on:click=move |_| {
                                                responding_approval.set(true);
                                                on_approval.run((false, false));
                                                responding_approval.set(false);
                                            }
                                        >
                                            "Deny"
                                        </button>
                                        <button
                                            type="button"
                                            class="zai-composer__permission-button"
                                            disabled=move || approval_busy.get() || responding_approval.get()
                                            on:click=move |_| {
                                                responding_approval.set(true);
                                                on_approval.run((true, true));
                                                responding_approval.set(false);
                                            }
                                        >
                                            "Allow for session"
                                        </button>
                                        <button
                                            type="button"
                                            class="zai-composer__permission-button zai-composer__permission-button--allow"
                                            disabled=move || approval_busy.get() || responding_approval.get()
                                            on:click=move |_| {
                                                responding_approval.set(true);
                                                on_approval.run((true, false));
                                                responding_approval.set(false);
                                            }
                                        >
                                            <Show
                                                when=move || approval_busy.get() || responding_approval.get()
                                                fallback=move || view! {
                                                    <Icon icon=LuCheck width="14px" height="14px" />
                                                    "Allow once"
                                                }
                                            >
                                                <span class="zai-composer__spinner">
                                                    <Icon icon=LuLoaderCircle width="14px" height="14px" />
                                                </span>
                                                "Responding…"
                                            </Show>
                                        </button>
                                    </div>
                                </div>
                            </div>
                        </div>
                    }
                })}
        </div>
    }
}
