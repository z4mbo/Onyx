use crate::model::{
    AccessMode, ApprovalDecision, ContextUsage, InteractionMode, MessageKind, ProviderId,
    ReasoningEffort, SpeedMode,
};
pub use crate::model::{
    ProviderUserInputAnswers, ProviderUserInputOption, ProviderUserInputPrompt,
    ProviderUserInputQuestion,
};
use std::{collections::HashSet, future::Future, path::PathBuf, pin::Pin};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

pub type DriverFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub const MAX_USER_INPUT_QUESTIONS: usize = 8;
pub const MAX_USER_INPUT_OPTIONS: usize = 32;
pub const MAX_USER_INPUT_ANSWERS: usize = 32;
pub const MAX_USER_INPUT_TEXT_BYTES: usize = 4 * 1024;
pub const MAX_USER_INPUT_TOTAL_ANSWER_BYTES: usize = 64 * 1024;
pub const TURN_CANCELLED_ERROR: &str = "Turn cancelled";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderSessionConfig {
    pub provider: ProviderId,
    pub model: Option<String>,
    pub workspace: PathBuf,
    pub continuation: Option<String>,
    pub reasoning: Option<ReasoningEffort>,
    pub speed_mode: SpeedMode,
    pub interaction_mode: InteractionMode,
    pub access_mode: AccessMode,
}

impl ProviderSessionConfig {
    pub fn approval_policy(&self) -> &'static str {
        match self.access_mode {
            AccessMode::ApprovalRequired => "untrusted",
            AccessMode::AutoAcceptEdits => "on-request",
            AccessMode::FullAccess => "never",
        }
    }

    pub fn sandbox_name(&self) -> &'static str {
        match self.access_mode {
            // Match the interactive CLI: workspace writes remain possible, but
            // the provider must ask before an untrusted action. A read-only
            // sandbox here would silently turn "Ask" into "Never allow".
            AccessMode::ApprovalRequired | AccessMode::AutoAcceptEdits => "workspace-write",
            AccessMode::FullAccess => "danger-full-access",
        }
    }
}

impl ProviderSessionConfig {
    pub fn model(&self) -> Option<&str> {
        self.model
            .as_deref()
            .filter(|value| !value.is_empty() && *value != "default")
    }
}

#[derive(Debug)]
pub struct ProviderApproval {
    pub title: String,
    pub detail: String,
    pub risk: String,
    pub responder: oneshot::Sender<ApprovalDecision>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderUserInputResponse {
    Answered(ProviderUserInputAnswers),
    Cancelled,
}

#[derive(Debug)]
pub struct ProviderUserInput {
    pub prompt: ProviderUserInputPrompt,
    pub responder: oneshot::Sender<ProviderUserInputResponse>,
}

impl ProviderUserInputPrompt {
    pub fn new(
        title: impl Into<String>,
        questions: Vec<ProviderUserInputQuestion>,
        auto_resolution_ms: Option<u64>,
    ) -> Result<Self, String> {
        let title = bounded_required(title.into(), "User-input title")?;
        if questions.is_empty() {
            return Err("Provider user-input request did not include any questions".to_string());
        }
        if questions.len() > MAX_USER_INPUT_QUESTIONS {
            return Err(format!(
                "Provider user-input request exceeded the {MAX_USER_INPUT_QUESTIONS}-question limit"
            ));
        }

        let mut ids = HashSet::with_capacity(questions.len());
        let mut normalized = Vec::with_capacity(questions.len());
        for mut question in questions {
            question.id = bounded_required(question.id, "User-input question id")?;
            question.header = bounded_required(question.header, "User-input question header")?;
            question.question = bounded_required(question.question, "User-input question text")?;
            if !ids.insert(question.id.clone()) {
                return Err(format!(
                    "Provider user-input request repeated question id {}",
                    question.id
                ));
            }
            if question.options.len() > MAX_USER_INPUT_OPTIONS {
                return Err(format!(
                    "Provider user-input question exceeded the {MAX_USER_INPUT_OPTIONS}-option limit"
                ));
            }
            for option in &mut question.options {
                option.label =
                    bounded_required(std::mem::take(&mut option.label), "User-input option label")?;
                option.description = bounded_optional(
                    std::mem::take(&mut option.description),
                    "User-input option description",
                )?;
            }
            normalized.push(question);
        }

        Ok(Self {
            title,
            questions: normalized,
            auto_resolution_ms,
        })
    }

