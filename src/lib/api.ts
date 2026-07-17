import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  ActiveAppContext,
  AppSettings,
  CapabilityId,
  CodexAccountStatus,
  CodexDeviceLoginStart,
  CodexLoginStart,
  CodexRateLimits,
  HoldPayload,
  ModelOption,
  OpenRouterAuthPayload,
  ProviderId,
  SearchReply,
  SearchRequest,
  SpeechSynthesisReply,
  SpeechSynthesisRequest,
  ShortcutPayload,
  TranscriptionReply,
  TtsConfig,
  TtsSpeakReply,
  TtsVoiceOption,
} from "../types";
import { getPreviewConnection, setPreviewConnection } from "./storage";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function getBackendSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export function applyBackendSettings(settings: AppSettings): Promise<AppSettings> {
  return isTauri ? invoke<AppSettings>("apply_settings", { settings }) : Promise.resolve(settings);
}

export function transcribe(audioBase64: string, format: string): Promise<TranscriptionReply> {
  if (!isTauri) {
    return Promise.resolve({
      text: "Questa è una trascrizione di anteprima.",
      model: "preview/browser",
    });
  }
  return invoke<TranscriptionReply>("transcribe_audio", {
    request: { audioBase64, format },
  });
}

export function injectText(text: string): Promise<void> {
  if (!isTauri) {
    void navigator.clipboard?.writeText(text).catch(() => undefined);
    return Promise.resolve();
  }
  return invoke("inject_text", { text });
}

export function providerConnectionStatus(providerName: ProviderId): Promise<boolean> {
  if (!isTauri) return Promise.resolve(getPreviewConnection(providerName));
  return invoke<boolean>("provider_connection_status", { providerName });
}

export function saveProviderApiKey(providerName: ProviderId, apiKey: string): Promise<void> {
  if (!isTauri) {
    if (apiKey.trim().length < 12) return Promise.reject(new Error("La chiave deve contenere almeno 12 caratteri."));
    setPreviewConnection(providerName, true);
    return Promise.resolve();
  }
  return invoke("save_provider_api_key", { providerName, apiKey });
}

export function disconnectProvider(providerName: ProviderId): Promise<void> {
  if (!isTauri) {
    setPreviewConnection(providerName, false);
    return Promise.resolve();
  }
  return invoke("disconnect_provider", { providerName });
}

export function listModels(providerName: ProviderId, capability: CapabilityId): Promise<ModelOption[]> {
  if (!isTauri) return Promise.resolve(fallbackModels(providerName, capability));
  return invoke<ModelOption[]>("list_models", { providerName, capability });
}

export function searchWeb(request: SearchRequest): Promise<SearchReply> {
  if (!isTauri) {
    return new Promise((resolve) => {
      window.setTimeout(() => resolve({
        answer: `Modalità anteprima: ho ricevuto “${request.query}”. Nella build desktop Onyx usa ${providerLabel(request.provider)} con ricerca web verificabile e mostra qui le fonti consultate.`,
        model: request.model,
        sources: [
          {
            title: "Onyx · sorgente dimostrativa",
            url: "https://www.example.com/",
            snippet: "La modalità browser non invia richieste reali né memorizza chiavi API.",
          },
        ],
        usage: { inputTokens: 18, outputTokens: 38, cost: 0 },
      }), 900);
    });
  }
  return invoke<SearchReply>("search_web", { request });
}

export function synthesizeSpeech(request: SpeechSynthesisRequest): Promise<SpeechSynthesisReply> {
  if (!isTauri) {
    return Promise.reject(new Error("La sintesi vocale cloud è disponibile nella build desktop."));
  }
  return invoke<SpeechSynthesisReply>("synthesize_speech", { request });
}

export function getTtsConfig(): Promise<TtsConfig> {
  if (!isTauri) return Promise.reject(new Error("La configurazione TTS nativa Ã¨ disponibile nella build desktop."));
  return invoke<TtsConfig>("get_tts_config");
}

export function saveTtsConfig(config: TtsConfig): Promise<TtsConfig> {
  if (!isTauri) return Promise.resolve(config);
  return invoke<TtsConfig>("save_tts_config", { config });
}

export function listTtsVoices(providerName: TtsConfig["provider"], model: string): Promise<TtsVoiceOption[]> {
  if (!isTauri) return Promise.resolve([]);
  return invoke<TtsVoiceOption[]>("list_tts_voices", { providerName, model });
}

