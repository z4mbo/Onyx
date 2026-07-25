use std::cell::RefCell;

use gloo_timers::future::TimeoutFuture;
use icondata::{LuArrowUp, LuMic, LuSquare, LuX};
use leptos::ev::{KeyboardEvent, SubmitEvent};
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen::{JsCast, closure::Closure};
use wasm_bindgen_futures::spawn_local;

use crate::{
    bridge, markdown,
    model::{
        ActiveAppContext, ChatMessageInput, ChatRequest, HoldMode, HoldPayload, HoldPhase,
        VoiceHistoryItem,
    },
    storage, theme,
};

use super::OnyxOrb;

thread_local! {
    static AUDIO_CAPTURE: RefCell<Option<bridge::AudioCaptureHandle>> =
        const { RefCell::new(None) };
}

fn capture_is_retained() -> bool {
    AUDIO_CAPTURE.with(|capture| capture.borrow().is_some())
}

fn retain_capture(handle: bridge::AudioCaptureHandle) {
    AUDIO_CAPTURE.with(|capture| {
        *capture.borrow_mut() = Some(handle);
    });
}

fn release_capture() {
    AUDIO_CAPTURE.with(|capture| {
        capture.borrow_mut().take();
    });
}

async fn begin_capture(level: RwSignal<f64>) -> Result<(), String> {
    let handle = bridge::start_audio_capture(move |next| level.set(next)).await?;
    retain_capture(handle);
    Ok(())
}

async fn cancel_capture() {
    let _ = bridge::cancel_audio_capture().await;
    release_capture();
}

fn default_active_app() -> ActiveAppContext {
    ActiveAppContext {
        name: "Active app".to_owned(),
        process: String::new(),
        accent: "#7165e8".to_owned(),
        symbol: "O".to_owned(),
    }
}

fn speech_failure_notice(cause: &str) -> String {
    let compact = cause.split_whitespace().collect::<Vec<_>>().join(" ");
    let summary = if compact.is_empty() {
        "No error details were returned.".to_owned()
    } else {
        let mut bounded = compact.chars().take(180).collect::<String>();
        if compact.chars().count() > 180 {
            bounded.push('…');
        }
        bounded
    };
    format!("The reply is ready, but speech synthesis failed: {summary}")
}

fn hide_hud_after(delay_ms: u32) {
    spawn_local(async move {
        TimeoutFuture::new(delay_ms).await;
        let _ = bridge::hide_window("hud").await;
    });
}

async fn finish_hud(
    phase: RwSignal<String>,
    level: RwSignal<f64>,
    app: RwSignal<ActiveAppContext>,
    starting: RwSignal<bool>,
    finishing: RwSignal<bool>,
    pending_stop: RwSignal<bool>,
) {
    if starting.get_untracked() {
        pending_stop.set(true);
        return;
    }
    if finishing.get_untracked() || !capture_is_retained() {
        return;
    }
    finishing.set(true);
    phase.set("Transcribing".to_owned());
    let result = async {
        let audio = bridge::stop_audio_capture().await?;
        release_capture();
        let transcription = bridge::transcribe_audio(&audio.audio_base64, &audio.format).await?;
        storage::append_voice_history(VoiceHistoryItem {
            id: storage::unique_id("voice"),
            created_at: storage::timestamp(),
            kind: "dictation".to_owned(),
            text: transcription.text.clone(),
            answer: None,
            app_name: Some(app.get_untracked().name),
            model: Some(transcription.model),
        });
        bridge::inject_text(&transcription.text).await?;
        Ok::<(), String>(())
    }
    .await;
    match result {
        Ok(()) => {
            phase.set("Inserted".to_owned());
            hide_hud_after(650);
        }
        Err(cause) => {
            release_capture();
            let cause = if cause.chars().count() > 140 {
                format!("{}…", cause.chars().take(140).collect::<String>())
            } else {
                cause
            };
            phase.set(cause);
            hide_hud_after(6_500);
        }
    }
    finishing.set(false);
    level.set(0.0);
}

async fn start_hud(
    phase: RwSignal<String>,
    level: RwSignal<f64>,
    app: RwSignal<ActiveAppContext>,
    starting: RwSignal<bool>,
    finishing: RwSignal<bool>,
    pending_stop: RwSignal<bool>,
) {
    if starting.get_untracked() || finishing.get_untracked() || capture_is_retained() {
        return;
    }
    theme::apply_document_theme();
    starting.set(true);
    pending_stop.set(false);
    phase.set("Starting microphone".to_owned());
    let result = async {
        app.set(bridge::active_app_context().await?);
        begin_capture(level).await
    }
    .await;
    match result {
        Ok(()) => {
            phase.set("Listening".to_owned());
            starting.set(false);
            if pending_stop.get_untracked() {
                finish_hud(phase, level, app, starting, finishing, pending_stop).await;
            }
        }
        Err(cause) => {
            starting.set(false);
            phase.set(cause);
            hide_hud_after(6_500);
        }
    }
}

