use icondata::LuLoaderCircle;
use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use leptos_icons::Icon;
use wasm_bindgen_futures::spawn_local;

use crate::{bridge, model::AccountProfile};

use super::OnyxOrb;

const DASHBOARD_ICONS_BASE: &str = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons@46b860c70e866212311aef2f98da3775c17f5068/svg";

#[component]
pub fn AccountGate(
    profile: Signal<Option<AccountProfile>>,
    loading: Signal<bool>,
    error: Signal<Option<String>>,
    children: ChildrenFn,
) -> impl IntoView {
    let email = RwSignal::new(String::new());
    let busy = RwSignal::new(None::<String>);
    let message = RwSignal::new(None::<String>);
    let signed_in = Signal::derive(move || profile.get().is_some());

    let begin = Callback::new(move |login_hint: Option<String>| {
        if busy.get().is_some() {
            return;
        }
        busy.set(Some(if login_hint.is_some() {
            "email".to_owned()
        } else {
            "social".to_owned()
        }));
        message.set(None);
        spawn_local(async move {
            let result = async {
                let flow = bridge::start_clerk_oauth(login_hint.as_deref()).await?;
                bridge::open_url(&flow.authorize_url).await?;
                Ok::<(), String>(())
            }
            .await;
            match result {
                Ok(()) => message.set(Some(
                    "Finish signing in in your browser. Onyx will continue automatically."
                        .to_owned(),
                )),
                Err(cause) => message.set(Some(cause)),
            }
            busy.set(None);
        });
    });

    view! {
        <Show
            when=move || signed_in.get()
            fallback=move || view! {
                <main class="onyx-account-gate">
                    <section class="onyx-account-gate__content">
                        <header class="onyx-account-gate__intro">
                            <OnyxOrb label="Onyx" />
                            <h1>"Welcome to Onyx"</h1>
                            <p>"Sign in securely in your browser to sync coding sessions, chats, voice history, and preferences."</p>
                        </header>

                        <Show
                            when=move || !loading.get()
                            fallback=move || view! {
                                <div class="onyx-account-gate__loading">
                                    <Icon icon=LuLoaderCircle width="16px" height="16px" />
                                    "Preparing secure sign in…"
                                </div>
                            }
                        >
                            <div class="onyx-account-gate__auth" aria-label="Sign in to Onyx">
                                <div class="onyx-account-gate__providers">
                                    <button
                                        type="button"
                                        on:click=move |_| begin.run(None)
                                        disabled=move || busy.get().is_some()
                                    >
                                        <img src=format!("{DASHBOARD_ICONS_BASE}/apple.svg") alt="" draggable="false" />
                                        <span>{move || if busy.get().as_deref() == Some("social") { "Opening…" } else { "Continue with Apple" }}</span>
                                    </button>
                                    <button
                                        type="button"
                                        on:click=move |_| begin.run(None)
                                        disabled=move || busy.get().is_some()
                                    >
                                        <img src=format!("{DASHBOARD_ICONS_BASE}/google.svg") alt="" draggable="false" />
                                        <span>{move || if busy.get().as_deref() == Some("social") { "Opening…" } else { "Continue with Google" }}</span>
                                    </button>
                                </div>

                                <div class="onyx-account-gate__divider">
                                    <span>"or continue with email"</span>
                                </div>

                                <form
                                    class="onyx-account-gate__form"
                                    on:submit=move |event: SubmitEvent| {
                                        event.prevent_default();
                                        let identifier = email.get().trim().to_owned();
                                        if !identifier.is_empty() {
                                            begin.run(Some(identifier));
                                        }
                                    }
                                >
                                    <label for="onyx-account-email">"Email address"</label>
                                    <input
                                        id="onyx-account-email"
                                        type="email"
                                        inputmode="email"
                                        autocomplete="email"
                                        autocapitalize="none"
                                        spellcheck="false"
                                        placeholder="you@example.com"
                                        prop:value=move || email.get()
                                        on:input=move |event| email.set(event_target_value(&event))
                                    />
                                    <button
                                        class="onyx-account-gate__primary"
                                        type="submit"
                                        disabled=move || busy.get().is_some() || email.get().trim().is_empty()
                                    >
                                        {move || if busy.get().as_deref() == Some("email") { "Opening…" } else { "Continue with email" }}
                                    </button>
                                </form>
                            </div>
                        </Show>

                        <Show when=move || error.get().is_some() || message.get().is_some()>
                            <p class="onyx-account-gate__message" role="status" aria-live="polite">
                                {move || error.get().or_else(|| message.get()).unwrap_or_default()}
                            </p>
                        </Show>
                    </section>
                </main>
            }
        >
            {children()}
        </Show>
    }
}
