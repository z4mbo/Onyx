use super::{
    claude::ClaudeDriver,
    cli::CliDriver,
    codex::CodexDriver,
    driver::{
        ProviderActivity, ProviderApproval, ProviderDriver, ProviderEvent, ProviderSession,
        ProviderSessionConfig, ProviderUserInput, ProviderUserInputAnswers,
        ProviderUserInputPrompt, ProviderUserInputResponse, TURN_CANCELLED_ERROR,
    },
};
use crate::{
    model::{
        ApprovalDecision, ApprovalRequest, Message, MessageKind, MessageRole, ProviderId,
        ProviderUserInputRequest, SessionEvent,
    },
    openrouter::ApprovalRegistry,
};
use chrono::Utc;
use parking_lot::Mutex as ParkingMutex;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const EVENT_CHANNEL_CAPACITY: usize = 64;
const MAX_TURN_CONTENT: usize = 8 * 1024 * 1024;
const MAX_ACTIVITIES: usize = 256;
const MAX_ACTIVITY_DETAIL: usize = 64 * 1024;
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(600);
const USER_INPUT_TIMEOUT: Duration = Duration::from_secs(600);

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

struct PendingUserInput {
    session_id: String,
    prompt: ProviderUserInputPrompt,
    responder: oneshot::Sender<ProviderUserInputResponse>,
}

type UserInputRegistry = ParkingMutex<HashMap<String, PendingUserInput>>;

