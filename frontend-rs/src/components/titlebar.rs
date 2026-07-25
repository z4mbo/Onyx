use icondata::{
    LuDownload, LuLogOut, LuPanelBottom, LuPanelRight, LuPlus, LuSettings, LuUserRound, LuX,
};
use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos_icons::Icon;

use crate::model::{AccountProfile, UpdateInfo};

use super::OnyxOrb;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitlebarTab {
    pub id: String,
    pub label: String,
    pub active: bool,
    pub running: bool,
    pub project_initial: char,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitlebarSession {
    pub id: String,
    pub label: String,
    pub project: String,
}

#[component]
pub fn Titlebar(
    tabs: Signal<Vec<TitlebarTab>>,
    sessions: Signal<Vec<TitlebarSession>>,
    on_select: Callback<String>,
    on_close: Callback<String>,
    on_new: Callback<()>,
    on_open_session: Callback<String>,
    on_home: Callback<()>,
    on_settings: Callback<()>,
    on_sign_out: Callback<()>,
    update: Signal<Option<UpdateInfo>>,
    on_update: Callback<()>,
    profile: Signal<Option<AccountProfile>>,
    show_layout_controls: Signal<bool>,
    bottom_panel_open: Signal<bool>,
    right_panel_open: Signal<bool>,
    bottom_panel_available: Signal<bool>,
    right_panel_available: Signal<bool>,
    on_toggle_bottom_panel: Callback<()>,
    on_toggle_right_panel: Callback<()>,
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
                            key=|tab| {
                                (
                                    tab.id.clone(),
                                    tab.label.clone(),
                                    tab.active,
                                    tab.running,
                                )
                            }
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
                        aria-label="New or open session"
                        aria-expanded=move || new_menu_open.get()
                        title="New or open session"
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
                            <Show when=move || !sessions.read().is_empty()>
                                <p>"Open session"</p>
                                <For
                                    each=move || sessions.get()
                                    key=|session| {
                                        (
                                            session.id.clone(),
                                            session.label.clone(),
                                            session.project.clone(),
                                        )
                                    }
                                    children=move |session| {
                                        let id = session.id.clone();
                                        let initial = session
                                            .project
                                            .chars()
                                            .next()
                                            .unwrap_or('O')
                                            .to_ascii_uppercase();
                                        view! {
                                            <button on:click=move |_| {
                                                set_new_menu_open.set(false);
                                                on_open_session.run(id.clone());
                                            }>
                                                <span class="zai-titlebar__project-icon">{initial}</span>
                                                <span>
                                                    <strong>{session.label}</strong>
                                                    <small>{session.project}</small>
                                                </span>
                                            </button>
                                        }
                                    }
                                />
                            </Show>
                        </div>
                    </Show>
                </div>

                <div
                    class="zai-titlebar__drag-space"
                    data-tauri-drag-region=""
                    style="flex:1;height:100%"
                />

                <Show when=move || show_layout_controls.get()>
                    <div class="zai-layout-controls" data-slot="workspace-layout-controls">
                        <button
                            type="button"
                            class="zai-workspace-icon-button"
                            data-active=move || if bottom_panel_open.get() { "true" } else { "false" }
                            aria-label="Toggle terminal drawer (⌘J)"
                            aria-pressed=move || bottom_panel_open.get()
                            title="Toggle terminal drawer (⌘J)"
                            disabled=move || !bottom_panel_available.get()
                            on:click=move |_| on_toggle_bottom_panel.run(())
                        >
                            <Icon icon=LuPanelBottom width="16px" height="16px" />
                        </button>
                        <button
                            type="button"
                            class="zai-workspace-icon-button"
                            data-active=move || if right_panel_open.get() { "true" } else { "false" }
                            aria-label="Toggle right panel (⌘⇧J)"
                            aria-pressed=move || right_panel_open.get()
                            title="Toggle right panel (⌘⇧J)"
                            disabled=move || !right_panel_available.get()
                            on:click=move |_| on_toggle_right_panel.run(())
                        >
                            <Icon icon=LuPanelRight width="16px" height="16px" />
                        </button>
                    </div>
                </Show>

                <Show when=move || update.get().is_some()>
                    <button
                        type="button"
                        class="zai-titlebar__update"
                        on:click=move |_| on_update.run(())
                        aria-label=move || update
                            .get()
                            .map(|update| format!("Update Onyx to {}", update.version))
                            .unwrap_or_else(|| "Open update".to_owned())
                        title=move || update
                            .get()
                            .map(|update| format!("Onyx {} is ready", update.version))
                            .unwrap_or_else(|| "Open update".to_owned())
                    >
                        <Icon icon=LuDownload width="13px" height="13px" />
                        <span>"Update"</span>
                    </button>
                </Show>

                <div class="zai-titlebar__profile-wrap">
                    <button
                        type="button"
                        class="zai-titlebar__control zai-titlebar__profile"
                        style="-webkit-app-region:no-drag;appearance:none;border:0;margin:0;padding:0"
                        on:click=move |_| set_profile_menu_open.update(|open| *open = !*open)
                        aria-label="Account and settings"
                        aria-haspopup="menu"
                        aria-expanded=move || profile_menu_open.get()
                        title=move || profile
                            .get()
                            .map(|profile| profile.name)
                            .unwrap_or_else(|| "Account and settings".to_owned())
                    >
                        {move || profile.get().and_then(|profile| profile.image_url).map(|url| view! {
                            <img
                                class="zai-titlebar__profile-avatar"
                                src=url
                                alt=""
                                referrerpolicy="no-referrer"
                            />
                        }.into_any()).unwrap_or_else(|| view! {
                            <span class="zai-titlebar__profile-avatar zai-titlebar__profile-avatar--fallback">
                                <Icon icon=LuUserRound width="14px" height="14px" />
                            </span>
                        }.into_any())}
                        <Show when=move || profile.get().is_some()>
                            <span class="zai-titlebar__profile-name">
                                {move || profile.get().map(|profile| profile.name).unwrap_or_default()}
                            </span>
                        </Show>
                    </button>
                    <Show when=move || profile_menu_open.get()>
                        <div class="zai-titlebar__profile-menu" role="menu">
                            <Show when=move || profile.get().is_some()>
                                <div class="zai-titlebar__profile-summary">
                                    <strong>{move || profile.get().map(|profile| profile.name).unwrap_or_default()}</strong>
                                    <span>{move || profile.get().map(|profile| profile.email).unwrap_or_default()}</span>
                                </div>
                            </Show>
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
                            <Show when=move || profile.get().is_some()>
                                <button
                                    role="menuitem"
                                    on:click=move |_| {
                                        set_profile_menu_open.set(false);
                                        on_sign_out.run(());
                                    }
                                >
                                    <Icon icon=LuLogOut width="14px" height="14px" />
                                    <span>"Sign out"</span>
                                </button>
                            </Show>
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
