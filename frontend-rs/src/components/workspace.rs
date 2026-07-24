use std::{cell::RefCell, collections::HashMap};

use icondata::{
    LuArrowLeft, LuArrowRight, LuBox, LuChevronDown, LuCloudUpload, LuEraser, LuExternalLink,
    LuFile, LuFileDiff, LuFolder, LuGitCommitHorizontal, LuGitPullRequest, LuGlobe,
    LuMessageSquare, LuPanelBottom, LuPanelRight, LuPlus, LuRefreshCw, LuSquareTerminal, LuTrash2,
    LuX,
};
use leptos::ev::{KeyboardEvent, SubmitEvent};
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen_futures::spawn_local;

use crate::{
    bridge,
    model::{EditorTarget, RepoSummary, WorkspaceEntry, WorkspaceFile},
    storage,
};

use super::ProviderBadge;
use crate::model::ProviderBrand;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkspaceSurfaceKind {
    Chat,
    Browser,
    Terminal,
    Files,
    Diff,
}

impl WorkspaceSurfaceKind {
    pub const ALL: [Self; 5] = [
        Self::Chat,
        Self::Browser,
        Self::Terminal,
        Self::Files,
        Self::Diff,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::Browser => "Browser",
            Self::Terminal => "Terminal",
            Self::Files => "Files",
            Self::Diff => "Diff",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Chat => "Use official, signed-in provider apps.",
            Self::Browser => "Open a local app or URL.",
            Self::Terminal => "Start a shell in this workspace.",
            Self::Files => "Browse and read workspace files.",
            Self::Diff => "Review changes in this workspace.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSurface {
    pub id: String,
    pub kind: WorkspaceSurfaceKind,
    pub title: String,
    pub resource_id: Option<String>,
    pub pending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceTerminal {
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionWorkspaceUi {
    pub right_panel_open: bool,
    pub bottom_panel_open: bool,
    pub surfaces: Vec<WorkspaceSurface>,
    pub active_surface_id: Option<String>,
    pub terminals: Vec<WorkspaceTerminal>,
    pub active_terminal_id: Option<String>,
    pub terminal_height: u32,
}

impl Default for SessionWorkspaceUi {
    fn default() -> Self {
        Self {
            right_panel_open: false,
            bottom_panel_open: false,
            surfaces: Vec::new(),
            active_surface_id: None,
            terminals: Vec::new(),
            active_terminal_id: None,
            terminal_height: 280,
        }
    }
}

thread_local! {
    static MOUNTED_TERMINALS: RefCell<HashMap<String, bridge::MountedTerminal>> =
        RefCell::new(HashMap::new());
}

fn surface_icon(kind: WorkspaceSurfaceKind) -> icondata::Icon {
    match kind {
        WorkspaceSurfaceKind::Chat => LuMessageSquare,
        WorkspaceSurfaceKind::Browser => LuGlobe,
        WorkspaceSurfaceKind::Terminal => LuSquareTerminal,
        WorkspaceSurfaceKind::Files => LuFolder,
        WorkspaceSurfaceKind::Diff => LuFileDiff,
    }
}

fn normalize_browser_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Enter a URL.".to_owned());
    }
    let value = if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_owned()
    } else if raw.starts_with("localhost") || raw.starts_with("127.") || raw.starts_with("[::1]") {
        format!("http://{raw}")
    } else {
        format!("https://{raw}")
    };
    let parsed = web_sys::Url::new(&value).map_err(|_| "Enter a valid HTTP or HTTPS URL.")?;
    if parsed.protocol() != "http:" && parsed.protocol() != "https:" {
        return Err("Only HTTP and HTTPS previews are supported.".to_owned());
    }
    Ok(parsed.href())
}

