use std::{cell::RefCell, collections::HashMap};

use icondata::{
    LuBox, LuChevronDown, LuCloudUpload, LuEraser, LuExternalLink, LuFile, LuFileDiff, LuFolder,
    LuGitCommitHorizontal, LuGitPullRequest, LuGlobe, LuMessageSquare, LuPanelBottom, LuPanelRight,
    LuPlus, LuRefreshCw, LuSquareTerminal, LuTrash2, LuX,
};
use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen_futures::spawn_local;

use crate::{
    bridge,
    model::{EditorTarget, RepoSummary, WorkspaceEntry, WorkspaceFile},
    storage,
};

use super::{InternalBrowser, ProviderBadge};
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
    pub panel_width: u32,
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
            panel_width: 420,
        }
    }
}

/// Bounds that match the panels' CSS, so a drag cannot leave either panel in a
/// size the stylesheet would fight over.
const MIN_TERMINAL_HEIGHT: f64 = 180.0;
const MIN_PANEL_WIDTH: f64 = 300.0;
const MAX_PANEL_WIDTH: f64 = 920.0;

fn viewport(vertical: bool) -> f64 {
    web_sys::window()
        .and_then(|window| {
            if vertical {
                window.inner_height().ok()
            } else {
                window.inner_width().ok()
            }
        })
        .and_then(|value| value.as_f64())
        .unwrap_or(1024.0)
}

/// Marks the document while a drag is in flight. The stylesheet uses it to hold
/// the resize cursor and suppress text selection across the whole window.
fn set_resizing(active: bool) {
    let Some(root) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
    else {
        return;
    };
    if active {
        let _ = root.set_attribute("data-workspace-panel-resizing", "true");
    } else {
        let _ = root.remove_attribute("data-workspace-panel-resizing");
    }
}

/// Captures the pointer on the handle so the drag keeps tracking even when the
/// cursor outruns the element.
fn begin_drag(event: &leptos::ev::PointerEvent) {
    use wasm_bindgen::JsCast;
    if let Some(handle) = event
        .current_target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
    {
        let _ = handle.set_pointer_capture(event.pointer_id());
    }
    set_resizing(true);
}

fn end_drag(event: &leptos::ev::PointerEvent) {
    use wasm_bindgen::JsCast;
    if let Some(handle) = event
        .current_target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
    {
        let _ = handle.release_pointer_capture(event.pointer_id());
    }
    set_resizing(false);
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
fn OfficialChatSurface(browser_label: String, on_error: Callback<String>) -> impl IntoView {
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Official {
        id: &'static str,
        name: &'static str,
        brand: ProviderBrand,
        detail: &'static str,
        url: &'static str,
    }
    const OFFICIAL: [Official; 4] = [
        Official {
            id: "chatgpt",
            name: "ChatGPT",
            brand: ProviderBrand::Openai,
            detail: "Your signed-in ChatGPT subscription",
            url: "https://chatgpt.com/",
        },
        Official {
            id: "claude",
            name: "Claude",
            brand: ProviderBrand::Anthropic,
            detail: "Your signed-in Claude subscription",
            url: "https://claude.ai/new",
        },
        Official {
            id: "gemini",
            name: "Gemini",
            brand: ProviderBrand::Google,
            detail: "Your signed-in Gemini subscription",
            url: "https://gemini.google.com/app",
        },
        Official {
            id: "grok",
            name: "Grok",
            brand: ProviderBrand::Xai,
            detail: "Your signed-in Grok subscription",
            url: "https://grok.com/",
        },
    ];
    let selected = RwSignal::new(None::<Official>);

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
                            data-active=move || selected.get() == Some(provider)
                            on:click=move |_| selected.set(Some(provider))
                        >
                            <ProviderBadge brand=Signal::derive(move || provider.brand) small=true />
                            <span>{provider.name}</span>
                        </button>
                    }
                />
            </nav>
            <div class="zai-provider-sidebar__host">
                <Show
                    when=move || selected.get().is_some()
                    fallback=move || view! {
                        <div class="zai-provider-sidebar__empty">
                            <Icon icon=LuMessageSquare width="22px" height="22px" />
                            <strong>"Open an official chat"</strong>
                            <span>"Choose a provider. Its signed-in web app stays inside this Onyx panel."</span>
                        </div>
                    }
                >
                    <For
                        each=move || selected.get()
                        key=|provider| provider.id
                        children={
                            let browser_label = browser_label.clone();
                            move |provider| view! {
                                <InternalBrowser
                                    label=format!("{browser_label}-{}", provider.id)
                                    initial_url=provider.url.to_owned()
                                    show_toolbar=false
                                    on_error=on_error
                                />
                            }
                        }
                    />
                </Show>
            </div>
            <footer>"Official provider site · isolated internal browser"</footer>
        </div>
    }
}

