use icondata::{LuArrowUp, LuMic, LuSquare, LuX};
use leptos::ev::{KeyboardEvent, SubmitEvent};
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen_futures::spawn_local;

use crate::{
    bridge,
    model::{HoldMode, HoldPayload, HoldPhase},
};

use super::OnyxOrb;

#[component]
pub fn Hud() -> impl IntoView {
    let (phase, set_phase) = signal("Ready".to_owned());
    let (level, set_level) = signal(0.0_f64);
    Effect::new(move |_| {
        spawn_local(async move {
            let result = bridge::listen::<HoldPayload, _>("onyx://hold", move |payload| {
                if payload.mode != HoldMode::Dictation {
                    return;
                }
                match payload.phase {
                    HoldPhase::Pressed => {
                        set_phase.set("Listening".to_owned());
                        set_level.set(0.78);
                    }
                    HoldPhase::Released => {
                        set_phase.set("Transcribing".to_owned());
                        set_level.set(0.0);
                    }
                }
            })
            .await;
            if let Ok(result) = result {
                result.forget();
            }
        });
    });

    let weights = [0.35, 0.6, 0.85, 1.0, 0.78, 0.5, 0.3];
    view! {
        <main
            class="onyx-hud"
            style="--app-accent:#7165e8"
            title=move || format!("{} · Active app", phase.get())
        >
            <OnyxOrb class="onyx-hud__app" />
            <i />
            <div class="onyx-hud__wave">
                <For
                    each=move || weights
                    key=|weight| format!("{weight}")
                    children=move |weight| view! {
                        <b style=move || format!("height:{}px", 4.0 + level.get() * 23.0 * weight) />
                    }
                />
            </div>
            <span class="onyx-hud__phase">{move || phase.get()}</span>
        </main>
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayMode {
    Inactive,
    Listening,
    Expanded,
}

impl OverlayMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::Listening => "listening",
            Self::Expanded => "expanded",
        }
    }
}

#[derive(Clone)]
struct IslandMessage {
    id: u32,
    user: bool,
    content: String,
}

