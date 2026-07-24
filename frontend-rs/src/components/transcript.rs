use icondata::{LuChevronRight, LuCircle, LuCircleAlert, LuSquareTerminal};
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen::{JsCast, closure::Closure};

use crate::{
    markdown,
    model::{AgentSession, Message, MessageKind, MessageRole},
};

#[derive(Clone)]
enum TranscriptItem {
    Single(Message),
    Tools { id: String, messages: Vec<Message> },
}

fn group_messages(messages: Vec<Message>) -> Vec<TranscriptItem> {
    let mut items = Vec::new();
    for message in messages {
        if message.kind == MessageKind::Tool {
            if let Some(TranscriptItem::Tools { messages, .. }) = items.last_mut() {
                messages.push(message);
            } else {
                items.push(TranscriptItem::Tools {
                    id: message.id.clone(),
                    messages: vec![message],
                });
            }
        } else {
            items.push(TranscriptItem::Single(message));
        }
    }
    items
}

fn tool_title(message: &Message) -> String {
    message
        .content
        .lines()
        .next()
        .filter(|line| !line.trim().is_empty())
        .unwrap_or("Tool activity")
        .to_owned()
}

#[component]
fn ToolMessage(message: Message) -> impl IntoView {
    let title = tool_title(&message);
    let detail = message
        .content
        .lines()
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n");
    let has_detail = detail.clone();
    view! {
        <details class="zai-tool-event">
            <summary>
                <Icon icon=LuSquareTerminal width="14px" height="14px" />
                <span>{title}</span>
                <Icon icon=LuChevronRight width="13px" height="13px" />
            </summary>
            <Show when=move || !has_detail.is_empty()>
                <pre>{detail.clone()}</pre>
            </Show>
        </details>
    }
}

#[component]
fn ToolGroup(messages: Vec<Message>) -> impl IntoView {
    if messages.len() == 1 {
        return view! { <ToolMessage message=messages[0].clone() /> }.into_any();
    }
    let count = messages.len();
    let latest = messages.last().map(tool_title).unwrap_or_default();
    view! {
        <details class="zai-tool-group">
            <summary>
                <Icon icon=LuSquareTerminal width="14px" height="14px" />
                <span class="zai-tool-group__count">{format!("{count} steps")}</span>
                <span class="zai-tool-group__latest">{latest}</span>
                <Icon icon=LuChevronRight width="13px" height="13px" />
            </summary>
            <div class="zai-tool-group__list">
                <For
                    each=move || messages.clone()
                    key=|message| message.id.clone()
                    children=|message| view! { <ToolMessage message=message /> }
                />
            </div>
        </details>
    }
    .into_any()
}

#[component]
pub fn Transcript(session: Signal<Option<AgentSession>>) -> impl IntoView {
    let scroller = NodeRef::<leptos::html::Div>::new();
    let pinned = RwSignal::new(true);
    let items = Signal::derive(move || {
        group_messages(
            session
                .get()
                .map(|session| session.messages)
                .unwrap_or_default(),
        )
    });
    let running = Signal::derive(move || {
        session
            .get()
            .is_some_and(|session| session.status.is_running())
    });

    Effect::new(move |_| {
        let content_version = session
            .get()
            .map(|session| {
                session
                    .messages
                    .iter()
                    .map(|message| message.content.as_str())
                    .collect::<String>()
            })
            .unwrap_or_default();
        let _ = content_version;
        if !pinned.get_untracked() {
            return;
        }
        // Capture the DOM node while this owner is alive. A queued animation
        // frame can run after a keyed session view has been disposed, so it
        // must never reach back into the disposed NodeRef.
        let Some(element) = scroller.get_untracked() else {
            return;
        };
        let callback = Closure::once(move || {
            if element.is_connected() {
                element.set_scroll_top(element.scroll_height());
            }
        });
        if let Some(window) = web_sys::window() {
            let _ = window.request_animation_frame(callback.as_ref().unchecked_ref());
            callback.forget();
        }
    });

    view! {
        <div
            node_ref=scroller
            class="zai-transcript"
            class:zai-transcript--running=move || running.get()
            role="log"
            aria-label="Conversation"
            aria-live="polite"
            aria-relevant="additions text"
            aria-atomic="false"
            aria-busy=move || running.get()
            on:scroll=move |event| {
                let element = event_target::<web_sys::HtmlElement>(&event);
                let remaining =
                    element.scroll_height() - element.scroll_top() - element.client_height();
                pinned.set(remaining < 80);
            }
        >
            <div class="zai-transcript__inner">
                <Show
                    when=move || !items.read().is_empty()
                    fallback=move || view! {
                        <div class="zai-transcript__empty">
                            {move || session
                                .get()
                                .map(|session| format!(
                                    "Start a conversation with {}.",
                                    session.provider.display_name(),
                                ))
                                .unwrap_or_else(|| "Start a conversation with Onyx.".to_owned())}
                        </div>
                    }
                >
                    <For
                        each=move || items.get()
                        key=|item| match item {
                            TranscriptItem::Single(message) => message.id.clone(),
                            TranscriptItem::Tools { id, .. } => id.clone(),
                        }
                        children=move |item| match item {
                            TranscriptItem::Tools { messages, .. } => {
                                view! { <ToolGroup messages=messages /> }.into_any()
                            }
                            TranscriptItem::Single(message) => {
                                let class = format!(
                                    "zai-message zai-message--{} zai-message--{}",
                                    message.role.as_str(),
                                    message.kind.as_str(),
                                );
                                let component = if message.role == MessageRole::User {
                                    "user-message"
                                } else {
                                    "assistant-message"
                                };
                                let is_error = message.kind == MessageKind::Error;
                                let slot = (message.role == MessageRole::User)
                                    .then_some("user-message-text");
                                let rendered = markdown::render(&message.content);
                                view! {
                                    <article class=class data-component=component>
                                        <Show when=move || is_error>
                                            <span class="zai-message__alert">
                                                <Icon icon=LuCircleAlert width="15px" height="15px" />
                                            </span>
                                        </Show>
                                        <div class="zai-message__content" data-slot=slot>
                                            <div class="zai-message-markdown" inner_html=rendered />
                                        </div>
                                    </article>
                                }
                                .into_any()
                            }
                        }
                    />
                </Show>

                <Show when=move || running.get()>
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
