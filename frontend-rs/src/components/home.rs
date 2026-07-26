use std::collections::BTreeMap;

use icondata::{
    LuCircleHelp, LuFolderPlus, LuGitBranch, LuMic, LuSearch, LuSettings, LuSquarePen, LuTrash2,
};
use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos_icons::Icon;

use crate::model::{AgentSession, workspace_name};

#[derive(Clone, PartialEq)]
struct Project {
    path: String,
    name: String,
    sessions: Vec<AgentSession>,
}

fn projects_for(sessions: Vec<AgentSession>, draft_workspace: String, query: &str) -> Vec<Project> {
    let mut by_path = BTreeMap::<String, Vec<AgentSession>>::new();
    for session in sessions {
        by_path
            .entry(session.workspace.clone())
            .or_default()
            .push(session);
    }
    if !draft_workspace.is_empty() {
        by_path.entry(draft_workspace).or_default();
    }

    let needle = query.trim().to_ascii_lowercase();
    by_path
        .into_iter()
        .map(|(path, sessions)| {
            let sessions = if needle.is_empty() {
                sessions
            } else {
                sessions
                    .into_iter()
                    .filter(|session| {
                        format!(
                            "{} {} {:?} {}",
                            session.title,
                            session.workspace,
                            session.provider,
                            session.model.as_deref().unwrap_or_default(),
                        )
                        .to_ascii_lowercase()
                        .contains(&needle)
                    })
                    .collect()
            };
            Project {
                name: workspace_name(&path),
                path,
                sessions,
            }
        })
        .collect()
}

