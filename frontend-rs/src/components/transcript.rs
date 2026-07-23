use icondata::LuCircle;
use leptos::prelude::*;
use leptos_icons::Icon;

use crate::model::AgentSession;

#[component]
pub fn Transcript(session: Signal<Option<AgentSession>>) -> impl IntoView {
    let messages = move || {
        session
            .get()
            .map(|session| session.messages)
            .unwrap_or_default()
    };
    let running = move || {
        session
            .get()
            .is_some_and(|session| session.status.is_running())
    };

    view! {
        <div
            class="zai-transcript"
            class:zai-transcript--running=running
            role="log"
            aria-label="Conversation"
            aria-live="polite"
            aria-relevant="additions text"
            aria-atomic="false"
            aria-busy=running
        >
            <div class="zai-transcript__inner">
                <Show
                    when=move || !messages().is_empty()
                    fallback=move || view! {
                        <div class="zai-transcript__empty">
                            {move || session.get()
                                .map(|session| format!("Start a conversation with {}.", session.provider.display_name()))
                                .unwrap_or_else(|| "Start a conversation with Onyx.".to_owned())}
                        </div>
                    }
                >
                    <For
                        each=messages
                        key=|message| message.id.clone()
                        children=|message| {
                            let class = format!(
                                "zai-message zai-message--{} zai-message--{}",
                                message.role.as_str(),
                                message.kind.as_str(),
                            );
                            let component = if message.role.as_str() == "user" {
                                "user-message"
                            } else {
                                "assistant-message"
                            };
                            view! {
                                <article class=class data-component=component>
                                    <div
                                        class="zai-message__content"
                                        data-slot=if message.role.as_str() == "user" {
                                            Some("user-message-text")
                                        } else {
                                            None
                                        }
                                    >
                                        <p>{message.content}</p>
                                    </div>
                                </article>
                            }
                        }
                    />
                </Show>

                <Show when=running>
                    <div class="zai-agent-working">
                        <Icon
                            icon=LuCircle
                            width="8px"
                            height="8px"
                            style="fill:currentColor"
                        />
                        <span>"Working…"</span>
                    </div>
                </Show>
            </div>
        </div>
    }
}