#[component]
fn BrowserSurface(
    browser_label: String,
    initial_url: String,
    on_error: Callback<String>,
) -> impl IntoView {
    view! {
        <InternalBrowser
            label=browser_label
            initial_url=initial_url
            show_toolbar=true
            on_error=on_error
        />
    }
}

#[component]
fn FilesSurface(
    workspace: Signal<String>,
    initial_path: Option<String>,
    on_error: Callback<String>,
) -> impl IntoView {
    let entries = RwSignal::new(Vec::<WorkspaceEntry>::new());
    let selected = RwSignal::new(initial_path.unwrap_or_default());
    let file = RwSignal::new(None::<WorkspaceFile>);
    let loading = RwSignal::new(false);
    let load = Callback::new(move |_: ()| {
        let workspace = workspace.get();
        loading.set(true);
        spawn_local(async move {
            // The surface can unmount while the request is in flight; try_* keeps
            // a disposed signal from panicking.
            match bridge::workspace_entries(&workspace).await {
                Ok(result) => {
                    let _ = entries.try_set(result);
                }
                Err(cause) => on_error.run(cause),
            }
            let _ = loading.try_set(false);
        });
    });
    Effect::new(move |_| load.run(()));
    Effect::new(move |_| {
        let path = selected.get();
        if path.is_empty() {
            file.set(None);
            return;
        }
        let workspace = workspace.get();
        loading.set(true);
        spawn_local(async move {
            match bridge::read_workspace_file(&workspace, &path).await {
                Ok(result) => {
                    let _ = file.try_set(Some(result));
                }
                Err(cause) => {
                    let _ = file.try_set(None);
                    on_error.run(cause);
                }
            }
            let _ = loading.try_set(false);
        });
    });

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

fn diff_section_references_path(section: &str, path: &str) -> bool {
    let old_path = format!("a/{path}");
    let new_path = format!("b/{path}");
    section.lines().any(|line| {
        line == format!("--- {old_path}")
            || line == format!("+++ {new_path}")
            || line.strip_prefix("diff --git ").is_some_and(|header| {
                header.contains(&format!("{old_path} "))
                    || header.contains(&format!("{old_path}\""))
                    || header.ends_with(&new_path)
                    || header.ends_with(&format!("{new_path}\""))
            })
    })
}

fn diff_for_path(value: &str, path: Option<&str>) -> String {
    let Some(path) = path.map(str::trim).filter(|path| !path.is_empty()) else {
        return value.to_owned();
    };
    let path = path.strip_prefix("./").unwrap_or(path);
    let mut selected = String::new();
    let mut section = String::new();
    for line in value.split_inclusive('\n') {
        if line.starts_with("diff --git ") && !section.is_empty() {
            if diff_section_references_path(&section, path) {
                selected.push_str(&section);
            }
            section.clear();
        }
        section.push_str(line);
    }
    if diff_section_references_path(&section, path) {
        selected.push_str(&section);
    }
    selected
}

#[component]
fn DiffSurface(
    workspace: Signal<String>,
    initial_path: Option<String>,
    on_error: Callback<String>,
) -> impl IntoView {
    let diff = RwSignal::new(String::new());
    let loading = RwSignal::new(false);
    let selected_path = initial_path.filter(|path| !path.trim().is_empty());
    let has_selected_path = selected_path.is_some();
    let selected_for_load = selected_path.clone();
    let title = selected_path
        .as_deref()
        .map(|path| format!("Working tree diff · {path}"))
        .unwrap_or_else(|| "Working tree diff".to_owned());
    let diff_lines =
        Signal::derive(move || diff.get().lines().map(str::to_owned).collect::<Vec<_>>());
    let load = Callback::new(move |_: ()| {
        let workspace = workspace.get();
        let selected_path = selected_for_load.clone();
        loading.set(true);
        spawn_local(async move {
            // The surface can unmount while the request is in flight; try_* keeps
            // a disposed signal from panicking.
            match bridge::git_diff(&workspace).await {
                Ok(value) => {
                    let _ = diff.try_set(diff_for_path(&value, selected_path.as_deref()));
                }
                Err(cause) => {
                    let _ = diff.try_set(String::new());
                    on_error.run(cause);
                }
            }
            let _ = loading.try_set(false);
        });
    });
    Effect::new(move |_| load.run(()));

    view! {
        <div class="zai-diff-surface" aria-busy=move || loading.get()>
            <header>
                <span title=title.clone()>{title.clone()}</span>
                <button on:click=move |_| load.run(()) aria-label="Refresh diff">
                    <Icon icon=LuRefreshCw width="14px" height="14px" />
                </button>
            </header>
            <Show
                when=move || !diff.get().is_empty()
                fallback=move || view! {
                    <div class="zai-surface-placeholder">
                        <span>{move || {
                            if loading.get() {
                                "Loading diff…"
                            } else if has_selected_path {
                                "No previewed changes for this file."
                            } else {
                                "No working tree changes."
                            }
                        }}</span>
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
    let disposed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mount_started = std::rc::Rc::new(std::cell::Cell::new(false));
    let disposed_for_effect = disposed.clone();
    let mount_started_for_effect = mount_started.clone();
    Effect::new(move |_| {
        if disposed_for_effect.load(std::sync::atomic::Ordering::Relaxed)
            || mount_started_for_effect.get()
            || MOUNTED_TERMINALS.with(|items| items.borrow().contains_key(&effect_key))
        {
            return;
        }
        let Some(element) = mount.get() else {
            return;
        };
        mount_started_for_effect.set(true);
        let write_id = id_for_effect.clone();
        let resize_id = id_for_effect.clone();
        let id = id_for_effect.clone();
        let key = effect_key.clone();
        let disposed = disposed_for_effect.clone();
        spawn_local(async move {
            match bridge::mount_terminal(
                &element,
                &id,
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
            )
            .await
            {
                Ok(handle) if disposed.load(std::sync::atomic::Ordering::Relaxed) => drop(handle),
                Ok(handle) => MOUNTED_TERMINALS.with(|items| {
                    items.borrow_mut().insert(key, handle);
                }),
                Err(_) if disposed.load(std::sync::atomic::Ordering::Relaxed) => {}
                Err(cause) => {
                    let _ = bridge::terminal_runtime_exit(&id, None, Some(&cause)).await;
                }
            }
        });
    });
    on_cleanup(move || {
        disposed.store(true, std::sync::atomic::Ordering::Relaxed);
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
    on_resize: Callback<u32>,
    on_error: Callback<String>,
) -> impl IntoView {
    let add_menu = RwSignal::new(false);
    let active = Signal::derive(move || {
        let ui = ui.get();
        let id = ui.active_surface_id?;
        ui.surfaces.into_iter().find(|surface| surface.id == id)
    });
    let drag = RwSignal::new(None::<(f64, f64)>);
    let apply_width = move |width: f64| {
        let max = (viewport(false) * 0.78).clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH);
        on_resize.run(width.clamp(MIN_PANEL_WIDTH, max).round() as u32);
    };

    view! {
        <Show when=move || ui.get().right_panel_open>
            <aside
                class="zai-right-workspace-panel"
                data-slot="right-workspace-panel"
                aria-label="Workspace tools"
                style=move || format!("width:{}px", ui.get().panel_width)
            >
                <div
                    class="zai-right-workspace-panel__resize-handle"
                    role="separator"
                    aria-orientation="vertical"
                    aria-label="Resize the workspace panel"
                    tabindex="0"
                    on:pointerdown=move |event: leptos::ev::PointerEvent| {
                        event.prevent_default();
                        begin_drag(&event);
                        drag.set(Some((
                            f64::from(event.client_x()),
                            f64::from(ui.get_untracked().panel_width),
                        )));
                    }
                    on:pointermove=move |event: leptos::ev::PointerEvent| {
                        let Some((origin, width)) = drag.get_untracked() else {
                            return;
                        };
                        // The handle is on the left edge, so dragging left grows it.
                        apply_width(width + (origin - f64::from(event.client_x())));
                    }
                    on:pointerup=move |event: leptos::ev::PointerEvent| {
                        drag.set(None);
                        end_drag(&event);
                    }
                    on:pointercancel=move |event: leptos::ev::PointerEvent| {
                        drag.set(None);
                        end_drag(&event);
                    }
                    on:keydown=move |event: KeyboardEvent| {
                        let step = match event.key().as_str() {
                            "ArrowLeft" => 24.0,
                            "ArrowRight" => -24.0,
                            _ => return,
                        };
                        event.prevent_default();
                        apply_width(f64::from(ui.get_untracked().panel_width) + step);
                    }
                />
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
                        {move || active.get().map(|surface| {
                            let browser_label = format!("internal-browser-{}", surface.id);
                            let resource_id = surface.resource_id.clone();
                            match surface.kind {
                            WorkspaceSurfaceKind::Chat => view! {
                                <OfficialChatSurface
                                    browser_label=browser_label
                                    on_error=on_error
                                />
                            }.into_any(),
                            WorkspaceSurfaceKind::Browser => view! {
                                <BrowserSurface
                                    browser_label=browser_label
                                    initial_url=resource_id.unwrap_or_default()
                                    on_error=on_error
                                />
                            }.into_any(),
                            WorkspaceSurfaceKind::Terminal => resource_id.map(|id| view! {
                                <TerminalViewport session_id=id />
                            }.into_any()).unwrap_or_else(|| view! {
                                <div class="zai-surface-placeholder"><span>"Terminal unavailable."</span></div>
                            }.into_any()),
                            WorkspaceSurfaceKind::Files => view! {
                                <FilesSurface
                                    workspace=workspace
                                    initial_path=resource_id
                                    on_error=on_error
                                />
                            }.into_any(),
                            WorkspaceSurfaceKind::Diff => view! {
                                <DiffSurface
                                    workspace=workspace
                                    initial_path=resource_id
                                    on_error=on_error
                                />
                            }.into_any(),
                        }})}
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
    on_resize: Callback<u32>,
) -> impl IntoView {
    let active = Signal::derive(move || {
        let ui = ui.get();
        let id = ui.active_terminal_id?;
        ui.terminals.into_iter().find(|terminal| terminal.id == id)
    });
    // Start of the drag: pointer position paired with the height at that moment.
    let drag = RwSignal::new(None::<(f64, f64)>);
    let apply_height = move |height: f64| {
        let max = (viewport(true) - 200.0).max(MIN_TERMINAL_HEIGHT);
        on_resize.run(height.clamp(MIN_TERMINAL_HEIGHT, max).round() as u32);
    };

    view! {
        <Show when=move || ui.get().bottom_panel_open>
            <aside
                class="zai-bottom-terminal"
                data-slot="bottom-terminal-panel"
                aria-label="Terminal drawer"
                style=move || format!("height:{}px", ui.get().terminal_height)
            >
                <div
                    class="zai-bottom-terminal__resize-handle"
                    role="separator"
                    aria-orientation="horizontal"
                    aria-label="Resize the terminal drawer"
                    tabindex="0"
                    on:pointerdown=move |event: leptos::ev::PointerEvent| {
                        event.prevent_default();
                        begin_drag(&event);
                        drag.set(Some((
                            f64::from(event.client_y()),
                            f64::from(ui.get_untracked().terminal_height),
                        )));
                    }
                    on:pointermove=move |event: leptos::ev::PointerEvent| {
                        let Some((origin, height)) = drag.get_untracked() else {
                            return;
                        };
                        // The handle sits on the top edge, so dragging up grows it.
                        apply_height(height + (origin - f64::from(event.client_y())));
                    }
                    on:pointerup=move |event: leptos::ev::PointerEvent| {
                        drag.set(None);
                        end_drag(&event);
                    }
                    on:pointercancel=move |event: leptos::ev::PointerEvent| {
                        drag.set(None);
                        end_drag(&event);
                    }
                    on:keydown=move |event: KeyboardEvent| {
                        let step = match event.key().as_str() {
                            "ArrowUp" => 24.0,
                            "ArrowDown" => -24.0,
                            _ => return,
                        };
                        event.prevent_default();
                        apply_height(f64::from(ui.get_untracked().terminal_height) + step);
                    }
                />
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

#[cfg(test)]
mod tests {
    use super::diff_for_path;

    #[test]
    fn selected_diff_only_keeps_the_requested_file() {
        let diff = "\
diff --git a/src/one.rs b/src/one.rs
--- a/src/one.rs
+++ b/src/one.rs
@@ -1 +1 @@
-one
+ONE
diff --git a/src/two.rs b/src/two.rs
--- a/src/two.rs
+++ b/src/two.rs
@@ -1 +1 @@
-two
+TWO
";

        let selected = diff_for_path(diff, Some("src/two.rs"));
        assert!(!selected.contains("src/one.rs"));
        assert!(selected.contains("src/two.rs"));
        assert!(selected.contains("+TWO"));
    }

    #[test]
    fn whole_diff_is_preserved_without_a_selected_file() {
        let diff = "diff --git a/a b/a\n+value\n";
        assert_eq!(diff_for_path(diff, None), diff);
    }
}
