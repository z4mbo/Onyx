use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    Claude,
    Codex,
    Gemini,
    Kimi,
    Openrouter,
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
    pub fn for_provider(provider: ProviderId) -> Self {
        match provider {
            ProviderId::Claude => Self::Anthropic,
            ProviderId::Codex => Self::Openai,
            ProviderId::Gemini => Self::Google,
            ProviderId::Kimi => Self::Moonshot,
            ProviderId::Openrouter => Self::Openrouter,
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
    /// t3code-style pseudo-level: engines receive xhigh plus an `ultracode`
    /// settings flag (Claude Code only).
    Ultracode,
}

impl ReasoningEffort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
            Self::Ultracode => "ultracode",
        }
    }

    /// The effort value engines that lack the ultracode pseudo-level accept.
    pub fn clamped_str(self) -> &'static str {
        match self {
            Self::Ultracode => "xhigh",
            other => other.as_str(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SpeedMode {
    #[default]
    Standard,
    Fast,
}

impl SpeedMode {
    pub fn as_service_tier(self) -> Option<&'static str> {
        match self {
            Self::Standard => None,
            Self::Fast => Some("fast"),
        }
    }
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Deny,
    Allow,
    /// Allow and let the provider remember the grant for the rest of the session.
    AllowSession,
}

impl ApprovalDecision {
    pub fn allowed(self) -> bool {
        !matches!(self, Self::Deny)
    }
}

impl ProviderId {
    pub fn command(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("claude"),
            Self::Codex => Some("codex"),
            Self::Gemini => Some("gemini"),
            Self::Kimi => Some("kimi"),
            Self::Openrouter => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini CLI",
            Self::Kimi => "Kimi Code",
            Self::Openrouter => "OpenRouter",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Idle,
    Running,
    WaitingApproval,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Text,
    Reasoning,
    Tool,
    Error,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub role: MessageRole,
    pub kind: MessageKind,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

impl Message {
    pub fn new(role: MessageRole, kind: MessageKind, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role,
            kind,
            content: content.into(),
            created_at: Utc::now(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: String,
    pub title: String,
    pub provider: ProviderId,
    #[serde(default)]
    pub provider_brand: ProviderBrand,
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning: Option<ReasoningEffort>,
    #[serde(default)]
    pub speed_mode: SpeedMode,
    #[serde(default)]
    pub interaction_mode: InteractionMode,
    #[serde(default)]
    pub access_mode: AccessMode,
    pub workspace: PathBuf,
    pub provider_session_id: Option<String>,
    pub status: SessionStatus,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub context_usage: Option<ContextUsage>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionInput {
    pub provider: ProviderId,
    #[serde(default)]
    pub provider_brand: Option<ProviderBrand>,
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning: Option<ReasoningEffort>,
    #[serde(default)]
    pub speed_mode: SpeedMode,
    #[serde(default)]
    pub interaction_mode: InteractionMode,
    #[serde(default)]
    pub access_mode: AccessMode,
    pub workspace: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionOptionsInput {
    pub session_id: String,
    pub provider: ProviderId,
    pub provider_brand: ProviderBrand,
    pub model: Option<String>,
    pub reasoning: Option<ReasoningEffort>,
    pub speed_mode: SpeedMode,
    pub interaction_mode: InteractionMode,
    pub access_mode: AccessMode,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelOption {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_default: bool,
    pub reasoning: Vec<ReasoningEffort>,
    pub default_reasoning: Option<ReasoningEffort>,
    pub speeds: Vec<SpeedMode>,
    pub default_speed: SpeedMode,
    pub context_length: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub label: String,
    pub used_percent: f64,
    pub window_minutes: Option<u64>,
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsage {
    pub provider: ProviderId,
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageInput {
    pub session_id: String,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "type",
    rename_all_fields = "camelCase"
)]
pub enum SessionEvent {
    Snapshot {
        session: AgentSession,
    },
    Delta {
        session_id: String,
        message_id: String,
        delta: String,
    },
    Activity {
        session_id: String,
        message: Message,
    },
    ContextUsage {
        session_id: String,
        usage: ContextUsage,
    },
    Removed {
        session_id: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub detail: String,
    pub risk: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub depth: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRouterModel {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub context_length: Option<u64>,
    pub prompt_price: Option<String>,
    pub completion_price: Option<String>,
    #[serde(default)]
    pub input_modalities: Vec<String>,
    #[serde(default)]
    pub output_modalities: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRouterStatus {
    pub connected: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiStatus {
    pub connected: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPosition {
    TopLeft,
    TopCenter,
    TopRight,
    Center,
    BottomLeft,
    #[default]
    BottomCenter,
    BottomRight,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VoiceSettings {
    pub dictation_shortcut: String,
    pub agent_shortcut: String,
    pub overlay_position: OverlayPosition,
    pub overlay_margin: u32,
    pub transcription_provider: String,
    pub transcription_model: String,
    pub agent_provider: ProviderId,
    pub agent_model: String,
    pub web_provider: ProviderId,
    pub web_model: String,
    pub files_provider: ProviderId,
    pub files_model: String,
    pub image_provider: ProviderId,
    pub image_model: String,
    pub reasoning: ReasoningEffort,
    pub language: Option<String>,
    pub speak_responses: bool,
    pub voice_provider: String,
    pub voice_id: String,
    pub voice_model: String,
    pub voice_rate: f32,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            dictation_shortcut: "Ctrl+Shift (hold)".into(),
            agent_shortcut: "Ctrl+Alt (hold)".into(),
            overlay_position: OverlayPosition::BottomCenter,
            overlay_margin: 18,
            transcription_provider: "openrouter".into(),
            transcription_model: "openai/whisper-large-v3-turbo".into(),
            agent_provider: ProviderId::Openrouter,
            agent_model: "openrouter/auto".into(),
            web_provider: ProviderId::Openrouter,
            web_model: "openrouter/auto".into(),
            files_provider: ProviderId::Codex,
            files_model: "gpt-5.4".into(),
            image_provider: ProviderId::Openrouter,
            image_model: String::new(),
            reasoning: ReasoningEffort::Medium,
            language: None,
            speak_responses: true,
            voice_provider: "openrouter".into(),
            voice_id: "alloy".into(),
            voice_model: "openai/gpt-4o-mini-tts-2025-12-15".into(),
            voice_rate: 1.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionRequest {
    pub audio_base64: String,
    pub format: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionReply {
    pub text: String,
    pub model: String,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldPayload {
    pub mode: String,
    pub phase: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveAppContext {
    pub name: String,
    pub process: String,
    pub accent: String,
    pub symbol: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessageInput {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub provider: ProviderId,
    pub model: String,
    pub messages: Vec<ChatMessageInput>,
    #[serde(default)]
    pub web_search: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMedia {
    pub kind: String,
    pub url: String,
    pub mime_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatReply {
    pub content: String,
    pub model: String,
    pub media: Vec<ChatMedia>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaGenerationRequest {
    pub model: String,
    pub prompt: String,
    pub aspect_ratio: Option<String>,
    #[serde(default = "default_media_source")]
    pub source: String,
}

fn default_media_source() -> String {
    "openrouter".to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoJob {
    pub id: String,
    pub status: String,
    pub polling_url: String,
    pub content_url: Option<String>,
    pub error: Option<String>,
}
