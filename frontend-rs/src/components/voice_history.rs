use icondata::{LuBot, LuClipboard, LuCopy, LuMic, LuTrash2};
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen_futures::spawn_local;

use crate::{bridge, storage};

#[component]
pub fn VoiceHistoryView(on_settings: Callback<()>) -> impl IntoView {
    let items = RwSignal::new(storage::load_voice_history());

    view! {
        <section class="zai-page-frame onyx-voice-history">
            <header>
                <div>
                    <h1>"Voice history"</h1>
                    <p>"Your local clipboard for dictation and agent conversations."</p>
                </div>
                <div>
                    <button class="zai-neutral-button" on:click=move |_| on_settings.run(())>
                        "Voice settings"
                    </button>
                    <button
                        class="zai-neutral-button"
                        disabled=move || items.read().is_empty()
                        on:click=move |_| {
                            storage::clear_voice_history();
                            items.set(Vec::new());
                        }
                    >
                        <Icon icon=LuTrash2 width="14px" height="14px" />
                        "Clear"
                    </button>
                </div>
            </header>
            <Show
                when=move || !items.read().is_empty()
                fallback=move || view! {
                    <div class="zai-home-empty">
                        <Icon icon=LuMic width="23px" height="23px" />
                        <strong>"No voice history yet"</strong>
                        <span>"Hold Ctrl+Shift to dictate or Ctrl+Alt to ask Onyx."</span>
                    </div>
                }
            >
                <div class="onyx-voice-history__list">
                    <For
                        each=move || items.get()
                        key=|item| item.id.clone()
                        children=move |item| {
                            let is_dictation = item.kind == "dictation";
                            let copy = item.answer.clone().unwrap_or_else(|| item.text.clone());
                            view! {
                                <article>
                                    <span>
                                        {if is_dictation {
                                            view! { <Icon icon=LuClipboard width="16px" height="16px" /> }.into_any()
                                        } else {
                                            view! { <Icon icon=LuBot width="16px" height="16px" /> }.into_any()
                                        }}
                                    </span>
                                    <div>
                                        <small>
                                            {format!(
                                                "{} · {} · {}",
                                                if is_dictation { "Dictation" } else { "Agent" },
                                                item.created_at,
                                                item.app_name.clone().unwrap_or_else(|| "Active app".to_owned()),
                                            )}
                                        </small>
                                        <p>{item.text}</p>
                                        {item.answer.map(|answer| view! { <blockquote>{answer}</blockquote> })}
                                    </div>
                                    <button
                                        aria-label="Copy"
                                        on:click=move |_| {
                                            let value = copy.clone();
                                            spawn_local(async move {
                                                let _ = bridge::copy_text(&value).await;
                                            });
                                        }
                                    >
                                        <Icon icon=LuCopy width="14px" height="14px" />
                                    </button>
                                </article>
                            }
                        }
                    />
                </div>
            </Show>
        </section>
    }
}