#[component]
pub fn Hud() -> impl IntoView {
    let phase = RwSignal::new("Ready".to_owned());
    let level = RwSignal::new(0.0_f64);
    let app = RwSignal::new(default_active_app());
    let starting = RwSignal::new(false);
    let finishing = RwSignal::new(false);
    let pending_stop = RwSignal::new(false);

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(listener) = bridge::listen::<HoldPayload, _>("onyx://hold", move |payload| {
                if payload.mode != HoldMode::Dictation {
                    return;
                }
                match payload.phase {
                    HoldPhase::Pressed => spawn_local(start_hud(
                        phase,
                        level,
                        app,
                        starting,
                        finishing,
                        pending_stop,
                    )),
                    HoldPhase::Released => spawn_local(finish_hud(
                        phase,
                        level,
                        app,
                        starting,
                        finishing,
                        pending_stop,
                    )),
                }
            })
            .await
            {
                listener.forget();
            }
        });
    });

    let weights = [0.35, 0.6, 0.85, 1.0, 0.78, 0.5, 0.3];
    view! {
        <main
            class="onyx-hud"
            style=move || format!("--app-accent:{}", app.get().accent)
            title=move || format!("{} · {}", phase.get(), app.get().name)
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
    id: String,
    user: bool,
    content: String,
}

#[derive(Clone, Copy)]
enum AgentSource {
    Voice,
    Typed,
}

#[derive(Clone, Copy)]
struct AgentState {
    mode: RwSignal<OverlayMode>,
    phase: RwSignal<String>,
    notice: RwSignal<Option<String>>,
    level: RwSignal<f64>,
    draft: RwSignal<String>,
    messages: RwSignal<Vec<IslandMessage>>,
    busy: RwSignal<bool>,
    generation: RwSignal<u64>,
    recording: RwSignal<bool>,
    starting: RwSignal<bool>,
    finishing: RwSignal<bool>,
    pending_stop: RwSignal<bool>,
}

async fn change_agent_mode(mode: RwSignal<OverlayMode>, next: OverlayMode) {
    mode.set(next);
    let _ = bridge::set_agent_overlay_mode(next.as_str()).await;
}

fn append_agent_error(messages: RwSignal<Vec<IslandMessage>>, cause: String) {
    messages.update(|items| {
        items.push(IslandMessage {
            id: storage::unique_id("agent-message"),
            user: false,
            content: cause,
        });
    });
}

fn request_needs_web(prompt: &str) -> bool {
    let prompt = prompt.to_ascii_lowercase();
    [
        "search", "look up", "latest", "today", "news", "weather", "web", "online", "source",
        "price",
    ]
    .iter()
    .any(|needle| prompt.contains(needle))
}

