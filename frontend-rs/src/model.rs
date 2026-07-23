#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    Claude,
    Codex,
    Gemini,
    Kimi,
    #[default]
    Openrouter,
}

impl ProviderId {
    pub const ALL: [Self; 5] = [
        Self::Claude,
        Self::Codex,
        Self::Gemini,
        Self::Kimi,
        Self::Openrouter,
    ];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini CLI",
            Self::Kimi => "Kimi Code",
            Self::Openrouter => "OpenRouter",
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Kimi => "kimi",
            Self::Openrouter => "openrouter",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "gemini" => Some(Self::Gemini),
            "kimi" => Some(Self::Kimi),
            "openrouter" => Some(Self::Openrouter),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProviderBrand {
    Openai,
    Anthropic,
    Google,
    Xai,
    Moonshot,
    #[default]
    Openrouter,
}

impl ProviderBrand {
    pub const fn for_provider(provider: ProviderId) -> Self {
        match provider {
            ProviderId::Claude => Self::Anthropic,
            ProviderId::Codex => Self::Openai,
            ProviderId::Gemini => Self::Google,
            ProviderId::Kimi => Self::Moonshot,
            ProviderId::Openrouter => Self::Openrouter,
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Openai => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Google => "Google",
            Self::Xai => "xAI",
            Self::Moonshot => "Moonshot",
            Self::Openrouter => "OpenRouter",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    Xhigh,
    Max,
    Ultracode,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SpeedMode {
    #[default]
    Standard,
    Fast,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    #[default]
    Build,
    Plan,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    #[default]
    ApprovalRequired,
    AutoAcceptEdits,
    FullAccess,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Idle,
    Running,
    WaitingApproval,
    Failed,
}

impl SessionStatus {
    pub const fn is_running(self) -> bool {
        matches!(self, Self::Running | Self::WaitingApproval)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

impl MessageRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
            Self::Tool => "tool",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Text,
    Reasoning,
    Tool,
    Error,
}

impl MessageKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Reasoning => "reasoning",
            Self::Tool => "tool",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub id: ProviderId,
    pub name: String,
    pub available: bool,
    pub executable_path: Option<String>,
    pub version: Option<String>,
    pub install_url: String,
    pub transport: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub role: MessageRole,
    pub kind: MessageKind,
    pub content: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsage {
    pub used_tokens: u64,
    pub max_tokens: Option<u64>,
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_output_tokens: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: String,
    pub title: String,
    pub provider: ProviderId,
    #[serde(default)]
    pub provider_brand: ProviderBrand,
    pub model: Option<String>,
    pub reasoning: Option<ReasoningEffort>,
    #[serde(default)]
    pub speed_mode: SpeedMode,
    #[serde(default)]
    pub interaction_mode: InteractionMode,
    #[serde(default)]
    pub access_mode: AccessMode,
    pub workspace: String,
    pub provider_session_id: Option<String>,
    pub status: SessionStatus,
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub context_usage: Option<ContextUsage>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    Snapshot {
        session: AgentSession,
    },
    Delta {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
        delta: String,
    },
    Activity {
        #[serde(rename = "sessionId")]
        session_id: String,
        message: Message,
    },
    ContextUsage {
        #[serde(rename = "sessionId")]
        session_id: String,
        usage: ContextUsage,
    },
    Removed {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionInput {
    pub provider: ProviderId,
    pub provider_brand: ProviderBrand,
    pub model: Option<String>,
    pub reasoning: Option<ReasoningEffort>,
    pub speed_mode: SpeedMode,
    pub interaction_mode: InteractionMode,
    pub access_mode: AccessMode,
    pub workspace: String,
}

impl Default for CreateSessionInput {
    fn default() -> Self {
        Self {
            provider: ProviderId::Claude,
            provider_brand: ProviderBrand::Anthropic,
            model: None,
            reasoning: Some(ReasoningEffort::Medium),
            speed_mode: SpeedMode::Standard,
            interaction_mode: InteractionMode::Build,
            access_mode: AccessMode::ApprovalRequired,
            workspace: String::new(),
        }
    }
}

pub fn workspace_name(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or("Project")
        .to_owned()
}

pub fn replace_session(sessions: &mut Vec<AgentSession>, session: AgentSession) {
    if let Some(existing) = sessions.iter_mut().find(|item| item.id == session.id) {
        *existing = session;
    } else {
        sessions.push(session);
    }
    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
}

pub fn apply_session_event(
    sessions: &mut Vec<AgentSession>,
    event: SessionEvent,
    event_time: &str,
) {
    match event {
        SessionEvent::Snapshot { session } => replace_session(sessions, session),
        SessionEvent::Delta {
            session_id,
            message_id,
            delta,
        } => {
            let Some(session) = sessions.iter_mut().find(|session| session.id == session_id) else {
                return;
            };
            if let Some(message) = session
                .messages
                .iter_mut()
                .find(|message| message.id == message_id)
            {
                message.content.push_str(&delta);
            } else {
                session.messages.push(Message {
                    id: message_id,
                    role: MessageRole::Assistant,
                    kind: MessageKind::Text,
                    content: delta,
                    created_at: event_time.to_owned(),
                });
            }
        }
        SessionEvent::Activity {
            session_id,
            message,
        } => {
            if let Some(session) = sessions.iter_mut().find(|session| session.id == session_id)
                && !session.messages.iter().any(|item| item.id == message.id)
            {
                session.messages.push(message);
            }
        }
        SessionEvent::ContextUsage { session_id, usage } => {
            if let Some(session) = sessions.iter_mut().find(|session| session.id == session_id) {
                session.context_usage = Some(usage);
                session.updated_at = event_time.to_owned();
            }
        }
        SessionEvent::Removed { session_id } => {
            sessions.retain(|session| session.id != session_id);
        }
    }
}

pub fn demo_providers() -> Vec<ProviderStatus> {
    ProviderId::ALL
        .into_iter()
        .map(|id| ProviderStatus {
            id,
            name: id.display_name().to_owned(),
            available: !matches!(id, ProviderId::Gemini),
            executable_path: (!matches!(id, ProviderId::Openrouter))
                .then(|| format!("/usr/local/bin/{id:?}").to_lowercase()),
            version: None,
            install_url: "https://github.com/z4mbo/Onyx#providers".to_owned(),
            transport: if matches!(id, ProviderId::Openrouter) {
                "HTTPS + SSE"
            } else {
                "stream-json"
            }
            .to_owned(),
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HoldMode {
    Dictation,
    Agent,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HoldPhase {
    Pressed,
    Released,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HoldPayload {
    pub mode: HoldMode,
    pub phase: HoldPhase,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str) -> AgentSession {
        AgentSession {
            id: id.to_owned(),
            title: "Session".to_owned(),
            provider: ProviderId::Claude,
            provider_brand: ProviderBrand::Anthropic,
            model: None,
            reasoning: Some(ReasoningEffort::Medium),
            speed_mode: SpeedMode::Standard,
            interaction_mode: InteractionMode::Build,
            access_mode: AccessMode::ApprovalRequired,
            workspace: "/tmp/onyx".to_owned(),
            provider_session_id: None,
            status: SessionStatus::Running,
            messages: Vec::new(),
            context_usage: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn tauri_payloads_keep_the_existing_wire_format() {
        let input = CreateSessionInput {
            workspace: "/tmp/onyx".to_owned(),
            ..CreateSessionInput::default()
        };
        let value = serde_json::to_value(input).expect("serialize create-session input");

        assert_eq!(value["provider"], "claude");
        assert_eq!(value["providerBrand"], "anthropic");
        assert_eq!(value["speedMode"], "standard");
        assert_eq!(value["interactionMode"], "build");
        assert_eq!(value["accessMode"], "approval_required");
        assert_eq!(value["workspace"], "/tmp/onyx");
    }

    #[test]
    fn workspace_labels_support_unix_and_windows_paths() {
        assert_eq!(workspace_name("/Users/onyx/Dev/project/"), "project");
        assert_eq!(workspace_name(r"C:\Users\onyx\project"), "project");
        assert_eq!(workspace_name(""), "Project");
    }

    #[test]
    fn first_delta_creates_then_extends_the_assistant_message() {
        let mut sessions = vec![session("a")];
        apply_session_event(
            &mut sessions,
            SessionEvent::Delta {
                session_id: "a".to_owned(),
                message_id: "assistant".to_owned(),
                delta: "hel".to_owned(),
            },
            "2026-01-01T00:00:01Z",
        );
        apply_session_event(
            &mut sessions,
            SessionEvent::Delta {
                session_id: "a".to_owned(),
                message_id: "assistant".to_owned(),
                delta: "lo".to_owned(),
            },
            "2026-01-01T00:00:02Z",
        );

        assert_eq!(sessions[0].messages.len(), 1);
        assert_eq!(sessions[0].messages[0].content, "hello");
        assert_eq!(sessions[0].messages[0].created_at, "2026-01-01T00:00:01Z");
    }

    #[test]
    fn activity_is_idempotent_and_context_usage_is_retained() {
        let mut sessions = vec![session("a")];
        let activity = Message {
            id: "tool".to_owned(),
            role: MessageRole::Tool,
            kind: MessageKind::Tool,
            content: "Read file".to_owned(),
            created_at: "2026-01-01T00:00:01Z".to_owned(),
        };
        for _ in 0..2 {
            apply_session_event(
                &mut sessions,
                SessionEvent::Activity {
                    session_id: "a".to_owned(),
                    message: activity.clone(),
                },
                "2026-01-01T00:00:01Z",
            );
        }
        let usage = ContextUsage {
            used_tokens: 42,
            max_tokens: Some(1_000),
            input_tokens: Some(30),
            cached_input_tokens: None,
            output_tokens: Some(12),
            reasoning_output_tokens: None,
        };
        apply_session_event(
            &mut sessions,
            SessionEvent::ContextUsage {
                session_id: "a".to_owned(),
                usage: usage.clone(),
            },
            "2026-01-01T00:00:02Z",
        );

        assert_eq!(sessions[0].messages, vec![activity]);
        assert_eq!(sessions[0].context_usage, Some(usage));
        assert_eq!(sessions[0].updated_at, "2026-01-01T00:00:02Z");
    }
}
