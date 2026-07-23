use super::{
    claude::ClaudeDriver,
    cli::CliDriver,
    codex::CodexDriver,
    driver::{
        ProviderActivity, ProviderApproval, ProviderDriver, ProviderEvent, ProviderSession,
        ProviderSessionConfig,
    },
};
use crate::{
    model::{ApprovalRequest, Message, MessageKind, MessageRole, ProviderId, SessionEvent},
    openrouter::ApprovalRegistry,
};
use chrono::Utc;
use parking_lot::Mutex as ParkingMutex;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, mpsc};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const EVENT_CHANNEL_CAPACITY: usize = 64;
const MAX_TURN_CONTENT: usize = 8 * 1024 * 1024;
const MAX_ACTIVITIES: usize = 256;
const MAX_ACTIVITY_DETAIL: usize = 64 * 1024;
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(600);

pub struct ProviderRunResult {
    pub content: String,
    pub provider_session_id: Option<String>,
    pub activities: Vec<Message>,
    pub context_usage: Option<crate::model::ContextUsage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SessionIdentity {
    provider: ProviderId,
    model: Option<String>,
    workspace: PathBuf,
    reasoning: Option<crate::model::ReasoningEffort>,
    speed_mode: crate::model::SpeedMode,
    interaction_mode: crate::model::InteractionMode,
    access_mode: crate::model::AccessMode,
}

impl From<&ProviderSessionConfig> for SessionIdentity {
    fn from(config: &ProviderSessionConfig) -> Self {
        Self {
            provider: config.provider,
            model: config.model.clone(),
            workspace: config.workspace.clone(),
            reasoning: config.reasoning,
            speed_mode: config.speed_mode,
            interaction_mode: config.interaction_mode,
            access_mode: config.access_mode,
        }
    }
}

struct SessionSlot {
    identity: SessionIdentity,
    session: Mutex<Box<dyn ProviderSession>>,
}

pub struct ProviderRuntime {
    drivers: HashMap<ProviderId, Arc<dyn ProviderDriver>>,
    sessions: ParkingMutex<HashMap<String, Arc<SessionSlot>>>,
}

impl ProviderRuntime {
    pub fn new() -> Self {
        let mut drivers: HashMap<ProviderId, Arc<dyn ProviderDriver>> = HashMap::new();
        drivers.insert(ProviderId::Claude, Arc::new(ClaudeDriver));
        drivers.insert(ProviderId::Codex, Arc::new(CodexDriver));
        drivers.insert(
            ProviderId::Gemini,
            Arc::new(CliDriver::new(ProviderId::Gemini)),
        );
        drivers.insert(ProviderId::Kimi, Arc::new(CliDriver::new(ProviderId::Kimi)));
        debug_assert!(
            drivers
                .iter()
                .all(|(provider, driver)| *provider == driver.provider())
        );
        Self {
            drivers,
            sessions: ParkingMutex::new(HashMap::new()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_turn(
        &self,
        app: AppHandle,
        provider: ProviderId,
        session_id: String,
        provider_session_id: Option<String>,
        model: Option<String>,
        workspace: PathBuf,
        reasoning: Option<crate::model::ReasoningEffort>,
        speed_mode: crate::model::SpeedMode,
        interaction_mode: crate::model::InteractionMode,
        access_mode: crate::model::AccessMode,
        prompt: String,
        message_id: String,
        cancellation: CancellationToken,
        approvals: ApprovalRegistry,
    ) -> Result<ProviderRunResult, String> {
        let config = ProviderSessionConfig {
            provider,
            model,
            workspace,
            continuation: provider_session_id,
            reasoning,
            speed_mode,
            interaction_mode,
            access_mode,
        };
        let slot = self.session(&session_id, config).await?;
        let mut session = slot.session.lock().await;
        if session.provider() != provider {
            return Err("Provider runtime returned the wrong session adapter".to_string());
        }
        let (sender, mut receiver) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let mut accumulator = TurnAccumulator::new(session.continuation());
        let mut turn = session.run_turn(&prompt, &cancellation, sender);
        let mut events_open = true;

        let mut result = loop {
            tokio::select! {
                event = receiver.recv(), if events_open => {
                    match event {
                        Some(event) => {
                            if let Err(error) = handle_event(
                                &app,
                                &session_id,
                                &message_id,
                                &cancellation,
                                &approvals,
                                &mut accumulator,
                                event,
                            ).await {
                                break Err(error);
                            }
                        }
                        None => events_open = false,
                    }
                }
                result = &mut turn => break result,
            }
        };
        drop(turn);

        if result.is_ok() {
            while let Ok(event) = receiver.try_recv() {
                if let Err(error) = handle_event(
                    &app,
                    &session_id,
                    &message_id,
                    &cancellation,
                    &approvals,
                    &mut accumulator,
                    event,
                )
                .await
                {
                    result = Err(error);
                    break;
                }
            }
            if result.is_ok() {
                accumulator.continuation = session.continuation().or(accumulator.continuation);
            }
        }

        if let Err(error) = result {
            cancellation.cancel();
            session.shutdown().await;
            drop(session);
            self.remove_if_same(&session_id, &slot);
            return Err(error);
        }
        if accumulator.content.trim().is_empty() {
            return Err(format!(
                "{} completed without returning a message",
                provider.display_name()
            ));
        }
        Ok(ProviderRunResult {
            content: accumulator.content.trim().to_string(),
            provider_session_id: accumulator.continuation,
            activities: accumulator.activities,
            context_usage: accumulator.context_usage,
        })
    }

    pub async fn remove_session(&self, session_id: &str) {
        let slot = self.sessions.lock().remove(session_id);
        if let Some(slot) = slot {
            slot.session.lock().await.shutdown().await;
        }
    }

    pub async fn shutdown_all(&self) {
        let sessions = self
            .sessions
            .lock()
            .drain()
            .map(|(_, slot)| slot)
            .collect::<Vec<_>>();
        let shutdowns = sessions
            .into_iter()
            .map(|slot| {
                tokio::spawn(async move {
                    slot.session.lock().await.shutdown().await;
                })
            })
            .collect::<Vec<_>>();
        for shutdown in shutdowns {
            let _ = shutdown.await;
        }
    }

    pub fn has_sessions(&self) -> bool {
        !self.sessions.lock().is_empty()
    }

    async fn session(
        &self,
        session_id: &str,
        config: ProviderSessionConfig,
    ) -> Result<Arc<SessionSlot>, String> {
        let identity = SessionIdentity::from(&config);
        let existing = self.sessions.lock().get(session_id).cloned();
        if let Some(slot) = existing {
            if slot.identity == identity {
                return Ok(slot);
            }
            self.remove_session(session_id).await;
        }
        let driver = self
            .drivers
            .get(&config.provider)
            .ok_or_else(|| "No runtime driver exists for this provider".to_string())?;
        let session = driver.connect(config).await?;
        let slot = Arc::new(SessionSlot {
            identity,
            session: Mutex::new(session),
        });
        self.sessions
            .lock()
            .insert(session_id.to_string(), slot.clone());
        Ok(slot)
    }

    fn remove_if_same(&self, session_id: &str, slot: &Arc<SessionSlot>) {
        let mut sessions = self.sessions.lock();
        if sessions
            .get(session_id)
            .is_some_and(|current| Arc::ptr_eq(current, slot))
        {
            sessions.remove(session_id);
        }
    }
}

struct TurnAccumulator {
    content: String,
    continuation: Option<String>,
    activities: Vec<Message>,
    context_usage: Option<crate::model::ContextUsage>,
}

impl TurnAccumulator {
    fn new(continuation: Option<String>) -> Self {
        Self {
            content: String::new(),
            continuation,
            activities: Vec::new(),
            context_usage: None,
        }
    }

    fn append_delta(&mut self, delta: &str) -> Result<(), String> {
        if self.content.len().saturating_add(delta.len()) > MAX_TURN_CONTENT {
            return Err("Provider response exceeded the 8 MiB turn limit".to_string());
        }
        self.content.push_str(delta);
        Ok(())
    }

    fn append_text(&mut self, text: &str) -> Result<String, String> {
        let mut delta = String::new();
        if !self.content.is_empty() && !self.content.ends_with('\n') && !text.starts_with('\n') {
            delta.push('\n');
        }
        delta.push_str(text);
        self.append_delta(&delta)?;
        Ok(delta)
    }

    fn push_activity(&mut self, activity: ProviderActivity) -> Result<Message, String> {
        if self.activities.len() >= MAX_ACTIVITIES {
            return Err("Provider emitted too many activities in one turn".to_string());
        }
        let detail = activity
            .detail
            .as_deref()
            .map(|detail| truncate(detail, MAX_ACTIVITY_DETAIL));
        let content = match detail {
            Some(detail) if !detail.trim().is_empty() => format!("{}\n{detail}", activity.title),
            _ => activity.title,
        };
        let message = Message::new(
            if activity.kind == MessageKind::Tool {
                MessageRole::Tool
            } else {
                MessageRole::System
            },
            activity.kind,
            content,
        );
        self.activities.push(message.clone());
        Ok(message)
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_event(
    app: &AppHandle,
    session_id: &str,
    message_id: &str,
    cancellation: &CancellationToken,
    approvals: &ApprovalRegistry,
    accumulator: &mut TurnAccumulator,
    event: ProviderEvent,
) -> Result<(), String> {
    match event {
        ProviderEvent::TextDelta(delta) => {
            accumulator.append_delta(&delta)?;
            let _ = app.emit(
                "onyx://session",
                SessionEvent::Delta {
                    session_id: session_id.to_string(),
                    message_id: message_id.to_string(),
                    delta,
                },
            );
        }
        ProviderEvent::Text(text) => {
            let delta = accumulator.append_text(&text)?;
            let _ = app.emit(
                "onyx://session",
                SessionEvent::Delta {
                    session_id: session_id.to_string(),
                    message_id: message_id.to_string(),
                    delta,
                },
            );
        }
        ProviderEvent::Activity(activity) => {
            let message = accumulator.push_activity(activity)?;
            let _ = app.emit(
                "onyx://session",
                SessionEvent::Activity {
                    session_id: session_id.to_string(),
                    message,
                },
            );
        }
        ProviderEvent::Continuation(id) => accumulator.continuation = Some(id),
        ProviderEvent::Approval(approval) => {
            relay_approval(app, session_id, cancellation, approvals, approval).await?;
        }
        ProviderEvent::ContextUsage(usage) => {
            accumulator.context_usage = Some(usage.clone());
            let _ = app.emit(
                "onyx://session",
                SessionEvent::ContextUsage {
                    session_id: session_id.to_string(),
                    usage,
                },
            );
        }
    }
    Ok(())
}

async fn relay_approval(
    app: &AppHandle,
    session_id: &str,
    cancellation: &CancellationToken,
    approvals: &ApprovalRegistry,
    approval: ProviderApproval,
) -> Result<(), String> {
    let ProviderApproval {
        title,
        detail,
        risk,
        responder,
    } = approval;
    let id = Uuid::new_v4().to_string();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    approvals.lock().insert(id.clone(), sender);
    let request = ApprovalRequest {
        id: id.clone(),
        session_id: session_id.to_string(),
        title: truncate(&title, 1024),
        detail: truncate(&detail, MAX_ACTIVITY_DETAIL),
        risk: truncate(&risk, 4096),
        created_at: Utc::now(),
    };
    if let Err(error) = app.emit("onyx://approval", request) {
        approvals.lock().remove(&id);
        let _ = responder.send(false);
        return Err(error.to_string());
    }

    let decision = tokio::select! {
        _ = cancellation.cancelled() => Err("Turn cancelled".to_string()),
        result = timeout(APPROVAL_TIMEOUT, receiver) => {
            match result {
                Ok(Ok(allow)) => Ok(allow),
                Ok(Err(_)) => Err("Approval request was closed".to_string()),
                Err(_) => Err("Approval request timed out".to_string()),
            }
        }
    };
    approvals.lock().remove(&id);
    let _ = responder.send(decision.as_ref().copied().unwrap_or(false));
    decision.map(|_| ())
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }
    let mut end = max;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n… output truncated …", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::{MAX_ACTIVITY_DETAIL, MAX_TURN_CONTENT, TurnAccumulator, truncate};
    use crate::{model::MessageKind, providers::driver::ProviderActivity};

    #[test]
    fn text_events_join_without_running_words_together() {
        let mut turn = TurnAccumulator::new(None);
        assert_eq!(turn.append_text("first").unwrap(), "first");
        assert_eq!(turn.append_text("second").unwrap(), "\nsecond");
        assert_eq!(turn.content, "first\nsecond");
    }

    #[test]
    fn turn_content_is_bounded() {
        let mut turn = TurnAccumulator::new(None);
        turn.append_delta(&"x".repeat(MAX_TURN_CONTENT)).unwrap();
        assert!(turn.append_delta("x").is_err());
    }

    #[test]
    fn activity_detail_is_bounded_on_utf8_boundary() {
        let mut turn = TurnAccumulator::new(None);
        let message = turn
            .push_activity(ProviderActivity {
                title: "Tool".to_string(),
                detail: Some("é".repeat(MAX_ACTIVITY_DETAIL)),
                kind: MessageKind::Tool,
            })
            .unwrap();
        assert!(message.content.is_char_boundary(message.content.len()));
        assert!(message.content.contains("output truncated"));
        assert!(truncate("small", 10) == "small");
    }
}