async fn ask_agent(text: String, source: AgentSource, state: AgentState) {
    let prompt = text.trim().to_owned();
    if prompt.is_empty() || state.busy.get_untracked() {
        return;
    }
    let generation = state.generation.get_untracked().wrapping_add(1);
    state.generation.set(generation);
    state.notice.set(None);
    let user = IslandMessage {
        id: storage::unique_id("agent-message"),
        user: true,
        content: prompt.clone(),
    };
    let mut history = state.messages.get_untracked();
    history.push(user);
    if history.len() > 20 {
        history.drain(..history.len() - 20);
    }
    state.messages.set(history.clone());
    state.draft.set(String::new());
    state.phase.set("Thinking".to_owned());
    state.busy.set(true);
    change_agent_mode(state.mode, OverlayMode::Expanded).await;

    let result = async {
        let settings = bridge::get_voice_settings().await?;
        let active = bridge::active_app_context().await?;
        let needs_web = request_needs_web(&prompt);
        let (provider, model, route_label) = if needs_web {
            (settings.web_provider, settings.web_model.clone(), "web")
        } else {
            (
                settings.agent_provider,
                settings.agent_model.clone(),
                "general",
            )
        };
        let history_len = history.len();
        let request_messages = history
            .into_iter()
            .enumerate()
            .map(|(index, message)| ChatMessageInput {
                role: if message.user { "user" } else { "assistant" }.to_owned(),
                content: if index + 1 == history_len {
                    format!(
                        "The active application is {}. This is a {route_label} {} request. \
                         Answer in concise Markdown and only use read-only tools when needed:\n\n{}",
                        active.name,
                        match source {
                            AgentSource::Voice => "voice",
                            AgentSource::Typed => "typed",
                        },
                        message.content,
                    )
                } else {
                    message.content
                },
            })
            .collect();
        let reply = bridge::chat_send(ChatRequest {
            provider,
            model,
            messages: request_messages,
            web_search: needs_web,
        })
        .await?;
        Ok::<_, String>((settings, active, reply))
    }
    .await;

    match result {
        Ok((settings, active, reply)) => {
            state.messages.update(|items| {
                items.push(IslandMessage {
                    id: storage::unique_id("agent-message"),
                    user: false,
                    content: reply.content.clone(),
                });
            });
            state.phase.set("Ready".to_owned());
            storage::append_voice_history(VoiceHistoryItem {
                id: storage::unique_id("voice"),
                created_at: storage::timestamp(),
                kind: "agent".to_owned(),
                text: prompt,
                answer: Some(reply.content.clone()),
                app_name: Some(active.name),
                model: Some(reply.model),
            });
            state.busy.set(false);
            if settings.speak_responses && matches!(source, AgentSource::Voice) {
                let content = reply.content;
                spawn_local(async move {
                    let result = bridge::speak_text(&content).await;
                    if state.generation.get_untracked() != generation {
                        return;
                    }
                    match result {
                        Ok(source) if source.is_empty() => {
                            state.phase.set("Speech unavailable".to_owned());
                            state.notice.set(Some(
                                "The reply is ready, but speech synthesis returned no audio. Check the Speech model in Settings."
                                    .to_owned(),
                            ));
                        }
                        Ok(source) => {
                            if bridge::play_audio(&source).await.is_err()
                                && state.generation.get_untracked() == generation
                            {
                                state.phase.set("Audio playback failed".to_owned());
                                state.notice.set(Some(
                                    "The reply is ready, but Onyx couldn’t play the generated audio. Check the current audio output and try again."
                                        .to_owned(),
                                ));
                            }
                        }
                        Err(cause) => {
                            state.phase.set("Speech unavailable".to_owned());
                            state.notice.set(Some(speech_failure_notice(&cause)));
                        }
                    }
                });
            }
        }
        Err(cause) => {
            state.phase.set("Couldn’t answer".to_owned());
            append_agent_error(state.messages, cause);
            state.busy.set(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::speech_failure_notice;

    #[test]
    fn speech_failure_notice_is_compact_and_bounded() {
        let cause = format!("  upstream\nfailed   {}", "x".repeat(220));
        let notice = speech_failure_notice(&cause);

        assert!(
            notice.starts_with("The reply is ready, but speech synthesis failed: upstream failed ")
        );
        assert!(notice.ends_with('…'));
        assert!(notice.chars().count() < 250);
    }
}

async fn finish_agent(state: AgentState) {
    if state.starting.get_untracked() {
        state.pending_stop.set(true);
        return;
    }
    if state.finishing.get_untracked() || !capture_is_retained() {
        return;
    }
    state.finishing.set(true);
    state.phase.set("Transcribing".to_owned());
    let result = async {
        let audio = bridge::stop_audio_capture().await?;
        release_capture();
        bridge::transcribe_audio(&audio.audio_base64, &audio.format).await
    }
    .await;
    state.recording.set(false);
    state.level.set(0.0);
    match result {
        Ok(transcription) => {
            ask_agent(transcription.text, AgentSource::Voice, state).await;
        }
        Err(cause) => {
            release_capture();
            change_agent_mode(state.mode, OverlayMode::Expanded).await;
            state.phase.set("Something went wrong".to_owned());
            append_agent_error(state.messages, cause);
        }
    }
    state.finishing.set(false);
}

async fn start_agent(from_panel: bool, state: AgentState) {
    if state.starting.get_untracked() || state.finishing.get_untracked() || capture_is_retained() {
        return;
    }
    theme::apply_document_theme();
    state.starting.set(true);
    state.pending_stop.set(false);
    state.phase.set("Starting microphone".to_owned());
    if !from_panel {
        change_agent_mode(state.mode, OverlayMode::Listening).await;
    }
    match begin_capture(state.level).await {
        Ok(()) => {
            state.recording.set(true);
            state.phase.set("Listening".to_owned());
            state.starting.set(false);
            if state.pending_stop.get_untracked() {
                finish_agent(state).await;
            }
        }
        Err(cause) => {
            state.starting.set(false);
            change_agent_mode(state.mode, OverlayMode::Expanded).await;
            state.phase.set("Microphone unavailable".to_owned());
            append_agent_error(state.messages, cause);
        }
    }
}

async fn collapse_agent(state: AgentState) {
    cancel_capture().await;
    state.recording.set(false);
    state.level.set(0.0);
    state.phase.set("Ask Onyx".to_owned());
    change_agent_mode(state.mode, OverlayMode::Inactive).await;
}

#[component]
pub fn AgentOverlay() -> impl IntoView {
    let mode = RwSignal::new(OverlayMode::Inactive);
    let phase = RwSignal::new("Ask Onyx".to_owned());
    let notice = RwSignal::new(None::<String>);
    let level = RwSignal::new(0.0_f64);
    let draft = RwSignal::new(String::new());
    let messages = RwSignal::new(Vec::<IslandMessage>::new());
    let busy = RwSignal::new(false);
    let generation = RwSignal::new(0_u64);
    let recording = RwSignal::new(false);
    let starting = RwSignal::new(false);
    let finishing = RwSignal::new(false);
    let pending_stop = RwSignal::new(false);
    let state = AgentState {
        mode,
        phase,
        notice,
        level,
        draft,
        messages,
        busy,
        generation,
        recording,
        starting,
        finishing,
        pending_stop,
    };
    let conversation = NodeRef::<leptos::html::Div>::new();

    let submit = Callback::new(move |_: ()| {
        spawn_local(ask_agent(draft.get_untracked(), AgentSource::Typed, state));
    });

    Effect::new(move |_| {
        let _ = messages.get();
        let _ = busy.get();
        let _ = notice.get();
        let conversation = conversation;
        spawn_local(async move {
            TimeoutFuture::new(0).await;
            if let Some(element) = conversation.get_untracked() {
                element.set_scroll_top(element.scroll_height());
            }
        });
    });

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(listener) = bridge::listen::<HoldPayload, _>("onyx://hold", move |payload| {
                if payload.mode != HoldMode::Agent {
                    return;
                }
                match payload.phase {
                    HoldPhase::Pressed => spawn_local(start_agent(false, state)),
                    HoldPhase::Released => spawn_local(finish_agent(state)),
                }
            })
            .await
            {
                listener.forget();
            }
        });
    });

    let escape = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
        if event.key() == "Escape" && mode.get_untracked() != OverlayMode::Inactive {
            spawn_local(collapse_agent(state));
        }
    }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);
    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback("keydown", escape.as_ref().unchecked_ref());
        escape.forget();
    }

    let listen_weights = [0.35, 0.62, 1.0, 0.74, 0.44];
    view! {
        <main
            class="onyx-agent"
            data-state=move || mode.get().as_str()
            on:mouseenter=move |_| {
                if mode.get_untracked() == OverlayMode::Inactive {
                    spawn_local(change_agent_mode(mode, OverlayMode::Expanded));
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
                            on:click=move |_| spawn_local(collapse_agent(state))
                            aria-label="Close"
                        >
                            <Icon icon=LuX width="15px" height="15px" />
                        </button>
                    </header>

                    <div node_ref=conversation class="onyx-agent__conversation" aria-live="polite">
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
                                key=|message| message.id.clone()
                                children=|message| {
                                    let rendered = markdown::render(&message.content);
                                    view! {
                                        <article
                                            class="onyx-agent__message"
                                            data-role=if message.user { "user" } else { "assistant" }
                                        >
                                            {if message.user {
                                                view! { <p>{message.content}</p> }.into_any()
                                            } else {
                                                view! { <div inner_html=rendered /> }.into_any()
                                            }}
                                        </article>
                                    }
                                }
                            />
                        </Show>
                        <Show when=move || busy.get()>
                            <div class="onyx-agent__typing" aria-label="Onyx is thinking"><i /><i /><i /></div>
                        </Show>
                        <Show when=move || notice.get().is_some()>
                            <article
                                class="onyx-agent__message"
                                data-role="assistant"
                                role="status"
                            >
                                <div class="zai-message-markdown">
                                    <p>{move || notice.get().unwrap_or_default()}</p>
                                </div>
                            </article>
                        </Show>
                    </div>

                    <form
                        class="onyx-agent__composer"
                        on:submit=move |event: SubmitEvent| {
                            event.prevent_default();
                            submit.run(());
                        }
                    >
                        <textarea
                            aria-label="Message Onyx"
                            rows="1"
                            placeholder="Ask or search anything…"
                            prop:value=move || draft.get()
                            on:input=move |event| draft.set(event_target_value(&event))
                            on:keydown=move |event: KeyboardEvent| {
                                if event.key() == "Enter" && !event.shift_key() && !event.is_composing() {
                                    event.prevent_default();
                                    submit.run(());
                                }
                            }
                        />
                        <button
                            type="button"
                            class="onyx-agent__voice"
                            aria-label=move || if recording.get() { "Stop recording" } else { "Ask with voice" }
                            on:click=move |_| {
                                if recording.get_untracked() {
                                    spawn_local(finish_agent(state));
                                } else {
                                    spawn_local(start_agent(true, state));
                                }
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
                            disabled=move || busy.get() || draft.get().trim().is_empty()
                        >
                            <Icon icon=LuArrowUp width="15px" height="15px" />
                        </button>
                    </form>
                </section>
            </Show>
        </main>
    }
}
