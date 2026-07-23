use icondata::{
    LuBot, LuBrainCircuit, LuCheck, LuChevronDown, LuLoaderCircle, LuLock, LuLockOpen, LuPenLine,
    LuPencilRuler, LuPlus, LuShieldAlert, LuZap,
};
use leptos::ev::{KeyboardEvent, SubmitEvent};
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen_futures::spawn_local;

use crate::{
    bridge,
    model::{
        AccessMode, ApprovalRequest, InteractionMode, ProviderBrand, ProviderId,
        ProviderModelOption, ProviderStatus, ReasoningEffort, SpeedMode,
    },
};

use super::ProviderBadge;

fn access_name(mode: AccessMode) -> &'static str {
    match mode {
        AccessMode::ApprovalRequired => "Ask",
        AccessMode::AutoAcceptEdits => "Auto edits",
        AccessMode::FullAccess => "Full access",
    }
}

fn access_description(mode: AccessMode) -> &'static str {
    match mode {
        AccessMode::ApprovalRequired => "Ask before edits, commands, and external actions",
        AccessMode::AutoAcceptEdits => "Apply workspace edits automatically; ask for other actions",
        AccessMode::FullAccess => "Allow provider actions without interactive approval",
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
    #[prop(default = false)] steerable: bool,
    #[prop(default = false)] hero: bool,
    #[prop(default = false)] autofocus: bool,
    on_brand: Callback<ProviderBrand>,
    on_model: Callback<String>,
    on_reasoning: Callback<ReasoningEffort>,
    on_speed_mode: Callback<SpeedMode>,
    on_interaction_mode: Callback<InteractionMode>,
    on_access_mode: Callback<AccessMode>,
    on_submit: Callback<String>,
    on_steer: Callback<String>,
    on_cancel: Callback<()>,
    on_approval: Callback<(bool, bool)>,
    on_error: Callback<String>,
) -> impl IntoView {
    let content = RwSignal::new(String::new());
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
    let can_steer = Signal::derive(move || running.get() && steerable && approval.get().is_none());
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
            || (running.get() && !can_steer.get())
            || approval.get().is_some()
            || !provider_available.get()
            || model.get().as_deref().is_none_or(str::is_empty)
    });
    let placeholder = Signal::derive(move || {
        if can_steer.get() {
            "Steer the agent — your message lands mid-run…".to_owned()
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
        if can_steer.get() {
            on_steer.run(value);
        } else {
            on_submit.run(value);
        }
        content.set(String::new());
        submitting.set(false);
    });

    let attach = Callback::new(move |_: ()| {
        if locked.get() || running.get() || submitting.get() {
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
                    content.update(|value| {
                        if !value.trim().is_empty() {
                            value.push('\n');
                        }
                        value.push_str(&references);
                    });
                }
                Ok(_) => {}
                Err(message) => on_error.run(message),
            }
        });
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
            <Show
                when=move || approval.get().is_some()
                fallback=move || view! {
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
                                    autofocus=autofocus || hero
                                    placeholder=move || placeholder.get()
                                    on:input=move |event| content.set(event_target_value(&event))
                                    on:keydown=move |event: KeyboardEvent| {
                                        if event.key() == "Escape" && running.get() {
                                            event.prevent_default();
                                            on_cancel.run(());
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
                                        submit.run(());
                                    }
                                />
                            </div>

                            <div class="zai-composer__footer">
                                <div class="zai-composer__controls">
                                    <button
                                        type="button"
                                        class="zai-composer__control zai-composer__attach"
                                        disabled=move || locked.get() || running.get()
                                        on:click=move |_| attach.run(())
                                        aria-label="Attach files"
                                        title="Attach files"
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
                                            class="zai-composer__native-select"
                                            aria-label="Model"
                                            prop:value=move || model.get().unwrap_or_default()
                                            disabled=move || locked.get() || models.read().is_empty()
                                            on:change=move |event| on_model.run(event_target_value(&event))
                                        >
                                            <For
                                                each=move || models.get()
                                                key=|item| item.id.clone()
                                                children=|item| view! {
                                                    <option value=item.id>{item.name}</option>
                                                }
                                            />
                                        </select>
                                    </label>

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
                                            class="zai-composer__control zai-composer__select-control"
                                            title="Model service tier"
                                        >
                                            <Icon icon=LuZap width="14px" height="14px" />
                                            <span>{move || if speed_mode.get() == SpeedMode::Fast { "Fast" } else { "Standard" }}</span>
                                            <Icon icon=LuChevronDown width="12px" height="12px" />
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
                                        class="zai-composer__control zai-composer__mode-toggle"
                                        class:active=move || interaction_mode.get() == InteractionMode::Plan
                                        disabled=move || locked.get()
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
                                        <span>{move || if interaction_mode.get() == InteractionMode::Plan { "Plan" } else { "Build" }}</span>
                                    </button>

                                    <label
                                        class="zai-composer__control zai-composer__select-control"
                                        title=move || access_description(access_mode.get())
                                    >
                                        {move || match access_mode.get() {
                                            AccessMode::ApprovalRequired => view! { <Icon icon=LuLock width="15px" height="15px" /> }.into_any(),
                                            AccessMode::AutoAcceptEdits => view! { <Icon icon=LuPenLine width="15px" height="15px" /> }.into_any(),
                                            AccessMode::FullAccess => view! { <Icon icon=LuLockOpen width="15px" height="15px" /> }.into_any(),
                                        }}
                                        <span>{move || access_name(access_mode.get())}</span>
                                        <Icon icon=LuChevronDown width="12px" height="12px" />
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
                                            <option value="approval_required">"Ask"</option>
                                            <option value="auto_accept_edits">"Auto edits"</option>
                                            <option value="full_access">"Full access"</option>
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
                                        <Show when=move || can_steer.get()>
                                            <button
                                                type="submit"
                                                class="zai-composer__submit zai-composer__steer"
                                                disabled=move || send_disabled.get()
                                                aria-label="Steer the running turn"
                                                title="Steer (Enter)"
                                            >
                                                <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
                                                    <path d="M2.5 7H11M11 7L7.5 3.5M11 7L7.5 10.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
                                                </svg>
                                            </button>
                                        </Show>
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
                }
            >
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
            </Show>
        </div>
    }
}