export function previewTts(text?: string): Promise<TtsSpeakReply> {
  if (!isTauri) return Promise.reject(new Error("L'anteprima TTS nativa Ã¨ disponibile nella build desktop."));
  return invoke<TtsSpeakReply>("preview_tts", { text });
}

export function speakTts(text: string): Promise<TtsSpeakReply> {
  if (!isTauri) return Promise.reject(new Error("La voce TTS nativa Ã¨ disponibile nella build desktop."));
  return invoke<TtsSpeakReply>("speak_tts", { text });
}

export function getActiveApp(): Promise<ActiveAppContext> {
  if (!isTauri) {
    return Promise.resolve({
      name: "Anteprima browser",
      process: "browser",
      accent: "#5e8ff7",
      symbol: "O",
    });
  }
  return invoke<ActiveAppContext>("active_app_context");
}

export function setAgentExpanded(expanded: boolean): Promise<void> {
  return isTauri ? invoke("set_agent_expanded", { expanded }) : Promise.resolve();
}

export function openExternal(target: string): Promise<void> {
  if (!/^https?:\/\//i.test(target)) return Promise.reject(new Error("Link non valido."));
  if (!isTauri) {
    window.open(target, "_blank", "noopener,noreferrer");
    return Promise.resolve();
  }
  return invoke("open_external", { target });
}

export function getOpenRouterConnectionStatus(): Promise<boolean> {
  return providerConnectionStatus("openrouter");
}

export function saveOpenRouterApiKey(apiKey: string): Promise<void> {
  return saveProviderApiKey("openrouter", apiKey);
}

export function disconnectOpenRouter(): Promise<void> {
  return disconnectProvider("openrouter");
}

export function beginOpenRouterOAuth(): Promise<void> {
  return isTauri
    ? invoke("begin_openrouter_oauth")
    : Promise.reject(new Error("OAuth OpenRouter è disponibile nella build desktop."));
}

export function chatgptAccountStatus(): Promise<CodexAccountStatus> {
  if (!isTauri) return Promise.resolve({ available: false, connected: false });
  return invoke<CodexAccountStatus>("chatgpt_account_status");
}

export function beginChatgptLogin(): Promise<CodexLoginStart> {
  if (!isTauri) return Promise.reject(new Error("Il login ChatGPT/Codex è disponibile nella build desktop."));
  return invoke<CodexLoginStart>("begin_chatgpt_login");
}

export function beginChatgptDeviceLogin(): Promise<CodexDeviceLoginStart> {
  if (!isTauri) return Promise.reject(new Error("Il device login ChatGPT/Codex è disponibile nella build desktop."));
  return invoke<CodexDeviceLoginStart>("begin_chatgpt_device_login");
}

export function disconnectChatgpt(): Promise<void> {
  return isTauri ? invoke("disconnect_chatgpt") : Promise.resolve();
}

export function chatgptRateLimits(): Promise<CodexRateLimits> {
  if (!isTauri) return Promise.resolve({});
  return invoke<CodexRateLimits>("chatgpt_rate_limits");
}

export function listOpenRouterTranscriptionModels(): Promise<ModelOption[]> {
  return listModels("openrouter", "stt");
}

export function hideWindow(label: "main" | "hud" | "agent"): Promise<void> {
  return isTauri ? invoke("hide_window", { label }) : Promise.resolve();
}

export function showMainWindow(): Promise<void> {
  return isTauri ? invoke("show_main_window") : Promise.resolve();
}

export function quitApp(): Promise<void> {
  return isTauri ? invoke("quit_app") : Promise.resolve();
}

export function getPlatform(): Promise<string> {
  return isTauri ? invoke<string>("platform") : Promise.resolve(navigator.platform.includes("Mac") ? "macos" : "windows");
}

export function onHold(handler: (payload: HoldPayload) => void): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<HoldPayload>("onyx://hold", (event) => handler(event.payload));
  }
  let ctrl = false;
  let shift = false;
  let alt = false;
  let active: HoldPayload["mode"] | null = null;

  const evaluate = () => {
    const next = ctrl && shift && !alt
      ? "dictation"
      : ctrl && alt && !shift
        ? "agent"
        : null;
    if (active && active !== next) handler({ mode: active, phase: "released" });
    if (next && next !== active) handler({ mode: next, phase: "pressed" });
    active = next;
  };
  const update = (event: KeyboardEvent, down: boolean) => {
    if (event.key === "Control") ctrl = down;
    else if (event.key === "Shift") shift = down;
    else if (event.key === "Alt") alt = down;
    else return;
    evaluate();
  };
  const down = (event: KeyboardEvent) => update(event, true);
  const up = (event: KeyboardEvent) => update(event, false);
  window.addEventListener("keydown", down);
  window.addEventListener("keyup", up);
  return Promise.resolve(() => {
    window.removeEventListener("keydown", down);
    window.removeEventListener("keyup", up);
  });
}

