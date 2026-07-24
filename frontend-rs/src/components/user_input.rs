use std::collections::BTreeSet;

use leptos::{ev::SubmitEvent, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{
    bridge,
    model::{ProviderUserInputAnswers, ProviderUserInputQuestion, ProviderUserInputRequest},
};

const MAX_ANSWER_BYTES: usize = 4 * 1024;

fn dom_fragment(value: &str) -> String {
    let fragment = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .collect::<String>();
    if fragment.is_empty() {
        "question".to_owned()
    } else {
        fragment
    }
}

fn normalized_values(selected: Vec<String>, other_active: bool, other_text: String) -> Vec<String> {
    let mut seen = BTreeSet::new();
    selected
        .into_iter()
        .chain(
            other_active
                .then(|| other_text.trim().to_owned())
                .filter(|value| !value.is_empty()),
        )
        .filter(|value| !value.trim().is_empty())
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn write_answer(
    answers: RwSignal<ProviderUserInputAnswers>,
    question_id: &str,
    selected: RwSignal<Vec<String>>,
    other_active: RwSignal<bool>,
    other_text: RwSignal<String>,
) {
    let values = normalized_values(
        selected.get_untracked(),
        other_active.get_untracked(),
        other_text.get_untracked(),
    );
    answers.update(|answers| {
        answers.insert(question_id.to_owned(), values);
    });
}

fn normalized_answers(answers: &ProviderUserInputAnswers) -> ProviderUserInputAnswers {
    answers
        .iter()
        .map(|(question_id, values)| {
            let mut seen = BTreeSet::new();
            let values = values
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .filter(|value| seen.insert((*value).to_owned()))
                .map(str::to_owned)
                .collect();
            (question_id.clone(), values)
        })
        .collect()
}

fn answers_are_complete(
    questions: &[ProviderUserInputQuestion],
    answers: &ProviderUserInputAnswers,
) -> bool {
    questions.iter().all(|question| {
        answers.get(&question.id).is_some_and(|values| {
            !values.is_empty()
                && (question.multi_select || values.len() == 1)
                && values.iter().all(|value| !value.trim().is_empty())
        })
    })
}

#[component]
fn UserInputQuestion(
    question: ProviderUserInputQuestion,
    request_fragment: String,
    answers: RwSignal<ProviderUserInputAnswers>,
    disabled: Signal<bool>,
) -> impl IntoView {
    let question_id = question.id.clone();
    let question_fragment = dom_fragment(&question.id);
    let field_id = format!("onyx-user-input-{request_fragment}-{question_fragment}");
    let description_id = format!("{field_id}-description");
    let selected = RwSignal::new(Vec::<String>::new());
    let other_active = RwSignal::new(false);
    let other_text = RwSignal::new(String::new());

    answers.update(|answers| {
        answers.entry(question.id.clone()).or_default();
    });

    if question.options.is_empty() {
        let input_type = if question.secret { "password" } else { "text" };
        let answer_id = question_id.clone();
        return view! {
            <fieldset class="zai-user-input__question" disabled=move || disabled.get()>
                <legend>
                    <span>{question.header}</span>
                    <strong>{question.question}</strong>
                </legend>
                <input
                    id=field_id
                    class="zai-user-input__free-text"
                    type=input_type
                    autocomplete=if question.secret { "new-password" } else { "off" }
                    maxlength=MAX_ANSWER_BYTES
                    aria-describedby=description_id.clone()
                    prop:value=move || other_text.get()
                    on:input=move |event| {
                        other_text.set(event_target_value(&event));
                        other_active.set(true);
                        write_answer(
                            answers,
                            &answer_id,
                            selected,
                            other_active,
                            other_text,
                        );
                    }
                />
                <small id=description_id class="zai-user-input__hint">
                    {if question.secret {
                        "The value stays hidden while you type."
                    } else {
                        "Enter a response to continue."
                    }}
                </small>
            </fieldset>
        }
        .into_any();
    }

    let multi_select = question.multi_select;
    let allow_other = question.allow_other;
    let options = question.options;
    let option_name = format!("{field_id}-choice");
    let options_name = option_name.clone();
    let other_id = format!("{field_id}-other");
    let other_text_id = format!("{field_id}-other-text");
    let other_answer_id = question_id.clone();

    view! {
        <fieldset class="zai-user-input__question" disabled=move || disabled.get()>
            <legend>
                <span>{question.header}</span>
                <strong>{question.question}</strong>
                <small>
                    {if multi_select {
                        "Choose one or more options."
                    } else {
                        "Choose one option."
                    }}
                </small>
            </legend>
            <div class="zai-user-input__options">
                <For
                    each=move || options.clone()
                    key=|option| option.label.clone()
                    children=move |option| {
                        let option_id =
                            format!("{field_id}-option-{}", dom_fragment(&option.label));
                        let option_description_id = format!("{option_id}-description");
                        let checked_label = option.label.clone();
                        let input_checked_label = option.label.clone();
                        let changed_label = option.label.clone();
                        let display_label = option.label;
                        let changed_question_id = question_id.clone();
                        let input_id = option_id.clone();
                        view! {
                            <label
                                class="zai-user-input__option"
                                class:zai-user-input__option--selected=move || {
                                    selected.read().contains(&checked_label)
                                }
                                for=option_id
                            >
                                <input
                                    id=input_id
                                    name=options_name.clone()
                                    type=if multi_select { "checkbox" } else { "radio" }
                                    aria-describedby=option_description_id.clone()
                                    prop:checked=move || {
                                        selected.read().contains(&input_checked_label)
                                    }
                                    on:change=move |event| {
                                        let checked = event_target_checked(&event);
                                        selected.update(|values| {
                                            if multi_select {
                                                if checked
                                                    && !values.iter().any(|value| value == &changed_label)
                                                {
                                                    values.push(changed_label.clone());
                                                } else if !checked {
                                                    values.retain(|value| value != &changed_label);
                                                }
                                            } else if checked {
                                                values.clear();
                                                values.push(changed_label.clone());
                                            }
                                        });
                                        if !multi_select && checked {
                                            other_active.set(false);
                                        }
                                        write_answer(
                                            answers,
                                            &changed_question_id,
                                            selected,
                                            other_active,
                                            other_text,
                                        );
                                    }
                                />
                                <span>
                                    <strong>{display_label}</strong>
                                    <small id=option_description_id>{option.description}</small>
                                </span>
                            </label>
                        }
                    }
                />
                {allow_other.then(move || {
                    let toggle_question_id = other_answer_id.clone();
                    let other_input_id = other_id.clone();
                    view! {
                        <div
                            class="zai-user-input__option zai-user-input__option--other"
                            class:zai-user-input__option--selected=move || other_active.get()
                        >
                            <label for=other_id>
                                <input
                                    id=other_input_id
                                    name=option_name.clone()
                                    type=if multi_select { "checkbox" } else { "radio" }
                                    prop:checked=move || other_active.get()
                                    on:change=move |event| {
                                        let checked = event_target_checked(&event);
                                        other_active.set(checked);
                                        if !multi_select && checked {
                                            selected.set(Vec::new());
                                        }
                                        write_answer(
                                            answers,
                                            &toggle_question_id,
                                            selected,
                                            other_active,
                                            other_text,
                                        );
                                    }
                                />
                                <span><strong>"Other"</strong></span>
                            </label>
                            <input
                                id=other_text_id
                                class="zai-user-input__other-text"
                                type=if question.secret { "password" } else { "text" }
                                autocomplete=if question.secret { "new-password" } else { "off" }
                                maxlength=MAX_ANSWER_BYTES
                                placeholder="Type another answer"
                                aria-label="Other answer"
                                disabled=move || disabled.get() || !other_active.get()
                                prop:value=move || other_text.get()
                                on:input=move |event| {
                                    other_text.set(event_target_value(&event));
                                    write_answer(
                                        answers,
                                        &other_answer_id,
                                        selected,
                                        other_active,
                                        other_text,
                                    );
                                }
                            />
                        </div>
                    }
                })}
            </div>
        </fieldset>
    }
    .into_any()
}

/// Renders a provider-neutral prompt emitted by a CLI transport.
///
/// The card owns only ephemeral form state. Answers are sent to the native
/// runtime and are never persisted by this component.
#[component]
pub fn UserInputCard(
    request: ProviderUserInputRequest,
    disabled: Signal<bool>,
    on_resolved: Callback<()>,
    on_error: Callback<String>,
) -> impl IntoView {
    let answers = RwSignal::new(ProviderUserInputAnswers::new());
    let responding = RwSignal::new(false);
    let inline_error = RwSignal::new(None::<String>);
    let request_id = StoredValue::new(request.id.clone());
    let questions = StoredValue::new(request.questions.clone());
    let card_id = format!("onyx-user-input-{}", dom_fragment(&request.id));
    let title_id = format!("{card_id}-title");
    let heading_id = title_id.clone();
    let request_fragment = dom_fragment(&request.id);
    let effective_disabled = Signal::derive(move || disabled.get() || responding.get());
    let can_submit = Signal::derive(move || {
        !effective_disabled.get() && answers_are_complete(&questions.get_value(), &answers.get())
    });

    let submit = Callback::new(move |_: ()| {
        if !can_submit.get_untracked() {
            return;
        }
        responding.set(true);
        inline_error.set(None);
        let request_id = request_id.get_value();
        let submitted_answers = normalized_answers(&answers.get_untracked());
        spawn_local(async move {
            match bridge::respond_user_input(&request_id, &submitted_answers).await {
                Ok(()) => on_resolved.run(()),
                Err(error) => {
                    responding.set(false);
                    inline_error.set(Some(error.clone()));
                    on_error.run(error);
                }
            }
        });
    });

    let cancel = Callback::new(move |_: ()| {
        if effective_disabled.get_untracked() {
            return;
        }
        responding.set(true);
        inline_error.set(None);
        let request_id = request_id.get_value();
        spawn_local(async move {
            match bridge::cancel_user_input(&request_id).await {
                Ok(()) => on_resolved.run(()),
                Err(error) => {
                    responding.set(false);
                    inline_error.set(Some(error.clone()));
                    on_error.run(error);
                }
            }
        });
    });

    view! {
        <section
            id=card_id
            class="zai-user-input"
            role="region"
            aria-labelledby=title_id
            aria-busy=move || responding.get()
        >
            <header class="zai-user-input__header">
                <span class="zai-user-input__eyebrow">"AGENT INPUT"</span>
                <h3 id=heading_id>{request.title}</h3>
                {request.auto_resolution_ms.map(|milliseconds| view! {
                    <small>
                        {format!(
                            "This request can resolve automatically after {} seconds.",
                            milliseconds.div_ceil(1_000)
                        )}
                    </small>
                })}
            </header>
            <form
                on:submit=move |event: SubmitEvent| {
                    event.prevent_default();
                    submit.run(());
                }
            >
                <div class="zai-user-input__questions">
                    <For
                        each=move || request.questions.clone()
                        key=|question| question.id.clone()
                        children=move |question| view! {
                            <UserInputQuestion
                                question
                                request_fragment=request_fragment.clone()
                                answers
                                disabled=effective_disabled
                            />
                        }
                    />
                </div>
                <Show when=move || inline_error.get().is_some()>
                    <p class="zai-user-input__error" role="alert">
                        {move || inline_error.get().unwrap_or_default()}
                    </p>
                </Show>
                <footer class="zai-user-input__actions">
                    <button
                        type="button"
                        class="zai-user-input__cancel"
                        disabled=move || effective_disabled.get()
                        on:click=move |_| cancel.run(())
                    >
                        "Cancel"
                    </button>
                    <button
                        type="submit"
                        class="zai-user-input__submit"
                        disabled=move || !can_submit.get()
                    >
                        {move || if responding.get() { "Sending…" } else { "Continue" }}
                    </button>
                </footer>
            </form>
        </section>
    }
}