#[component]
pub fn WorkspaceTopbarActions(
    repo: Signal<Option<RepoSummary>>,
    editors: Signal<Vec<EditorTarget>>,
    preferred_editor: Signal<String>,
    bottom_panel_open: Signal<bool>,
    right_panel_open: Signal<bool>,
    git_busy: Signal<Option<String>>,
    on_open: Callback<()>,
    on_open_target: Callback<String>,
    on_commit: Callback<()>,
    on_push: Callback<()>,
    on_create_pr: Callback<()>,
    on_toggle_bottom: Callback<()>,
    on_toggle_right: Callback<()>,
) -> impl IntoView {
    let open_menu = RwSignal::new(false);
    let git_menu = RwSignal::new(false);
    let primary = Signal::derive(move || {
        let Some(repo) = repo.get() else {
            return "commit";
        };
        if !repo.changed_files.is_empty() {
            "commit"
        } else if repo.ahead > 0 && repo.has_upstream {
            "push"
        } else {
            "create-pr"
        }
    });
    let action_disabled = move |action: &str| {
        let Some(repo) = repo.get() else {
            return true;
        };
        match action {
            "commit" => repo.changed_files.is_empty(),
            "push" => !repo.has_upstream || repo.ahead == 0,
            _ => !repo.has_remote || repo.pr_commit_count.unwrap_or_default() == 0,
        }
    };
    let action_label = |action: &str| match action {
        "commit" => "Commit",
        "push" => "Push",
        _ => "Create PR",
    };

    view! {
        <div class="zai-workspace-topbar-actions" data-slot="workspace-topbar-actions">
            <div class="zai-workspace-split-control" role="group" aria-label="Open workspace">
                <button
                    type="button"
                    class="zai-workspace-action zai-workspace-split-control__primary"
                    disabled=move || !editors.read().iter().any(|editor| editor.available)
                    on:click=move |_| on_open.run(())
                >
                    <Icon icon=LuBox width="15px" height="15px" />
                    <span>"Open"</span>
                </button>
                <button
                    type="button"
                    class="zai-workspace-split-control__chevron"
                    aria-label="Choose where to open"
                    on:click=move |_| {
                        git_menu.set(false);
                        open_menu.update(|open| *open = !*open);
                    }
                >
                    <Icon icon=LuChevronDown width="14px" height="14px" />
                </button>
                <Show when=move || open_menu.get()>
                    <div class="zai-workspace-action-menu" role="menu">
                        <For
                            each=move || editors.get()
                            key=|editor| editor.id.clone()
                            children=move |editor| {
                                let id = editor.id.clone();
                                let active_id = id.clone();
                                view! {
                                    <button
                                        type="button"
                                        role="menuitem"
                                        disabled=!editor.available
                                        data-selected=move || if preferred_editor.get() == active_id { "true" } else { "false" }
                                        on:click=move |_| {
                                            open_menu.set(false);
                                            on_open_target.run(id.clone());
                                        }
                                    >
                                        <Icon icon=LuExternalLink width="14px" height="14px" />
                                        <span>{editor.label}</span>
                                    </button>
                                }
                            }
                        />
                    </div>
                </Show>
            </div>

            <div class="zai-workspace-split-control" role="group" aria-label="Git actions">
                <button
                    type="button"
                    class="zai-workspace-action zai-workspace-split-control__primary"
                    data-action=move || primary.get()
                    disabled=move || action_disabled(primary.get()) || git_busy.get().is_some()
                    on:click=move |_| match primary.get() {
                        "commit" => on_commit.run(()),
                        "push" => on_push.run(()),
                        _ => on_create_pr.run(()),
                    }
                >
                    {move || match primary.get() {
                        "commit" => view! { <Icon icon=LuGitCommitHorizontal width="15px" height="15px" /> }.into_any(),
                        "push" => view! { <Icon icon=LuCloudUpload width="15px" height="15px" /> }.into_any(),
                        _ => view! { <Icon icon=LuGitPullRequest width="15px" height="15px" /> }.into_any(),
                    }}
                    <span>{move || action_label(primary.get())}</span>
                </button>
                <button
                    type="button"
                    class="zai-workspace-split-control__chevron"
                    aria-label="Git action options"
                    on:click=move |_| {
                        open_menu.set(false);
                        git_menu.update(|open| *open = !*open);
                    }
                >
                    <Icon icon=LuChevronDown width="14px" height="14px" />
                </button>
                <Show when=move || git_menu.get()>
                    <div class="zai-workspace-action-menu" role="menu">
                        <For
                            each=move || ["commit", "push", "create-pr"]
                            key=|action| *action
                            children=move |action| view! {
                                <button
                                    type="button"
                                    role="menuitem"
                                    disabled=move || action_disabled(action) || git_busy.get().is_some()
                                    on:click=move |_| {
                                        git_menu.set(false);
                                        match action {
                                            "commit" => on_commit.run(()),
                                            "push" => on_push.run(()),
                                            _ => on_create_pr.run(()),
                                        }
                                    }
                                >
                                    {match action {
                                        "commit" => view! { <Icon icon=LuGitCommitHorizontal width="14px" height="14px" /> }.into_any(),
                                        "push" => view! { <Icon icon=LuCloudUpload width="14px" height="14px" /> }.into_any(),
                                        _ => view! { <Icon icon=LuGitPullRequest width="14px" height="14px" /> }.into_any(),
                                    }}
                                    <span>{action_label(action)}</span>
                                </button>
                            }
                        />
                    </div>
                </Show>
            </div>

            <div class="zai-layout-controls" data-slot="workspace-layout-controls">
                <button
                    type="button"
                    class="zai-workspace-icon-button"
                    data-active=move || if bottom_panel_open.get() { "true" } else { "false" }
                    aria-label="Toggle terminal drawer (⌘J)"
                    on:click=move |_| on_toggle_bottom.run(())
                >
                    <Icon icon=LuPanelBottom width="16px" height="16px" />
                </button>
                <button
                    type="button"
                    class="zai-workspace-icon-button"
                    data-active=move || if right_panel_open.get() { "true" } else { "false" }
                    aria-label="Toggle right panel (⌘⇧J)"
                    on:click=move |_| on_toggle_right.run(())
                >
                    <Icon icon=LuPanelRight width="16px" height="16px" />
                </button>
            </div>
        </div>
    }
}