export function onShortcut(handler: (payload: ShortcutPayload) => void): Promise<UnlistenFn> {
  return listen<ShortcutPayload>("onyx://shortcut", (event) => handler(event.payload));
}

export function onOpenRouterAuth(handler: (event: OpenRouterAuthPayload) => void): Promise<UnlistenFn> {
  return listen<OpenRouterAuthPayload>("onyx://openrouter-auth", (event) => handler(event.payload));
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "Si è verificato un errore inatteso.";
}

export function fallbackModels(provider: ProviderId, capability: CapabilityId): ModelOption[] {
  const key = `${provider}:${capability}`;
  const catalogs: Record<string, ModelOption[]> = {
    "openrouter:stt": [
      { id: "openai/whisper-large-v3", name: "Whisper Large v3" },
      { id: "openai/gpt-4o-mini-transcribe", name: "GPT-4o mini Transcribe" },
    ],
    "openai:stt": [
      { id: "gpt-4o-mini-transcribe", name: "GPT-4o mini Transcribe" },
      { id: "gpt-4o-transcribe", name: "GPT-4o Transcribe" },
    ],
    "local:stt": [{ id: "local/whisper", name: "Whisper locale (endpoint)" }],
    "managed:stt": [{ id: "managed/fast-transcription", name: "Onyx Fast Transcription" }],
    "openrouter:web_search": [
      { id: "openrouter/auto", name: "OpenRouter Auto" },
      { id: "openai/gpt-4.1-mini", name: "GPT-4.1 mini" },
      { id: "anthropic/claude-sonnet-4", name: "Claude Sonnet 4" },
      { id: "google/gemini-2.5-flash", name: "Gemini 2.5 Flash" },
    ],
    "openai:web_search": [
      { id: "gpt-4.1-mini", name: "GPT-4.1 mini" },
      { id: "gpt-5-mini", name: "GPT-5 mini" },
    ],
    "chatgpt_codex:web_search": [
      { id: "codex/default", name: "Codex · automatico" },
      { id: "gpt-5.3-codex", name: "GPT-5.3 Codex" },
    ],
    "anthropic_api:web_search": [
      { id: "claude-sonnet-4-20250514", name: "Claude Sonnet 4" },
    ],
    "claude_subscription_agent_sdk:web_search": [
      { id: "claude-agent/default", name: "Claude Agent SDK (account)" },
    ],
    "local:web_search": [{ id: "local/openai-compatible", name: "LLM locale (OpenAI-compatible)" }],
    "managed:web_search": [
      { id: "managed/fast-search", name: "Onyx Search Fast" },
      { id: "managed/deep-search", name: "Onyx Search Deep" },
    ],
    "local:tts": [{ id: "local/system-voice", name: "Voce di sistema" }],
    "openrouter:tts": [
      { id: "openai/gpt-4o-mini-tts-2025-12-15", name: "GPT-4o mini TTS" },
      { id: "mistralai/voxtral-mini-tts-2603", name: "Voxtral Mini TTS" },
    ],
    "openai:tts": [{ id: "gpt-4o-mini-tts", name: "GPT-4o mini TTS" }],
    "openai:images": [{ id: "gpt-image-1", name: "GPT Image 1" }],
    "openrouter:images": [{ id: "openai/gpt-image-1", name: "GPT Image 1" }],
    "managed:images": [{ id: "managed/image", name: "Onyx Image" }],
    "managed:video": [{ id: "managed/video", name: "Onyx Video" }],
    "local:computer": [{ id: "local/openai-compatible", name: "LLM locale" }],
    "managed:computer": [{ id: "managed/computer-use", name: "Onyx Computer Use" }],
    "local:files": [{ id: "local/openai-compatible", name: "LLM locale" }],
    "managed:files": [{ id: "managed/files", name: "Onyx Files" }],
  };
  return catalogs[key] ?? [
    { id: `${provider}/default`, name: `${providerLabel(provider)} · automatico` },
  ];
}

export function providerLabel(provider: ProviderId): string {
  return ({
    openrouter: "OpenRouter",
    openai: "OpenAI API",
    chatgpt_codex: "ChatGPT / Codex",
    local: "Locale",
    managed: "Onyx Managed",
    anthropic_api: "Anthropic API",
    claude_subscription_agent_sdk: "Claude Agent SDK",
  } as const)[provider];
}