#[component]
pub fn HomeView(
    sessions: Signal<Vec<AgentSession>>,
    draft_workspace: Signal<String>,
    on_new: Callback<Option<String>>,
    on_select: Callback<String>,
    on_delete: Callback<String>,
    on_choose_workspace: Callback<()>,
    on_settings: Callback<()>,
    on_voice: Callback<()>,
) -> impl IntoView {
    let (query, set_query) = signal(String::new());
    let projects = Memo::new(move |_| {
        projects_for(sessions.get(), draft_workspace.get(), query.get().as_str())
    });
    let has_projects = move || !projects.read().is_empty();
    let has_sessions = move || {
        projects
            .read()
            .iter()
            .any(|project| !project.sessions.is_empty())
    };

    view! {
        <section class="zai-page-frame zai-home-view">
            <div class="zai-home-layout">
                <aside class="zai-home-sidebar">
                    <div class="zai-home-projects-heading">
                        <h2>"Projects"</h2>
                        <button
                            class="zai-icon-button"
                            aria-label="Add project"
                            on:click=move |_| on_choose_workspace.run(())
                        >
                            <Icon icon=LuFolderPlus width="17px" height="17px" />
                        </button>
                    </div>

                    <div class="zai-home-project-list">
                        <Show
                            when=has_projects
                            fallback=move || view! {
                                <button
                                    class="zai-home-project zai-home-project--empty"
                                    on:click=move |_| on_choose_workspace.run(())
                                >
                                    <span class="zai-project-avatar">"+"</span>
                                    <span>"Add project"</span>
                                </button>
                            }
                        >
                            <For
                                each=move || projects.get()
                                key=|project| project.path.clone()
                                children=move |project| {
                                    let path = project.path.clone();
                                    let action_path = project.path.clone();
                                    let initial = project.name.chars().next().unwrap_or('P').to_ascii_uppercase();
                                    let action_label = format!("New session in {}", project.name);
                                    view! {
                                        <div class="zai-home-project-row">
                                            <button
                                                class="zai-home-project"
                                                on:click=move |_| on_new.run(Some(path.clone()))
                                            >
                                                <span class="zai-project-avatar">{initial}</span>
                                                <span>{project.name}</span>
                                            </button>
                                            <button
                                                class="zai-home-project-action"
                                                aria-label=action_label
                                                on:click=move |_| on_new.run(Some(action_path.clone()))
                                            >
                                                <Icon icon=LuSquarePen width="14px" height="14px" />
                                            </button>
                                        </div>
                                    }
                                }
                            />
                        </Show>
                    </div>

                    <nav class="zai-home-nav" aria-label="Application">
                        <button on:click=move |_| on_voice.run(())>
                            <Icon icon=LuMic width="15px" height="15px" />
                            <span>"Voice history"</span>
                        </button>
                        <button on:click=move |_| on_settings.run(())>
                            <Icon icon=LuSettings width="15px" height="15px" />
                            <span>"Settings"</span>
                        </button>
                        <a
                            href="https://github.com/z4mbo/Onyx#readme"
                            target="_blank"
                            rel="noreferrer"
                        >
                            <Icon icon=LuCircleHelp width="15px" height="15px" />
                            <span>"Help"</span>
                        </a>
                    </nav>
                </aside>

                <div class="zai-home-main">
                    <label class="zai-home-search">
                        <Icon icon=LuSearch width="16px" height="16px" />
                        <input
                            prop:value=move || query.get()
                            on:input=move |event| set_query.set(event_target_value(&event))
                            placeholder="Search sessions"
                            aria-label="Search sessions"
                        />
                    </label>

                    <div class="zai-home-results">
                        <Show
                            when=has_sessions
                            fallback=move || {
                                let searching = !query.get().is_empty();
                                view! {
                                    <div class="zai-home-empty">
                                        <strong>{if searching { "No matching sessions" } else { "Nothing here yet" }}</strong>
                                        <span>{if searching { "Try another search" } else { "Create a session to get started" }}</span>
                                        <Show when=move || !searching>
                                            <button
                                                class="zai-neutral-button"
                                                on:click=move |_| on_new.run(None)
                                            >
                                                <Icon icon=LuSquarePen width="15px" height="15px" />
                                                "New session"
                                            </button>
                                        </Show>
                                    </div>
                                }
                            }
                        >
                            <div class="zai-session-history">
                                <For
                                    each=move || {
                                        projects
                                            .get()
                                            .into_iter()
                                            .filter(|project| !project.sessions.is_empty())
                                            .collect::<Vec<_>>()
                                    }
                                    key=|project| project.path.clone()
                                    children=move |project| {
                                        let project_path = project.path.clone();
                                        let project_initial = project.name.chars().next().unwrap_or('P').to_ascii_uppercase();
                                        let count = project.sessions.len();
                                        let count_label = format!("{count} session{}", if count == 1 { "" } else { "s" });
                                        let action_label = format!("New session in {}", project.name);
                                        view! {
                                            <section class="onyx-project-sessions">
                                                <header>
                                                    <span class="zai-project-avatar">{project_initial}</span>
                                                    <div><strong>{project.name}</strong><small>{count_label}</small></div>
                                                    <button
                                                        on:click=move |_| on_new.run(Some(project_path.clone()))
                                                        aria-label=action_label
                                                    >
                                                        <Icon icon=LuSquarePen width="14px" height="14px" />
                                                    </button>
                                                </header>
                                                <For
                                                    each=move || project.sessions.clone()
                                                    key=|session| session.id.clone()
                                                    children=move |session| {
                                                        let select_id = session.id.clone();
                                                        let delete_id = session.id.clone();
                                                        let delete_label = format!("Delete {}", session.title);
                                                        let model = session.model.clone().unwrap_or_else(|| "Default model".to_owned());
                                                        let provider = format!("{:?}", session.provider).to_ascii_lowercase();
                                                        view! {
                                                            <div class="zai-session-history-item">
                                                                <button
                                                                    class="zai-session-history-row"
                                                                    on:click=move |_| on_select.run(select_id.clone())
                                                                >
                                                                    <div class="zai-session-history-copy">
                                                                        <strong>{session.title}</strong>
                                                                        <span>{model}</span>
                                                                    </div>
                                                                    <div class="zai-session-history-meta">
                                                                        <Icon icon=LuGitBranch width="13px" height="13px" />
                                                                        <span>{provider}</span>
                                                                    </div>
                                                                </button>
                                                                <button
                                                                    type="button"
                                                                    class="zai-session-history-delete"
                                                                    aria-label=delete_label
                                                                    title="Delete session"
                                                                    on:click=move |event: MouseEvent| {
                                                                        event.stop_propagation();
                                                                        on_delete.run(delete_id.clone());
                                                                    }
                                                                >
                                                                    <Icon icon=LuTrash2 width="14px" height="14px" />
                                                                </button>
                                                            </div>
                                                        }
                                                    }
                                                />
                                            </section>
                                        }
                                    }
                                />
                            </div>
                        </Show>
                    </div>
                </div>
            </div>
        </section>
    }
}
