use std::{
    cell::{Cell, RefCell},
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    rc::Rc,
};

use icondata::{LuChevronRight, LuCircle, LuCircleAlert, LuSquareTerminal};
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen::{JsCast, closure::Closure};

use crate::{
    markdown,
    model::{AccountProfile, AgentSession, Message, MessageKind, MessageRole},
    storage,
};

#[derive(Clone)]
enum TranscriptItem {
    Single(Message),
    Tools { id: String, messages: Vec<Message> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PromptJump {
    id: String,
    prompt: String,
    response: String,
}

fn compact_preview(value: &str, limit: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut preview = compact.chars().take(limit).collect::<String>();
    if compact.chars().count() > limit {
        preview.push('…');
    }
    preview
}

fn prompt_jumps(messages: &[Message]) -> Vec<PromptJump> {
    messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message.role == MessageRole::User)
        .map(|(index, message)| {
            let response = messages[index + 1..]
                .iter()
                .take_while(|candidate| candidate.role != MessageRole::User)
                .find(|candidate| {
                    candidate.role == MessageRole::Assistant
                        && candidate.kind == MessageKind::Text
                        && !candidate.content.trim().is_empty()
                })
                .map(|candidate| compact_preview(&candidate.content, 180))
                .unwrap_or_default();
            PromptJump {
                id: message.id.clone(),
                prompt: compact_preview(&message.content, 120),
                response,
            }
        })
        .collect()
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

fn message_revision(message: &Message) -> u64 {
    let mut hasher = DefaultHasher::new();
    message.id.hash(&mut hasher);
    message.content.hash(&mut hasher);
    hasher.finish()
}

fn tool_is_running(message: &Message) -> bool {
    tool_title(message).starts_with("Running ")
}

#[component]
fn ToolMessage(message: Message, current: bool) -> impl IntoView {
    let title = tool_title(&message);
    let detail = message
        .content
        .lines()
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n");
    let has_detail = detail.clone();
    view! {
        <details
            class="zai-tool-event"
            data-current=if current { "true" } else { "false" }
            aria-busy=if current { "true" } else { "false" }
            open=current
        >
            <summary>
                <Icon icon=LuSquareTerminal width="14px" height="14px" />
                <span>{title}</span>
                <Show when=move || current>
                    <span class="zai-tool-event__running">"Running"</span>
                </Show>
                <Icon
                    icon=LuChevronRight
                    width="13px"
                    height="13px"
                    attr:class="zai-tool-chevron"
                />
            </summary>
            <Show when=move || !has_detail.is_empty()>
                <pre>{detail.clone()}</pre>
            </Show>
        </details>
    }
}

#[component]
fn ToolGroup(messages: Vec<Message>, running: bool) -> impl IntoView {
    let current_id = running
        .then(|| messages.last())
        .flatten()
        .filter(|message| tool_is_running(message))
        .map(|message| message.id.clone());
    if messages.len() == 1 {
        let message = messages[0].clone();
        let current = current_id.as_deref() == Some(message.id.as_str());
        return view! { <ToolMessage message=message current=current /> }.into_any();
    }
    let count = messages.len();
    let latest = messages.last().map(tool_title).unwrap_or_default();
    let group_current = current_id.is_some();
    view! {
        <details
            class="zai-tool-group"
            data-current=if group_current { "true" } else { "false" }
            aria-busy=if group_current { "true" } else { "false" }
            open=group_current
        >
            <summary>
                <Icon icon=LuSquareTerminal width="14px" height="14px" />
                <span class="zai-tool-group__count">{format!("{count} steps")}</span>
                <span class="zai-tool-group__latest">{latest}</span>
                <Icon
                    icon=LuChevronRight
                    width="13px"
                    height="13px"
                    attr:class="zai-tool-chevron"
                />
            </summary>
            <div class="zai-tool-group__list">
                <For
                    each=move || messages.clone()
                    key=message_revision
                    children=move |message| {
                        let current = current_id.as_deref() == Some(message.id.as_str());
                        view! { <ToolMessage message=message current=current /> }
                    }
                />
            </div>
        </details>
    }
    .into_any()
}

#[component]
fn UserMessage(message: Message, profile: Signal<Option<AccountProfile>>) -> impl IntoView {
    let prompt_dom_id = format!("onyx-prompt-{}", message.id);
    let created_at = message.created_at;
    let timestamp = storage::display_time(&created_at);
    let rendered = markdown::render(&message.content);

    view! {
        <article
            id=prompt_dom_id
            class="zai-message zai-message--user zai-message--text"
            data-component="user-message"
        >
            <div class="zai-message__user-meta">
                {move || profile.get().and_then(|profile| profile.image_url).map(|url| view! {
                    <img
                        class="zai-message__user-avatar"
                        src=url
                        alt=""
                        referrerpolicy="no-referrer"
                    />
                }.into_any()).unwrap_or_else(|| {
                    let initial = profile
                        .get()
                        .map(|profile| profile.name)
                        .unwrap_or_else(|| "You".to_owned())
                        .chars()
                        .next()
                        .unwrap_or('Y')
                        .to_uppercase()
                        .to_string();
                    view! {
                        <span class="zai-message__user-avatar zai-message__user-avatar--fallback">
                            {initial}
                        </span>
                    }.into_any()
                })}
                <strong>
                    {move || profile.get().map(|profile| profile.name).unwrap_or_else(|| "You".to_owned())}
                </strong>
                <time datetime=created_at>{timestamp}</time>
            </div>
            <div class="zai-message__content" data-slot="user-message-text">
                <div class="zai-message-markdown" inner_html=rendered />
            </div>
        </article>
    }
}

#[component]
pub fn Transcript(
    session: Signal<Option<AgentSession>>,
    profile: Signal<Option<AccountProfile>>,
) -> impl IntoView {
    let scroller = NodeRef::<leptos::html::Div>::new();
    let pinned = RwSignal::new(true);
    let last_user_message = Rc::new(RefCell::new(None::<String>));
    let active_prompt = RwSignal::new(None::<String>);
    let hovered_prompt = RwSignal::new(None::<PromptJump>);
    let prompt_elements = Rc::new(RefCell::new(Vec::<(String, web_sys::Element)>::new()));
    let scroll_frame_pending = Rc::new(Cell::new(false));
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
    let prompts = Signal::derive(move || {
        session
            .get()
            .map(|session| prompt_jumps(&session.messages))
            .unwrap_or_default()
    });

    let cached_prompts = prompt_elements.clone();
    Effect::new(move |_| {
        let prompt_ids = prompts
            .get()
            .into_iter()
            .map(|prompt| prompt.id)
            .collect::<Vec<_>>();
        if prompt_ids.is_empty() {
            cached_prompts.borrow_mut().clear();
            active_prompt.set(None);
            return;
        }
        if !prompt_ids
            .iter()
            .any(|prompt| active_prompt.read().as_deref() == Some(prompt.as_str()))
        {
            active_prompt.set(prompt_ids.first().cloned());
        }
        if cached_prompts
            .borrow()
            .iter()
            .map(|(id, _)| id)
            .eq(prompt_ids.iter())
        {
            return;
        }
        cached_prompts.borrow_mut().clear();
        let cache = cached_prompts.clone();
        let callback = Closure::once(move || {
            let Some(document) = web_sys::window().and_then(|window| window.document()) else {
                return;
            };
            let elements = prompt_ids
                .into_iter()
                .filter_map(|id| {
                    document
                        .get_element_by_id(&format!("onyx-prompt-{id}"))
                        .map(|element| (id, element))
                })
                .collect();
            *cache.borrow_mut() = elements;
        });
        if let Some(window) = web_sys::window()
            && window
                .request_animation_frame(callback.as_ref().unchecked_ref())
                .is_ok()
        {
            callback.forget();
        }
    });

    Effect::new(move |_| {
        let snapshot = session.get();
        let latest_user_message = snapshot.as_ref().and_then(|session| {
            session
                .messages
                .iter()
                .rev()
                .find(|message| message.role == MessageRole::User)
                .map(|message| message.id.clone())
        });
        let content_version = snapshot.and_then(|session| {
            session.messages.last().map(|message| {
                (
                    session.messages.len(),
                    message.id.clone(),
                    message.content.len(),
                    session.status,
                )
            })
        });
        let _ = content_version;
        let force_to_latest_prompt = {
            let mut previous = last_user_message.borrow_mut();
            let changed = latest_user_message.is_some() && *previous != latest_user_message;
            *previous = latest_user_message;
            changed
        };
        if force_to_latest_prompt {
            pinned.set(true);
        }
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
        <div class="zai-transcript-shell">
            <Show when=move || !prompts.read().is_empty()>
                <nav
                    class="zai-prompt-rail"
                    aria-label="Jump to a prompt"
                    style=move || {
                        let count = prompts.read().len();
                        let gap = match count {
                            0..=16 => 8,
                            17..=32 => 4,
                            33..=64 => 2,
                            _ => 0,
                        };
                        format!("--prompt-rail-gap:{gap}px")
                    }
                >
                    <div class="zai-prompt-rail__track">
                        <For
                            each=move || prompts.get()
                            key=|prompt| prompt.id.clone()
                            children=move |prompt| {
                                let target_id = prompt.id.clone();
                                let active_id = prompt.id.clone();
                                let hover_prompt = prompt.clone();
                                let focus_prompt = prompt.clone();
                                let prompt_preview = prompt.prompt.clone();
                                view! {
                                    <button
                                        type="button"
                                        class:active=move || active_prompt.read().as_deref() == Some(active_id.as_str())
                                        aria-label=format!("Jump to prompt: {prompt_preview}")
                                        on:mouseenter=move |_| hovered_prompt.set(Some(hover_prompt.clone()))
                                        on:mouseleave=move |_| hovered_prompt.set(None)
                                        on:focus=move |_| hovered_prompt.set(Some(focus_prompt.clone()))
                                        on:blur=move |_| hovered_prompt.set(None)
                                        on:click=move |_| {
                                            pinned.set(false);
                                            active_prompt.set(Some(target_id.clone()));
                                            if let Some(element) = web_sys::window()
                                                .and_then(|window| window.document())
                                                .and_then(|document| document.get_element_by_id(&format!("onyx-prompt-{target_id}")))
                                            {
                                                element.scroll_into_view_with_bool(true);
                                            }
                                        }
                                    >
                                        <span class="zai-prompt-rail__mark" aria-hidden="true"></span>
                                    </button>
                                }
                            }
                        />
                    </div>
                    <Show when=move || hovered_prompt.get().is_some()>
                        {move || hovered_prompt.get().map(|prompt| {
                            let response = (!prompt.response.is_empty()).then_some(prompt.response);
                            view! {
                                <span class="zai-prompt-rail__preview" role="tooltip">
                                    <strong>{prompt.prompt}</strong>
                                    {response.map(|response| view! { <span>{response}</span> })}
                                </span>
                            }
                        })}
                    </Show>
                </nav>
            </Show>
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
                    if scroll_frame_pending.replace(true) {
                        return;
                    }
                    let pending = scroll_frame_pending.clone();
                    let cached_prompts = prompt_elements.clone();
                    let callback = Closure::once(move || {
                        pending.set(false);
                        if !element.is_connected() {
                            return;
                        }
                        let anchor = element.get_bounding_client_rect().top()
                            + f64::from(element.client_height()) * 0.32;
                        let elements = cached_prompts.borrow();
                        if elements.is_empty() {
                            return;
                        }
                        let mut low = 0;
                        let mut high = elements.len();
                        while low < high {
                            let middle = low + (high - low) / 2;
                            if elements[middle].1.get_bounding_client_rect().top() <= anchor {
                                low = middle + 1;
                            } else {
                                high = middle;
                            }
                        }
                        let index = low.saturating_sub(1).min(elements.len() - 1);
                        active_prompt.set(Some(elements[index].0.clone()));
                    });
                    if let Some(window) = web_sys::window()
                        && window
                            .request_animation_frame(callback.as_ref().unchecked_ref())
                            .is_ok()
                    {
                        callback.forget();
                    } else {
                        scroll_frame_pending.set(false);
                    }
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
                            TranscriptItem::Single(message) => {
                                format!("single:{}:{}", message.id, message_revision(message))
                            }
                            TranscriptItem::Tools { id, messages } => format!(
                                "tools:{id}:{}:{}",
                                messages.len(),
                                messages.last().map(message_revision).unwrap_or_default(),
                            ),
                        }
                        children=move |item| match item {
                            TranscriptItem::Tools { messages, .. } => {
                                view! { <ToolGroup messages=messages running=running.get() /> }.into_any()
                            }
                            TranscriptItem::Single(message) if message.role == MessageRole::User => {
                                view! { <UserMessage message=message profile=profile /> }.into_any()
                            }
                            TranscriptItem::Single(message) => {
                                let class = format!(
                                    "zai-message zai-message--{} zai-message--{}",
                                    message.role.as_str(),
                                    message.kind.as_str(),
                                );
                                let is_error = message.kind == MessageKind::Error;
                                let rendered = markdown::render(&message.content);
                                view! {
                                    <article class=class data-component="assistant-message">
                                        <Show when=move || is_error>
                                            <span class="zai-message__alert">
                                                <Icon icon=LuCircleAlert width="15px" height="15px" />
                                            </span>
                                        </Show>
                                        <div class="zai-message__content">
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
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::prompt_jumps;
    use crate::model::{Message, MessageKind, MessageRole};

    fn message(id: &str, role: MessageRole, content: &str) -> Message {
        Message {
            id: id.to_owned(),
            role,
            kind: MessageKind::Text,
            content: content.to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn prompt_rail_pairs_each_user_prompt_with_the_next_answer() {
        let messages = vec![
            message("u1", MessageRole::User, " First   prompt "),
            message("a1", MessageRole::Assistant, "First answer"),
            message("u2", MessageRole::User, "Second prompt"),
            message("a2", MessageRole::Assistant, "Second answer"),
        ];
        let jumps = prompt_jumps(&messages);
        assert_eq!(jumps.len(), 2);
        assert_eq!(jumps[0].prompt, "First prompt");
        assert_eq!(jumps[0].response, "First answer");
        assert_eq!(jumps[1].id, "u2");
    }
}
