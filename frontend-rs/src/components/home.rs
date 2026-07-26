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
    on_delete_project: Callback<String>,
    on_choose_workspace: Callback<()>,
    on_settings: Callback<()>,
    on_voice: Callback<()>,
) -> impl IntoView {
    let (query, set_query) = signal(String::new());
    // Removing a project also removes its sessions, so it asks first.
    let (confirming, set_confirming) = signal(None::<String>);
    let projects = Memo::new(move |_| {
        projects_for(sessions.get(), draft_workspace.get(), query.get().as_str())
    });
    // The filtered list hides sessions that do not match a search; removal has
    // to speak for every session the project actually holds.
    let session_count = move |path: &str| {
        sessions
            .read()
            .iter()
            .filter(|session| session.workspace == path)
            .count()
    };
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
                                    let remove_path = project.path.clone();
                                    let confirm_path = project.path.clone();
                                    let pending_path = project.path.clone();
                                    let initial = project.name.chars().next().unwrap_or('P').to_ascii_uppercase();
                                    let action_label = format!("New session in {}", project.name);
                                    let remove_label = format!("Remove {}", project.name);
                                    let count = session_count(&project.path);
                                    let confirm_title = if count == 0 {
                                        format!("Remove {}", project.name)
                                    } else {
                                        format!(
                                            "Remove {} and delete {count} session{}",
                                            project.name,
                                            if count == 1 { "" } else { "s" },
                                        )
                                    };
                                    let pending = Signal::derive(move || {
                                        confirming.read().as_deref() == Some(pending_path.as_str())
                                    });
                                    view! {
                                        <div
                                            class="zai-home-project-row"
                                            data-confirming=move || if pending.get() { "true" } else { "false" }
                                        >
                                            <button
                                                class="zai-home-project"
                                                on:click=move |_| on_new.run(Some(path.clone()))
                                            >
                                                <span class="zai-project-avatar">{initial}</span>
                                                <span>{project.name}</span>
                                            </button>
                                            <Show
                                                when=move || pending.get()
                                                fallback={
                                                    let action_path = action_path.clone();
                                                    let remove_path = remove_path.clone();
                                                    let action_label = action_label.clone();
                                                    let remove_label = remove_label.clone();
                                                    move || {
                                                        let action_path = action_path.clone();
                                                        let remove_path = remove_path.clone();
                                                        view! {
                                                            <button
                                                                class="zai-home-project-action"
                                                                aria-label=action_label.clone()
                                                                title=action_label.clone()
                                                                on:click=move |_| on_new.run(Some(action_path.clone()))
                                                            >
                                                                <Icon icon=LuSquarePen width="14px" height="14px" />
                                                            </button>
                                                            <button
                                                                class="zai-home-project-action zai-home-project-remove"
                                                                aria-label=remove_label.clone()
                                                                title=remove_label.clone()
                                                                on:click=move |event: MouseEvent| {
                                                                    event.stop_propagation();
                                                                    set_confirming.set(Some(remove_path.clone()));
                                                                }
                                                            >
                                                                <Icon icon=LuTrash2 width="14px" height="14px" />
                                                            </button>
                                                        }
                                                    }
                                                }
                                            >
                                                {
                                                    let confirm_path = confirm_path.clone();
                                                    view! {
                                                        <button
                                                            class="zai-home-project-confirm"
                                                            title=confirm_title.clone()
                                                            on:click=move |event: MouseEvent| {
                                                                event.stop_propagation();
                                                                set_confirming.set(None);
                                                                on_delete_project.run(confirm_path.clone());
                                                            }
                                                        >
                                                            "Remove"
                                                        </button>
                                                        <button
                                                            class="zai-home-project-cancel"
                                                            title="Keep this project"
                                                            on:click=move |event: MouseEvent| {
                                                                event.stop_propagation();
                                                                set_confirming.set(None);
                                                            }
                                                        >
                                                            "Cancel"
                                                        </button>
                                                    }
                                                }
                                            </Show>
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
                                        let remove_path = project.path.clone();
                                        let confirm_path = project.path.clone();
                                        let pending_path = project.path.clone();
                                        let project_initial = project.name.chars().next().unwrap_or('P').to_ascii_uppercase();
                                        let count = project.sessions.len();
                                        let count_label = format!("{count} session{}", if count == 1 { "" } else { "s" });
                                        let action_label = format!("New session in {}", project.name);
                                        let total = session_count(&project.path);
                                        let remove_label = format!(
                                            "Remove {} and delete {total} session{}",
                                            project.name,
                                            if total == 1 { "" } else { "s" },
                                        );
                                        let pending = Signal::derive(move || {
                                            confirming.read().as_deref() == Some(pending_path.as_str())
                                        });
                                        view! {
                                            <section class="onyx-project-sessions">
                                                <header>
                                                    <span class="zai-project-avatar">{project_initial}</span>
                                                    <div><strong>{project.name}</strong><small>{count_label}</small></div>
                                                    <Show
                                                        when=move || pending.get()
                                                        fallback={
                                                            let project_path = project_path.clone();
                                                            let remove_path = remove_path.clone();
                                                            let action_label = action_label.clone();
                                                            let remove_label = remove_label.clone();
                                                            move || {
                                                                let project_path = project_path.clone();
                                                                let remove_path = remove_path.clone();
                                                                view! {
                                                                    <button
                                                                        on:click=move |_| on_new.run(Some(project_path.clone()))
                                                                        aria-label=action_label.clone()
                                                                        title=action_label.clone()
                                                                    >
                                                                        <Icon icon=LuSquarePen width="14px" height="14px" />
                                                                    </button>
                                                                    <button
                                                                        class="zai-home-project-remove"
                                                                        aria-label=remove_label.clone()
                                                                        title=remove_label.clone()
                                                                        on:click=move |event: MouseEvent| {
                                                                            event.stop_propagation();
                                                                            set_confirming.set(Some(remove_path.clone()));
                                                                        }
                                                                    >
                                                                        <Icon icon=LuTrash2 width="14px" height="14px" />
                                                                    </button>
                                                                }
                                                            }
                                                        }
                                                    >
                                                        {
                                                            let confirm_path = confirm_path.clone();
                                                            view! {
                                                                <button
                                                                    class="zai-home-project-confirm"
                                                                    on:click=move |event: MouseEvent| {
                                                                        event.stop_propagation();
                                                                        set_confirming.set(None);
                                                                        on_delete_project.run(confirm_path.clone());
                                                                    }
                                                                >
                                                                    "Remove project"
                                                                </button>
                                                                <button
                                                                    class="zai-home-project-cancel"
                                                                    on:click=move |event: MouseEvent| {
                                                                        event.stop_propagation();
                                                                        set_confirming.set(None);
                                                                    }
                                                                >
                                                                    "Cancel"
                                                                </button>
                                                            }
                                                        }
                                                    </Show>
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
