use std::collections::{HashMap, HashSet};

use gloo_timers::future::TimeoutFuture;
use icondata::{LuGauge, LuGitBranch, LuTimerReset};
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen::{JsCast, closure::Closure};
use wasm_bindgen_futures::spawn_local;

use crate::{
    bridge,
    catalog::{fallback_catalogs, models_for_brand, runtime_for_brand, selected_or_default},
    components::{
        AccountGate, AgentOverlay, BottomTerminalPanel, ColorScheme, Composer, GitCommitDialog,
        HomeView, Hud, RightWorkspacePanel, SessionWorkspaceUi, SettingsDialog, Titlebar,
        TitlebarSession, TitlebarTab, Transcript, UpdateDialog, UserInputCard, VoiceHistoryView,
        WorkspaceSurface, WorkspaceSurfaceKind, WorkspaceTerminal, WorkspaceTopbarActions,
    },
    model::{
        AccessMode, AccountEvent, AccountProfile, AgentSession, ApprovalRequest, ConnectionStatus,
        CreateSessionInput, EditorTarget, InteractionMode, NativeVoicePermissions, OpenRouterModel,
        ProviderBrand, ProviderId, ProviderUsage, ProviderUserInputRequest, ReasoningEffort,
        RepoSummary, SessionEvent, SpeedMode, TerminalEvent, UpdateInfo, UpdateProgress,
        UpdateSessionOptionsInput, apply_session_event, demo_providers, replace_session,
        workspace_name,
    },
    storage, theme,
};

const DRAFT_TAB_ID: &str = "onyx:draft";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Home,
    Draft,
    Session,
    Voice,
}

impl Page {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Draft => "draft",
            Self::Session => "session",
            Self::Voice => "voice",
        }
    }
}

#[derive(Clone)]
enum SessionOptionPatch {
    Brand(ProviderBrand),
    Model(String),
    Reasoning(ReasoningEffort),
    Speed(SpeedMode),
    Interaction(InteractionMode),
    Access(AccessMode),
}

fn compact_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0).replace(".0M", "M")
    } else if tokens >= 1_000 {
        format!("{}K", tokens.div_ceil(1_000))
    } else {
        tokens.to_string()
    }
}

fn merge_command_session(sessions: &mut Vec<AgentSession>, mut incoming: AgentSession) {
    if let Some(current) = sessions.iter().find(|item| item.id == incoming.id) {
        for message in &mut incoming.messages {
            if let Some(existing) = current
                .messages
                .iter()
                .find(|existing| existing.id == message.id)
                && existing.content.starts_with(&message.content)
            {
                message.content = existing.content.clone();
            }
        }
        for existing in &current.messages {
            if !incoming
                .messages
                .iter()
                .any(|message| message.id == existing.id)
            {
                incoming.messages.push(existing.clone());
            }
        }
    }
    replace_session(sessions, incoming);
}

fn merge_live_command_session(
    sessions: RwSignal<Vec<AgentSession>>,
    tombstones: RwSignal<HashSet<String>>,
    incoming: AgentSession,
) {
    if !tombstones.read().contains(&incoming.id) {
        sessions.update(|items| merge_command_session(items, incoming));
    }
}

fn update_workspace_ui<F>(
    states: RwSignal<HashMap<String, SessionWorkspaceUi>>,
    session_id: &str,
    update: F,
) where
    F: FnOnce(&mut SessionWorkspaceUi),
{
    states.update(|states| {
        let state = states.entry(session_id.to_owned()).or_default();
        update(state);
    });
}

fn session_event_id(event: &SessionEvent) -> &str {
    match event {
        SessionEvent::Snapshot { session } => &session.id,
        SessionEvent::Delta { session_id, .. }
        | SessionEvent::Activity { session_id, .. }
        | SessionEvent::ActivityDelta { session_id, .. }
        | SessionEvent::ContextUsage { session_id, .. }
        | SessionEvent::Removed { session_id } => session_id,
    }
}

#[allow(clippy::too_many_arguments)]
fn consume_session_event(
    event: SessionEvent,
    sessions: RwSignal<Vec<AgentSession>>,
    approvals: RwSignal<Vec<ApprovalRequest>>,
    user_inputs: RwSignal<Vec<ProviderUserInputRequest>>,
    open_session_ids: RwSignal<Vec<String>>,
    current_id: RwSignal<Option<String>>,
    page: RwSignal<Page>,
    draft_open: RwSignal<bool>,
    workspace_states: RwSignal<HashMap<String, SessionWorkspaceUi>>,
    pending_events: RwSignal<HashMap<String, Vec<SessionEvent>>>,
    tombstones: RwSignal<HashSet<String>>,
) {
    let id = session_event_id(&event).to_owned();
    if matches!(event, SessionEvent::Removed { .. }) {
        tombstones.update(|items| {
            items.insert(id.clone());
        });
        pending_events.update(|items| {
            items.remove(&id);
        });
        let mut disposed = None;
        workspace_states.update(|states| {
            disposed = states.remove(&id);
        });
        if let Some(disposed) = disposed {
            for terminal in disposed.terminals {
                spawn_local(async move {
                    let _ = bridge::terminal_close(&terminal.id).await;
                    let _ = bridge::terminal_runtime_forget(&terminal.id).await;
                });
            }
        }
        sessions.update(|items| items.retain(|session| session.id != id));
        approvals.update(|items| items.retain(|request| request.session_id != id));
        user_inputs.update(|items| items.retain(|request| request.session_id != id));
        open_session_ids.update(|items| items.retain(|item| item != &id));
        if current_id.read().as_deref() == Some(id.as_str()) {
            current_id.set(None);
            page.set(if draft_open.get_untracked() {
                Page::Draft
            } else {
                Page::Home
            });
        }
        return;
    }
    if tombstones.read().contains(&id) {
        return;
    }
    if !matches!(event, SessionEvent::Snapshot { .. })
        && !sessions.read().iter().any(|session| session.id == id)
    {
        pending_events.update(|items| {
            items.entry(id).or_default().push(event);
        });
        return;
    }

    let completed_snapshot = match &event {
        SessionEvent::Snapshot { session } if !session.status.is_running() => {
            Some(session.id.clone())
        }
        _ => None,
    };
    sessions.update(|items| apply_session_event(items, event, &storage::timestamp()));
    if let Some(id) = completed_snapshot {
        approvals.update(|items| items.retain(|request| request.session_id != id));
        user_inputs.update(|items| items.retain(|request| request.session_id != id));
    }
    let mut buffered = None;
    pending_events.update(|items| {
        buffered = items.remove(&id);
    });
    if let Some(buffered) = buffered {
        for buffered_event in buffered {
            consume_session_event(
                buffered_event,
                sessions,
                approvals,
                user_inputs,
                open_session_ids,
                current_id,
                page,
                draft_open,
                workspace_states,
                pending_events,
                tombstones,
            );
        }
    }
}

fn selected_wsl_distribution() -> Option<String> {
    let preferences = storage::read_json::<serde_json::Value>(
        storage::DESKTOP_PREFERENCES_KEY,
        serde_json::Value::Null,
    );
    match preferences.get("wslMode").and_then(|value| value.as_str()) {
        Some("default") => Some(String::new()),
        Some("distribution") => preferences
            .get("wslDistribution")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned),
        _ => None,
    }
}

fn apply_cloud_snapshot(raw: &str) {
    let Ok(snapshot) = serde_json::from_str::<serde_json::Value>(raw) else {
        return;
    };
    if let Some(history) = snapshot
        .get("voiceHistory")
        .filter(|value| value.is_array())
    {
        storage::write_json(storage::VOICE_HISTORY_KEY, history);
    }
    if let Some(preferences) = snapshot.get("preferences") {
        if let Some(scheme) = preferences
            .get("colorScheme")
            .and_then(|value| value.as_str())
        {
            storage::set(storage::COLOR_SCHEME_KEY, scheme);
        }
        if let Some(desktop) = preferences.get("desktop").filter(|value| !value.is_null()) {
            storage::write_json(storage::DESKTOP_PREFERENCES_KEY, desktop);
        }
    }
    storage::dispatch("onyx:cloud-hydrated");
    storage::dispatch("onyx:voice-history");
    theme::apply_document_theme();
}

fn cloud_snapshot(sessions: &[AgentSession]) -> String {
    serde_json::json!({
        "version": 1,
        "exportedAt": storage::timestamp(),
        "sessions": sessions,
        "voiceHistory": storage::read_json::<serde_json::Value>(
            storage::VOICE_HISTORY_KEY,
            serde_json::json!([]),
        ),
        "preferences": {
            "colorScheme": storage::get(storage::COLOR_SCHEME_KEY),
            "desktop": storage::read_json::<serde_json::Value>(
                storage::DESKTOP_PREFERENCES_KEY,
                serde_json::Value::Null,
            ),
        },
    })
    .to_string()
}

