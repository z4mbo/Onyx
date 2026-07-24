use icondata::{LuDownload, LuLoaderCircle, LuSparkles, LuX};
use leptos::prelude::*;
use leptos_icons::Icon;

use crate::{
    markdown,
    model::{UpdateInfo, UpdateProgress},
};

const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

fn current_release_notes() -> String {
    let mut lines = CHANGELOG.lines();
    let mut notes = Vec::new();
    let mut in_release = false;
    for line in lines.by_ref() {
        if line.starts_with("## ") {
            if in_release {
                break;
            }
            in_release = true;
            continue;
        }
        if in_release {
            notes.push(line);
        }
    }
    let notes = notes.join("\n").trim().to_owned();
    if notes.is_empty() {
        "Performance, reliability, and interface improvements.".to_owned()
    } else {
        notes
    }
}

#[component]
pub fn UpdateDialog(
    open: Signal<bool>,
    update: Signal<Option<UpdateInfo>>,
    installing: Signal<bool>,
    progress: Signal<Option<UpdateProgress>>,
    on_close: Callback<()>,
    on_install: Callback<()>,
) -> impl IntoView {
    let version = Signal::derive(move || {
        update
            .get()
            .map(|value| value.version)
            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned())
    });
    let title = Signal::derive(move || {
        if update.get().is_some() {
            "A new Onyx is ready".to_owned()
        } else {
            format!("What’s new in Onyx {}", version.get())
        }
    });
    let notes = Signal::derive(move || {
        update
            .get()
            .and_then(|value| value.notes)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(current_release_notes)
    });
    let rendered_notes = Signal::derive(move || markdown::render(&notes.get()));
    let progress_label = Signal::derive(move || {
        let Some(progress) = progress.get() else {
            return "Preparing download…".to_owned();
        };
        match progress.total.filter(|total| *total > 0) {
            Some(total) => format!(
                "Downloading… {}%",
                progress.downloaded.saturating_mul(100) / total
            ),
            None => format!(
                "Downloading… {:.1} MB",
                progress.downloaded as f64 / 1_048_576.0
            ),
        }
    });
    let progress_width = Signal::derive(move || {
        progress
            .get()
            .and_then(|progress| {
                progress
                    .total
                    .filter(|total| *total > 0)
                    .map(|total| progress.downloaded.saturating_mul(100) / total)
            })
            .unwrap_or(8)
            .min(100)
    });

    view! {
        <Show when=move || open.get()>
            <div class="zai-update-dialog-scrim">
                <section
                    class="zai-update-dialog"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="zai-update-dialog-title"
                >
                    <header>
                        <span class="zai-update-dialog__icon">
                            <Icon icon=LuSparkles width="18px" height="18px" />
                        </span>
                        <div>
                            <p>"ONYX UPDATE"</p>
                            <h2 id="zai-update-dialog-title">{move || title.get()}</h2>
                        </div>
                        <button
                            type="button"
                            disabled=move || installing.get()
                            on:click=move |_| on_close.run(())
                            aria-label="Close update notes"
                        >
                            <Icon icon=LuX width="15px" height="15px" />
                        </button>
                    </header>
                    <div class="zai-update-dialog__body">
                        <div
                            class="zai-update-dialog__notes zai-message-markdown"
                            inner_html=move || rendered_notes.get()
                        />
                        <Show when=move || installing.get()>
                            <div class="zai-update-dialog__progress" role="status">
                                <div>
                                    <Icon
                                        icon=LuLoaderCircle
                                        width="14px"
                                        height="14px"
                                        attr:class="spin"
                                    />
                                    <span>{move || progress_label.get()}</span>
                                </div>
                                <span>
                                    <i style=move || format!("width:{}%", progress_width.get())></i>
                                </span>
                            </div>
                        </Show>
                    </div>
                    <footer>
                        <span>{move || format!("Version {}", version.get())}</span>
                        <Show
                            when=move || update.get().is_some()
                            fallback=move || view! {
                                <button
                                    type="button"
                                    class="zai-update-dialog__primary"
                                    on:click=move |_| on_close.run(())
                                >
                                    "Continue"
                                </button>
                            }
                        >
                            <button
                                type="button"
                                class="zai-update-dialog__primary"
                                disabled=move || installing.get()
                                on:click=move |_| on_install.run(())
                            >
                                <Icon icon=LuDownload width="14px" height="14px" />
                                {move || if installing.get() {
                                    "Installing…"
                                } else {
                                    "Update & restart"
                                }}
                            </button>
                        </Show>
                    </footer>
                </section>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::current_release_notes;

    #[test]
    fn bundled_release_notes_are_not_empty() {
        assert!(!current_release_notes().trim().is_empty());
    }
}
