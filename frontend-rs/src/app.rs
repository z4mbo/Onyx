use icondata::{LuGauge, LuGitBranch, LuSettings, LuTimerReset, LuX};
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen_futures::spawn_local;

use crate::{
    bridge,
    components::{AgentOverlay, Composer, HomeView, Hud, Titlebar, TitlebarTab, Transcript},
    model::{
        AccessMode, AgentSession, CreateSessionInput, InteractionMode, ProviderBrand, ProviderId,
        ReasoningEffort, SessionEvent, SpeedMode, apply_session_event, demo_providers,
        replace_session, workspace_name,
    },
    theme,
};

const DRAFT_TAB_ID: &str = "onyx:draft";
const LAST_WORKSPACE_KEY: &str = "onyx.last-workspace";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Home,
    Draft,
    Session,
}

impl Page {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Draft => "draft",
            Self::Session => "session",
        }
    }
}

fn stored_workspace() -> String {
    web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item(LAST_WORKSPACE_KEY).ok().flatten())
        .unwrap_or_default()
}

fn remember_workspace(workspace: &str) {
    if let Some(storage) =
        web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.set_item(LAST_WORKSPACE_KEY, workspace);
    }
}

fn current_timestamp() -> String {
    js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default()
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

    let page = RwSignal::new(Page::Home);
    let sessions = RwSignal::new(Vec::<AgentSession>::new());
    let providers = RwSignal::new(demo_providers());
    let draft_workspace = RwSignal::new(stored_workspace());
    let draft_provider = RwSignal::new(ProviderId::Claude);
    let draft_reasoning = RwSignal::new(ReasoningEffort::Medium);
    let draft_interaction = RwSignal::new(InteractionMode::Build);
    let draft_access = RwSignal::new(AccessMode::ApprovalRequired);
    let active_session_id = RwSignal::new(None::<String>);
    let open_session_ids = RwSignal::new(Vec::<String>::new());
    let draft_open = RwSignal::new(false);
    let settings_open = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let current_session = Signal::derive(move || {
        let id = active_session_id.get()?;
        sessions
            .read()
            .iter()
            .find(|session| session.id == id)
            .cloned()
    });
    let tabs = Signal::derive(move || {
        let mut tabs = Vec::new();
        if draft_open.get() {
            tabs.push(TitlebarTab {
                id: DRAFT_TAB_ID.to_owned(),
                label: "New session".to_owned(),
                active: page.get() == Page::Draft,
                running: false,
                project_initial: workspace_name(&draft_workspace.get())
                    .chars()
                    .next()
                    .unwrap_or('O')
                    .to_ascii_uppercase(),
            });
        }
        for id in open_session_ids.get() {
            if let Some(session) = sessions.read().iter().find(|session| session.id == id) {
                tabs.push(TitlebarTab {
                    id: session.id.clone(),
                    label: session.title.clone(),
                    active: page.get() == Page::Session
                        && active_session_id.read().as_deref() == Some(session.id.as_str()),
                    running: session.status.is_running(),
                    project_initial: workspace_name(&session.workspace)
                        .chars()
                        .next()
                        .unwrap_or('O')
                        .to_ascii_uppercase(),
                });
            }
        }
        tabs
    });

    Effect::new(move |_| {
        spawn_local(async move {
            match bridge::list_providers().await {
                Ok(result) => providers.set(result),
                Err(message) => error.set(Some(message)),
            }
        });
        spawn_local(async move {
            match bridge::list_sessions().await {
                Ok(mut result) => {
                    result.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
                    sessions.set(result);
                }
                Err(message) => error.set(Some(message)),
            }
        });
        if bridge::is_tauri() {
            spawn_local(async move {
                match bridge::listen::<SessionEvent, _>("onyx://session", move |event| {
                    let now = current_timestamp();
                    sessions.update(|sessions| apply_session_event(sessions, event, &now));
                })
                .await
                {
                    // This is a root component whose lifetime matches the WebView.
                    Ok(result) => result.forget(),
                    Err(message) => error.set(Some(message)),
                }
            });
        }
    });

    let open_draft = Callback::new(move |workspace: Option<String>| {
        if let Some(workspace) = workspace {
            remember_workspace(&workspace);
            draft_workspace.set(workspace);
        }
        draft_open.set(true);
        page.set(Page::Draft);
    });
    let open_session = Callback::new(move |id: String| {
        open_session_ids.update(|ids| {
            if !ids.contains(&id) {
                ids.push(id.clone());
            }
        });
        active_session_id.set(Some(id));
        page.set(Page::Session);
    });
    let choose_workspace = Callback::new(move |_: ()| {
        spawn_local(async move {
            match bridge::choose_workspace().await {
                Ok(Some(workspace)) => {
                    remember_workspace(&workspace);
                    draft_workspace.set(workspace);
                    draft_open.set(true);
                    page.set(Page::Draft);
                }
                Ok(None) => {}
                Err(message) => error.set(Some(message)),
            }
        });
    });
    let remove_session = Callback::new(move |id: String| {
        spawn_local(async move {
            if let Err(message) = bridge::delete_session(&id).await {
                error.set(Some(message));
                return;
            }
            sessions.update(|sessions| sessions.retain(|session| session.id != id));
            open_session_ids.update(|ids| ids.retain(|item| item != &id));
            if active_session_id.read().as_deref() == Some(id.as_str()) {
                active_session_id.set(None);
                page.set(Page::Home);
            }
        });
    });
    let submit_draft = Callback::new(move |content: String| {
        spawn_local(async move {
            error.set(None);
            let workspace = if draft_workspace.get().is_empty() {
                match bridge::choose_workspace().await {
                    Ok(Some(workspace)) => {
                        remember_workspace(&workspace);
                        draft_workspace.set(workspace.clone());
                        workspace
                    }
                    Ok(None) => return,
                    Err(message) => {
                        error.set(Some(message));
                        return;
                    }
                }
            } else {
                draft_workspace.get()
            };
            let input = CreateSessionInput {
                provider: draft_provider.get(),
                provider_brand: ProviderBrand::for_provider(draft_provider.get()),
                model: None,
                reasoning: Some(draft_reasoning.get()),
                speed_mode: SpeedMode::Standard,
                interaction_mode: draft_interaction.get(),
                access_mode: draft_access.get(),
                workspace,
            };
            let result = async {
                let session = bridge::create_session(input).await?;
                bridge::send_message(&session, &content).await
            }
            .await;
            match result {
                Ok(session) => {
                    let id = session.id.clone();
                    sessions.update(|sessions| replace_session(sessions, session));
                    open_session_ids.update(|ids| {
                        if !ids.contains(&id) {
                            ids.push(id.clone());
                        }
                    });
                    active_session_id.set(Some(id));
                    draft_open.set(false);
                    page.set(Page::Session);
                }
                Err(message) => error.set(Some(message)),
            }
        });
    });
    let continue_session = Callback::new(move |content: String| {
        let Some(session) = current_session.get() else {
            return;
        };
        spawn_local(async move {
            match bridge::send_message(&session, &content).await {
                Ok(session) => sessions.update(|sessions| replace_session(sessions, session)),
                Err(message) => error.set(Some(message)),
            }
        });
    });
    let close_tab = Callback::new(move |id: String| {
        if id == DRAFT_TAB_ID {
            draft_open.set(false);
            if page.get() == Page::Draft {
                page.set(Page::Home);
            }
            return;
        }
        open_session_ids.update(|ids| ids.retain(|item| item != &id));
        if active_session_id.read().as_deref() == Some(id.as_str()) {
            active_session_id.set(None);
            page.set(Page::Home);
        }
    });
    let select_tab = Callback::new(move |id: String| {
        if id == DRAFT_TAB_ID {
            page.set(Page::Draft);
        } else {
            open_session.run(id);
        }
    });

    let page_view = move || {
        match page.get() {
        Page::Home => view! {
            <HomeView
                sessions=Signal::derive(move || sessions.get())
                draft_workspace=Signal::derive(move || draft_workspace.get())
                on_new=open_draft
                on_select=open_session
                on_delete=remove_session
                on_choose_workspace=choose_workspace
                on_settings=Callback::new(move |_| settings_open.set(true))
                on_chat=Callback::new(move |_| error.set(Some("Subscription chat is not ported to the Rust preview yet.".to_owned())))
                on_voice=Callback::new(move |_| error.set(Some("Voice history is not ported to the Rust preview yet.".to_owned())))
            />
        }
        .into_any(),
        Page::Draft => view! {
            <section class="zai-new-session">
                <div class="zai-new-session__stage">
                    <div class="zai-new-session__content">
                        <div class="zai-new-session__composer">
                            <Composer
                                provider=draft_provider
                                reasoning=draft_reasoning
                                interaction_mode=draft_interaction
                                access_mode=draft_access
                                workspace=Signal::derive(move || draft_workspace.get())
                                providers=Signal::derive(move || providers.get())
                                hero=true
                                running=false
                                on_submit=submit_draft
                                on_attach=Callback::new(move |_| error.set(Some("File attachment is the next Rust migration slice.".to_owned())))
                            />
                        </div>
                        <div class="zai-new-session__workspace-row">
                            <button
                                on:click=move |_| choose_workspace.run(())
                                title=move || if draft_workspace.get().is_empty() {
                                    "Choose a project".to_owned()
                                } else {
                                    draft_workspace.get()
                                }
                            >
                                <span class="zai-project-avatar">
                                    {move || workspace_name(&draft_workspace.get())
                                        .chars()
                                        .next()
                                        .unwrap_or('P')
                                        .to_ascii_uppercase()}
                                </span>
                                <span>{move || if draft_workspace.get().is_empty() {
                                    "Choose project".to_owned()
                                } else {
                                    workspace_name(&draft_workspace.get())
                                }}</span>
                                <span class="zai-workspace-chevron">"⌄"</span>
                            </button>
                            <span class="zai-workspace-divider">"/"</span>
                            <button class="zai-git-status">
                                <Icon icon=LuGitBranch width="14px" height="14px" />
                                "No Git"
                            </button>
                            <span class="zai-workspace-divider">"/"</span>
                            <span class="zai-draft-usage">
                                <Icon icon=LuGauge width="14px" height="14px" />
                                "Context —"
                            </span>
                            <span class="zai-workspace-divider">"/"</span>
                            <span class="zai-draft-usage">
                                <Icon icon=LuTimerReset width="14px" height="14px" />
                                "Usage not reported"
                            </span>
                        </div>
                    </div>
                </div>
            </section>
        }
        .into_any(),
        Page::Session => (move || {
            match current_session.get() {
                Some(session) => {
                    let session_signal = Signal::derive(move || current_session.get());
                    let workspace = session.workspace.clone();
                    let project = workspace_name(&workspace);
                    let title = session.title.clone();
                    view! {
                        <section class="zai-session-page">
                            <div class="zai-session-panel">
                                <div class="zai-session-workspace">
                                    <div class="zai-session-body">
                                        <section class="zai-conversation-pane">
                                            <header class="zai-workspace-header">
                                                <div class="zai-workspace-header__identity">
                                                    <span class="zai-project-avatar">
                                                        {project.chars().next().unwrap_or('P').to_ascii_uppercase()}
                                                    </span>
                                                    <span class="zai-workspace-header__project">{project}</span>
                                                    <span aria-hidden="true" class="zai-workspace-header__slash">"/"</span>
                                                    <h2 title=title.clone()>{title.clone()}</h2>
                                                </div>
                                            </header>
                                            <Transcript session=session_signal />
                                            <div class="zai-session-composer">
                                                <Composer
                                                    provider=draft_provider
                                                    reasoning=draft_reasoning
                                                    interaction_mode=draft_interaction
                                                    access_mode=draft_access
                                                    workspace=Signal::derive(move || workspace.clone())
                                                    providers=Signal::derive(move || providers.get())
                                                    hero=false
                                                    running=session.status.is_running()
                                                    on_submit=continue_session
                                                    on_attach=Callback::new(move |_| error.set(Some("File attachment is the next Rust migration slice.".to_owned())))
                                                />
                                                <div class="zai-new-session__workspace-row zai-session-statusbar">
                                                    <button title=session.workspace>
                                                        <span class="zai-project-avatar">
                                                            {workspace_name(&session.workspace).chars().next().unwrap_or('P').to_ascii_uppercase()}
                                                        </span>
                                                        <span>{workspace_name(&session.workspace)}</span>
                                                        <span class="zai-workspace-chevron">"⌄"</span>
                                                    </button>
                                                    <span class="zai-workspace-divider">"/"</span>
                                                    <button class="zai-git-status">
                                                        <Icon icon=LuGitBranch width="14px" height="14px" />
                                                        "Git"
                                                    </button>
                                                </div>
                                            </div>
                                        </section>
                                    </div>
                                </div>
                            </div>
                        </section>
                    }
                    .into_any()
                }
                None => view! {
                    <Recovery message="The selected session is unavailable." />
                }
                .into_any(),
            }
        })
        .into_any(),
    }
    };

    view! {
        <div class="zai-shell" data-page=move || page.get().as_str()>
            <Titlebar
                tabs=tabs
                on_select=select_tab
                on_close=close_tab
                on_new=Callback::new(move |_| open_draft.run(None))
                on_home=Callback::new(move |_| page.set(Page::Home))
                on_settings=Callback::new(move |_| settings_open.set(true))
            />

            <main class="zai-main">{page_view}</main>

            <Show when=move || settings_open.get()>
                <div class="zai-modal-scrim">
                    <section
                        class="zai-settings-dialog"
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="zai-settings-page-title"
                        tabindex="-1"
                    >
                        <aside class="zai-settings-sidebar">
                            <div>
                                <h2>"Desktop"</h2>
                                <nav>
                                    <button class="active">
                                        <Icon icon=LuSettings width="17px" height="17px" />
                                        "General"
                                    </button>
                                </nav>
                            </div>
                            <div class="zai-settings-version">
                                <strong>"Onyx Desktop"</strong><span>"Rust preview · v0.2.0"</span>
                            </div>
                        </aside>
                        <div class="zai-settings-content">
                            <button
                                class="zai-settings-close"
                                on:click=move |_| settings_open.set(false)
                                aria-label="Close settings"
                            >
                                <Icon icon=LuX width="16px" height="16px" />
                            </button>
                            <div class="zai-settings-page">
                                <h1 id="zai-settings-page-title">"General"</h1>
                                <section class="zai-settings-card">
                                    <div class="zai-setting-row">
                                        <div>
                                            <strong>"Frontend"</strong>
                                            <span>"Leptos compiled to WebAssembly; production remains on Solid during migration"</span>
                                        </div>
                                        <span class="zai-setting-value">"Rust preview"</span>
                                    </div>
                                    <div class="zai-setting-row">
                                        <div>
                                            <strong>"Regression policy"</strong>
                                            <span>"The entrypoint cannot switch until visual and functional parity pass"</span>
                                        </div>
                                        <span class="zai-setting-value">"Protected"</span>
                                    </div>
                                </section>
                            </div>
                        </div>
                    </section>
                </div>
            </Show>

            {move || error.get().map(|message| view! {
                <div class="zai-toast" role="status">
                    <span>{message}</span>
                    <button
                        class="zai-update-dismiss"
                        on:click=move |_| error.set(None)
                        aria-label="Dismiss"
                    >
                        "✕"
                    </button>
                </div>
            })}
        </div>
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
