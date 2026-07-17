export type OverlayPosition =
  | "top_left"
  | "top_center"
  | "top_right"
  | "center"
  | "bottom_left"
  | "bottom_center"
  | "bottom_right";

export type DictationPhase =
  | "idle"
  | "starting"
  | "listening"
  | "transcribing"
  | "success"
  | "error";

export type ProviderId =
  | "openrouter"
  | "openai"
  | "chatgpt_codex"
  | "local"
  | "managed"
  | "anthropic_api"
  | "claude_subscription_agent_sdk";

export type CapabilityId =
  | "stt"
  | "web_search"
  | "computer"
  | "files"
  | "tts"
  | "images"
  | "video";

export type ReasoningEffort = "none" | "low" | "medium" | "high" | "xhigh";

export type TtsProvider = "system" | "openrouter" | "openai";

export interface VoiceSettings {
  provider: TtsProvider;
  /** SpeechSynthesisVoice.voiceURI for the local/system provider. */
  voiceId: string;
  /** Cloud TTS model slug. Ignored by the system provider. */
  model: string;
  /** Provider voice identifier (for example alloy). */
  cloudVoice: string;
  /** Playback speed, normalized between 0.6x and 1.4x. */
  rate: number;
}

export interface SystemVoiceOption {
  id: string;
  name: string;
  language: string;
  local: boolean;
}

export interface SpeechSynthesisRequest {
  provider: "openrouter" | "openai";
  model: string;
  voice: string;
  input: string;
  speed: number;
  responseFormat: "mp3";
}

export interface SpeechSynthesisReply {
  audioBase64: string;
  mimeType: string;
  model: string;
  generationId?: string | null;
}

export interface TtsConfig {
  provider: TtsProvider;
  model: string;
  voice: string;
  speed: number;
  instructions?: string | null;
  fallbackToSystem: boolean;
  systemVoice?: string | null;
}

export interface TtsVoiceOption {
  id: string;
  name: string;
  provider: TtsProvider;
  language?: string | null;
  local: boolean;
}

export interface TtsSpeakReply {
  requestedProvider: TtsProvider;
  provider: TtsProvider;
  model?: string | null;
  voice: string;
  characters: number;
  usedFallback: boolean;
  generationId?: string | null;
  warning?: string | null;
}

export interface AppSettings {
  /** Kept for backward compatibility with Onyx 0.8. */
  wisprShortcut: string;
  dictationShortcut: string;
  agentShortcut: string;
  overlayPosition: OverlayPosition;
  overlayMargin: number;
  sttProvider: ProviderId;
  sttModel: string;
  agentProvider: ProviderId;
  agentModel: string;
  reasoning: ReasoningEffort;
  language: string | null;
  speakResponses: boolean;
  voicePreset: string;
}

export interface TranscriptionReply {
  text: string;
  model: string;
  generationId?: string | null;
  seconds?: number | null;
  cost?: number | null;
}

export interface ModelOption {
  id: string;
  name: string;
  description?: string | null;
  promptPrice?: string | null;
  completionPrice?: string | null;
}

export interface SearchRequest {
  query: string;
  provider: ProviderId;
  model: string;
  reasoning?: ReasoningEffort | null;
}

export interface SearchSource {
  title: string;
  url: string;
  snippet?: string | null;
}

export interface SearchUsage {
  inputTokens?: number | null;
  outputTokens?: number | null;
  cost?: number | null;
}

export interface SearchReply {
  answer: string;
  model: string;
  sources: SearchSource[];
  usage: SearchUsage;
}

export interface ActiveAppContext {
  name: string;
  process: string;
  accent: string;
  symbol: string;
}

export interface HoldPayload {
  mode: "dictation" | "agent";
  phase: "pressed" | "released";
}

export interface OpenRouterAuthPayload {
  status: "waiting" | "connected" | "error";
  message?: string | null;
}

export interface CodexAccountStatus {
  available: boolean;
  connected: boolean;
  authMode?: string | null;
  email?: string | null;
  planType?: string | null;
}

export interface CodexLoginStart {
  loginId: string;
  authUrl: string;
}

export interface CodexDeviceLoginStart {
  loginId: string;
  verificationUrl: string;
  userCode: string;
}

export interface CodexRateLimits {
  primaryUsedPercent?: number | null;
  primaryWindowMinutes?: number | null;
  primaryResetsAt?: number | null;
  secondaryUsedPercent?: number | null;
  secondaryWindowMinutes?: number | null;
  secondaryResetsAt?: number | null;
}

/** Compatibility payload emitted by Onyx 0.8. */
export interface ShortcutPayload {
  mode: "wispr";
}

export interface ModelSelection {
  provider: ProviderId;
  model: string;
}

export interface CapabilityRoute {
  capability: CapabilityId;
  primary: ModelSelection;
  fallbacks: ModelSelection[];
}

export interface OnyxProfile {
  firstName: string;
  lastName: string;
  email: string;
  language: string;
  authMode: "preview" | "clerk";
}

export interface SearchHistoryItem {
  id: string;
  createdAt: string;
  query: string;
  answer: string;
  model: string;
  sources: SearchSource[];
  usage: SearchUsage;
}

export const DEFAULT_SETTINGS: AppSettings = {
  wisprShortcut: "Ctrl+Shift (hold)",
  dictationShortcut: "Ctrl+Shift (hold)",
  agentShortcut: "Ctrl+Alt (hold)",
  overlayPosition: "bottom_center",
  overlayMargin: 18,
  sttProvider: "openrouter",
  sttModel: "openai/whisper-large-v3",
  agentProvider: "openrouter",
  agentModel: "openrouter/auto",
  reasoning: "medium",
  language: "it",
  speakResponses: true,
  voicePreset: "sky",
};

export const DEFAULT_VOICE_SETTINGS: VoiceSettings = {
  provider: "system",
  voiceId: "",
  model: "openai/gpt-4o-mini-tts-2025-12-15",
  cloudVoice: "alloy",
  rate: 1,
};

export const DEFAULT_ROUTES: CapabilityRoute[] = [
  {
    capability: "stt",
    primary: { provider: "openrouter", model: "openai/whisper-large-v3" },
    fallbacks: [{ provider: "openai", model: "gpt-4o-mini-transcribe" }],
  },
  {
    capability: "web_search",
    primary: { provider: "openrouter", model: "openrouter/auto" },
    fallbacks: [{ provider: "openai", model: "gpt-4.1-mini" }],
  },
  {
    capability: "computer",
    primary: { provider: "managed", model: "managed/computer-use" },
    fallbacks: [],
  },
  {
    capability: "files",
    primary: { provider: "local", model: "local/openai-compatible" },
    fallbacks: [],
  },
  {
    capability: "tts",
    primary: { provider: "local", model: "local/system-voice" },
    fallbacks: [],
  },
  {
    capability: "images",
    primary: { provider: "openai", model: "gpt-image-1" },
    fallbacks: [],
  },
  {
    capability: "video",
    primary: { provider: "managed", model: "managed/video" },
    fallbacks: [],
  },
];
