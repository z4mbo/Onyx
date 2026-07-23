use crate::model::{
    AccessMode, ContextUsage, InteractionMode, MessageKind, ProviderId, ReasoningEffort, SpeedMode,
};
use std::{future::Future, path::PathBuf, pin::Pin};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

pub type DriverFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

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
            AccessMode::ApprovalRequired => "read-only",
            AccessMode::AutoAcceptEdits => "workspace-write",
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
    pub responder: oneshot::Sender<bool>,
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
    ContextUsage(ContextUsage),
}

pub trait ProviderSession: Send {
    fn provider(&self) -> ProviderId;

    fn continuation(&self) -> Option<String>;

    fn run_turn<'a>(
        &'a mut self,
        prompt: &'a str,
        cancellation: &'a CancellationToken,
        events: mpsc::Sender<ProviderEvent>,
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
