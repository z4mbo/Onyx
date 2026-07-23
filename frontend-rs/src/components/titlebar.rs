use icondata::{LuPlus, LuSettings, LuUserRound, LuX};
use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos_icons::Icon;

use super::OnyxOrb;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitlebarTab {
    pub id: String,
    pub label: String,
    pub active: bool,
    pub running: bool,
    pub project_initial: char,
}

#[component]
pub fn Titlebar(
    tabs: Signal<Vec<TitlebarTab>>,
    on_select: Callback<String>,
    on_close: Callback<String>,
    on_new: Callback<()>,
    on_home: Callback<()>,
    on_settings: Callback<()>,
) -> impl IntoView {
    let (new_menu_open, set_new_menu_open) = signal(false);
    let (profile_menu_open, set_profile_menu_open) = signal(false);
    let is_macos = web_sys::window()
        .map(|window| window.navigator().platform().unwrap_or_default())
        .is_some_and(|platform| platform.to_ascii_lowercase().contains("mac"));

    view! {
        <header
            class="zai-titlebar"
            data-slot="zai-titlebar"
            data-platform=if is_macos { "macos" } else { "other" }
            data-tauri-drag-region=""
            style=move || format!(
                "-webkit-app-region:drag;box-sizing:border-box;display:flex;height:36px;\
                 min-height:36px;width:100%;padding-left:{};overflow:visible",
                if is_macos { "84px" } else { "0" },
            )
        >
            <div
                class="zai-titlebar__inner"
                data-tauri-drag-region=""
                style="box-sizing:border-box;display:flex;align-items:center;gap:6px;height:36px;\
                       width:100%;padding:8px 12px 0 8px;overflow:visible"
            >
                <button
                    type="button"
                    class="zai-titlebar__control zai-titlebar__home"
                    style="-webkit-app-region:no-drag;appearance:none;border:0;margin:0;padding:0;\
                           display:inline-flex;align-items:center;justify-content:center;width:36px;\
                           height:28px;min-width:36px;border-radius:6px;background:transparent"
                    on:click=move |_| on_home.run(())
                    aria-label="Home"
                    title="Home"
                >
                    <OnyxOrb class="zai-app-icon" label="Onyx" />
                </button>

                <div
                    class="zai-titlebar__tabs"
                    data-slot="zai-titlebar-tabs"
                    style="position:relative;min-width:0;max-width:100%;overflow:hidden"
                >
                    <div
                        class="zai-titlebar__tabs-scroll"
                        data-slot="zai-titlebar-tabs-scroll"
                        role="tablist"
                        aria-label="Open sessions"
                        style="-webkit-app-region:no-drag;display:flex;align-items:center;min-width:0;\
                               max-width:100%;overflow-x:auto;scrollbar-width:none"
                    >
                        <For
                            each=move || tabs.get()
                            key=|tab| tab.id.clone()
                            children=move |tab| {
                                let select_id = tab.id.clone();
                                let close_id = tab.id.clone();
                                let close_id_aux = tab.id.clone();
                                view! {
                                    <div
                                        class="zai-titlebar__tab-slot"
                                        data-tab-id=tab.id
                                        data-active=if tab.active { "true" } else { "false" }
                                        data-running=if tab.running { "true" } else { "false" }
                                        style="display:flex;position:relative;flex:0 1 224px;width:224px;\
                                               min-width:28px;max-width:224px;height:28px"
                                        on:auxclick=move |event: MouseEvent| {
                                            if event.button() == 1 {
                                                event.prevent_default();
                                                on_close.run(close_id_aux.clone());
                                            }
                                        }
                                    >
                                        <div
                                            class="zai-titlebar__tab"
                                            data-slot="zai-titlebar-tab-item"
                                            data-active=if tab.active { "true" } else { "false" }
                                            data-running=if tab.running { "true" } else { "false" }
                                            style="display:flex;align-items:center;gap:6px;width:100%;height:28px;\
                                                   min-width:0;padding:0 6px;overflow:hidden;border-radius:6px;\
                                                   white-space:nowrap"
                                        >
                                            <button
                                                type="button"
                                                class="zai-titlebar__tab-select"
                                                role="tab"
                                                aria-selected=tab.active
                                                tabindex=if tab.active { 0 } else { -1 }
                                                style="-webkit-app-region:no-drag;appearance:none;border:0;margin:0;\
                                                       padding:0;display:flex;align-items:center;gap:6px;flex:1;\
                                                       height:100%;min-width:0;background:transparent;text-align:left"
                                                on:click=move |_| on_select.run(select_id.clone())
                                            >
                                                <span
                                                    class="zai-titlebar__tab-icon"
                                                    style="display:inline-flex;align-items:center;justify-content:center;\
                                                           width:16px;height:16px;min-width:16px"
                                                >
                                                    <Show
                                                        when=move || tab.running
                                                        fallback=move || view! {
                                                            <span class="zai-titlebar__project-icon">
                                                                {tab.project_initial}
                                                            </span>
                                                        }
                                                    >
                                                        <RunningIndicator />
                                                    </Show>
                                                </span>
                                                <span
                                                    class="zai-titlebar__tab-label"
                                                    style="flex:1;min-width:0;overflow:hidden;text-overflow:clip;\
                                                           white-space:nowrap;font-size:13px;font-weight:500;\
                                                           line-height:16px"
                                                >
                                                    {tab.label}
                                                </span>
                                            </button>
                                            <button
                                                type="button"
                                                class="zai-titlebar__tab-close"
                                                data-slot="zai-titlebar-tab-close"
                                                style="-webkit-app-region:no-drag;appearance:none;border:0;margin:0;\
                                                       padding:0;display:inline-flex;align-items:center;\
                                                       justify-content:center;width:20px;height:20px;min-width:20px;\
                                                       border-radius:4px;background:transparent"
                                                on:click=move |event: MouseEvent| {
                                                    event.prevent_default();
                                                    event.stop_propagation();
                                                    on_close.run(close_id.clone());
                                                }
                                                aria-label="Close session"
                                                title="Close session"
                                            >
                                                <Icon icon=LuX width="12px" height="12px" />
                                            </button>
                                        </div>
                                    </div>
                                }
                            }
                        />
                    </div>
                </div>

                <div class="zai-titlebar__new-wrap">
                    <button
                        type="button"
                        class="zai-titlebar__control zai-titlebar__new"
                        style="-webkit-app-region:no-drag;appearance:none;border:0;margin:0;padding:0;\
                               display:inline-flex;align-items:center;justify-content:center;width:28px;\
                               height:28px;min-width:28px;border-radius:6px;background:transparent"
                        on:click=move |_| set_new_menu_open.update(|open| *open = !*open)
                        aria-label="New session"
                        aria-expanded=move || new_menu_open.get()
                        title="New session"
                    >
                        <Icon icon=LuPlus width="14px" height="14px" />
                    </button>
                    <Show when=move || new_menu_open.get()>
                        <div class="zai-titlebar__new-menu">
                            <button on:click=move |_| {
                                set_new_menu_open.set(false);
                                on_new.run(());
                            }>
                                <Icon icon=LuPlus width="14px" height="14px" />
                                <span><strong>"New session"</strong><small>"Start in the current project"</small></span>
                            </button>
                        </div>
                    </Show>
                </div>

                <div
                    class="zai-titlebar__drag-space"
                    data-tauri-drag-region=""
                    style="flex:1;height:100%"
                />

                <div class="zai-titlebar__profile-wrap">
                    <button
                        type="button"
                        class="zai-titlebar__control zai-titlebar__profile"
                        style="-webkit-app-region:no-drag;appearance:none;border:0;margin:0;padding:0"
                        on:click=move |_| set_profile_menu_open.update(|open| *open = !*open)
                        aria-label="Account and settings"
                        aria-haspopup="menu"
                        aria-expanded=move || profile_menu_open.get()
                        title="Account and settings"
                    >
                        <span class="zai-titlebar__profile-avatar zai-titlebar__profile-avatar--fallback">
                            <Icon icon=LuUserRound width="14px" height="14px" />
                        </span>
                    </button>
                    <Show when=move || profile_menu_open.get()>
                        <div class="zai-titlebar__profile-menu" role="menu">
                            <button
                                role="menuitem"
                                on:click=move |_| {
                                    set_profile_menu_open.set(false);
                                    on_settings.run(());
                                }
                            >
                                <Icon icon=LuSettings width="14px" height="14px" />
                                <span>"Settings"</span>
                            </button>
                        </div>
                    </Show>
                </div>
            </div>
        </header>
    }
}

#[component]
fn RunningIndicator() -> impl IntoView {
    let dots = (0..25)
        .map(|index| {
            (
                index,
                1.5 + f64::from(index % 5) * 3.0,
                1.5 + f64::from(index / 5) * 3.0,
            )
        })
        .collect::<Vec<_>>();
    view! {
        <svg
            class="zai-titlebar__running-indicator"
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            aria-hidden="true"
        >
            <For
                each=move || dots.clone()
                key=|(index, _, _)| *index
                children=|(index, x, y)| view! {
                    <rect data-dot=index x=x y=y width="2" height="2" fill="currentColor" />
                }
            />
        </svg>
    }
}