#[component]
pub fn App() -> impl IntoView {
    if let Some(window_name) = theme::window_name() {
        theme::mark_window(&window_name);
        return match window_name.as_str() {
            "hud" => view! { <Hud /> }.into_any(),
            "agent" => view! { <AgentOverlay /> }.into_any(),
            _ => view! { <Recovery message="Unknown Onyx window." /> }.into_any(),
        };
    }

    let providers = RwSignal::new(demo_providers());
    let sessions = RwSignal::new(Vec::<AgentSession>::new());
    let page = RwSignal::new(Page::default());
    let current_id = RwSignal::new(None::<String>);
    let draft_open = RwSignal::new(false);
    let open_session_ids = RwSignal::new(Vec::<String>::new());

    let draft_provider = RwSignal::new(ProviderId::Claude);
    let draft_title = RwSignal::new(String::new());
    let draft_brand = RwSignal::new(ProviderBrand::Anthropic);
    let draft_model = RwSignal::new(Some("default".to_owned()));
    let draft_reasoning = RwSignal::new(None::<ReasoningEffort>);
    let draft_speed = RwSignal::new(SpeedMode::Standard);
    let draft_interaction = RwSignal::new(InteractionMode::Build);
    let draft_access = RwSignal::new(AccessMode::AutoAcceptEdits);
    let draft_workspace =
        RwSignal::new(storage::get(storage::LAST_WORKSPACE_KEY).unwrap_or_default());
    let draft_git_ready = RwSignal::new(false);
    let catalogs = RwSignal::new(fallback_catalogs());
    let draft_usage = RwSignal::new(None::<ProviderUsage>);
    let current_usage = RwSignal::new(None::<ProviderUsage>);

    let settings_open = RwSignal::new(false);
    let openrouter = RwSignal::new(ConnectionStatus { connected: false });
    let openai = RwSignal::new(ConnectionStatus { connected: false });
    let openrouter_models = RwSignal::new(Vec::<OpenRouterModel>::new());
    let color_scheme = RwSignal::new(ColorScheme::stored());
    let approvals = RwSignal::new(Vec::<ApprovalRequest>::new());
    let user_inputs = RwSignal::new(Vec::<ProviderUserInputRequest>::new());
    let approval_busy = RwSignal::new(false);
    let notice = RwSignal::new(None::<String>);
    let notice_kind = RwSignal::new("error".to_owned());
    let notice_generation = RwSignal::new(0_u64);
    let available_update = RwSignal::new(None::<UpdateInfo>);
    let installing_update = RwSignal::new(false);
    let update_progress = RwSignal::new(None::<UpdateProgress>);
    let update_dialog_open = RwSignal::new(
        storage::get(storage::RELEASE_NOTES_SEEN_KEY).as_deref() != Some(env!("CARGO_PKG_VERSION")),
    );
    let renaming_session = RwSignal::new(None::<String>);
    let rename_value = RwSignal::new(String::new());
    let queued_messages = RwSignal::new(HashMap::<String, Vec<String>>::new());
    let queue_dispatching = RwSignal::new(HashSet::<String>::new());
    let queue_paused = RwSignal::new(HashSet::<String>::new());

    let workspace_states = RwSignal::new(HashMap::<String, SessionWorkspaceUi>::new());
    let pending_events = RwSignal::new(HashMap::<String, Vec<SessionEvent>>::new());
    let queued_events = RwSignal::new(Vec::<SessionEvent>::new());
    let tombstones = RwSignal::new(HashSet::<String>::new());
    let events_ready = RwSignal::new(false);
    let repo = RwSignal::new(None::<RepoSummary>);
    let editors = RwSignal::new(Vec::<EditorTarget>::new());
    let preferred_editor =
        RwSignal::new(storage::get(storage::PREFERRED_EDITOR_KEY).unwrap_or_default());
    let git_busy = RwSignal::new(None::<String>);
    let commit_open = RwSignal::new(false);
    let account_profile = RwSignal::new(None::<AccountProfile>);
    let account_loading = RwSignal::new(true);
    let account_error = RwSignal::new(None::<String>);
    let cloud_configured = RwSignal::new(false);
    let cloud_authenticated = RwSignal::new(false);

    let current = Signal::derive(move || {
        let id = current_id.get()?;
        sessions
            .read()
            .iter()
            .find(|session| session.id == id)
            .cloned()
    });
    let draft_models = Signal::derive(move || {
        models_for_brand(draft_brand.get(), &catalogs.get(), &openrouter_models.get())
    });
    let active_approval = Signal::derive(move || {
        let id = current_id.get()?;
        approvals
            .read()
            .iter()
            .find(|request| request.session_id == id)
            .cloned()
    });
    let active_user_input = Signal::derive(move || {
        let id = current_id.get()?;
        user_inputs
            .read()
            .iter()
            .find(|request| request.session_id == id)
            .cloned()
    });
    let active_ui = Signal::derive(move || {
        current_id
            .get()
            .and_then(|id| workspace_states.read().get(&id).cloned())
            .unwrap_or_default()
    });
    let session_models = Signal::derive(move || {
        current
            .get()
            .map(|session| {
                models_for_brand(
                    session.provider_brand,
                    &catalogs.get(),
                    &openrouter_models.get(),
                )
            })
            .unwrap_or_default()
    });

    let show_notice = Callback::new(move |(message, kind): (String, &'static str)| {
        notice_generation.update(|value| *value += 1);
        let generation = notice_generation.get_untracked();
        notice_kind.set(kind.to_owned());
        notice.set(Some(message));
        spawn_local(async move {
            TimeoutFuture::new(6_500).await;
            if notice_generation.get() == generation {
                notice.set(None);
            }
        });
    });
    let show_error = Callback::new(move |message: String| show_notice.run((message, "error")));

    Effect::new(move |_| {
        let workspace = draft_workspace.get();
        if !workspace.trim().is_empty() {
            storage::set(storage::LAST_WORKSPACE_KEY, &workspace);
        }
    });

    Effect::new(move |_| {
        let provider = draft_provider.get();
        spawn_local(async move {
            draft_usage.set(bridge::provider_usage(provider).await.unwrap_or(None));
        });
    });
    Effect::new(move |_| {
        let visible = !settings_open.get() && !commit_open.get() && !update_dialog_open.get();
        if bridge::is_tauri() {
            spawn_local(async move {
                let _ = bridge::invoke::<(), _>(
                    "internal_browsers_set_visible",
                    &serde_json::json!({ "visible": visible }),
                )
                .await;
            });
        }
    });
    Effect::new(move |_| {
        let Some(session) = current.get() else {
            current_usage.set(None);
            return;
        };
        spawn_local(async move {
            current_usage.set(
                bridge::provider_usage(session.provider)
                    .await
                    .unwrap_or(None),
            );
        });
    });
    Effect::new(move |_| {
        let Some(session) = current.get() else {
            repo.set(None);
            return;
        };
        if session.status.is_running() {
            return;
        }
        spawn_local(async move {
            let result = bridge::repo_summary(&session.workspace).await;
            if current_id.read().as_deref() == Some(session.id.as_str()) {
                match result {
                    Ok(summary) => repo.set(Some(summary)),
                    Err(cause) => {
                        repo.set(None);
                        show_error.run(cause);
                    }
                }
            }
        });
    });

    Effect::new(move |_| {
        let version = sessions
            .read()
            .iter()
            .map(|session| format!("{}:{}:{:?}", session.id, session.updated_at, session.status))
            .collect::<String>();
        if version.is_empty() || !cloud_authenticated.get() {
            return;
        }
        spawn_local(async move {
            TimeoutFuture::new(1_500).await;
            if cloud_authenticated.get() {
                let _ = bridge::push_cloud(&cloud_snapshot(&sessions.get())).await;
            }
        });
    });

    let open_draft = Callback::new(move |workspace: Option<String>| {
        let is_new = !draft_open.get_untracked();
        if is_new {
            draft_title.set(String::new());
            rename_value.set(String::new());
            renaming_session.set(Some(DRAFT_TAB_ID.to_owned()));
        }
        if let Some(workspace) = workspace {
            draft_workspace.set(workspace);
            draft_git_ready.set(false);
        }
        draft_open.set(true);
        current_id.set(None);
        page.set(Page::Draft);
    });
    let open_session = Callback::new(move |id: String| {
        if !sessions.read().iter().any(|session| session.id == id) {
            return;
        }
        open_session_ids.update(|items| {
            if !items.contains(&id) {
                items.push(id.clone());
            }
        });
        current_id.set(Some(id));
        page.set(Page::Session);
    });
    let choose_workspace = Callback::new(move |_: ()| {
        spawn_local(async move {
            match bridge::choose_workspace().await {
                Ok(Some(workspace)) => open_draft.run(Some(workspace)),
                Ok(None) => {}
                Err(cause) => show_error.run(cause),
            }
        });
    });
    let close_tab = Callback::new(move |id: String| {
        if id == DRAFT_TAB_ID {
            draft_open.set(false);
            if page.get() == Page::Draft {
                if let Some(fallback) = open_session_ids.read().last().cloned() {
                    open_session.run(fallback);
                } else {
                    page.set(Page::Home);
                }
            }
            return;
        }
        if let Some(ui) = workspace_states.read().get(&id).cloned() {
            for terminal in ui.terminals {
                let terminal_id = terminal.id;
                spawn_local(async move {
                    let _ = bridge::terminal_close(&terminal_id).await;
                    let _ = bridge::terminal_runtime_forget(&terminal_id).await;
                });
            }
        }
        workspace_states.update(|states| {
            states.remove(&id);
        });
        open_session_ids.update(|items| items.retain(|item| item != &id));
        if current_id.read().as_deref() == Some(id.as_str()) {
            current_id.set(None);
            if let Some(fallback) = open_session_ids.read().last().cloned() {
                open_session.run(fallback);
            } else if draft_open.get() {
                open_draft.run(None);
            } else {
                page.set(Page::Home);
            }
        }
    });
    let delete_session = Callback::new(move |id: String| {
        queued_messages.update(|queues| {
            queues.remove(&id);
        });
        queue_dispatching.update(|ids| {
            ids.remove(&id);
        });
        queue_paused.update(|ids| {
            ids.remove(&id);
        });
        spawn_local(async move {
            match bridge::delete_session(&id).await {
                Ok(()) => {
                    consume_session_event(
                        SessionEvent::Removed { session_id: id },
                        sessions,
                        approvals,
                        user_inputs,
                        open_session_ids,
                        current_id,
                        page,
                        draft_open,
                        workspace_states,
                        pending_events,
                        tombstones,
                    );
                }
                Err(cause) => show_error.run(cause),
            }
        });
    });
    let begin_rename = Callback::new(move |id: String| {
        if id == DRAFT_TAB_ID {
            rename_value.set(draft_title.get_untracked());
            renaming_session.set(Some(id));
            return;
        }
        let Some(session) = sessions
            .read_untracked()
            .iter()
            .find(|session| session.id == id)
            .cloned()
        else {
            show_error.run("Session not found".to_owned());
            return;
        };
        rename_value.set(session.title);
        renaming_session.set(Some(session.id));
    });
    let save_rename = Callback::new(move |_: ()| {
        let Some(id) = renaming_session.get_untracked() else {
            return;
        };
        let title = rename_value.get_untracked();
        if id == DRAFT_TAB_ID {
            let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
            if title.is_empty() {
                show_error.run("Choose a name for this session.".to_owned());
                return;
            }
            if title.chars().count() > 80 || title.chars().any(char::is_control) {
                show_error
                    .run("Session names must contain at most 80 supported characters.".to_owned());
                return;
            }
            draft_title.set(title);
            renaming_session.set(None);
            return;
        }
        spawn_local(async move {
            match bridge::rename_session(&id, &title).await {
                Ok(session) => {
                    merge_live_command_session(sessions, tombstones, session);
                    renaming_session.set(None);
                }
                Err(cause) => show_error.run(cause),
            }
        });
    });

    let select_draft_brand = Callback::new(move |brand: ProviderBrand| {
        draft_brand.set(brand);
        draft_provider.set(runtime_for_brand(brand));
        let models = models_for_brand(brand, &catalogs.get(), &openrouter_models.get());
        if let Some(model) = selected_or_default(&models) {
            draft_model.set(Some(model.id));
            draft_reasoning.set(
                model
                    .default_reasoning
                    .or_else(|| model.reasoning.first().copied()),
            );
            draft_speed.set(model.default_speed);
        } else {
            draft_model.set(None);
            draft_reasoning.set(None);
        }
    });
    let select_draft_model = Callback::new(move |id: String| {
        draft_model.set(Some(id.clone()));
        if let Some(model) = draft_models.read().iter().find(|model| model.id == id) {
            draft_reasoning.set(
                model
                    .default_reasoning
                    .or_else(|| model.reasoning.first().copied()),
            );
            draft_speed.set(model.default_speed);
        }
    });
    let start_session = Callback::new(move |content: String| {
        spawn_local(async move {
            let title = draft_title.get();
            if title.trim().is_empty() {
                show_error.run("Choose a name for this session.".to_owned());
                return;
            }
            let workspace = if draft_workspace.get().trim().is_empty() {
                match bridge::choose_workspace().await {
                    Ok(Some(workspace)) => {
                        draft_workspace.set(workspace.clone());
                        workspace
                    }
                    Ok(None) => return,
                    Err(cause) => {
                        show_error.run(cause);
                        return;
                    }
                }
            } else {
                draft_workspace.get()
            };
            if draft_provider.get() == ProviderId::Openrouter
                && draft_model.get().as_deref().is_none_or(str::is_empty)
            {
                show_error.run("Choose an OpenRouter model first.".to_owned());
                return;
            }
            let input = CreateSessionInput {
                title,
                provider: draft_provider.get(),
                provider_brand: draft_brand.get(),
                model: draft_model.get(),
                reasoning: draft_reasoning.get(),
                speed_mode: draft_speed.get(),
                interaction_mode: draft_interaction.get(),
                access_mode: draft_access.get(),
                workspace,
            };
            let result = async {
                let session = bridge::create_session(input).await?;
                let id = session.id.clone();
                tombstones.update(|items| {
                    items.remove(&id);
                });
                sessions.update(|items| replace_session(items, session));
                draft_title.set(String::new());
                draft_open.set(false);
                open_session_ids.update(|items| {
                    if !items.contains(&id) {
                        items.push(id.clone());
                    }
                });
                current_id.set(Some(id.clone()));
                page.set(Page::Session);
                bridge::send_message_id(&id, &content).await
            }
            .await;
            match result {
                Ok(session) => {
                    merge_live_command_session(sessions, tombstones, session);
                }
                Err(cause) => show_error.run(cause),
            }
        });
    });
    let continue_session = Callback::new(move |content: String| {
        let Some(session) = current.get() else {
            return;
        };
        spawn_local(async move {
            match bridge::send_message_id(&session.id, &content).await {
                Ok(session) => merge_live_command_session(sessions, tombstones, session),
                Err(cause) => show_error.run(cause),
            }
        });
    });
    let queue_session = Callback::new(move |content: String| {
        let Some(session) = current.get_untracked() else {
            return;
        };
        let content = content.trim().to_owned();
        if content.is_empty() {
            return;
        }
        if content.len() > 24 * 1024 {
            show_error.run("Queued CLI messages are limited to 24 KiB.".to_owned());
            return;
        }
        let accepted = queued_messages.try_update(|queues| {
            let queue = queues.entry(session.id.clone()).or_default();
            let total_bytes = queue.iter().map(String::len).sum::<usize>() + content.len();
            if queue.len() >= 8 || total_bytes > 96 * 1024 {
                return false;
            }
            queue.push(content);
            true
        });
        queue_paused.update(|ids| {
            ids.remove(&session.id);
        });
        if accepted != Some(true) {
            show_error.run("The queue is full (8 messages or 96 KiB maximum).".to_owned());
        }
    });
    let steer_queued_session = Callback::new(move |_: ()| {
        let Some(session) = current.get_untracked() else {
            return;
        };
        let session_id = session.id;
        let content = queued_messages
            .try_update(|queues| {
                let queue = queues.get_mut(&session_id)?;
                let content = (!queue.is_empty()).then(|| queue.remove(0))?;
                if queue.is_empty() {
                    queues.remove(&session_id);
                }
                Some(content)
            })
            .flatten();
        let Some(content) = content else {
            return;
        };
        spawn_local(async move {
            match bridge::steer_turn(&session_id, &content).await {
                Ok(session) => merge_live_command_session(sessions, tombstones, session),
                Err(cause) => {
                    queued_messages.update(|queues| {
                        queues.entry(session_id).or_default().insert(0, content);
                    });
                    show_error.run(cause);
                }
            }
        });
    });
    Effect::new(move |_| {
        let queued = queued_messages.get();
        let dispatching = queue_dispatching.get();
        let paused = queue_paused.get();
        let ready = sessions
            .get()
            .into_iter()
            .filter(|session| {
                !session.status.is_running()
                    && queued
                        .get(&session.id)
                        .is_some_and(|messages| !messages.is_empty())
                    && !dispatching.contains(&session.id)
                    && !paused.contains(&session.id)
            })
            .map(|session| session.id)
            .collect::<Vec<_>>();
        for session_id in ready {
            let content = queued_messages
                .try_update(|queues| {
                    let queue = queues.get_mut(&session_id)?;
                    let content = (!queue.is_empty()).then(|| queue.remove(0))?;
                    if queue.is_empty() {
                        queues.remove(&session_id);
                    }
                    Some(content)
                })
                .flatten();
            let Some(content) = content else {
                continue;
            };
            queue_dispatching.update(|ids| {
                ids.insert(session_id.clone());
            });
            spawn_local(async move {
                match bridge::send_message_id(&session_id, &content).await {
                    Ok(session) => merge_live_command_session(sessions, tombstones, session),
                    Err(cause) => {
                        queued_messages.update(|queues| {
                            queues
                                .entry(session_id.clone())
                                .or_default()
                                .insert(0, content);
                        });
                        queue_paused.update(|ids| {
                            ids.insert(session_id.clone());
                        });
                        show_error.run(format!("Queued message was preserved: {cause}"));
                    }
                }
                queue_dispatching.update(|ids| {
                    ids.remove(&session_id);
                });
            });
        }
    });
    let cancel_session = Callback::new(move |_: ()| {
        let Some(session) = current.get() else {
            return;
        };
        spawn_local(async move {
            if let Err(cause) = bridge::cancel_turn(&session.id).await {
                show_error.run(cause);
            }
        });
    });
    let respond_approval = Callback::new(move |(allow, for_session): (bool, bool)| {
        let Some(request) = active_approval.get() else {
            return;
        };
        approval_busy.set(true);
        spawn_local(async move {
            match bridge::respond_approval(&request.id, allow, for_session).await {
                Ok(()) => approvals.update(|items| items.retain(|item| item.id != request.id)),
                Err(cause) => show_error.run(cause),
            }
            approval_busy.set(false);
        });
    });
    let update_options = Callback::new(move |patch: SessionOptionPatch| {
        let Some(session) = current.get() else {
            return;
        };
        let mut input = UpdateSessionOptionsInput {
            session_id: session.id.clone(),
            provider: session.provider,
            provider_brand: session.provider_brand,
            model: session.model.clone(),
            reasoning: session.reasoning,
            speed_mode: session.speed_mode,
            interaction_mode: session.interaction_mode,
            access_mode: session.access_mode,
        };
        match patch {
            SessionOptionPatch::Brand(brand) => {
                input.provider_brand = brand;
                input.provider = runtime_for_brand(brand);
                let models = models_for_brand(brand, &catalogs.get(), &openrouter_models.get());
                let selected = selected_or_default(&models);
                input.model = selected.as_ref().map(|model| model.id.clone());
                input.reasoning = selected.as_ref().and_then(|model| {
                    model
                        .default_reasoning
                        .or_else(|| model.reasoning.first().copied())
                });
                input.speed_mode = selected
                    .as_ref()
                    .map(|model| model.default_speed)
                    .unwrap_or_default();
            }
            SessionOptionPatch::Model(id) => {
                input.model = Some(id.clone());
                if let Some(model) = session_models.read().iter().find(|model| model.id == id) {
                    input.reasoning = model
                        .default_reasoning
                        .or_else(|| model.reasoning.first().copied());
                    input.speed_mode = model.default_speed;
                }
            }
            SessionOptionPatch::Reasoning(value) => input.reasoning = Some(value),
            SessionOptionPatch::Speed(value) => input.speed_mode = value,
            SessionOptionPatch::Interaction(value) => input.interaction_mode = value,
            SessionOptionPatch::Access(value) => input.access_mode = value,
        }
        spawn_local(async move {
            match bridge::update_session_options(input).await {
                Ok(session) => {
                    merge_live_command_session(sessions, tombstones, session);
                    show_notice.run((
                        "Session options saved. The provider reconnects on the next turn."
                            .to_owned(),
                        "success",
                    ));
                }
                Err(cause) => show_error.run(cause),
            }
        });
    });

    let initialize_draft_git = Callback::new(move |_: ()| {
        let workspace = draft_workspace.get();
        if workspace.is_empty() {
            choose_workspace.run(());
            return;
        }
        spawn_local(async move {
            match bridge::init_git(&workspace).await {
                Ok(result) => {
                    draft_git_ready.set(true);
                    show_notice.run((result.message, "success"));
                }
                Err(cause) => show_error.run(cause),
            }
        });
    });
    let initialize_current_git = Callback::new(move |_: ()| {
        let Some(session) = current.get() else {
            return;
        };
        if repo.get().is_some_and(|summary| summary.is_repo) {
            return;
        }
        spawn_local(async move {
            match bridge::init_git(&session.workspace).await {
                Ok(result) => {
                    show_notice.run((result.message, "success"));
                    match bridge::repo_summary(&session.workspace).await {
                        Ok(summary) => repo.set(Some(summary)),
                        Err(cause) => show_error.run(cause),
                    }
                }
                Err(cause) => show_error.run(cause),
            }
        });
    });
    let open_workspace = Callback::new(move |_: ()| {
        let Some(session) = current.get() else {
            return;
        };
        let candidates = editors.get();
        let preferred = preferred_editor.get();
        let target = candidates
            .iter()
            .find(|editor| editor.id == preferred && editor.available)
            .or_else(|| candidates.iter().find(|editor| editor.available))
            .cloned();
        match target {
            Some(target) => spawn_local(async move {
                if let Err(cause) = bridge::open_workspace(&session.workspace, &target.id).await {
                    show_error.run(cause);
                }
            }),
            None => show_error.run("No supported editor or file manager is available.".to_owned()),
        }
    });
    let choose_editor = Callback::new(move |target: String| {
        preferred_editor.set(target.clone());
        storage::set(storage::PREFERRED_EDITOR_KEY, &target);
        let Some(session) = current.get() else {
            return;
        };
        spawn_local(async move {
            if let Err(cause) = bridge::open_workspace(&session.workspace, &target).await {
                show_error.run(cause);
            }
        });
    });

    let commit_workspace = Callback::new(move |message: Option<String>| {
        let Some(session) = current.get() else {
            return;
        };
        if session.status.is_running() {
            show_error.run("Wait for the active agent turn before committing.".to_owned());
            return;
        }
        if git_busy.get_untracked().is_some() {
            return;
        }
        git_busy.set(Some("commit".to_owned()));
        spawn_local(async move {
            match bridge::commit_workspace(&session.workspace, message).await {
                Ok(result) => {
                    commit_open.set(false);
                    show_notice.run((result.message, "success"));
                    repo.set(bridge::repo_summary(&session.workspace).await.ok());
                }
                Err(cause) => show_error.run(cause),
            }
            git_busy.set(None);
        });
    });
    let push_workspace = Callback::new(move |_: ()| {
        let Some(session) = current.get() else {
            return;
        };
        if session.status.is_running() {
            show_error.run("Wait for the active agent turn before pushing.".to_owned());
            return;
        }
        if git_busy.get_untracked().is_some() {
            return;
        }
        git_busy.set(Some("push".to_owned()));
        spawn_local(async move {
            match bridge::push_workspace(&session.workspace).await {
                Ok(result) => {
                    show_notice.run((result.message, "success"));
                    repo.set(bridge::repo_summary(&session.workspace).await.ok());
                }
                Err(cause) => show_error.run(cause),
            }
            git_busy.set(None);
        });
    });
    let create_pr = Callback::new(move |_: ()| {
        let Some(session) = current.get() else {
            return;
        };
        if session.status.is_running() {
            show_error.run("Wait for the active agent turn before creating a PR.".to_owned());
            return;
        }
        if git_busy.get_untracked().is_some() {
            return;
        }
        git_busy.set(Some("create-pr".to_owned()));
        spawn_local(async move {
            match bridge::create_pull_request(&session.workspace).await {
                Ok(result) => {
                    show_notice.run((result.message, "success"));
                    if let Some(url) = result.url {
                        let _ = bridge::open_url(&url).await;
                    }
                    repo.set(bridge::repo_summary(&session.workspace).await.ok());
                }
                Err(cause) => show_error.run(cause),
            }
            git_busy.set(None);
        });
    });

    let add_surface = Callback::new(move |kind: WorkspaceSurfaceKind| {
        let Some(session) = current.get() else {
            return;
        };
        spawn_local(async move {
            let resource_id = if kind == WorkspaceSurfaceKind::Terminal {
                match bridge::terminal_open(
                    &session.workspace,
                    100,
                    30,
                    selected_wsl_distribution(),
                )
                .await
                {
                    Ok(terminal) => Some(terminal.id),
                    Err(cause) => {
                        show_error.run(cause);
                        return;
                    }
                }
            } else {
                None
            };
            let title = if kind == WorkspaceSurfaceKind::Terminal {
                let count = workspace_states
                    .read()
                    .get(&session.id)
                    .map(|ui| {
                        ui.surfaces
                            .iter()
                            .filter(|surface| surface.kind == WorkspaceSurfaceKind::Terminal)
                            .count()
                            + 1
                    })
                    .unwrap_or(1);
                format!("Terminal {count}")
            } else {
                kind.label().to_owned()
            };
            let surface = WorkspaceSurface {
                id: storage::unique_id("surface"),
                kind,
                title,
                resource_id,
                pending: false,
            };
            let active = surface.id.clone();
            update_workspace_ui(workspace_states, &session.id, |ui| {
                ui.right_panel_open = true;
                ui.surfaces.push(surface);
                ui.active_surface_id = Some(active);
            });
        });
    });
    let close_surface = Callback::new(move |surface_id: String| {
        let Some(session) = current.get() else {
            return;
        };
        let mut terminal_to_close = None;
        update_workspace_ui(workspace_states, &session.id, |ui| {
            if let Some(index) = ui
                .surfaces
                .iter()
                .position(|surface| surface.id == surface_id)
            {
                let removed = ui.surfaces.remove(index);
                if removed.kind == WorkspaceSurfaceKind::Terminal {
                    terminal_to_close = removed.resource_id;
                }
                if ui.active_surface_id.as_deref() == Some(surface_id.as_str()) {
                    ui.active_surface_id = ui
                        .surfaces
                        .get(index.min(ui.surfaces.len().saturating_sub(1)))
                        .map(|surface| surface.id.clone());
                }
            }
        });
        if let Some(id) = terminal_to_close {
            spawn_local(async move {
                let _ = bridge::terminal_close(&id).await;
                let _ = bridge::terminal_runtime_forget(&id).await;
            });
        }
    });
    let toggle_right = Callback::new(move |_: ()| {
        let Some(session) = current.get() else {
            return;
        };
        update_workspace_ui(workspace_states, &session.id, |ui| {
            ui.right_panel_open = !ui.right_panel_open;
        });
    });
    let new_terminal = Callback::new(move |_: ()| {
        let Some(session) = current.get() else {
            return;
        };
        spawn_local(async move {
            match bridge::terminal_open(&session.workspace, 120, 24, selected_wsl_distribution())
                .await
            {
                Ok(opened) => {
                    update_workspace_ui(workspace_states, &session.id, |ui| {
                        let terminal = WorkspaceTerminal {
                            id: opened.id,
                            title: format!("Terminal {}", ui.terminals.len() + 1),
                            cwd: opened.cwd,
                            status: "running".to_owned(),
                        };
                        ui.active_terminal_id = Some(terminal.id.clone());
                        ui.terminals.push(terminal);
                        ui.bottom_panel_open = true;
                    });
                }
                Err(cause) => show_error.run(cause),
            }
        });
    });
    let open_provider_cli = Callback::new(move |_: ()| {
        let Some(session) = current.get_untracked() else {
            return;
        };
        if session.status.is_running() {
            show_error.run("Wait for the active turn before opening the official CLI.".to_owned());
            return;
        }
        spawn_local(async move {
            match bridge::provider_terminal_open(
                session.provider,
                "interactive",
                Some(&session.workspace),
                session.provider_session_id.as_deref(),
            )
            .await
            {
                Ok(opened) => {
                    update_workspace_ui(workspace_states, &session.id, |ui| {
                        let terminal = WorkspaceTerminal {
                            id: opened.id,
                            title: format!("{} CLI", session.provider.display_name()),
                            cwd: opened.cwd,
                            status: "running".to_owned(),
                        };
                        ui.active_terminal_id = Some(terminal.id.clone());
                        ui.terminals.push(terminal);
                        ui.bottom_panel_open = true;
                    });
                }
                Err(cause) => show_error.run(cause),
            }
        });
    });
    let toggle_bottom = Callback::new(move |_: ()| {
        let Some(session) = current.get() else {
            return;
        };
        let needs_terminal = workspace_states
            .read()
            .get(&session.id)
            .is_none_or(|ui| ui.terminals.is_empty());
        update_workspace_ui(workspace_states, &session.id, |ui| {
            ui.bottom_panel_open = !ui.bottom_panel_open;
        });
        if needs_terminal
            && workspace_states
                .read()
                .get(&session.id)
                .is_some_and(|ui| ui.bottom_panel_open)
        {
            new_terminal.run(());
        }
    });
    let close_terminal = Callback::new(move |terminal_id: String| {
        let Some(session) = current.get() else {
            return;
        };
        update_workspace_ui(workspace_states, &session.id, |ui| {
            if let Some(index) = ui
                .terminals
                .iter()
                .position(|terminal| terminal.id == terminal_id)
            {
                ui.terminals.remove(index);
                if ui.active_terminal_id.as_deref() == Some(terminal_id.as_str()) {
                    ui.active_terminal_id = ui
                        .terminals
                        .get(index.min(ui.terminals.len().saturating_sub(1)))
                        .map(|terminal| terminal.id.clone());
                }
                if ui.terminals.is_empty() {
                    ui.bottom_panel_open = false;
                }
            }
        });
        spawn_local(async move {
            let _ = bridge::terminal_close(&terminal_id).await;
            let _ = bridge::terminal_runtime_forget(&terminal_id).await;
        });
    });
    let clear_terminal = Callback::new(move |terminal_id: String| {
        spawn_local(async move {
            let _ = bridge::terminal_runtime_clear(&terminal_id).await;
        });
    });

    let sign_out = Callback::new(move |_: ()| {
        spawn_local(async move {
            match bridge::clerk_sign_out().await {
                Ok(()) => account_profile.set(None),
                Err(cause) => show_error.run(cause),
            }
        });
    });

    let tabs = Signal::derive(move || {
        let mut result = Vec::new();
        if draft_open.get() {
            result.push(TitlebarTab {
                id: DRAFT_TAB_ID.to_owned(),
                label: {
                    let title = draft_title.get();
                    if title.is_empty() {
                        "New session".to_owned()
                    } else {
                        title
                    }
                },
                active: page.get() == Page::Draft,
                running: false,
                project_initial: workspace_name(&draft_workspace.get())
                    .chars()
                    .next()
                    .unwrap_or('+')
                    .to_ascii_uppercase(),
            });
        }
        for id in open_session_ids.get() {
            if let Some(session) = sessions.read().iter().find(|session| session.id == id) {
                result.push(TitlebarTab {
                    id: session.id.clone(),
                    label: if session.title.is_empty() {
                        "Untitled session".to_owned()
                    } else {
                        session.title.clone()
                    },
                    active: page.get() == Page::Session
                        && current_id.read().as_deref() == Some(session.id.as_str()),
                    running: session.status.is_running(),
                    project_initial: workspace_name(&session.workspace)
                        .chars()
                        .next()
                        .unwrap_or('O')
                        .to_ascii_uppercase(),
                });
            }
        }
        result
    });
    let closed_sessions = Signal::derive(move || {
        sessions
            .read()
            .iter()
            .filter(|session| !open_session_ids.read().contains(&session.id))
            .map(|session| TitlebarSession {
                id: session.id.clone(),
                label: session.title.clone(),
                project: workspace_name(&session.workspace),
            })
            .collect()
    });

    Effect::new(move |_| {
        spawn_local(async move {
            match bridge::list_sessions().await {
                Ok(mut session_list) => {
                    session_list.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
                    sessions.set(session_list);
                }
                Err(cause) => show_error.run(format!("Could not restore sessions: {cause}")),
            }

            events_ready.set(true);
            let queued = queued_events.get_untracked();
            queued_events.set(Vec::new());
            for event in queued {
                consume_session_event(
                    event,
                    sessions,
                    approvals,
                    user_inputs,
                    open_session_ids,
                    current_id,
                    page,
                    draft_open,
                    workspace_states,
                    pending_events,
                    tombstones,
                );
            }

            match bridge::list_providers().await {
                Ok(provider_list) => {
                    providers.set(provider_list.clone());
                    let available = provider_list
                        .iter()
                        .find(|provider| provider.id == draft_provider.get() && provider.available)
                        .or_else(|| provider_list.iter().find(|provider| provider.available));
                    if let Some(available) = available {
                        let brand = ProviderBrand::for_provider(available.id);
                        select_draft_brand.run(brand);
                    }
                }
                Err(cause) => show_error.run(format!("Could not discover providers: {cause}")),
            }

            match bridge::openrouter_status().await {
                Ok(status) => {
                    openrouter.set(status);
                    if status.connected {
                        openrouter_models
                            .set(bridge::openrouter_models().await.unwrap_or_default());
                    }
                }
                Err(cause) => {
                    show_error.run(format!("Could not read the OpenRouter status: {cause}"))
                }
            }
            match bridge::openai_status().await {
                Ok(status) => openai.set(status),
                Err(cause) => show_error.run(format!("Could not read the OpenAI status: {cause}")),
            }
            match bridge::workspace_editors().await {
                Ok(editor_list) => {
                    editors.set(editor_list.clone());
                    if !editor_list
                        .iter()
                        .any(|editor| editor.id == preferred_editor.get() && editor.available)
                        && let Some(editor) = editor_list.iter().find(|editor| editor.available)
                    {
                        preferred_editor.set(editor.id.clone());
                    }
                }
                Err(cause) => show_error.run(format!("Could not discover editors: {cause}")),
            }

            for provider in ProviderId::ALL {
                if provider == ProviderId::Openrouter {
                    continue;
                }
                if let Ok(models) = bridge::provider_models(provider).await
                    && !models.is_empty()
                {
                    catalogs.update(|catalogs| {
                        catalogs.insert(provider, models);
                    });
                }
            }
        });

        if bridge::is_tauri() {
            spawn_local(async move {
                match bridge::listen::<SessionEvent, _>("onyx://session", move |event| {
                    if events_ready.get_untracked() {
                        consume_session_event(
                            event,
                            sessions,
                            approvals,
                            user_inputs,
                            open_session_ids,
                            current_id,
                            page,
                            draft_open,
                            workspace_states,
                            pending_events,
                            tombstones,
                        );
                    } else {
                        queued_events.update(|items| items.push(event));
                    }
                })
                .await
                {
                    Ok(listener) => listener.forget(),
                    Err(cause) => show_error.run(cause),
                }
            });
            spawn_local(async move {
                match bridge::listen::<ApprovalRequest, _>("onyx://approval", move |request| {
                    approvals.update(|items| {
                        if !items.iter().any(|item| item.id == request.id) {
                            items.push(request);
                        }
                    });
                })
                .await
                {
                    Ok(listener) => listener.forget(),
                    Err(cause) => show_error.run(cause),
                }
            });
            spawn_local(async move {
                match bridge::listen::<ProviderUserInputRequest, _>(
                    "onyx://user-input",
                    move |request| {
                        user_inputs.update(|items| {
                            if !items.iter().any(|item| item.id == request.id) {
                                items.push(request);
                            }
                        });
                    },
                )
                .await
                {
                    Ok(listener) => listener.forget(),
                    Err(cause) => show_error.run(cause),
                }
            });
            spawn_local(async move {
                match bridge::listen::<TerminalEvent, _>("onyx://terminal", move |event| {
                    let id = event.session_id.clone();
                    match event.kind.as_str() {
                        "data" => {
                            if let Some(data) = event.data {
                                spawn_local(async move {
                                    let _ = bridge::terminal_runtime_write(&id, &data).await;
                                });
                            }
                        }
                        "exit" | "error" => {
                            let error = (event.kind == "error")
                                .then_some(event.data.as_deref())
                                .flatten()
                                .map(str::to_owned);
                            let exit_code = event.exit_code;
                            let runtime_id = id.clone();
                            spawn_local(async move {
                                let _ = bridge::terminal_runtime_exit(
                                    &runtime_id,
                                    exit_code,
                                    error.as_deref(),
                                )
                                .await;
                            });
                            workspace_states.update(|states| {
                                for ui in states.values_mut() {
                                    if let Some(terminal) =
                                        ui.terminals.iter_mut().find(|terminal| terminal.id == id)
                                    {
                                        terminal.status = "exited".to_owned();
                                    }
                                }
                            });
                        }
                        _ => {}
                    }
                })
                .await
                {
                    Ok(listener) => listener.forget(),
                    Err(cause) => show_error.run(cause),
                }
            });
            spawn_local(async move {
                match bridge::listen::<AccountEvent, _>("onyx://account-changed", move |event| {
                    if let Some(error) = event.error {
                        account_error.set(Some(error));
                    } else {
                        account_profile.set(event.profile);
                        account_error.set(None);
                    }
                    account_loading.set(false);
                })
                .await
                {
                    Ok(listener) => listener.forget(),
                    Err(cause) => account_error.set(Some(cause)),
                }
            });
            spawn_local(async move {
                if let Ok(listener) = bridge::listen::<NativeVoicePermissions, _>(
                    "onyx://native-permissions",
                    move |permissions| {
                        if !permissions.accessibility || !permissions.input_monitoring {
                            show_error.run(
                                "Voice needs macOS Accessibility and Input Monitoring permissions. Open Settings → Voice to enable them."
                                    .to_owned(),
                            );
                        }
                    },
                )
                .await
                {
                    listener.forget();
                }
            });
        }
    });

    Effect::new(move |_| {
        spawn_local(async move {
            if cfg!(debug_assertions) {
                account_loading.set(false);
            } else {
                match bridge::account_profile().await {
                    Ok(profile) => {
                        account_profile.set(profile);
                        account_loading.set(false);
                    }
                    Err(cause) => {
                        account_error.set(Some(cause));
                        account_loading.set(false);
                    }
                }
            }
            cloud_configured.set(bridge::cloud_configured().await.unwrap_or(false));
            match bridge::start_cloud_auth(move |authenticated| {
                cloud_authenticated.set(authenticated);
                if authenticated {
                    spawn_local(async move {
                        if let Ok(Some(raw)) = bridge::pull_cloud().await {
                            apply_cloud_snapshot(&raw);
                        }
                    });
                }
            }) {
                Ok(Some(handle)) => std::mem::forget(handle),
                Ok(None) => {}
                Err(cause) => account_error.set(Some(cause)),
            }
        });
    });

    Effect::new(move |_| {
        spawn_local(async move {
            TimeoutFuture::new(5_000).await;
            if let Ok(update) = bridge::check_update().await {
                available_update.set(update);
            }
        });
    });

    let install_banner_update = Callback::new(move |_: ()| {
        installing_update.set(true);
        update_progress.set(None);
        update_dialog_open.set(true);
        spawn_local(async move {
            if let Err(cause) = bridge::install_update(move |downloaded, total| {
                update_progress.set(Some(UpdateProgress {
                    downloaded,
                    total,
                    finished: false,
                }));
            })
            .await
            {
                installing_update.set(false);
                show_error.run(cause);
            }
        });
    });

    let keyboard_callback = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
        let modifier = event.meta_key() || event.ctrl_key();
        if modifier && event.key() == "," {
            event.prevent_default();
            if !commit_open.get() {
                settings_open.set(true);
            }
        } else if modifier && event.key().eq_ignore_ascii_case("n") {
            event.prevent_default();
            if !settings_open.get() && !commit_open.get() {
                open_draft.run(None);
            }
        } else if modifier
            && event.key().eq_ignore_ascii_case("j")
            && page.get() == Page::Session
            && !settings_open.get()
            && !commit_open.get()
        {
            event.prevent_default();
            if event.shift_key() {
                toggle_right.run(());
            } else {
                toggle_bottom.run(());
            }
        } else if event.key() == "Escape"
            && current
                .get()
                .is_some_and(|session| session.status.is_running())
        {
            event.prevent_default();
            cancel_session.run(());
        }
    }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);
    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback(
            "keydown",
            keyboard_callback.as_ref().unchecked_ref(),
        );
        keyboard_callback.forget();
    }

    let page_view = move || match page.get() {
        Page::Home => view! {
            <HomeView
                sessions=Signal::derive(move || sessions.get())
                draft_workspace=Signal::derive(move || draft_workspace.get())
                on_new=open_draft
                on_select=open_session
                on_delete=delete_session
                on_choose_workspace=choose_workspace
                on_settings=Callback::new(move |_| settings_open.set(true))
                on_voice=Callback::new(move |_| page.set(Page::Voice))
            />
        }
        .into_any(),
        Page::Voice => view! {
            <VoiceHistoryView on_settings=Callback::new(move |_| settings_open.set(true)) />
        }
        .into_any(),
        Page::Draft => {
            let draft_context = Signal::derive(move || {
                draft_models
                    .read()
                    .iter()
                    .find(|model| Some(model.id.as_str()) == draft_model.read().as_deref())
                    .and_then(|model| model.context_length)
            });
            view! {
                <section class="zai-new-session">
                    <div class="zai-new-session__stage">
                        <div class="zai-new-session__content">
                            <div class="zai-new-session__composer">
                                <Composer
                                    provider=Signal::derive(move || draft_provider.get())
                                    brand=Signal::derive(move || draft_brand.get())
                                    model=Signal::derive(move || draft_model.get())
                                    reasoning=Signal::derive(move || draft_reasoning.get())
                                    speed_mode=Signal::derive(move || draft_speed.get())
                                    interaction_mode=Signal::derive(move || draft_interaction.get())
                                    access_mode=Signal::derive(move || draft_access.get())
                                    workspace=Signal::derive(move || draft_workspace.get())
                                    providers=Signal::derive(move || providers.get())
                                    models=draft_models
                                    locked=Signal::derive(move || false)
                                    running=Signal::derive(move || false)
                                    approval=Signal::derive(move || None)
                                    approval_busy=Signal::derive(move || false)
                                    queued_messages=Signal::derive(Vec::new)
                                    hero=true
                                    autofocus=false
                                    on_brand=select_draft_brand
                                    on_model=select_draft_model
                                    on_reasoning=Callback::new(move |value| draft_reasoning.set(Some(value)))
                                    on_speed_mode=Callback::new(move |value| draft_speed.set(value))
                                    on_interaction_mode=Callback::new(move |value| draft_interaction.set(value))
                                    on_access_mode=Callback::new(move |value| draft_access.set(value))
                                    on_submit=start_session
                                    on_queue=Callback::new(move |_| {})
                                    on_steer_queued=Callback::new(move |_| {})
                                    on_cancel=Callback::new(move |_| {})
                                    on_approval=Callback::new(move |_| {})
                                    on_error=show_error
                                />
                            </div>
                            <div class="zai-new-session__workspace-row">
                                <button on:click=move |_| choose_workspace.run(()) title=move || draft_workspace.get()>
                                    <span class="zai-project-avatar">
                                        {move || workspace_name(&draft_workspace.get()).chars().next().unwrap_or('P').to_ascii_uppercase()}
                                    </span>
                                    <span>{move || if draft_workspace.get().is_empty() { "Choose project".to_owned() } else { workspace_name(&draft_workspace.get()) }}</span>
                                    <span class="zai-workspace-chevron">"⌄"</span>
                                </button>
                                <span class="zai-workspace-divider">"/"</span>
                                <button class="zai-git-status" on:click=move |_| initialize_draft_git.run(())>
                                    <Icon icon=LuGitBranch width="14px" height="14px" />
                                    {move || if draft_git_ready.get() { "main" } else { "No Git" }}
                                </button>
                                <span class="zai-workspace-divider">"/"</span>
                                <span class="zai-draft-usage">
                                    <Icon icon=LuGauge width="14px" height="14px" />
                                    {move || format!("Context {}", draft_context.get().map(compact_tokens).unwrap_or_else(|| "—".to_owned()))}
                                </span>
                                <span class="zai-workspace-divider">"/"</span>
                                <span class="zai-draft-usage">
                                    <Icon icon=LuTimerReset width="14px" height="14px" />
                                    {move || draft_usage.get().filter(|usage| !usage.windows.is_empty()).map(|usage| {
                                        usage.windows.into_iter().map(|window| format!("{} {:.0}%", window.label, window.used_percent)).collect::<Vec<_>>().join(" · ")
                                    }).unwrap_or_else(|| "Usage not reported".to_owned())}
                                </span>
                            </div>
                        </div>
                    </div>
                </section>
            }
            .into_any()
        }
        Page::Session => {
            let Some(session) = current.get() else {
                return view! { <Recovery message="The selected session is unavailable." /> }
                    .into_any();
            };
            let session_id = session.id.clone();
            let activate_surface_session_id = session_id.clone();
            let close_surface_panel_session_id = session_id.clone();
            let activate_terminal_session_id = session_id.clone();
            let close_terminal_panel_session_id = session_id.clone();
            let workspace = session.workspace.clone();
            let project = workspace_name(&workspace);
            let running = Signal::derive(move || {
                current
                    .get()
                    .is_some_and(|session| session.status.is_running())
            });
            let workspace_signal = Signal::derive(move || {
                current
                    .get()
                    .map(|session| session.workspace)
                    .unwrap_or_default()
            });
            let context_label = Signal::derive(move || {
                let Some(session) = current.get() else {
                    return "Context not reported".to_owned();
                };
                let max = session
                    .context_usage
                    .as_ref()
                    .and_then(|usage| usage.max_tokens)
                    .or_else(|| {
                        session_models
                            .read()
                            .iter()
                            .find(|model| Some(model.id.as_str()) == session.model.as_deref())
                            .and_then(|model| model.context_length)
                    });
                max.map(|max| {
                    format!(
                        "Context {} / {}",
                        compact_tokens(
                            session
                                .context_usage
                                .as_ref()
                                .map(|usage| usage.used_tokens)
                                .unwrap_or_default()
                        ),
                        compact_tokens(max),
                    )
                })
                .unwrap_or_else(|| "Context not reported".to_owned())
            });
            view! {
                <section class="zai-session-page">
                    <div class="zai-session-panel">
                        <div class="zai-session-workspace">
                            <div class="zai-session-body">
                                <section class="zai-conversation-pane">
                                    <header class="zai-workspace-header">
                                        <div class="zai-workspace-header__identity">
                                            <span class="zai-project-avatar">{project.chars().next().unwrap_or('P').to_ascii_uppercase()}</span>
                                            <span class="zai-workspace-header__project">{project}</span>
                                            <span class="zai-workspace-header__slash">"/"</span>
                                            <div class="zai-session-title">
                                                <h2 title=session.title.clone()>{session.title.clone()}</h2>
                                            </div>
                                            <Show when=move || repo.get().and_then(|repo| repo.branch).is_some()>
                                                <span class="zai-workspace-header__branch">
                                                    <Icon icon=LuGitBranch width="13px" height="13px" />
                                                    {move || repo.get().and_then(|repo| repo.branch).unwrap_or_default()}
                                                </span>
                                            </Show>
                                        </div>
                                        <WorkspaceTopbarActions
                                            repo=Signal::derive(move || repo.get())
                                            editors=Signal::derive(move || editors.get())
                                            preferred_editor=Signal::derive(move || preferred_editor.get())
                                            bottom_panel_open=Signal::derive(move || active_ui.get().bottom_panel_open)
                                            right_panel_open=Signal::derive(move || active_ui.get().right_panel_open)
                                            git_busy=Signal::derive(move || git_busy.get())
                                            on_open=open_workspace
                                            on_open_target=choose_editor
                                            on_commit=Callback::new(move |_| commit_open.set(true))
                                            on_push=push_workspace
                                            on_create_pr=create_pr
                                            on_open_cli=open_provider_cli
                                            on_toggle_bottom=toggle_bottom
                                            on_toggle_right=toggle_right
                                        />
                                    </header>

                                    <Transcript
                                        session=Signal::derive(move || current.get())
                                        profile=Signal::derive(move || account_profile.get())
                                    />
                                    <For
                                        each=move || active_user_input.get().into_iter()
                                        key=|request| request.id.clone()
                                        children=move |request| {
                                            let resolved_id = request.id.clone();
                                            view! {
                                                <UserInputCard
                                                    request
                                                    disabled=Signal::derive(move || false)
                                                    on_resolved=Callback::new(move |_| {
                                                        user_inputs.update(|items| {
                                                            items.retain(|item| item.id != resolved_id);
                                                        });
                                                    })
                                                    on_error=show_error
                                                />
                                            }
                                        }
                                    />
                                    <div class="zai-session-composer">
                                        <Composer
                                            provider=Signal::derive(move || current.get().map(|session| session.provider).unwrap_or(ProviderId::Claude))
                                            brand=Signal::derive(move || current.get().map(|session| session.provider_brand).unwrap_or(ProviderBrand::Anthropic))
                                            model=Signal::derive(move || current.get().and_then(|session| session.model))
                                            reasoning=Signal::derive(move || current.get().and_then(|session| session.reasoning))
                                            speed_mode=Signal::derive(move || current.get().map(|session| session.speed_mode).unwrap_or_default())
                                            interaction_mode=Signal::derive(move || current.get().map(|session| session.interaction_mode).unwrap_or_default())
                                            access_mode=Signal::derive(move || current.get().map(|session| session.access_mode).unwrap_or_default())
                                            workspace=workspace_signal
                                            providers=Signal::derive(move || providers.get())
                                            models=session_models
                                            locked=running
                                            running=running
                                            steerable=session.provider == ProviderId::Claude || session.provider == ProviderId::Codex
                                            approval=active_approval
                                            approval_busy=Signal::derive(move || approval_busy.get())
                                            queued_messages=Signal::derive(move || {
                                                current_id
                                                    .read()
                                                    .as_ref()
                                                    .and_then(|id| queued_messages.read().get(id).cloned())
                                                    .unwrap_or_default()
                                            })
                                            hero=false
                                            autofocus=false
                                            on_brand=Callback::new(move |value| update_options.run(SessionOptionPatch::Brand(value)))
                                            on_model=Callback::new(move |value| update_options.run(SessionOptionPatch::Model(value)))
                                            on_reasoning=Callback::new(move |value| update_options.run(SessionOptionPatch::Reasoning(value)))
                                            on_speed_mode=Callback::new(move |value| update_options.run(SessionOptionPatch::Speed(value)))
                                            on_interaction_mode=Callback::new(move |value| update_options.run(SessionOptionPatch::Interaction(value)))
                                            on_access_mode=Callback::new(move |value| update_options.run(SessionOptionPatch::Access(value)))
                                            on_submit=continue_session
                                            on_queue=queue_session
                                            on_steer_queued=steer_queued_session
                                            on_cancel=cancel_session
                                            on_approval=respond_approval
                                            on_error=show_error
                                        />
                                        <div class="zai-new-session__workspace-row zai-session-statusbar">
                                            <button on:click=move |_| open_workspace.run(()) title=workspace.clone()>
                                                <span class="zai-project-avatar">{workspace_name(&workspace).chars().next().unwrap_or('P').to_ascii_uppercase()}</span>
                                                <span>{workspace_name(&workspace)}</span>
                                                <span class="zai-workspace-chevron">"⌄"</span>
                                            </button>
                                            <span class="zai-workspace-divider">"/"</span>
                                            <button class="zai-git-status" on:click=move |_| initialize_current_git.run(())>
                                                <Icon icon=LuGitBranch width="14px" height="14px" />
                                                {move || repo.get().filter(|repo| repo.is_repo).and_then(|repo| repo.branch).unwrap_or_else(|| "No Git".to_owned())}
                                            </button>
                                            <span class="zai-workspace-divider">"/"</span>
                                            <span><Icon icon=LuGauge width="14px" height="14px" />{move || context_label.get()}</span>
                                            <span class="zai-workspace-divider">"/"</span>
                                            <span><Icon icon=LuTimerReset width="14px" height="14px" />{move || current_usage.get().filter(|usage| !usage.windows.is_empty()).map(|usage| usage.windows.into_iter().map(|window| format!("{} {:.0}%", window.label, window.used_percent)).collect::<Vec<_>>().join(" · ")).unwrap_or_else(|| "Usage not reported".to_owned())}</span>
                                        </div>
                                    </div>
                                </section>

                                <RightWorkspacePanel
                                    ui=active_ui
                                    workspace=workspace_signal
                                    repo_is_ready=Signal::derive(move || repo.get().is_some_and(|repo| repo.is_repo))
                                    on_activate=Callback::new(move |id| update_workspace_ui(workspace_states, &activate_surface_session_id, |ui| ui.active_surface_id = Some(id)))
                                    on_close_surface=close_surface
                                    on_add_surface=add_surface
                                    on_close_panel=Callback::new(move |_| update_workspace_ui(workspace_states, &close_surface_panel_session_id, |ui| ui.right_panel_open = false))
                                    on_error=show_error
                                />
                            </div>

                            <BottomTerminalPanel
                                ui=active_ui
                                on_activate=Callback::new(move |id| update_workspace_ui(workspace_states, &activate_terminal_session_id, |ui| ui.active_terminal_id = Some(id)))
                                on_close_terminal=close_terminal
                                on_new_terminal=new_terminal
                                on_clear=clear_terminal
                                on_close_panel=Callback::new(move |_| update_workspace_ui(workspace_states, &close_terminal_panel_session_id, |ui| ui.bottom_panel_open = false))
                            />
                        </div>
                    </div>
                </section>
            }
            .into_any()
        }
    };

    let shell = move || {
        view! {
            <div class="zai-shell" data-page=move || page.get().as_str()>
            <Titlebar
                tabs=tabs
                sessions=closed_sessions
                on_select=Callback::new(move |id| if id == DRAFT_TAB_ID { open_draft.run(None) } else { open_session.run(id) })
                on_close=close_tab
                on_delete=delete_session
                on_new=Callback::new(move |_| open_draft.run(None))
                on_open_session=open_session
                editing_tab=Signal::derive(move || renaming_session.get())
                rename_value=Signal::derive(move || rename_value.get())
                on_begin_rename=begin_rename
                on_rename_input=Callback::new(move |value| rename_value.set(value))
                on_commit_rename=save_rename
                on_cancel_rename=Callback::new(move |_| renaming_session.set(None))
                on_home=Callback::new(move |_| page.set(Page::Home))
                on_settings=Callback::new(move |_| settings_open.set(true))
                on_sign_out=sign_out
                update=Signal::derive(move || available_update.get())
                on_update=Callback::new(move |_| update_dialog_open.set(true))
                profile=Signal::derive(move || account_profile.get())
                show_layout_controls=Signal::derive(move || false)
                bottom_panel_open=Signal::derive(move || page.get() == Page::Session && active_ui.get().bottom_panel_open)
                right_panel_open=Signal::derive(move || page.get() == Page::Session && active_ui.get().right_panel_open)
                bottom_panel_available=Signal::derive(move || false)
                right_panel_available=Signal::derive(move || false)
                on_toggle_bottom_panel=toggle_bottom
                on_toggle_right_panel=toggle_right
            />

            <main class="zai-main">{page_view}</main>

            <SettingsDialog
                open=Signal::derive(move || settings_open.get())
                sessions=Signal::derive(move || sessions.get())
                providers=providers
                catalogs=Signal::derive(move || catalogs.get())
                openrouter=openrouter
                openai=openai
                openrouter_models=openrouter_models
                color_scheme=color_scheme
                profile=Signal::derive(move || account_profile.get())
                cloud_configured=Signal::derive(move || cloud_configured.get())
                cloud_authenticated=Signal::derive(move || cloud_authenticated.get())
                on_close=Callback::new(move |_| settings_open.set(false))
                on_sign_out=sign_out
            />
            <GitCommitDialog
                open=Signal::derive(move || commit_open.get())
                summary=Signal::derive(move || repo.get())
                busy=Signal::derive(move || git_busy.read().as_deref() == Some("commit"))
                on_close=Callback::new(move |_| commit_open.set(false))
                on_commit=commit_workspace
            />
            <UpdateDialog
                open=Signal::derive(move || update_dialog_open.get())
                update=Signal::derive(move || available_update.get())
                installing=Signal::derive(move || installing_update.get())
                progress=Signal::derive(move || update_progress.get())
                on_close=Callback::new(move |_| {
                    update_dialog_open.set(false);
                    if available_update.read().is_none() {
                        storage::set(
                            storage::RELEASE_NOTES_SEEN_KEY,
                            env!("CARGO_PKG_VERSION"),
                        );
                    }
                })
                on_install=install_banner_update
            />

            <Show when=move || notice.get().is_some()>
                <div class="zai-toast" data-kind=move || notice_kind.get() role="status">
                    {move || notice.get().unwrap_or_default()}
                </div>
            </Show>
            </div>
        }
    };

    view! {
        <AccountGate
            profile=Signal::derive(move || account_profile.get())
            loading=Signal::derive(move || account_loading.get())
            error=Signal::derive(move || account_error.get())
        >
            {shell}
        </AccountGate>
    }
    .into_any()
}

#[component]
fn Recovery(#[prop(into)] message: String) -> impl IntoView {
    view! {
        <main class="onyx-recovery" role="alert">
            <div>
                <strong>"Onyx needs to reload"</strong>
                <p>{message}</p>
                <button on:click=move |_| {
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().reload();
                    }
                }>
                    "Reload Onyx"
                </button>
            </div>
        </main>
    }
}
