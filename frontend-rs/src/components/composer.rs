use icondata::{
    LuBot, LuBrainCircuit, LuChevronDown, LuLock, LuLockOpen, LuPenLine, LuPencilRuler, LuPlus,
};
use leptos::ev::{KeyboardEvent, SubmitEvent};
use leptos::prelude::*;
use leptos_icons::Icon;

use crate::model::{
    AccessMode, InteractionMode, ProviderBrand, ProviderId, ProviderStatus, ReasoningEffort,
};

#[component]
pub fn Composer(
    provider: RwSignal<ProviderId>,
    reasoning: RwSignal<ReasoningEffort>,
    interaction_mode: RwSignal<InteractionMode>,
    access_mode: RwSignal<AccessMode>,
    workspace: Signal<String>,
    providers: Signal<Vec<ProviderStatus>>,
    #[prop(default = true)] hero: bool,
    #[prop(default = false)] running: bool,
    on_submit: Callback<String>,
    on_attach: Callback<()>,
) -> impl IntoView {
    let (content, set_content) = signal(String::new());
    let submit = Callback::new(move |_: ()| {
        let value = content.get().trim().to_owned();
        if value.is_empty() || running {
            return;
        }
        on_submit.run(value);
        set_content.set(String::new());
    });
    let selected_brand = move || ProviderBrand::for_provider(provider.get());
    let provider_name = move || selected_brand().display_name();
    let provider_initial = move || provider_name().chars().next().unwrap_or('O');
    let send_disabled = move || content.get().trim().is_empty() || running;
    let placeholder = move || {
        if workspace.get().is_empty() {
            "Choose a project, then tell Onyx what to build…"
        } else {
            "Tell Onyx what to build…"
        }
    };
    let access_copy = move || match access_mode.get() {
        AccessMode::ApprovalRequired => ("Ask", "approval_required"),
        AccessMode::AutoAcceptEdits => ("Auto edits", "auto_accept_edits"),
        AccessMode::FullAccess => ("Full access", "full_access"),
    };

    view! {
        <div
            class="zai-composer t3-composer"
            class:zai-composer--hero=hero
            class:zai-composer--docked=!hero
            class:zai-composer--running=running
            data-component="onyx-composer"
            data-layout=if hero { "hero" } else { "docked" }
            data-provider=move || provider.get().as_str()
        >
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
                            autofocus=hero
                            placeholder=placeholder
                            on:input=move |event| set_content.set(event_target_value(&event))
                            on:keydown=move |event: KeyboardEvent| {
                                if event.key() != "Enter" || event.shift_key() || event.is_composing() {
                                    return;
                                }
                                event.prevent_default();
                                if !event.repeat() {
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
                                disabled=running
                                on:click=move |_| on_attach.run(())
                                aria-label="Attach files"
                                title="Attach files"
                            >
                                <Icon icon=LuPlus width="18px" height="18px" />
                            </button>

                            <label class="zai-composer__control zai-composer__select-control zai-composer__provider-tile">
                                <span class="provider-badge provider-badge-sm">
                                    <span class="zai-composer__provider-initial">{provider_initial}</span>
                                </span>
                                <span class="zai-composer__control-label">{provider_name}</span>
                                <Icon icon=LuChevronDown width="12px" height="12px" />
                                <select
                                    class="zai-composer__native-select"
                                    aria-label="Provider"
                                    prop:value=move || provider.get().as_str()
                                    disabled=running
                                    on:change=move |event| {
                                        if let Some(next) = ProviderId::from_str(&event_target_value(&event)) {
                                            provider.set(next);
                                        }
                                    }
                                >
                                    <For
                                        each=move || providers.get()
                                        key=|status| status.id
                                        children=|status| view! {
                                            <option
                                                value=status.id.as_str()
                                                disabled=!status.available
                                            >
                                                {format!(
                                                    "{}{}",
                                                    status.name,
                                                    if status.available { "" } else { " — unavailable" },
                                                )}
                                            </option>
                                        }
                                    />
                                </select>
                            </label>

                            <label class="zai-composer__control zai-composer__select-control zai-composer__model-tile">
                                <span class="zai-composer__control-label">"Default model"</span>
                                <Icon icon=LuChevronDown width="12px" height="12px" />
                                <select class="zai-composer__native-select" aria-label="Model" disabled=running>
                                    <option value="default">"Default model"</option>
                                </select>
                            </label>

                            <label class="zai-composer__control zai-composer__select-control" title="Reasoning effort">
                                <Icon icon=LuBrainCircuit width="15px" height="15px" />
                                <span>{move || match reasoning.get() {
                                    ReasoningEffort::None => "None",
                                    ReasoningEffort::Minimal => "Minimal",
                                    ReasoningEffort::Low => "Low",
                                    ReasoningEffort::Medium => "Medium",
                                    ReasoningEffort::High => "High",
                                    ReasoningEffort::Xhigh => "Extra high",
                                    ReasoningEffort::Max => "Max",
                                    ReasoningEffort::Ultracode => "Ultracode",
                                }}</span>
                                <Icon icon=LuChevronDown width="12px" height="12px" />
                                <select
                                    class="zai-composer__native-select"
                                    aria-label="Reasoning"
                                    disabled=running
                                    prop:value=move || match reasoning.get() {
                                        ReasoningEffort::None => "none",
                                        ReasoningEffort::Minimal => "minimal",
                                        ReasoningEffort::Low => "low",
                                        ReasoningEffort::Medium => "medium",
                                        ReasoningEffort::High => "high",
                                        ReasoningEffort::Xhigh => "xhigh",
                                        ReasoningEffort::Max => "max",
                                        ReasoningEffort::Ultracode => "ultracode",
                                    }
                                    on:change=move |event| {
                                        reasoning.set(match event_target_value(&event).as_str() {
                                            "none" => ReasoningEffort::None,
                                            "minimal" => ReasoningEffort::Minimal,
                                            "low" => ReasoningEffort::Low,
                                            "high" => ReasoningEffort::High,
                                            "xhigh" => ReasoningEffort::Xhigh,
                                            "max" => ReasoningEffort::Max,
                                            "ultracode" => ReasoningEffort::Ultracode,
                                            _ => ReasoningEffort::Medium,
                                        });
                                    }
                                >
                                    <option value="none">"None"</option>
                                    <option value="minimal">"Minimal"</option>
                                    <option value="low">"Low"</option>
                                    <option value="medium">"Medium"</option>
                                    <option value="high">"High"</option>
                                    <option value="xhigh">"Extra high"</option>
                                    <option value="max">"Max"</option>
                                    <option value="ultracode">"Ultracode"</option>
                                </select>
                            </label>

                            <button
                                type="button"
                                class="zai-composer__control zai-composer__mode-toggle"
                                class:active=move || interaction_mode.get() == InteractionMode::Plan
                                on:click=move |_| {
                                    if !running {
                                        interaction_mode.update(|mode| {
                                            *mode = if *mode == InteractionMode::Plan {
                                                InteractionMode::Build
                                            } else {
                                                InteractionMode::Plan
                                            };
                                        });
                                    }
                                }
                            >
                                {move || if interaction_mode.get() == InteractionMode::Plan {
                                    view! { <Icon icon=LuPencilRuler width="15px" height="15px" /> }.into_any()
                                } else {
                                    view! { <Icon icon=LuBot width="15px" height="15px" /> }.into_any()
                                }}
                                <span>{move || if interaction_mode.get() == InteractionMode::Plan { "Plan" } else { "Build" }}</span>
                            </button>

                            <label class="zai-composer__control zai-composer__select-control">
                                {move || match access_mode.get() {
                                    AccessMode::ApprovalRequired => view! { <Icon icon=LuLock width="15px" height="15px" /> }.into_any(),
                                    AccessMode::AutoAcceptEdits => view! { <Icon icon=LuPenLine width="15px" height="15px" /> }.into_any(),
                                    AccessMode::FullAccess => view! { <Icon icon=LuLockOpen width="15px" height="15px" /> }.into_any(),
                                }}
                                <span>{move || access_copy().0}</span>
                                <Icon icon=LuChevronDown width="12px" height="12px" />
                                <select
                                    class="zai-composer__native-select"
                                    aria-label="Access"
                                    prop:value=move || access_copy().1
                                    disabled=running
                                    on:change=move |event| {
                                        access_mode.set(match event_target_value(&event).as_str() {
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
                            <button
                                type="submit"
                                class="zai-composer__submit"
                                disabled=send_disabled
                                aria-label="Send message"
                                title="Send (Enter)"
                            >
                                <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
                                    <path d="M7 11.5V2.5M7 2.5L3 6.5M7 2.5L11 6.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
                                </svg>
                            </button>
                        </div>
                    </div>
                </div>
            </form>
        </div>
    }
}
