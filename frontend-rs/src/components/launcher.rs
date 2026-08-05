use gloo_timers::future::TimeoutFuture;
use icondata::LuSearch;
use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen_futures::spawn_local;

pub use crate::launcher::LauncherTarget;
use crate::launcher::build_items;
use crate::model::AgentSession;

#[component]
pub fn LauncherDialog(
    open: Signal<bool>,
    sessions: Signal<Vec<AgentSession>>,
    draft_workspace: Signal<String>,
    on_close: Callback<()>,
    on_action: Callback<LauncherTarget>,
) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let selected = RwSignal::new(0_usize);
    let input_ref = NodeRef::<leptos::html::Input>::new();
    // Frozen while the dialog is open: a background session update re-sorting
    // recents between render and Enter must not change what Enter opens.
    let snapshot = RwSignal::new(Vec::<AgentSession>::new());

    Effect::new(move |_| {
        if open.get() {
            query.set(String::new());
            selected.set(0);
            snapshot.set(sessions.get_untracked());
            spawn_local(async move {
                // The input mounts on the same tick the dialog opens; yield
                // once so focus lands on the rendered element.
                TimeoutFuture::new(30).await;
                if let Some(input) = input_ref.get_untracked() {
                    let _ = input.focus();
                }
            });
        }
    });

    let items = Memo::new(move |_| {
        build_items(
            &snapshot.get(),
            draft_workspace.get().as_str(),
            query.get().as_str(),
        )
    });

    let run_item = move |index: usize| {
        if let Some(item) = items.read_untracked().get(index) {
            on_action.run(item.target.clone());
            on_close.run(());
        }
    };

    let scroll_selected_into_view = move |align_top: bool| {
        let index = selected.get_untracked();
        spawn_local(async move {
            TimeoutFuture::new(0).await;
            let element = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| {
                    document
                        .query_selector(&format!("[data-launcher-index='{index}']"))
                        .ok()
                        .flatten()
                });
            if let Some(element) = element {
                element.scroll_into_view_with_bool(align_top);
            }
        });
    };

    let keydown = move |event: web_sys::KeyboardEvent| {
        let count = items.read_untracked().len();
        // ⌘1–⌘9 (Ctrl elsewhere) executes the Nth visible row directly.
        if (event.meta_key() || event.ctrl_key())
            && let Some(digit) = event.key().chars().next().and_then(|c| c.to_digit(10))
            && (1..=9).contains(&digit)
            && (digit as usize) <= count
        {
            event.prevent_default();
            event.stop_propagation();
            run_item(digit as usize - 1);
            return;
        }
        match event.key().as_str() {
            "ArrowDown" => {
                event.prevent_default();
                if count > 0 {
                    selected.set((selected.get_untracked() + 1).min(count - 1));
                    scroll_selected_into_view(false);
                }
            }
            "ArrowUp" => {
                event.prevent_default();
                selected.set(selected.get_untracked().saturating_sub(1));
                scroll_selected_into_view(true);
            }
            "Enter" => {
                event.prevent_default();
                if count > 0 {
                    run_item(selected.get_untracked().min(count - 1));
                }
            }
            "Escape" => {
                // Keep the global Escape handler from cancelling a running
                // turn when the user only meant to close the launcher.
                event.prevent_default();
                event.stop_propagation();
                on_close.run(());
            }
            _ => {}
        }
    };

    view! {
        <Show when=move || open.get()>
            <div class="zai-launcher-scrim" on:mousedown=move |_| on_close.run(())>
                <section
                    class="zai-launcher"
                    role="dialog"
                    aria-modal="true"
                    aria-label="Launcher"
                    on:mousedown=move |event: MouseEvent| event.stop_propagation()
                >
                    <label class="zai-launcher-search">
                        <Icon icon=LuSearch width="16px" height="16px" />
                        <input
                            node_ref=input_ref
                            prop:value=move || query.get()
                            on:input=move |event| {
                                query.set(event_target_value(&event));
                                selected.set(0);
                            }
                            on:keydown=keydown
                            placeholder="Search sessions, projects, and actions"
                            aria-label="Search sessions, projects, and actions"
                        />
                    </label>
                    <div class="zai-launcher-list" role="listbox">
                        {move || {
                            let list = items.get();
                            if list.is_empty() {
                                return view! {
                                    <p class="zai-launcher-empty">"No matches"</p>
                                }
                                .into_any();
                            }
                            let selected_index = selected.get().min(list.len() - 1);
                            list.iter()
                                .enumerate()
                                .map(|(index, item)| {
                                    let header = (index == 0
                                        || list[index - 1].group != item.group)
                                        .then_some(item.group);
                                    view! {
                                        {header
                                            .map(|group| {
                                                view! {
                                                    <p class="zai-launcher-group">{group}</p>
                                                }
                                            })}
                                        <button
                                            type="button"
                                            class="zai-launcher-item"
                                            data-launcher-index=index
                                            data-selected=if index == selected_index {
                                                "true"
                                            } else {
                                                "false"
                                            }
                                            // pointermove (not pointerenter) so keyboard
                                            // scrolling the list under a resting cursor
                                            // does not yank the selection back.
                                            on:pointermove=move |_| {
                                                if selected.get_untracked() != index {
                                                    selected.set(index);
                                                }
                                            }
                                            on:click=move |_| run_item(index)
                                        >
                                            <span class="zai-launcher-item-title">
                                                {item.title.clone()}
                                            </span>
                                            <span class="zai-launcher-item-detail">
                                                {item.detail.clone()}
                                            </span>
                                            {(index < 9)
                                                .then(|| {
                                                    view! {
                                                        <kbd class="zai-launcher-item-jump">
                                                            {format!("⌘{}", index + 1)}
                                                        </kbd>
                                                    }
                                                })}
                                        </button>
                                    }
                                })
                                .collect_view()
                                .into_any()
                        }}
                    </div>
                    <footer class="zai-launcher-footer">
                        <span><kbd>"↑↓"</kbd>" navigate"</span>
                        <span><kbd>"↵"</kbd>" open"</span>
                        <span><kbd>"⌘1-9"</kbd>" jump"</span>
                        <span><kbd>">"</kbd>" actions"</span>
                        <span><kbd>"esc"</kbd>" close"</span>
                    </footer>
                </section>
            </div>
        </Show>
    }
}