#[component]
fn OfficialChatSurface(on_error: Callback<String>) -> impl IntoView {
    #[derive(Clone, Copy)]
    struct Official {
        name: &'static str,
        brand: ProviderBrand,
        detail: &'static str,
        url: &'static str,
    }
    const OFFICIAL: [Official; 4] = [
        Official {
            name: "ChatGPT",
            brand: ProviderBrand::Openai,
            detail: "Your signed-in ChatGPT subscription",
            url: "https://chatgpt.com/",
        },
        Official {
            name: "Claude",
            brand: ProviderBrand::Anthropic,
            detail: "Your signed-in Claude subscription",
            url: "https://claude.ai/new",
        },
        Official {
            name: "Gemini",
            brand: ProviderBrand::Google,
            detail: "Your signed-in Gemini subscription",
            url: "https://gemini.google.com/app",
        },
        Official {
            name: "Grok",
            brand: ProviderBrand::Xai,
            detail: "Your signed-in Grok subscription",
            url: "https://grok.com/",
        },
    ];

    let open = Callback::new(move |url: String| {
        spawn_local(async move {
            if let Err(cause) = bridge::open_url(&url).await {
                on_error.run(cause);
            }
        });
    });

    view! {
        <div class="zai-provider-sidebar">
            <nav aria-label="Official chat provider">
                <For
                    each=move || OFFICIAL
                    key=|provider| provider.name
                    children=move |provider| view! {
                        <button
                            type="button"
                            title=provider.detail
                            on:click=move |_| open.run(provider.url.to_owned())
                        >
                            <ProviderBadge brand=Signal::derive(move || provider.brand) small=true />
                            <span>{provider.name}</span>
                            <Icon icon=LuExternalLink width="12px" height="12px" />
                        </button>
                    }
                />
            </nav>
            <div class="zai-provider-sidebar__host">
                <div class="zai-provider-sidebar__empty">
                    <Icon icon=LuMessageSquare width="22px" height="22px" />
                    <strong>"Open an official chat"</strong>
                    <span>"Subscription web apps open in your browser and keep their own signed-in sessions."</span>
                </div>
            </div>
            <footer>"Official provider site · opens outside Onyx"</footer>
        </div>
    }
}

