use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPosition {
    TopLeft,
    TopCenter,
    TopRight,
    Center,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    /// Kept for migration from Onyx 0.8. The modifier listener is now hold-based.
    pub wispr_shortcut: String,
    pub dictation_shortcut: String,
    pub agent_shortcut: String,
    pub overlay_position: OverlayPosition,
    pub overlay_margin: u32,
    pub stt_provider: String,
    pub stt_model: String,
    pub agent_provider: String,
    pub agent_model: String,
    pub reasoning: String,
    pub language: Option<String>,
    pub speak_responses: bool,
    pub voice_preset: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            wispr_shortcut: "Ctrl+Shift (hold)".into(),
            dictation_shortcut: "Ctrl+Shift (hold)".into(),
            agent_shortcut: "Ctrl+Alt (hold)".into(),
            overlay_position: OverlayPosition::BottomCenter,
            overlay_margin: 18,
            stt_provider: "openrouter".into(),
            stt_model: "openai/whisper-large-v3".into(),
            agent_provider: "openrouter".into(),
            agent_model: "openrouter/auto".into(),
            reasoning: "medium".into(),
            language: Some("it".into()),
            speak_responses: true,
            voice_preset: "sky".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionRequest {
    pub audio_base64: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionReply {
    pub text: String,
    pub model: String,
    pub generation_id: Option<String>,
    pub seconds: Option<f64>,
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub prompt_price: Option<String>,
    pub completion_price: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    pub provider: String,
    pub model: String,
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSource {
    pub title: String,
    pub url: String,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchReply {
    pub answer: String,
    pub model: String,
    pub sources: Vec<SearchSource>,
    pub usage: SearchUsage,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRouterAuthEvent {
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldPayload {
    pub mode: String,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveAppContext {
    pub name: String,
    pub process: String,
    pub accent: String,
    pub symbol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct TtsConfig {
    /// `openai`, `openrouter`, or `system`.
    pub provider: String,
    /// The exact cloud model sent to the selected provider.
    pub model: String,
    /// Provider voice identifier. OpenRouter voice names are model-specific.
    pub voice: String,
    /// Provider playback speed multiplier.
    pub speed: f32,
    /// Optional style instruction for models that support it.
    pub instructions: Option<String>,
    /// If cloud synthesis fails, use the operating-system voice.
    pub fallback_to_system: bool,
    /// Empty/None lets the operating system choose its default voice.
    pub system_voice: Option<String>,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            provider: "openai".into(),
            model: "gpt-4o-mini-tts".into(),
            voice: "marin".into(),
            speed: 1.0,
            instructions: Some("Parla in modo naturale, caldo e chiaro.".into()),
            fallback_to_system: true,
            system_voice: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsVoiceOption {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub language: Option<String>,
    pub local: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsSpeakReply {
    pub requested_provider: String,
    pub provider: String,
    pub model: Option<String>,
    pub voice: String,
    pub characters: usize,
    pub used_fallback: bool,
    pub generation_id: Option<String>,
    pub warning: Option<String>,
}