pub struct ProviderRuntime {
    drivers: HashMap<ProviderId, Arc<dyn ProviderDriver>>,
    sessions: ParkingMutex<HashMap<String, Arc<SessionSlot>>>,
    /// Live steering channels for sessions whose transport accepts mid-turn
    /// user messages; entries exist only while a turn is running.
    steerers: ParkingMutex<HashMap<String, mpsc::UnboundedSender<String>>>,
    /// Provider-native structured questions waiting for a GUI response.
    ///
    /// The runtime API is intentionally provider-local for now. The Tauri
    /// command and Leptos card can call `respond_user_input` without exposing a
    /// raw Codex or Claude protocol object to the webview.
    user_inputs: UserInputRegistry,
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
            steerers: ParkingMutex::new(HashMap::new()),
            user_inputs: ParkingMutex::new(HashMap::new()),
        }
    }

    /// Queues a user message into the running turn of this session. Fails when
    /// no turn is active or the provider transport cannot steer mid-turn.
    pub fn steer(&self, session_id: &str, text: String) -> Result<(), String> {
        let sender = self
            .steerers
            .lock()
            .get(session_id)
            .cloned()
            .ok_or_else(|| {
                "This session has no running turn that accepts steering messages".to_string()
            })?;
        sender
            .send(text)
            .map_err(|_| "The running turn stopped accepting steering messages".to_string())
    }

    /// Resolves a structured question previously emitted on
    /// `onyx://user-input` after the Tauri boundary validates its answer map.
    pub fn respond_user_input(
        &self,
        request_id: &str,
        answers: ProviderUserInputAnswers,
    ) -> Result<(), String> {
        let mut pending = self.user_inputs.lock();
        let request = pending
            .get(request_id)
            .ok_or_else(|| "Provider user-input request was not found".to_string())?;
        let answers = request.prompt.validate_answers(answers)?;
        let request = pending
            .remove(request_id)
            .ok_or_else(|| "Provider user-input request was already resolved".to_string())?;
        request
            .responder
            .send(ProviderUserInputResponse::Answered(answers))
            .map_err(|_| "Provider stopped waiting for user input".to_string())
    }

    pub fn cancel_user_input(&self, request_id: &str) -> Result<(), String> {
        let request = self
            .user_inputs
            .lock()
            .remove(request_id)
            .ok_or_else(|| "Provider user-input request was not found".to_string())?;
        request
            .responder
            .send(ProviderUserInputResponse::Cancelled)
            .map_err(|_| "Provider stopped waiting for user input".to_string())
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
        let slot = self.session(&session_id, config, &cancellation).await?;
        let mut session = slot.session.lock().await;
        if cancellation.is_cancelled() {
            drop(session);
            self.remove_session(&session_id).await;
            return Err(TURN_CANCELLED_ERROR.to_string());
        }
        if session.provider() != provider {
            return Err("Provider runtime returned the wrong session adapter".to_string());
        }
        let (sender, mut receiver) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let (steer_sender, steer_receiver) = mpsc::unbounded_channel();
        if session.supports_steering() {
            self.steerers
                .lock()
                .insert(session_id.clone(), steer_sender);
        }
        let mut accumulator = TurnAccumulator::new(session.continuation());
        let mut turn = session.run_turn(&prompt, &cancellation, sender, steer_receiver);
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
                                &self.user_inputs,
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
        self.steerers.lock().remove(&session_id);

        if result.is_ok() {
            while let Ok(event) = receiver.try_recv() {
                if let Err(error) = handle_event(
                    &app,
                    &session_id,
                    &message_id,
                    &cancellation,
                    &approvals,
                    &self.user_inputs,
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
            cancel_user_inputs_for_session(&self.user_inputs, &session_id);
            if !should_preserve_session_after_error(session.as_ref(), &error) {
                session.shutdown().await;
                drop(session);
                self.remove_if_same(&session_id, &slot);
            }
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
        self.steerers.lock().remove(session_id);
        cancel_user_inputs_for_session(&self.user_inputs, session_id);
        let slot = self.sessions.lock().remove(session_id);
        if let Some(slot) = slot {
            slot.session.lock().await.shutdown().await;
        }
    }

    pub async fn shutdown_all(&self) {
        cancel_all_user_inputs(&self.user_inputs);
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
        cancellation: &CancellationToken,
    ) -> Result<Arc<SessionSlot>, String> {
        if cancellation.is_cancelled() {
            self.remove_session(session_id).await;
            return Err(TURN_CANCELLED_ERROR.to_string());
        }
        let identity = SessionIdentity::from(&config);
        let existing = self.sessions.lock().get(session_id).cloned();
        if let Some(slot) = existing {
            if slot.identity == identity {
                if cancellation.is_cancelled() {
                    self.remove_session(session_id).await;
                    return Err(TURN_CANCELLED_ERROR.to_string());
                }
                return Ok(slot);
            }
            self.remove_session(session_id).await;
            if cancellation.is_cancelled() {
                return Err(TURN_CANCELLED_ERROR.to_string());
            }
        }
        let driver = self
            .drivers
            .get(&config.provider)
            .ok_or_else(|| "No runtime driver exists for this provider".to_string())?;
        let mut session = driver.connect(config).await?;
        if cancellation.is_cancelled() {
            session.shutdown().await;
            return Err(TURN_CANCELLED_ERROR.to_string());
        }
        let slot = Arc::new(SessionSlot {
            identity,
            session: Mutex::new(session),
        });
        self.sessions
            .lock()
            .insert(session_id.to_string(), slot.clone());
        if cancellation.is_cancelled() {
            self.remove_session(session_id).await;
            return Err(TURN_CANCELLED_ERROR.to_string());
        }
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
        let activity_id = activity.id;
        let detail = activity
            .detail
            .as_deref()
            .map(|detail| truncate(detail, MAX_ACTIVITY_DETAIL));
        let content = match detail {
            Some(detail) if !detail.trim().is_empty() => format!("{}\n{detail}", activity.title),
            _ => activity.title,
        };
        let mut message = Message::new(
            if activity.kind == MessageKind::Tool {
                MessageRole::Tool
            } else {
                MessageRole::System
            },
            activity.kind,
            content,
        );
        if let Some(id) = activity_id {
            message.id = id;
        }
        if let Some(existing) = self
            .activities
            .iter_mut()
            .find(|existing| existing.id == message.id)
        {
            message.created_at = existing.created_at;
            *existing = message.clone();
            return Ok(message);
        }
        if self.activities.len() >= MAX_ACTIVITIES {
            return Err("Provider emitted too many activities in one turn".to_string());
        }
        self.activities.push(message.clone());
        Ok(message)
    }

    fn append_activity_delta(&mut self, id: &str, delta: &str) -> Option<String> {
        let message = self
            .activities
            .iter_mut()
            .find(|message| message.id == id)?;
        let prefix = if message.content.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        let remaining = MAX_ACTIVITY_DETAIL.saturating_sub(message.content.len());
        if remaining == 0 {
            return Some(String::new());
        }
        let addition = format!("{prefix}{delta}");
        let mut end = addition.len().min(remaining);
        while end > 0 && !addition.is_char_boundary(end) {
            end -= 1;
        }
        let applied = addition[..end].to_owned();
        message.content.push_str(&applied);
        Some(applied)
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_event(
    app: &AppHandle,
    session_id: &str,
    message_id: &str,
    cancellation: &CancellationToken,
    approvals: &ApprovalRegistry,
    user_inputs: &UserInputRegistry,
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
        ProviderEvent::ActivityDelta { id, delta } => {
            if let Some(delta) = accumulator.append_activity_delta(&id, &delta)
                && !delta.is_empty()
            {
                let _ = app.emit(
                    "onyx://session",
                    SessionEvent::ActivityDelta {
                        session_id: session_id.to_string(),
                        message_id: id,
                        delta,
                    },
                );
            }
        }
        ProviderEvent::Continuation(id) => accumulator.continuation = Some(id),
        ProviderEvent::Approval(approval) => {
            relay_approval(app, session_id, cancellation, approvals, approval).await?;
        }
        ProviderEvent::UserInput(request) => {
            relay_user_input(app, session_id, cancellation, user_inputs, request).await?;
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
        let _ = responder.send(ApprovalDecision::Deny);
        return Err(error.to_string());
    }

    let decision = wait_for_approval_decision(cancellation, receiver).await;
    approvals.lock().remove(&id);
    let _ = responder.send(*decision.as_ref().unwrap_or(&ApprovalDecision::Deny));
    decision.map(|_| ())
}

async fn relay_user_input(
    app: &AppHandle,
    session_id: &str,
    cancellation: &CancellationToken,
    user_inputs: &UserInputRegistry,
    request: ProviderUserInput,
) -> Result<(), String> {
    let ProviderUserInput { prompt, responder } = request;
    let id = Uuid::new_v4().to_string();
    let (sender, receiver) = oneshot::channel();
    user_inputs.lock().insert(
        id.clone(),
        PendingUserInput {
            session_id: session_id.to_string(),
            prompt: prompt.clone(),
            responder: sender,
        },
    );
    let event = ProviderUserInputRequest {
        id: id.clone(),
        session_id: session_id.to_string(),
        title: prompt.title,
        questions: prompt.questions,
        auto_resolution_ms: prompt.auto_resolution_ms,
        created_at: Utc::now(),
    };
    if let Err(error) = app.emit("onyx://user-input", event) {
        user_inputs.lock().remove(&id);
        let _ = responder.send(ProviderUserInputResponse::Cancelled);
        return Err(error.to_string());
    }

    let response = wait_for_user_input_response(cancellation, receiver).await;
    user_inputs.lock().remove(&id);
    match response {
        Ok(response) => responder
            .send(response)
            .map_err(|_| "Provider stopped waiting for user input".to_string()),
        Err(error) => {
            let _ = responder.send(ProviderUserInputResponse::Cancelled);
            Err(error)
        }
    }
}

async fn wait_for_approval_decision(
    cancellation: &CancellationToken,
    receiver: oneshot::Receiver<ApprovalDecision>,
) -> Result<ApprovalDecision, String> {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Ok(ApprovalDecision::Deny),
        result = timeout(APPROVAL_TIMEOUT, receiver) => {
            match result {
                Ok(Ok(decision)) => Ok(decision),
                Ok(Err(_)) => Err("Approval request was closed".to_string()),
                Err(_) => Err("Approval request timed out".to_string()),
            }
        }
    }
}

async fn wait_for_user_input_response(
    cancellation: &CancellationToken,
    receiver: oneshot::Receiver<ProviderUserInputResponse>,
) -> Result<ProviderUserInputResponse, String> {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Ok(ProviderUserInputResponse::Cancelled),
        result = timeout(USER_INPUT_TIMEOUT, receiver) => {
            match result {
                Ok(Ok(response)) => Ok(response),
                Ok(Err(_)) => Err("Provider user-input request was closed".to_string()),
                Err(_) => Err("Provider user-input request timed out".to_string()),
            }
        }
    }
}

fn cancel_user_inputs_for_session(user_inputs: &UserInputRegistry, session_id: &str) {
    let mut pending = user_inputs.lock();
    let ids = pending
        .iter()
        .filter(|(_, request)| request.session_id == session_id)
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    for id in ids {
        if let Some(request) = pending.remove(&id) {
            let _ = request.responder.send(ProviderUserInputResponse::Cancelled);
        }
    }
}

fn cancel_all_user_inputs(user_inputs: &UserInputRegistry) {
    for (_, request) in user_inputs.lock().drain() {
        let _ = request.responder.send(ProviderUserInputResponse::Cancelled);
    }
}

fn should_preserve_session_after_error(session: &dyn ProviderSession, error: &str) -> bool {
    error == TURN_CANCELLED_ERROR && session.survives_turn_cancellation()
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
    use super::{
        MAX_ACTIVITY_DETAIL, MAX_TURN_CONTENT, PendingUserInput, ProviderRuntime, TurnAccumulator,
        should_preserve_session_after_error, truncate, wait_for_approval_decision,
        wait_for_user_input_response,
    };
    use crate::{
        model::{
            AccessMode, ApprovalDecision, InteractionMode, MessageKind, ProviderId, SpeedMode,
        },
        providers::driver::{
            DriverFuture, ProviderActivity, ProviderDriver, ProviderEvent, ProviderSession,
            ProviderSessionConfig, ProviderUserInputAnswers, ProviderUserInputPrompt,
            ProviderUserInputQuestion, ProviderUserInputResponse, TURN_CANCELLED_ERROR,
        },
    };
    use parking_lot::Mutex as ParkingMutex;
    use std::{
        collections::HashMap,
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    use tokio::{
        sync::{Notify, mpsc, oneshot},
        time::timeout,
    };
    use tokio_util::sync::CancellationToken;

    struct CancellationPolicySession {
        reusable: bool,
    }

    impl ProviderSession for CancellationPolicySession {
        fn provider(&self) -> ProviderId {
            ProviderId::Codex
        }

        fn continuation(&self) -> Option<String> {
            None
        }

        fn survives_turn_cancellation(&self) -> bool {
            self.reusable
        }

        fn run_turn<'a>(
            &'a mut self,
            _prompt: &'a str,
            _cancellation: &'a CancellationToken,
            _events: mpsc::Sender<ProviderEvent>,
            _steer: mpsc::UnboundedReceiver<String>,
        ) -> DriverFuture<'a, Result<(), String>> {
            Box::pin(async { unreachable!("policy-only test session") })
        }

        fn shutdown<'a>(&'a mut self) -> DriverFuture<'a, ()> {
            Box::pin(async {})
        }
    }

    struct ControlledDriver {
        connections: AtomicUsize,
        shutdowns: Arc<AtomicUsize>,
        started: Notify,
        release: Notify,
    }

    impl ControlledDriver {
        fn new() -> Self {
            Self {
                connections: AtomicUsize::new(0),
                shutdowns: Arc::new(AtomicUsize::new(0)),
                started: Notify::new(),
                release: Notify::new(),
            }
        }
    }

    impl ProviderDriver for ControlledDriver {
        fn provider(&self) -> ProviderId {
            ProviderId::Codex
        }

        fn connect<'a>(
            &'a self,
            _config: ProviderSessionConfig,
        ) -> DriverFuture<'a, Result<Box<dyn ProviderSession>, String>> {
            self.connections.fetch_add(1, Ordering::SeqCst);
            self.started.notify_one();
            Box::pin(async move {
                self.release.notified().await;
                Ok(Box::new(ControlledSession {
                    shutdowns: self.shutdowns.clone(),
                }) as Box<dyn ProviderSession>)
            })
        }
    }

    struct ControlledSession {
        shutdowns: Arc<AtomicUsize>,
    }

    impl ProviderSession for ControlledSession {
        fn provider(&self) -> ProviderId {
            ProviderId::Codex
        }

        fn continuation(&self) -> Option<String> {
            None
        }

        fn run_turn<'a>(
            &'a mut self,
            _prompt: &'a str,
            _cancellation: &'a CancellationToken,
            _events: mpsc::Sender<ProviderEvent>,
            _steer: mpsc::UnboundedReceiver<String>,
        ) -> DriverFuture<'a, Result<(), String>> {
            Box::pin(async { unreachable!("acquisition-only test session") })
        }

        fn shutdown<'a>(&'a mut self) -> DriverFuture<'a, ()> {
            Box::pin(async move {
                self.shutdowns.fetch_add(1, Ordering::SeqCst);
            })
        }
    }

    fn controlled_runtime(driver: Arc<ControlledDriver>) -> ProviderRuntime {
        let driver: Arc<dyn ProviderDriver> = driver;
        ProviderRuntime {
            drivers: HashMap::from([(ProviderId::Codex, driver)]),
            sessions: ParkingMutex::new(HashMap::new()),
            steerers: ParkingMutex::new(HashMap::new()),
            user_inputs: ParkingMutex::new(HashMap::new()),
        }
    }

    fn test_config() -> ProviderSessionConfig {
        ProviderSessionConfig {
            provider: ProviderId::Codex,
            model: None,
            workspace: PathBuf::from("."),
            continuation: None,
            reasoning: None,
            speed_mode: SpeedMode::Standard,
            interaction_mode: InteractionMode::Build,
            access_mode: AccessMode::ApprovalRequired,
        }
    }

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
                id: None,
                title: "Tool".to_string(),
                detail: Some("é".repeat(MAX_ACTIVITY_DETAIL)),
                kind: MessageKind::Tool,
            })
            .unwrap();
        assert!(message.content.is_char_boundary(message.content.len()));
        assert!(message.content.contains("output truncated"));
        assert!(truncate("small", 10) == "small");
    }

    #[test]
    fn stable_activity_is_updated_and_stream_output_is_bounded() {
        let mut turn = TurnAccumulator::new(None);
        turn.push_activity(ProviderActivity {
            id: Some("command-1".to_owned()),
            title: "Running cargo test".to_owned(),
            detail: None,
            kind: MessageKind::Tool,
        })
        .unwrap();
        assert_eq!(
            turn.append_activity_delta("command-1", "first line")
                .as_deref(),
            Some("\nfirst line")
        );
        turn.push_activity(ProviderActivity {
            id: Some("command-1".to_owned()),
            title: "Ran cargo test".to_owned(),
            detail: Some("ok".to_owned()),
            kind: MessageKind::Tool,
        })
        .unwrap();
        assert_eq!(turn.activities.len(), 1);
        assert_eq!(turn.activities[0].content, "Ran cargo test\nok");
    }

    #[test]
    fn only_native_safe_cancellations_preserve_a_provider_session() {
        let reusable = CancellationPolicySession { reusable: true };
        let disposable = CancellationPolicySession { reusable: false };
        assert!(should_preserve_session_after_error(
            &reusable,
            "Turn cancelled"
        ));
        assert!(!should_preserve_session_after_error(
            &disposable,
            "Turn cancelled"
        ));
        assert!(!should_preserve_session_after_error(
            &reusable,
            "transport failed"
        ));
    }

    #[tokio::test]
    async fn pre_cancelled_acquisition_never_connects_a_provider() {
        let driver = Arc::new(ControlledDriver::new());
        let runtime = controlled_runtime(driver.clone());
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let result = timeout(
            Duration::from_secs(1),
            runtime.session("deleted", test_config(), &cancellation),
        )
        .await
        .expect("pre-cancelled acquisition must not hang");

        match result {
            Err(error) => assert_eq!(error, TURN_CANCELLED_ERROR),
            Ok(_) => panic!("pre-cancelled acquisition unexpectedly created a session"),
        }
        assert_eq!(driver.connections.load(Ordering::SeqCst), 0);
        assert!(!runtime.has_sessions());
    }

    #[tokio::test]
    async fn cancellation_during_connect_shuts_down_without_registering_a_session() {
        let driver = Arc::new(ControlledDriver::new());
        let runtime = Arc::new(controlled_runtime(driver.clone()));
        let cancellation = CancellationToken::new();
        let acquisition = tokio::spawn({
            let runtime = runtime.clone();
            let cancellation = cancellation.clone();
            async move {
                runtime
                    .session("deleted", test_config(), &cancellation)
                    .await
                    .map(|_| ())
            }
        });

        timeout(Duration::from_secs(1), driver.started.notified())
            .await
            .expect("provider connection did not start");
        cancellation.cancel();
        driver.release.notify_one();

        let result = timeout(Duration::from_secs(1), acquisition)
            .await
            .expect("cancelled acquisition did not finish")
            .expect("cancelled acquisition task panicked");
        assert_eq!(result, Err(TURN_CANCELLED_ERROR.to_string()));
        assert_eq!(driver.connections.load(Ordering::SeqCst), 1);
        assert_eq!(driver.shutdowns.load(Ordering::SeqCst), 1);
        assert!(!runtime.has_sessions());
    }

    #[tokio::test]
    async fn cancellation_immediately_resolves_pending_provider_prompts_safely() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let (_approval_sender, approval_receiver) = oneshot::channel();
        assert_eq!(
            timeout(
                Duration::from_millis(100),
                wait_for_approval_decision(&cancellation, approval_receiver)
            )
            .await
            .expect("approval cancellation must not hang")
            .unwrap(),
            ApprovalDecision::Deny
        );

        let (_input_sender, input_receiver) = oneshot::channel();
        assert_eq!(
            timeout(
                Duration::from_millis(100),
                wait_for_user_input_response(&cancellation, input_receiver)
            )
            .await
            .expect("user-input cancellation must not hang")
            .unwrap(),
            ProviderUserInputResponse::Cancelled
        );
    }

    #[tokio::test]
    async fn pending_user_input_validates_before_resolving() {
        let runtime = ProviderRuntime::new();
        let prompt = ProviderUserInputPrompt::new(
            "Question",
            vec![ProviderUserInputQuestion {
                id: "scope".to_string(),
                header: "Scope".to_string(),
                question: "Which scope?".to_string(),
                options: Vec::new(),
                multi_select: false,
                allow_other: true,
                secret: false,
            }],
            None,
        )
        .unwrap();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        runtime.user_inputs.lock().insert(
            "request-1".to_string(),
            PendingUserInput {
                session_id: "session-1".to_string(),
                prompt,
                responder: sender,
            },
        );

        assert!(
            runtime
                .respond_user_input("request-1", ProviderUserInputAnswers::new())
                .is_err()
        );
        runtime
            .respond_user_input(
                "request-1",
                ProviderUserInputAnswers::from([(
                    "scope".to_string(),
                    vec!["Focused".to_string()],
                )]),
            )
            .unwrap();
        assert!(matches!(
            receiver.await.unwrap(),
            ProviderUserInputResponse::Answered(answers)
                if answers["scope"] == ["Focused"]
        ));
    }

    #[tokio::test]
    async fn pending_user_input_can_be_cancelled() {
        let runtime = ProviderRuntime::new();
        let prompt = ProviderUserInputPrompt::new(
            "Question",
            vec![ProviderUserInputQuestion {
                id: "scope".to_string(),
                header: "Scope".to_string(),
                question: "Which scope?".to_string(),
                options: Vec::new(),
                multi_select: false,
                allow_other: true,
                secret: false,
            }],
            None,
        )
        .unwrap();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        runtime.user_inputs.lock().insert(
            "request-2".to_string(),
            PendingUserInput {
                session_id: "session-1".to_string(),
                prompt,
                responder: sender,
            },
        );
        runtime.cancel_user_input("request-2").unwrap();
        assert_eq!(
            receiver.await.unwrap(),
            ProviderUserInputResponse::Cancelled
        );
    }
}