#[component]
fn BrowserSurface(on_error: Callback<String>) -> impl IntoView {
    let history = RwSignal::new(Vec::<String>::new());
    let index = RwSignal::new(-1_i32);
    let input = RwSignal::new(String::new());
    let current = Signal::derive(move || {
        let index = index.get();
        (index >= 0)
            .then(|| history.read().get(index as usize).cloned())
            .flatten()
            .unwrap_or_default()
    });
    let navigate = Callback::new(move |raw: String| match normalize_browser_url(&raw) {
        Ok(url) => {
            let keep = (index.get() + 1).max(0) as usize;
            history.update(|entries| {
                entries.truncate(keep);
                entries.push(url.clone());
            });
            index.set(history.read().len() as i32 - 1);
            input.set(url);
        }
        Err(cause) => on_error.run(cause),
    });

    view! {
        <div class="zai-browser-surface">
            <form
                class="zai-browser-toolbar"
                on:submit=move |event: SubmitEvent| {
                    event.prevent_default();
                    navigate.run(input.get());
                }
            >
                <button
                    type="button"
                    aria-label="Back"
                    disabled=move || index.get() <= 0
                    on:click=move |_| {
                        if index.get() > 0 {
                            index.update(|value| *value -= 1);
                            input.set(current.get());
                        }
                    }
                >
                    <Icon icon=LuArrowLeft width="15px" height="15px" />
                </button>
                <button
                    type="button"
                    aria-label="Forward"
                    disabled=move || {
                        index.get() < 0
                            || (index.get() as usize)
                                >= history.read().len().saturating_sub(1)
                    }
                    on:click=move |_| {
                        if (index.get() as usize) < history.read().len().saturating_sub(1) {
                            index.update(|value| *value += 1);
                            input.set(current.get());
                        }
                    }
                >
                    <Icon icon=LuArrowRight width="15px" height="15px" />
                </button>
                <button type="button" aria-label="Reload" disabled=move || current.get().is_empty()>
                    <Icon icon=LuRefreshCw width="15px" height="15px" />
                </button>
                <label>
                    <Icon icon=LuGlobe width="15px" height="15px" />
                    <span class="sr-only">"URL"</span>
                    <input
                        prop:value=move || input.get()
                        spellcheck="false"
                        autocomplete="off"
                        on:input=move |event| input.set(event_target_value(&event))
                    />
                </label>
                <button
                    type="button"
                    aria-label="Open in default browser"
                    disabled=move || current.get().is_empty()
                    on:click=move |_| {
                        let url = current.get();
                        spawn_local(async move {
                            if let Err(cause) = bridge::open_url(&url).await {
                                on_error.run(cause);
                            }
                        });
                    }
                >
                    <Icon icon=LuExternalLink width="15px" height="15px" />
                </button>
            </form>
            <Show
                when=move || !current.get().is_empty()
                fallback=move || view! {
                    <div class="zai-browser-empty">
                        <Icon icon=LuGlobe width="23px" height="23px" />
                        <strong>"Open a preview"</strong>
                        <span>"Enter a localhost or HTTPS URL above."</span>
                    </div>
                }
            >
                <iframe
                    src=move || current.get()
                    title=move || format!("Browser: {}", current.get())
                    sandbox="allow-downloads allow-forms allow-modals allow-popups allow-same-origin allow-scripts"
                    referrerpolicy="strict-origin-when-cross-origin"
                ></iframe>
                <div class="zai-browser-hint">
                    "Some sites block embedding; use the external-open icon when a page stays blank."
                </div>
            </Show>
        </div>
    }
}

#[component]
fn FilesSurface(workspace: Signal<String>, on_error: Callback<String>) -> impl IntoView {
    let entries = RwSignal::new(Vec::<WorkspaceEntry>::new());
    let selected = RwSignal::new(String::new());
    let file = RwSignal::new(None::<WorkspaceFile>);
    let loading = RwSignal::new(false);
    let load = Callback::new(move |_: ()| {
        let workspace = workspace.get();
        loading.set(true);
        spawn_local(async move {
            match bridge::workspace_entries(&workspace).await {
                Ok(result) => entries.set(result),
                Err(cause) => on_error.run(cause),
            }
            loading.set(false);
        });
    });
    Effect::new(move |_| load.run(()));

    view! {
        <div class="zai-files-surface" aria-busy=move || loading.get()>
            <aside>
                <header>
                    <span>"Files"</span>
                    <button on:click=move |_| load.run(()) aria-label="Refresh files">
                        <Icon icon=LuRefreshCw width="14px" height="14px" />
                    </button>
                </header>
                <div class="zai-files-list">
                    <For
                        each=move || entries.get()
                        key=|entry| entry.path.clone()
                        children=move |entry| {
                            let path = entry.path.clone();
                            let active_path = path.clone();
                            view! {
                                <button
                                    type="button"
                                    data-selected=move || if selected.get() == active_path { "true" } else { "false" }
                                    disabled=entry.is_directory
                                    title=entry.path
                                    style=format!("padding-left:{}px", 10 + entry.depth.min(8) * 14)
                                    on:click=move |_| {
                                        selected.set(path.clone());
                                        let workspace = workspace.get();
                                        let path = path.clone();
                                        loading.set(true);
                                        spawn_local(async move {
                                            match bridge::read_workspace_file(&workspace, &path).await {
                                                Ok(result) => file.set(Some(result)),
                                                Err(cause) => {
                                                    file.set(None);
                                                    on_error.run(cause);
                                                }
                                            }
                                            loading.set(false);
                                        });
                                    }
                                >
                                    <Icon
                                        icon=if entry.is_directory { LuFolder } else { LuFile }
                                        width="14px"
                                        height="14px"
                                    />
                                    <span>{entry.name}</span>
                                </button>
                            }
                        }
                    />
                </div>
            </aside>
            <section class="zai-file-preview">
                <Show
                    when=move || file.get().is_some()
                    fallback=move || view! {
                        <div class="zai-surface-placeholder">
                            <Icon icon=LuFile width="20px" height="20px" />
                            <span>"Select a text file to preview it."</span>
                        </div>
                    }
                >
                    {move || file.get().map(|value| view! {
                        <header title=value.path.clone()>{value.path.clone()}</header>
                        <pre><code>{value.content}</code></pre>
                        <Show when=move || value.truncated>
                            <div class="zai-surface-note">"Preview truncated at the safe read limit."</div>
                        </Show>
                    })}
                </Show>
            </section>
        </div>
    }
}