#[component]
pub fn AgentOverlay() -> impl IntoView {
    let (mode, set_mode) = signal(OverlayMode::Inactive);
    let (phase, set_phase) = signal("Ask Onyx".to_owned());
    let (level, set_level) = signal(0.0_f64);
    let (draft, set_draft) = signal(String::new());
    let (messages, set_messages) = signal(Vec::<IslandMessage>::new());
    let (recording, set_recording) = signal(false);
    let ask = Callback::new(move |_: ()| {
        let prompt = draft.get().trim().to_owned();
        if prompt.is_empty() {
            return;
        }
        let next_id = messages.with(|items| items.len() as u32 + 1);
        set_messages.update(|items| {
            items.push(IslandMessage {
                id: next_id,
                user: true,
                content: prompt,
            });
            items.push(IslandMessage {
                id: next_id + 1,
                user: false,
                content: "The Rust overlay is connected. Agent routing and audio capture remain behind the parity gate.".to_owned(),
            });
        });
        set_draft.set(String::new());
        set_phase.set("Ready".to_owned());
        set_mode.set(OverlayMode::Expanded);
    });

    Effect::new(move |_| {
        spawn_local(async move {
            let result = bridge::listen::<HoldPayload, _>("onyx://hold", move |payload| {
                if payload.mode != HoldMode::Agent {
                    return;
                }
                match payload.phase {
                    HoldPhase::Pressed => {
                        set_mode.set(OverlayMode::Listening);
                        set_phase.set("Listening".to_owned());
                        set_recording.set(true);
                        set_level.set(0.76);
                    }
                    HoldPhase::Released => {
                        set_mode.set(OverlayMode::Expanded);
                        set_phase.set("Transcribing".to_owned());
                        set_recording.set(false);
                        set_level.set(0.0);
                    }
                }
            })
            .await;
            if let Ok(result) = result {
                result.forget();
            }
        });
    });

    let listen_weights = [0.35, 0.62, 1.0, 0.74, 0.44];
    view! {
        <main
            class="onyx-agent"
            data-state=move || mode.get().as_str()
            on:mouseenter=move |_| {
                if mode.get() == OverlayMode::Inactive {
                    set_mode.set(OverlayMode::Expanded);
                }
            }
        >
            <Show when=move || mode.get() == OverlayMode::Inactive>
                <div class="onyx-agent__hotspot" aria-label="Open Onyx Agent"><i /></div>
            </Show>

            <Show when=move || mode.get() == OverlayMode::Listening>
                <section class="onyx-agent__listening">
                    <OnyxOrb class="onyx-agent__orb" />
                    <div class="onyx-agent__wave" aria-hidden="true">
                        <For
                            each=move || listen_weights
                            key=|weight| format!("{weight}")
                            children=move |weight| view! {
                                <i style=move || format!("height:{}px", 4.0 + level.get() * 18.0 * weight) />
                            }
                        />
                    </div>
                    <span>{move || phase.get()}</span>
                </section>
            </Show>

            <Show when=move || mode.get() == OverlayMode::Expanded>
                <section class="onyx-agent__panel">
                    <header class="onyx-agent__header">
                        <OnyxOrb class="onyx-agent__orb" />
                        <div><strong>"Onyx Agent"</strong><span>{move || phase.get()}</span></div>
                        <button
                            on:click=move |_| {
                                set_mode.set(OverlayMode::Inactive);
                                set_phase.set("Ask Onyx".to_owned());
                            }
                            aria-label="Close"
                        >
                            <Icon icon=LuX width="15px" height="15px" />
                        </button>
                    </header>

                    <div class="onyx-agent__conversation" aria-live="polite">
                        <Show
                            when=move || !messages.read().is_empty()
                            fallback=move || view! {
                                <div class="onyx-agent__empty">
                                    <strong>"What can I help with?"</strong>
                                    <span>"Type below, tap the mic, or hold Control + Option."</span>
                                </div>
                            }
                        >
                            <For
                                each=move || messages.get()
                                key=|message| message.id
                                children=|message| view! {
                                    <article
                                        class="onyx-agent__message"
                                        data-role=if message.user { "user" } else { "assistant" }
                                    >
                                        <p>{message.content}</p>
                                    </article>
                                }
                            />
                        </Show>
                    </div>

                    <form
                        class="onyx-agent__composer"
                        on:submit=move |event: SubmitEvent| {
                            event.prevent_default();
                            ask.run(());
                        }
                    >
                        <textarea
                            aria-label="Message Onyx"
                            rows="1"
                            placeholder="Ask or search anything…"
                            prop:value=move || draft.get()
                            on:input=move |event| set_draft.set(event_target_value(&event))
                            on:keydown=move |event: KeyboardEvent| {
                                if event.key() == "Enter" && !event.shift_key() {
                                    event.prevent_default();
                                    ask.run(());
                                }
                            }
                        />
                        <button
                            type="button"
                            class="onyx-agent__voice"
                            aria-label=move || if recording.get() { "Stop recording" } else { "Ask with voice" }
                            on:click=move |_| {
                                set_recording.update(|recording| *recording = !*recording);
                                set_phase.set(if recording.get() { "Listening" } else { "Ask Onyx" }.to_owned());
                            }
                        >
                            {move || if recording.get() {
                                view! { <Icon icon=LuSquare width="13px" height="13px" /> }.into_any()
                            } else {
                                view! { <Icon icon=LuMic width="15px" height="15px" /> }.into_any()
                            }}
                        </button>
                        <button
                            type="submit"
                            class="onyx-agent__send"
                            aria-label="Send"
                            disabled=move || draft.get().trim().is_empty()
                        >
                            <Icon icon=LuArrowUp width="15px" height="15px" />
                        </button>
                    </form>
                </section>
            </Show>
        </main>
    }
}