    pub fn validate_answers(
        &self,
        mut answers: ProviderUserInputAnswers,
    ) -> Result<ProviderUserInputAnswers, String> {
        if answers.len() != self.questions.len() {
            return Err("Answer every provider question before submitting".to_string());
        }

        let mut total_bytes = 0_usize;
        for question in &self.questions {
            let values = answers
                .get_mut(&question.id)
                .ok_or_else(|| format!("Missing answer for {}", question.header))?;
            if values.is_empty() {
                return Err(format!("Answer {} before submitting", question.header));
            }
            if values.len() > MAX_USER_INPUT_ANSWERS || (!question.multi_select && values.len() > 1)
            {
                return Err(format!(
                    "Too many answers were provided for {}",
                    question.header
                ));
            }

            for answer in values {
                *answer = bounded_required(std::mem::take(answer), "Provider user-input answer")?;
                total_bytes = total_bytes.saturating_add(answer.len());
                if total_bytes > MAX_USER_INPUT_TOTAL_ANSWER_BYTES {
                    return Err(format!(
                        "Provider user-input answers exceeded the {} KiB limit",
                        MAX_USER_INPUT_TOTAL_ANSWER_BYTES / 1024
                    ));
                }
                if !question.options.is_empty()
                    && !question.allow_other
                    && !question
                        .options
                        .iter()
                        .any(|option| option.label == *answer)
                {
                    return Err(format!(
                        "Answer for {} is not one of the provider options",
                        question.header
                    ));
                }
            }
        }

        if answers
            .keys()
            .any(|id| !self.questions.iter().any(|question| question.id == *id))
        {
            return Err("Answers included an unknown provider question".to_string());
        }
        Ok(answers)
    }
}

fn bounded_required(value: String, label: &str) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(format!("{label} is empty"));
    }
    if value.len() > MAX_USER_INPUT_TEXT_BYTES {
        return Err(format!(
            "{label} exceeded the {} KiB limit",
            MAX_USER_INPUT_TEXT_BYTES / 1024
        ));
    }
    Ok(value)
}

fn bounded_optional(value: String, label: &str) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.len() > MAX_USER_INPUT_TEXT_BYTES {
        return Err(format!(
            "{label} exceeded the {} KiB limit",
            MAX_USER_INPUT_TEXT_BYTES / 1024
        ));
    }
    Ok(value)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderActivity {
    pub title: String,
    pub detail: Option<String>,
    pub kind: MessageKind,
}

impl ProviderActivity {
    pub fn tool(title: impl Into<String>, detail: Option<String>) -> Self {
        Self {
            title: title.into(),
            detail,
            kind: MessageKind::Tool,
        }
    }

    pub fn error(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            detail: None,
            kind: MessageKind::Error,
        }
    }
}

#[derive(Debug)]
pub enum ProviderEvent {
    TextDelta(String),
    Text(String),
    Activity(ProviderActivity),
    Continuation(String),
    Approval(ProviderApproval),
    UserInput(ProviderUserInput),
    ContextUsage(ContextUsage),
}

pub trait ProviderSession: Send {
    fn provider(&self) -> ProviderId;

    fn continuation(&self) -> Option<String>;

    /// Whether the live transport can accept additional user messages while a
    /// turn is running (Claude Code stream-json input, Codex `turn/steer`).
    fn supports_steering(&self) -> bool {
        false
    }

    /// Whether the transport remains synchronized and reusable after
    /// `run_turn` returns `TURN_CANCELLED_ERROR`.
    ///
    /// Persistent transports must opt in only after their native interrupt
    /// protocol reaches a terminal turn state. One-shot and process-killing
    /// adapters intentionally use the safe default.
    fn survives_turn_cancellation(&self) -> bool {
        false
    }

    fn run_turn<'a>(
        &'a mut self,
        prompt: &'a str,
        cancellation: &'a CancellationToken,
        events: mpsc::Sender<ProviderEvent>,
        steer: mpsc::UnboundedReceiver<String>,
    ) -> DriverFuture<'a, Result<(), String>>;

    fn shutdown<'a>(&'a mut self) -> DriverFuture<'a, ()>;
}

pub trait ProviderDriver: Send + Sync {
    fn provider(&self) -> ProviderId;

    fn connect<'a>(
        &'a self,
        config: ProviderSessionConfig,
    ) -> DriverFuture<'a, Result<Box<dyn ProviderSession>, String>>;
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_USER_INPUT_QUESTIONS, ProviderUserInputAnswers, ProviderUserInputOption,
        ProviderUserInputPrompt, ProviderUserInputQuestion,
    };

    fn question(id: &str) -> ProviderUserInputQuestion {
        ProviderUserInputQuestion {
            id: id.to_string(),
            header: "Choice".to_string(),
            question: "Which option?".to_string(),
            options: vec![
                ProviderUserInputOption {
                    label: "One".to_string(),
                    description: "First".to_string(),
                },
                ProviderUserInputOption {
                    label: "Two".to_string(),
                    description: "Second".to_string(),
                },
            ],
            multi_select: false,
            allow_other: false,
            secret: false,
        }
    }

    #[test]
    fn validates_and_normalizes_user_input_answers() {
        let prompt =
            ProviderUserInputPrompt::new(" Question ", vec![question("choice")], None).unwrap();
        let answers =
            ProviderUserInputAnswers::from([("choice".to_string(), vec![" Two ".to_string()])]);
        assert_eq!(prompt.validate_answers(answers).unwrap()["choice"], ["Two"]);
    }

    #[test]
    fn rejects_unknown_options_and_duplicate_questions() {
        let prompt =
            ProviderUserInputPrompt::new("Question", vec![question("choice")], None).unwrap();
        let answers =
            ProviderUserInputAnswers::from([("choice".to_string(), vec!["Three".to_string()])]);
        assert!(prompt.validate_answers(answers).is_err());
        assert!(
            ProviderUserInputPrompt::new(
                "Question",
                vec![question("same"), question("same")],
                None
            )
            .is_err()
        );
    }

    #[test]
    fn bounds_question_count() {
        let questions = (0..=MAX_USER_INPUT_QUESTIONS)
            .map(|index| question(&format!("q-{index}")))
            .collect();
        assert!(ProviderUserInputPrompt::new("Question", questions, None).is_err());
    }
}