#[component]
fn DiffSurface(workspace: Signal<String>, on_error: Callback<String>) -> impl IntoView {
    let diff = RwSignal::new(String::new());
    let loading = RwSignal::new(false);
    let diff_lines =
        Signal::derive(move || diff.get().lines().map(str::to_owned).collect::<Vec<_>>());
    let load = Callback::new(move |_: ()| {
        let workspace = workspace.get();
        loading.set(true);
        spawn_local(async move {
            match bridge::git_diff(&workspace).await {
                Ok(value) => diff.set(value),
                Err(cause) => {
                    diff.set(String::new());
                    on_error.run(cause);
                }
            }
            loading.set(false);
        });
    });
    Effect::new(move |_| load.run(()));

    view! {
        <div class="zai-diff-surface" aria-busy=move || loading.get()>
            <header>
                <span>"Working tree diff"</span>
                <button on:click=move |_| load.run(()) aria-label="Refresh diff">
                    <Icon icon=LuRefreshCw width="14px" height="14px" />
                </button>
            </header>
            <Show
                when=move || !diff.get().is_empty()
                fallback=move || view! {
                    <div class="zai-surface-placeholder">
                        <span>{move || if loading.get() { "Loading diff…" } else { "No working tree changes." }}</span>
                    </div>
                }
            >
                <pre><code>
                    <For
                        each=move || diff_lines.get()
                        key=|line| line.clone()
                        children=|line| {
                            let class = if line.starts_with('+') && !line.starts_with("+++") {
                                "addition"
                            } else if line.starts_with('-') && !line.starts_with("---") {
                                "deletion"
                            } else if line.starts_with("@@") {
                                "hunk"
                            } else if line.starts_with("diff --git") {
                                "heading"
                            } else {
                                ""
                            };
                            view! { <span class=class>{format!("{line}\n")}</span> }
                        }
                    />
                </code></pre>
            </Show>
        </div>
    }
}

#[component]
pub fn TerminalViewport(
    session_id: String,
    #[prop(default = true)] autofocus: bool,
) -> impl IntoView {
    let mount = NodeRef::<leptos::html::Div>::new();
    let mount_key = storage::unique_id("terminal-mount");
    let effect_key = mount_key.clone();
    let id_for_effect = session_id.clone();
    Effect::new(move |_| {
        if MOUNTED_TERMINALS.with(|items| items.borrow().contains_key(&effect_key)) {
            return;
        }
        let Some(element) = mount.get() else {
            return;
        };
        let write_id = id_for_effect.clone();
        let resize_id = id_for_effect.clone();
        match bridge::mount_terminal(
            &element,
            &id_for_effect,
            move |data| {
                let session_id = write_id.clone();
                spawn_local(async move {
                    let _ = bridge::terminal_write(&session_id, &data).await;
                });
            },
            move |cols, rows| {
                let session_id = resize_id.clone();
                spawn_local(async move {
                    let _ = bridge::terminal_resize(&session_id, cols, rows).await;
                });
            },
            autofocus,
        ) {
            Ok(handle) => MOUNTED_TERMINALS.with(|items| {
                items.borrow_mut().insert(effect_key.clone(), handle);
            }),
            Err(cause) => {
                let id = id_for_effect.clone();
                spawn_local(async move {
                    let _ = bridge::terminal_runtime_exit(&id, None, Some(&cause)).await;
                });
            }
        }
    });
    on_cleanup(move || {
        MOUNTED_TERMINALS.with(|items| {
            items.borrow_mut().remove(&mount_key);
        });
    });

    view! {
        <div
            node_ref=mount
            class="zai-xterm-viewport"
            data-terminal-session=session_id
            aria-label="Interactive terminal"
        />
    }
}

