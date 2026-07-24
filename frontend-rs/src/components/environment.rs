use icondata::{
    LuArrowUpRight, LuBot, LuBraces, LuChevronDown, LuChevronRight, LuCloudUpload, LuFile,
    LuFolder, LuFolderGit2, LuGitBranch, LuGitCommitHorizontal, LuGitCompareArrows, LuLaptop,
    LuLink,
};
use leptos::prelude::*;
use leptos_icons::Icon;

use crate::model::{ProviderBrand, RepoSummary, SessionStatus};

use super::ProviderBadge;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalWorkspace {
    pub label: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentCompare {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentAgent {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub brand: ProviderBrand,
    pub status: SessionStatus,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnvironmentSourceKind {
    File,
    Directory,
    Url,
    Tool,
    #[default]
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentSource {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub kind: EnvironmentSourceKind,
}

#[derive(Clone, Copy)]
struct ChangePresentation {
    code: &'static str,
    kind: &'static str,
    label: &'static str,
}

fn change_presentation(status: &str) -> ChangePresentation {
    let status = status.trim().to_ascii_uppercase();
    if status.contains('U') && !status.contains('?') {
        ChangePresentation {
            code: "!",
            kind: "conflicted",
            label: "Conflicted",
        }
    } else if status.contains('?') {
        ChangePresentation {
            code: "U",
            kind: "untracked",
            label: "Untracked",
        }
    } else if status.contains('R') {
        ChangePresentation {
            code: "R",
            kind: "renamed",
            label: "Renamed",
        }
    } else if status.contains('D') {
        ChangePresentation {
            code: "D",
            kind: "deleted",
            label: "Deleted",
        }
    } else if status.contains('A') {
        ChangePresentation {
            code: "A",
            kind: "added",
            label: "Added",
        }
    } else {
        ChangePresentation {
            code: "M",
            kind: "modified",
            label: "Modified",
        }
    }
}

fn agent_status(status: SessionStatus) -> (&'static str, &'static str) {
    match status {
        SessionStatus::Idle => ("idle", "Ready"),
        SessionStatus::Running => ("running", "Working"),
        SessionStatus::WaitingApproval => ("waiting", "Needs approval"),
        SessionStatus::Failed => ("failed", "Failed"),
    }
}

fn source_icon(kind: EnvironmentSourceKind) -> icondata::Icon {
    match kind {
        EnvironmentSourceKind::File => LuFile,
        EnvironmentSourceKind::Directory => LuFolder,
        EnvironmentSourceKind::Url => LuLink,
        EnvironmentSourceKind::Tool => LuBraces,
        EnvironmentSourceKind::Other => LuFile,
    }
}

#[component]
pub fn EnvironmentPanel(
    workspace: Signal<Option<LocalWorkspace>>,
    repo: Signal<Option<RepoSummary>>,
    branches: Signal<Vec<String>>,
    compare: Signal<Option<EnvironmentCompare>>,
    agent: Signal<Option<EnvironmentAgent>>,
    subagents: Signal<Vec<EnvironmentAgent>>,
    sources: Signal<Vec<EnvironmentSource>>,
    git_busy: Signal<Option<String>>,
    on_change: Callback<String>,
    on_open_workspace: Callback<String>,
    on_branch: Callback<String>,
    on_commit: Callback<()>,
    on_push: Callback<()>,
    on_compare: Callback<String>,
    on_agent: Callback<String>,
    on_source: Callback<String>,
) -> impl IntoView {
    let has_content = Signal::derive(move || {
        workspace.read().is_some()
            || repo.read().is_some()
            || compare.read().is_some()
            || agent.read().is_some()
            || !subagents.read().is_empty()
            || !sources.read().is_empty()
    });

    view! {
        <aside class="onyx-environment" data-slot="environment-panel" aria-label="Environment">
            <header class="onyx-environment__header">
                <span class="onyx-environment__header-icon" aria-hidden="true">
                    <Icon icon=LuLaptop width="15px" height="15px" />
                </span>
                <h2>"Environment"</h2>
                {move || workspace.get().map(|workspace| view! {
                    <span class="onyx-environment__location" title=workspace.path>
                        {workspace.label}
                    </span>
                })}
            </header>

            <div class="onyx-environment__body">
                {move || repo.get().map(|repo| view! {
                    <ChangesSection repo on_change />
                })}

                <Show when=move || {
                    workspace.read().is_some() || repo.read().is_some() || compare.read().is_some()
                }>
                    <details class="onyx-environment__section" open=true>
                        <summary>
                            <Icon
                                icon=LuChevronRight
                                width="13px"
                                height="13px"
                                attr:class="onyx-environment__section-chevron"
                            />
                            <span>"Workspace"</span>
                        </summary>
                        <div class="onyx-environment__section-body">
                            {move || workspace.get().map(|workspace| view! {
                                <WorkspaceCard workspace on_open=on_open_workspace />
                            })}
                            {move || {
                                let busy = git_busy.get();
                                repo.get().map(|repo| view! {
                                    <SourceControlCard
                                        repo
                                        branches
                                        busy
                                        on_branch
                                        on_commit
                                        on_push
                                    />
                                })
                            }}
                            {move || compare.get().map(|compare| view! {
                                <CompareCard compare on_compare />
                            })}
                        </div>
                    </details>
                </Show>

                {move || agent.get().map(|agent| view! {
                    <AgentSection agent on_agent />
                })}

                <Show when=move || !subagents.read().is_empty()>
                    <SubagentsSection agents=subagents on_agent />
                </Show>

                <Show when=move || !sources.read().is_empty()>
                    <SourcesSection sources on_source />
                </Show>

                <Show when=move || !has_content.get()>
                    <div class="onyx-environment__empty">
                        <Icon icon=LuLaptop width="20px" height="20px" />
                        <span>"No environment details are available."</span>
                    </div>
                </Show>
            </div>
        </aside>
    }
}

#[component]
fn ChangesSection(repo: RepoSummary, on_change: Callback<String>) -> impl IntoView {
    if !repo.is_repo {
        return view! {
            <details class="onyx-environment__section" open=true>
                <summary>
                    <Icon
                        icon=LuChevronRight
                        width="13px"
                        height="13px"
                        attr:class="onyx-environment__section-chevron"
                    />
                    <span>"Changes"</span>
                </summary>
                <div class="onyx-environment__section-body">
                    <p class="onyx-environment__note">
                        "This workspace is not a Git repository."
                    </p>
                </div>
            </details>
        }
        .into_any();
    }

    let staged_count = repo.staged_count;
    let unstaged_count = repo.unstaged_count;
    let untracked_count = repo.untracked_count;
    let changes = repo.changed_files;
    let change_count = changes.len();

    if changes.is_empty() {
        return view! {
            <details class="onyx-environment__section" open=true>
                <summary>
                    <Icon
                        icon=LuChevronRight
                        width="13px"
                        height="13px"
                        attr:class="onyx-environment__section-chevron"
                    />
                    <span>"Changes"</span>
                    <span class="onyx-environment__section-count">0</span>
                </summary>
                <div class="onyx-environment__section-body">
                    <p class="onyx-environment__note">"Working tree clean."</p>
                </div>
            </details>
        }
        .into_any();
    }

    view! {
        <details class="onyx-environment__section" open=true>
            <summary>
                <Icon
                    icon=LuChevronRight
                    width="13px"
                    height="13px"
                    attr:class="onyx-environment__section-chevron"
                />
                <span>"Changes"</span>
                <span class="onyx-environment__section-count">{change_count}</span>
            </summary>
            <div class="onyx-environment__section-body">
                <div class="onyx-environment__change-list">
                    <For
                        each=move || changes.clone()
                        key=|change| format!("{}:{}", change.status, change.path)
                        children=move |change| {
                            let presentation = change_presentation(&change.status);
                            let click_path = change.path.clone();
                            let display_path = change.path.clone();
                            let title = format!("{} · {}", presentation.label, change.path);
                            view! {
                                <button
                                    type="button"
                                    class="onyx-environment__change"
                                    title=title
                                    on:click=move |_| on_change.run(click_path.clone())
                                >
                                    <span
                                        class="onyx-environment__change-status"
                                        data-kind=presentation.kind
                                        aria-label=presentation.label
                                    >
                                        {presentation.code}
                                    </span>
                                    <span class="onyx-environment__change-path">{display_path}</span>
                                </button>
                            }
                        }
                    />
                </div>
                <div class="onyx-environment__change-summary" aria-label="Change summary">
                    <Show when=move || { staged_count > 0 }>
                        <span>{format!("{staged_count} staged")}</span>
                    </Show>
                    <Show when=move || { unstaged_count > 0 }>
                        <span>{format!("{unstaged_count} unstaged")}</span>
                    </Show>
                    <Show when=move || { untracked_count > 0 }>
                        <span>{format!("{untracked_count} untracked")}</span>
                    </Show>
                </div>
            </div>
        </details>
    }
    .into_any()
}

#[component]
fn WorkspaceCard(workspace: LocalWorkspace, on_open: Callback<String>) -> impl IntoView {
    let open_path = workspace.path.clone();
    let title_path = workspace.path.clone();
    let display_path = workspace.path;
    view! {
        <button
            type="button"
            class="onyx-environment__card onyx-environment__workspace"
            title=title_path
            on:click=move |_| on_open.run(open_path.clone())
        >
            <span class="onyx-environment__card-icon" aria-hidden="true">
                <Icon icon=LuFolderGit2 width="15px" height="15px" />
            </span>
            <span class="onyx-environment__card-copy">
                <strong>{workspace.label}</strong>
                <small>{display_path}</small>
            </span>
            <Icon icon=LuArrowUpRight width="13px" height="13px" />
        </button>
    }
}

#[component]
fn SourceControlCard(
    repo: RepoSummary,
    branches: Signal<Vec<String>>,
    busy: Option<String>,
    on_branch: Callback<String>,
    on_commit: Callback<()>,
    on_push: Callback<()>,
) -> impl IntoView {
    if !repo.is_repo {
        return ().into_any();
    }

    let is_busy = busy.is_some();
    let committing = busy.as_deref() == Some("commit");
    let pushing = busy.as_deref() == Some("push");
    let switching = busy.as_deref() == Some("branch");
    let can_commit = repo.is_repo && !repo.changed_files.is_empty() && !is_busy;
    let can_push = repo.is_repo && repo.has_upstream && repo.ahead > 0 && !is_busy;
    let current_branch = repo.branch.unwrap_or_default();
    let current_for_options = current_branch.clone();
    let current_for_change = current_branch.clone();
    let current_for_labels = current_branch.clone();
    let selected_branch = current_branch.clone();
    let detached = current_branch.is_empty();
    let branch_title = if current_branch.is_empty() {
        "Detached HEAD — switch to a local branch".to_string()
    } else {
        format!("Current branch: {current_branch}")
    };
    let branch_options = Signal::derive(move || {
        let mut options = branches.get();
        if !current_for_options.is_empty()
            && !options.iter().any(|branch| branch == &current_for_options)
        {
            options.push(current_for_options.clone());
            options.sort();
        }
        options
    });
    let ahead = repo.ahead;
    let behind = repo.behind;

    view! {
        <div class="onyx-environment__source-control">
            <div
                class="onyx-environment__branch"
                data-busy=if switching { "true" } else { "false" }
            >
                <Icon icon=LuGitBranch width="14px" height="14px" />
                <select
                    aria-label="Switch local Git branch"
                    title=branch_title
                    prop:value=selected_branch
                    disabled=move || is_busy || branches.read().is_empty()
                    on:change=move |event| {
                        let next = event_target_value(&event);
                        if !next.is_empty() && next != current_for_change {
                            on_branch.run(next);
                        }
                    }
                >
                    <Show when=move || detached>
                        <option value="" disabled=true>"Detached HEAD"</option>
                    </Show>
                    <For
                        each=move || branch_options.get()
                        key=|branch| branch.clone()
                        children=move |branch| {
                            let label = if branch == current_for_labels {
                                format!("{branch} (current)")
                            } else {
                                branch.clone()
                            };
                            view! { <option value=branch>{label}</option> }
                        }
                    />
                </select>
                <span class="onyx-environment__sync-state">
                    <Show when=move || { ahead > 0 }>
                        <small aria-label=format!("{ahead} commits ahead")>{format!("↑{ahead}")}</small>
                    </Show>
                    <Show when=move || { behind > 0 }>
                        <small aria-label=format!("{behind} commits behind")>{format!("↓{behind}")}</small>
                    </Show>
                </span>
                <Show when=move || switching>
                    <small class="onyx-environment__branch-progress">"Switching…"</small>
                </Show>
                <Icon
                    icon=LuChevronDown
                    width="12px"
                    height="12px"
                    attr:class="onyx-environment__branch-chevron"
                />
            </div>
            <div class="onyx-environment__git-actions" role="group" aria-label="Git actions">
                <button
                    type="button"
                    disabled=move || !can_commit
                    on:click=move |_| on_commit.run(())
                >
                    <Icon icon=LuGitCommitHorizontal width="13px" height="13px" />
                    <span>{if committing { "Committing…" } else { "Commit" }}</span>
                </button>
                <button
                    type="button"
                    disabled=move || !can_push
                    on:click=move |_| on_push.run(())
                >
                    <Icon icon=LuCloudUpload width="13px" height="13px" />
                    <span>{if pushing { "Pushing…" } else { "Push" }}</span>
                </button>
            </div>
        </div>
    }
    .into_any()
}

#[component]
fn CompareCard(compare: EnvironmentCompare, on_compare: Callback<String>) -> impl IntoView {
    let compare_id = compare.id.clone();
    view! {
        <button
            type="button"
            class="onyx-environment__card onyx-environment__compare"
            on:click=move |_| on_compare.run(compare_id.clone())
        >
            <span class="onyx-environment__card-icon" aria-hidden="true">
                <Icon icon=LuGitCompareArrows width="15px" height="15px" />
            </span>
            <span class="onyx-environment__card-copy">
                <strong>{compare.label}</strong>
                {compare.detail.map(|detail| view! { <small>{detail}</small> })}
            </span>
            <Icon icon=LuChevronRight width="13px" height="13px" />
        </button>
    }
}

#[component]
fn AgentSection(agent: EnvironmentAgent, on_agent: Callback<String>) -> impl IntoView {
    let agent_id = agent.id.clone();
    let brand = agent.brand;
    let (status_kind, status_label) = agent_status(agent.status);

    view! {
        <details class="onyx-environment__section" open=true>
            <summary>
                <Icon
                    icon=LuChevronRight
                    width="13px"
                    height="13px"
                    attr:class="onyx-environment__section-chevron"
                />
                <span>"Agent"</span>
                <span class="onyx-environment__agent-state" data-status=status_kind>
                    <i aria-hidden="true"></i>
                    {status_label}
                </span>
            </summary>
            <div class="onyx-environment__section-body">
                <button
                    type="button"
                    class="onyx-environment__card onyx-environment__agent"
                    on:click=move |_| on_agent.run(agent_id.clone())
                >
                    <ProviderBadge brand=Signal::derive(move || brand) small=true />
                    <span class="onyx-environment__card-copy">
                        <strong>{agent.label}</strong>
                        {agent.detail.map(|detail| view! { <small>{detail}</small> })}
                    </span>
                    <Icon icon=LuBot width="14px" height="14px" />
                </button>
            </div>
        </details>
    }
}

#[component]
fn SubagentsSection(
    agents: Signal<Vec<EnvironmentAgent>>,
    on_agent: Callback<String>,
) -> impl IntoView {
    view! {
        <details class="onyx-environment__section" open=true>
            <summary>
                <Icon
                    icon=LuChevronRight
                    width="13px"
                    height="13px"
                    attr:class="onyx-environment__section-chevron"
                />
                <span title="Other active Onyx sessions in this workspace">
                    "Other running sessions"
                </span>
                <span class="onyx-environment__section-count">{move || agents.read().len()}</span>
            </summary>
            <div class="onyx-environment__section-body">
                <For
                    each=move || agents.get()
                    key=|agent| agent.id.clone()
                    children=move |agent| {
                        let agent_id = agent.id.clone();
                        let brand = agent.brand;
                        let (status_kind, status_label) = agent_status(agent.status);
                        view! {
                            <button
                                type="button"
                                class="onyx-environment__card onyx-environment__agent"
                                on:click=move |_| on_agent.run(agent_id.clone())
                            >
                                <ProviderBadge brand=Signal::derive(move || brand) small=true />
                                <span class="onyx-environment__card-copy">
                                    <strong>{agent.label}</strong>
                                    {agent.detail.map(|detail| view! { <small>{detail}</small> })}
                                </span>
                                <span
                                    class="onyx-environment__agent-state"
                                    data-status=status_kind
                                    title=status_label
                                    aria-label=status_label
                                >
                                    <i aria-hidden="true"></i>
                                </span>
                            </button>
                        }
                    }
                />
            </div>
        </details>
    }
}

#[component]
fn SourcesSection(
    sources: Signal<Vec<EnvironmentSource>>,
    on_source: Callback<String>,
) -> impl IntoView {
    view! {
        <details class="onyx-environment__section" open=true>
            <summary>
                <Icon
                    icon=LuChevronRight
                    width="13px"
                    height="13px"
                    attr:class="onyx-environment__section-chevron"
                />
                <span>"Sources"</span>
                <span class="onyx-environment__section-count">{move || sources.read().len()}</span>
            </summary>
            <div class="onyx-environment__section-body">
                <div class="onyx-environment__source-list">
                    <For
                        each=move || sources.get()
                        key=|source| source.id.clone()
                        children=move |source| {
                            let source_id = source.id.clone();
                            view! {
                                <button
                                    type="button"
                                    class="onyx-environment__source"
                                    on:click=move |_| on_source.run(source_id.clone())
                                >
                                    <Icon
                                        icon=source_icon(source.kind)
                                        width="14px"
                                        height="14px"
                                    />
                                    <span class="onyx-environment__card-copy">
                                        <strong>{source.label}</strong>
                                        {source.detail.map(|detail| view! { <small>{detail}</small> })}
                                    </span>
                                </button>
                            }
                        }
                    />
                </div>
            </div>
        </details>
    }
}

#[cfg(test)]
mod tests {
    use super::change_presentation;

    #[test]
    fn presents_common_git_statuses() {
        assert_eq!(change_presentation(" M").kind, "modified");
        assert_eq!(change_presentation("A ").kind, "added");
        assert_eq!(change_presentation("??").kind, "untracked");
        assert_eq!(change_presentation("UU").kind, "conflicted");
        assert_eq!(change_presentation("R ").kind, "renamed");
        assert_eq!(change_presentation(" D").kind, "deleted");
    }
}
