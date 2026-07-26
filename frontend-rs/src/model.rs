#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
            Self::Xai => "xai",
            Self::Moonshot => "moonshot",
            Self::Openrouter => "openrouter",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "openai" => Some(Self::Openai),
            "anthropic" => Some(Self::Anthropic),
            "google" => Some(Self::Google),
            "xai" => Some(Self::Xai),
            "moonshot" => Some(Self::Moonshot),
            "openrouter" => Some(Self::Openrouter),
            _ => None,
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

impl ReasoningEffort {
    pub const ALL: [Self; 8] = [
        Self::None,
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::Xhigh,
        Self::Max,
        Self::Ultracode,
    ];

    pub const fn as_str(self) -> &'static str {
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

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Minimal => "Minimal",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Xhigh => "Extra high",
            Self::Max => "Max",
            Self::Ultracode => "Ultracode",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|effort| effort.as_str() == value)
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
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Fast => "fast",
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

impl AccessMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApprovalRequired => "approval_required",
            Self::AutoAcceptEdits => "auto_accept_edits",
            Self::FullAccess => "full_access",
        }
    }
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelOption {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_default: bool,
    #[serde(default)]
    pub reasoning: Vec<ReasoningEffort>,
    pub default_reasoning: Option<ReasoningEffort>,
    #[serde(default)]
    pub speeds: Vec<SpeedMode>,
    pub default_speed: SpeedMode,
    pub context_length: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub label: String,
    pub used_percent: f64,
    pub window_minutes: Option<u64>,
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsage {
    pub provider: ProviderId,
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
    pub updated_at: String,
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
    ActivityDelta {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
        delta: String,
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
    pub title: String,
    pub provider: ProviderId,
    pub provider_brand: ProviderBrand,
    pub model: Option<String>,
    pub reasoning: Option<ReasoningEffort>,
    pub speed_mode: SpeedMode,
    pub interaction_mode: InteractionMode,
    pub access_mode: AccessMode,
    pub workspace: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameSessionInput {
    pub session_id: String,
    pub title: String,
}

#[derive(Clone, Debug, Serialize)]
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
pub struct ApprovalRequest {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub detail: String,
    pub risk: String,
    pub created_at: String,
}

pub type ProviderUserInputAnswers = BTreeMap<String, Vec<String>>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUserInputOption {
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUserInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    #[serde(default)]
    pub options: Vec<ProviderUserInputOption>,
    #[serde(default)]
    pub multi_select: bool,
    #[serde(default)]
    pub allow_other: bool,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUserInputRequest {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub questions: Vec<ProviderUserInputQuestion>,
    pub auto_resolution_ms: Option<u64>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub depth: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepoFileChange {
    pub path: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    pub is_repo: bool,
    pub branch: Option<String>,
    #[serde(default)]
    pub changed_files: Vec<RepoFileChange>,
    pub staged_count: usize,
    pub unstaged_count: usize,
    pub untracked_count: usize,
    pub ahead: u32,
    pub behind: u32,
    pub has_upstream: bool,
    pub has_remote: bool,
    pub pr_commit_count: Option<u32>,
    pub pr_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFile {
    pub path: String,
    pub content: String,
    pub truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EditorTarget {
    pub id: String,
    pub label: String,
    pub available: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitActionResult {
    pub message: String,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSession {
    pub id: String,
    pub cwd: String,
    pub shell: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalEvent {
    pub session_id: String,
    pub kind: String,
    pub data: Option<String>,
    pub exit_code: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenRouterVoiceModel {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub supported_voices: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenRouterVoiceCatalog {
    #[serde(default)]
    pub transcription: Vec<OpenRouterVoiceModel>,
    #[serde(default)]
    pub speech: Vec<OpenRouterVoiceModel>,
}

pub const OPENAI_SPEECH_VOICES: [&str; 11] = [
    "alloy", "ash", "ballad", "coral", "echo", "fable", "nova", "onyx", "sage", "shimmer", "verse",
];
pub const DEFAULT_OPENROUTER_SPEECH_MODEL: &str = "deepgram/aura-2";
pub const DEFAULT_OPENROUTER_SPEECH_VOICE: &str = "aura-2-livia-it";

pub fn supported_speech_voices(
    catalog: &OpenRouterVoiceCatalog,
    provider: &str,
    model: &str,
) -> Option<Vec<String>> {
    if provider.eq_ignore_ascii_case("openai") {
        return Some(
            OPENAI_SPEECH_VOICES
                .into_iter()
                .map(str::to_owned)
                .collect(),
        );
    }
    if !provider.eq_ignore_ascii_case("openrouter") {
        return None;
    }
    catalog
        .speech
        .iter()
        .find(|entry| entry.id == model)
        .and_then(|model| {
            (!model.supported_voices.is_empty()).then(|| model.supported_voices.clone())
        })
}

pub fn normalized_speech_voice(
    catalog: &OpenRouterVoiceCatalog,
    provider: &str,
    model: &str,
    current_voice: &str,
) -> Option<String> {
    let voices = supported_speech_voices(catalog, provider, model)?;
    (!voices.is_empty() && !voices.iter().any(|entry| entry == current_voice))
        .then(|| voices[0].clone())
}

pub fn resolved_openrouter_speech_selection(
    catalog: &OpenRouterVoiceCatalog,
    current_model: &str,
    current_voice: &str,
) -> Option<(String, String)> {
    let selected = catalog
        .speech
        .iter()
        .find(|model| model.id == current_model && !model.supported_voices.is_empty())
        .or_else(|| {
            catalog.speech.iter().find(|model| {
                model.id == DEFAULT_OPENROUTER_SPEECH_MODEL && !model.supported_voices.is_empty()
            })
        })
        .or_else(|| {
            catalog
                .speech
                .iter()
                .find(|model| !model.supported_voices.is_empty())
        })?;
    let voice = selected
        .supported_voices
        .iter()
        .find(|voice| selected.id == current_model && voice.as_str() == current_voice)
        .cloned()
        .unwrap_or_else(|| selected.supported_voices[0].clone());
    Some((selected.id.clone(), voice))
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub connected: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
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

impl OverlayPosition {
    pub const ALL: [Self; 7] = [
        Self::TopLeft,
        Self::TopCenter,
        Self::TopRight,
        Self::Center,
        Self::BottomLeft,
        Self::BottomCenter,
        Self::BottomRight,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TopLeft => "top_left",
            Self::TopCenter => "top_center",
            Self::TopRight => "top_right",
            Self::Center => "center",
            Self::BottomLeft => "bottom_left",
            Self::BottomCenter => "bottom_center",
            Self::BottomRight => "bottom_right",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|position| position.as_str() == value)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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
            dictation_shortcut: "Ctrl+Shift (hold)".to_owned(),
            agent_shortcut: "Ctrl+Alt (hold)".to_owned(),
            overlay_position: OverlayPosition::BottomCenter,
            overlay_margin: 18,
            transcription_provider: "openrouter".to_owned(),
            transcription_model: "openai/whisper-large-v3-turbo".to_owned(),
            agent_provider: ProviderId::Openrouter,
            agent_model: "openrouter/auto".to_owned(),
            web_provider: ProviderId::Openrouter,
            web_model: "openrouter/auto".to_owned(),
            files_provider: ProviderId::Codex,
            files_model: "gpt-5.4".to_owned(),
            image_provider: ProviderId::Openrouter,
            image_model: String::new(),
            reasoning: ReasoningEffort::Medium,
            language: None,
            speak_responses: true,
            voice_provider: "openrouter".to_owned(),
            voice_id: DEFAULT_OPENROUTER_SPEECH_VOICE.to_owned(),
            voice_model: DEFAULT_OPENROUTER_SPEECH_MODEL.to_owned(),
            voice_rate: 1.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionReply {
    pub text: String,
    pub model: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeVoicePermissions {
    pub input_monitoring: bool,
    pub accessibility: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveAppContext {
    pub name: String,
    pub process: String,
    pub accent: String,
    pub symbol: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessageInput {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub provider: ProviderId,
    pub model: String,
    pub messages: Vec<ChatMessageInput>,
    #[serde(default)]
    pub web_search: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatReply {
    pub content: String,
    pub model: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountProfile {
    pub id: String,
    pub name: String,
    pub email: String,
    pub image_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OAuthStart {
    pub authorize_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountEvent {
    pub profile: Option<AccountProfile>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapturedAudio {
    pub audio_base64: String,
    pub format: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    #[serde(default)]
    pub finished: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceHistoryItem {
    pub id: String,
    pub created_at: String,
    pub kind: String,
    pub text: String,
    pub answer: Option<String>,
    pub app_name: Option<String>,
    pub model: Option<String>,
}

impl Default for CreateSessionInput {
    fn default() -> Self {
        Self {
            title: "New session".to_owned(),
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
            if let Some(session) = sessions.iter_mut().find(|session| session.id == session_id) {
                if let Some(existing) = session
                    .messages
                    .iter_mut()
                    .find(|item| item.id == message.id)
                {
                    *existing = message;
                } else {
                    session.messages.push(message);
                }
            }
        }
        SessionEvent::ActivityDelta {
            session_id,
            message_id,
            delta,
        } => {
            if let Some(session) = sessions.iter_mut().find(|session| session.id == session_id)
                && let Some(message) = session
                    .messages
                    .iter_mut()
                    .find(|message| message.id == message_id)
            {
                if !message.content.ends_with('\n') {
                    message.content.push('\n');
                }
                message.content.push_str(&delta);
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
        assert_eq!(value["title"], "New session");
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
    fn incompatible_speech_voice_resets_to_the_selected_models_catalog() {
        let catalog = OpenRouterVoiceCatalog {
            transcription: Vec::new(),
            speech: vec![OpenRouterVoiceModel {
                id: "provider/tts".to_owned(),
                name: "Provider TTS".to_owned(),
                supported_voices: vec!["voice-a".to_owned(), "voice-b".to_owned()],
            }],
        };

        assert_eq!(
            normalized_speech_voice(&catalog, "openrouter", "provider/tts", "old-voice"),
            Some("voice-a".to_owned())
        );
        assert_eq!(
            normalized_speech_voice(&catalog, "openrouter", "provider/tts", "voice-b"),
            None
        );
    }

    #[test]
    fn retired_openrouter_speech_selection_resolves_from_live_catalog() {
        let catalog = OpenRouterVoiceCatalog {
            transcription: Vec::new(),
            speech: vec![OpenRouterVoiceModel {
                id: "deepgram/aura-2".to_owned(),
                name: "Deepgram Aura 2".to_owned(),
                supported_voices: vec!["aura-2-livia-it".to_owned(), "aura-2-thalia-en".to_owned()],
            }],
        };

        assert_eq!(
            resolved_openrouter_speech_selection(
                &catalog,
                "openai/gpt-4o-mini-tts-2025-12-15",
                "alloy",
            ),
            Some(("deepgram/aura-2".to_owned(), "aura-2-livia-it".to_owned(),))
        );
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
    fn activity_is_upserted_and_activity_delta_is_retained() {
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
        let completed = Message {
            content: "Ran cargo test".to_owned(),
            ..activity.clone()
        };
        apply_session_event(
            &mut sessions,
            SessionEvent::Activity {
                session_id: "a".to_owned(),
                message: completed,
            },
            "2026-01-01T00:00:02Z",
        );
        apply_session_event(
            &mut sessions,
            SessionEvent::ActivityDelta {
                session_id: "a".to_owned(),
                message_id: "tool".to_owned(),
                delta: "ok".to_owned(),
            },
            "2026-01-01T00:00:02Z",
        );
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

        assert_eq!(sessions[0].messages.len(), 1);
        assert_eq!(sessions[0].messages[0].content, "Ran cargo test\nok");
        assert_eq!(sessions[0].context_usage, Some(usage));
        assert_eq!(sessions[0].updated_at, "2026-01-01T00:00:02Z");
    }
}