#[component]
pub fn RightWorkspacePanel(
    ui: Signal<SessionWorkspaceUi>,
    workspace: Signal<String>,
    repo_is_ready: Signal<bool>,
    on_activate: Callback<String>,
    on_close_surface: Callback<String>,
    on_add_surface: Callback<WorkspaceSurfaceKind>,
    on_close_panel: Callback<()>,
    on_error: Callback<String>,
) -> impl IntoView {
    let add_menu = RwSignal::new(false);
    let active = Signal::derive(move || {
        let ui = ui.get();
        let id = ui.active_surface_id?;
        ui.surfaces.into_iter().find(|surface| surface.id == id)
    });

    view! {
        <Show when=move || ui.get().right_panel_open>
            <aside
                class="zai-right-workspace-panel"
                data-slot="right-workspace-panel"
                aria-label="Workspace tools"
            >
                <header class="zai-right-workspace-panel__tabbar">
                    <div class="zai-right-workspace-panel__tabs" role="tablist">
                        <For
                            each=move || ui.get().surfaces
                            key=|surface| surface.id.clone()
                            children=move |surface| {
                                let select_id = surface.id.clone();
                                let active_id = select_id.clone();
                                let close_id = surface.id.clone();
                                view! {
                                    <div
                                        class="zai-surface-tab"
                                        data-active=move || if ui.read().active_surface_id.as_deref() == Some(active_id.as_str()) { "true" } else { "false" }
                                    >
                                        <button
                                            class="zai-surface-tab__select"
                                            role="tab"
                                            on:click=move |_| on_activate.run(select_id.clone())
                                        >
                                            <Icon icon=surface_icon(surface.kind) width="14px" height="14px" />
                                            <span>{surface.title.clone()}</span>
                                        </button>
                                        <button
                                            class="zai-surface-tab__close"
                                            aria-label=format!("Close {}", surface.title)
                                            on:click=move |_| on_close_surface.run(close_id.clone())
                                        >
                                            <Icon icon=LuX width="13px" height="13px" />
                                        </button>
                                    </div>
                                }
                            }
                        />
                        <div class="zai-add-surface">
                            <button
                                class="zai-add-surface__button"
                                aria-label="Add panel surface"
                                on:click=move |_| add_menu.update(|open| *open = !*open)
                            >
                                <Icon icon=LuPlus width="15px" height="15px" />
                            </button>
                            <Show when=move || add_menu.get()>
                                <div class="zai-add-surface__menu" role="menu">
                                    <For
                                        each=move || WorkspaceSurfaceKind::ALL
                                        key=|kind| *kind
                                        children=move |kind| view! {
                                            <button
                                                disabled=move || kind == WorkspaceSurfaceKind::Diff && !repo_is_ready.get()
                                                on:click=move |_| {
                                                    add_menu.set(false);
                                                    on_add_surface.run(kind);
                                                }
                                            >
                                                <Icon icon=surface_icon(kind) width="14px" height="14px" />
                                                <span>{kind.label()}</span>
                                            </button>
                                        }
                                    />
                                </div>
                            </Show>
                        </div>
                    </div>
                    <button
                        class="zai-right-workspace-panel__close"
                        aria-label="Close right panel"
                        on:click=move |_| on_close_panel.run(())
                    >
                        <Icon icon=LuX width="15px" height="15px" />
                    </button>
                </header>

                <Show
                    when=move || active.get().is_some()
                    fallback=move || view! {
                        <div class="zai-surface-empty">
                            <div class="zai-surface-empty__intro">
                                <strong>"Open a surface"</strong>
                                <span>"Choose what to show in the right panel."</span>
                            </div>
                            <div class="zai-surface-empty__grid">
                                <For
                                    each=move || WorkspaceSurfaceKind::ALL
                                    key=|kind| *kind
                                    children=move |kind| view! {
                                        <button
                                            disabled=move || kind == WorkspaceSurfaceKind::Diff && !repo_is_ready.get()
                                            on:click=move |_| on_add_surface.run(kind)
                                        >
                                            <Icon icon=surface_icon(kind) width="20px" height="20px" />
                                            <strong>{kind.label()}</strong>
                                            <span>{kind.description()}</span>
                                        </button>
                                    }
                                />
                            </div>
                        </div>
                    }
                >
                    <section class="zai-right-workspace-panel__content" role="tabpanel">
                        {move || active.get().map(|surface| match surface.kind {
                            WorkspaceSurfaceKind::Chat => view! {
                                <OfficialChatSurface on_error=on_error />
                            }.into_any(),
                            WorkspaceSurfaceKind::Browser => view! {
                                <BrowserSurface on_error=on_error />
                            }.into_any(),
                            WorkspaceSurfaceKind::Terminal => surface.resource_id.map(|id| view! {
                                <TerminalViewport session_id=id />
                            }.into_any()).unwrap_or_else(|| view! {
                                <div class="zai-surface-placeholder"><span>"Terminal unavailable."</span></div>
                            }.into_any()),
                            WorkspaceSurfaceKind::Files => view! {
                                <FilesSurface workspace=workspace on_error=on_error />
                            }.into_any(),
                            WorkspaceSurfaceKind::Diff => view! {
                                <DiffSurface workspace=workspace on_error=on_error />
                            }.into_any(),
                        })}
                    </section>
                </Show>
            </aside>
        </Show>
    }
}

#[component]
pub fn BottomTerminalPanel(
    ui: Signal<SessionWorkspaceUi>,
    on_activate: Callback<String>,
    on_close_terminal: Callback<String>,
    on_new_terminal: Callback<()>,
    on_clear: Callback<String>,
    on_close_panel: Callback<()>,
) -> impl IntoView {
    let active = Signal::derive(move || {
        let ui = ui.get();
        let id = ui.active_terminal_id?;
        ui.terminals.into_iter().find(|terminal| terminal.id == id)
    });

    view! {
        <Show when=move || ui.get().bottom_panel_open>
            <aside
                class="zai-bottom-terminal"
                data-slot="bottom-terminal-panel"
                aria-label="Terminal drawer"
                style=move || format!("height:{}px", ui.get().terminal_height)
            >
                <div class="zai-bottom-terminal__resize-handle" role="separator" />
                <header class="zai-bottom-terminal__header">
                    <div class="zai-bottom-terminal__tabs" role="tablist">
                        <For
                            each=move || ui.get().terminals
                            key=|terminal| terminal.id.clone()
                            children=move |terminal| {
                                let select_id = terminal.id.clone();
                                let active_id = select_id.clone();
                                let close_id = terminal.id.clone();
                                let title = terminal.title.clone();
                                view! {
                                    <div
                                        class="zai-terminal-tab"
                                        data-active=move || if ui.read().active_terminal_id.as_deref() == Some(active_id.as_str()) { "true" } else { "false" }
                                    >
                                        <button
                                            role="tab"
                                            title=format!("{} — {}", terminal.title, terminal.cwd)
                                            on:click=move |_| on_activate.run(select_id.clone())
                                        >
                                            <Icon icon=LuSquareTerminal width="14px" height="14px" />
                                            <span>{title}</span>
                                        </button>
                                        <button
                                            class="zai-terminal-tab__close"
                                            aria-label="Close terminal"
                                            on:click=move |_| on_close_terminal.run(close_id.clone())
                                        >
                                            <Icon icon=LuX width="13px" height="13px" />
                                        </button>
                                    </div>
                                }
                            }
                        />
                    </div>
                    <div class="zai-terminal-actions" role="toolbar">
                        <button on:click=move |_| on_new_terminal.run(()) aria-label="New terminal">
                            <Icon icon=LuPlus width="14px" height="14px" />
                        </button>
                        <button
                            disabled=move || active.get().is_none()
                            on:click=move |_| {
                                if let Some(terminal) = active.get() {
                                    on_clear.run(terminal.id);
                                }
                            }
                            aria-label="Clear terminal"
                        >
                            <Icon icon=LuEraser width="14px" height="14px" />
                        </button>
                        <button
                            disabled=move || active.get().is_none()
                            on:click=move |_| {
                                if let Some(terminal) = active.get() {
                                    on_close_terminal.run(terminal.id);
                                }
                            }
                            aria-label="Close terminal"
                        >
                            <Icon icon=LuTrash2 width="14px" height="14px" />
                        </button>
                        <button on:click=move |_| on_close_panel.run(()) aria-label="Close terminal drawer">
                            <Icon icon=LuX width="14px" height="14px" />
                        </button>
                    </div>
                </header>
                <Show
                    when=move || active.get().is_some()
                    fallback=move || view! {
                        <div class="zai-bottom-terminal__empty">
                            <Icon icon=LuSquareTerminal width="22px" height="22px" />
                            <span>"No terminal sessions for this workspace yet."</span>
                            <button on:click=move |_| on_new_terminal.run(())>"New terminal"</button>
                        </div>
                    }
                >
                    {move || active.get().map(|terminal| view! {
                        <section class="zai-bottom-terminal__viewport" role="tabpanel">
                            <TerminalViewport session_id=terminal.id />
                        </section>
                    })}
                </Show>
            </aside>
        </Show>
    }
}

#[component]
pub fn GitCommitDialog(
    open: Signal<bool>,
    summary: Signal<Option<RepoSummary>>,
    busy: Signal<bool>,
    on_close: Callback<()>,
    on_commit: Callback<Option<String>>,
) -> impl IntoView {
    let message = RwSignal::new(String::new());
    view! {
        <Show when=move || open.get()>
            <div class="zai-git-dialog-scrim">
                <section
                    class="zai-git-dialog"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="zai-git-commit-title"
                    aria-busy=move || busy.get()
                >
                    <header>
                        <span class="zai-git-dialog__icon">
                            <Icon icon=LuGitCommitHorizontal width="18px" height="18px" />
                        </span>
                        <div>
                            <h2 id="zai-git-commit-title">"Commit changes"</h2>
                            <p>"Review and commit every current workspace change."</p>
                        </div>
                        <button disabled=move || busy.get() on:click=move |_| on_close.run(())>
                            <Icon icon=LuX width="15px" height="15px" />
                        </button>
                    </header>
                    <div class="zai-git-dialog__body">
                        <div class="zai-git-dialog__summary">
                            <span>{move || format!("{} changed files", summary.get().map(|value| value.changed_files.len()).unwrap_or_default())}</span>
                            <span>{move || format!("{} staged", summary.get().map(|value| value.staged_count).unwrap_or_default())}</span>
                            <span>{move || format!("{} untracked", summary.get().map(|value| value.untracked_count).unwrap_or_default())}</span>
                        </div>
                        <div class="zai-git-dialog__files">
                            <For
                                each=move || summary.get().map(|value| value.changed_files).unwrap_or_default()
                                key=|file| file.path.clone()
                                children=|file| view! {
                                    <div><code>{file.status}</code><span title=file.path.clone()>{file.path.clone()}</span></div>
                                }
                            />
                        </div>
                        <label>
                            <span>"Commit message " <small>"optional"</small></span>
                            <input
                                prop:value=move || message.get()
                                maxlength="240"
                                autocomplete="off"
                                placeholder="Leave blank to generate a concise workspace message"
                                disabled=move || busy.get()
                                on:input=move |event| message.set(event_target_value(&event))
                                on:keydown=move |event: KeyboardEvent| {
                                    if event.key() == "Enter" && !event.is_composing() {
                                        event.prevent_default();
                                        let value = message.get().trim().to_owned();
                                        on_commit.run((!value.is_empty()).then_some(value));
                                    }
                                }
                            />
                        </label>
                    </div>
                    <footer>
                        <button
                            class="zai-git-dialog__cancel"
                            disabled=move || busy.get()
                            on:click=move |_| on_close.run(())
                        >
                            "Cancel"
                        </button>
                        <button
                            class="zai-git-dialog__commit"
                            disabled=move || busy.get() || summary.get().is_none_or(|value| value.changed_files.is_empty())
                            on:click=move |_| {
                                let value = message.get().trim().to_owned();
                                on_commit.run((!value.is_empty()).then_some(value));
                            }
                        >
                            {move || if busy.get() { "Committing…" } else { "Commit all changes" }}
                        </button>
                    </footer>
                </section>
            </div>
        </Show>
    }
}
