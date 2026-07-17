import {
  DEFAULT_ROUTES,
  DEFAULT_SETTINGS,
  DEFAULT_VOICE_SETTINGS,
  type AppSettings,
  type CapabilityId,
  type CapabilityRoute,
  type OnyxProfile,
  type OverlayPosition,
  type ProviderId,
  type ReasoningEffort,
  type SearchHistoryItem,
  type TtsProvider,
  type VoiceSettings,
} from "../types";

const SETTINGS_KEY = "onyx.settings.v1";
const LEGACY_HISTORY_KEY = "onyx.history.v1";
const LAST_ERROR_KEY = "onyx.last-error.v1";
const PROFILE_KEY = "onyx.profile.v2";
const JOURNEY_KEY = "onyx.journey.v2";
const ROUTES_KEY = "onyx.routes.v2";
const SEARCH_HISTORY_KEY = "onyx.search-history.v2";
const PREVIEW_CONNECTIONS_KEY = "onyx.preview-connections.v2";
const VOICE_SETTINGS_KEY = "onyx.voice.v1";

const POSITIONS = new Set<OverlayPosition>([
  "top_left", "top_center", "top_right", "center",
  "bottom_left", "bottom_center", "bottom_right",
]);
const PROVIDERS = new Set<ProviderId>([
  "openrouter", "openai", "chatgpt_codex", "local", "managed", "anthropic_api",
  "claude_subscription_agent_sdk",
]);
const REASONING = new Set<ReasoningEffort>(["none", "low", "medium", "high", "xhigh"]);
const CAPABILITIES = new Set<CapabilityId>([
  "stt", "web_search", "computer", "files", "tts", "images", "video",
]);

export type JourneyStage = "auth" | "onboarding" | "app";

export function loadSettings(): AppSettings {
  localStorage.removeItem(LEGACY_HISTORY_KEY);
  try {
    const parsed = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "null") as (Partial<AppSettings> & {
      localSttModel?: unknown;
      cloudModel?: unknown;
    }) | null;
    if (!parsed) return { ...DEFAULT_SETTINGS };
    return {
      wisprShortcut: "Ctrl+Shift (hold)",
      dictationShortcut: "Ctrl+Shift (hold)",
      agentShortcut: "Ctrl+Alt (hold)",
      overlayPosition: POSITIONS.has(parsed.overlayPosition as OverlayPosition)
        ? parsed.overlayPosition as OverlayPosition
        : DEFAULT_SETTINGS.overlayPosition,
      overlayMargin: typeof parsed.overlayMargin === "number" && Number.isFinite(parsed.overlayMargin)
        ? Math.min(120, Math.max(8, Math.round(parsed.overlayMargin)))
        : DEFAULT_SETTINGS.overlayMargin,
      sttProvider: validProvider(parsed.sttProvider, "openrouter"),
      sttModel: migrateSttModel(parsed),
      agentProvider: validProvider(parsed.agentProvider, "openrouter"),
      agentModel: validModel(parsed.agentModel) ? parsed.agentModel!.trim() : DEFAULT_SETTINGS.agentModel,
      reasoning: REASONING.has(parsed.reasoning as ReasoningEffort)
        ? parsed.reasoning as ReasoningEffort
        : DEFAULT_SETTINGS.reasoning,
      language: typeof parsed.language === "string" && parsed.language.trim()
        ? parsed.language.trim().toLowerCase()
        : null,
      speakResponses: typeof parsed.speakResponses === "boolean"
        ? parsed.speakResponses
        : DEFAULT_SETTINGS.speakResponses,
      voicePreset: typeof parsed.voicePreset === "string" && parsed.voicePreset.trim()
        ? parsed.voicePreset.trim()
        : DEFAULT_SETTINGS.voicePreset,
    };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

function migrateSttModel(parsed: Partial<AppSettings> & {
  localSttModel?: unknown;
  cloudModel?: unknown;
}): string {
  if (validModel(parsed.sttModel)) return parsed.sttModel!.trim();
  if (typeof parsed.cloudModel === "string" && parsed.cloudModel.includes("/") && validModel(parsed.cloudModel)) {
    return parsed.cloudModel.trim();
  }
  if (parsed.localSttModel === "large-v3-turbo") return "openai/whisper-large-v3-turbo";
  return DEFAULT_SETTINGS.sttModel;
}

function validProvider(value: unknown, fallback: ProviderId): ProviderId {
  return typeof value === "string" && PROVIDERS.has(value as ProviderId)
    ? value as ProviderId
    : fallback;
}

function validModel(value: unknown): value is string {
  return typeof value === "string"
    && value.trim().length > 2
    && value.trim().length <= 180
    && !value.includes("://")
    && /^[A-Za-z0-9._:/~\-]+$/.test(value.trim());
}

export function saveSettings(settings: AppSettings): void {
  localStorage.removeItem(LEGACY_HISTORY_KEY);
  // Modifier-only gestures are fixed native gestures and do not need persistence.
  const { agentShortcut: _agentShortcut, ...persisted } = settings;
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(persisted));
}

export function loadVoiceSettings(): VoiceSettings {
  try {
    const parsed = JSON.parse(localStorage.getItem(VOICE_SETTINGS_KEY) ?? "null") as Partial<VoiceSettings> | null;
    if (!parsed) return { ...DEFAULT_VOICE_SETTINGS };
    const provider: TtsProvider = parsed.provider === "openrouter" || parsed.provider === "openai"
      ? parsed.provider
      : "system";
    const voiceId = typeof parsed.voiceId === "string" ? parsed.voiceId.trim().slice(0, 300) : "";
    const model = typeof parsed.model === "string" && parsed.model.trim()
      ? parsed.model.trim().slice(0, 180)
      : DEFAULT_VOICE_SETTINGS.model;
    const cloudVoice = typeof parsed.cloudVoice === "string" && parsed.cloudVoice.trim()
      ? parsed.cloudVoice.trim().slice(0, 100)
      : DEFAULT_VOICE_SETTINGS.cloudVoice;
    const rate = typeof parsed.rate === "number" && Number.isFinite(parsed.rate)
      ? Math.min(1.4, Math.max(.6, Math.round(parsed.rate * 20) / 20))
      : DEFAULT_VOICE_SETTINGS.rate;
    return { provider, voiceId, model, cloudVoice, rate };
  } catch {
    return { ...DEFAULT_VOICE_SETTINGS };
  }
}

export function saveVoiceSettings(settings: VoiceSettings): void {
  const normalized: VoiceSettings = {
    provider: settings.provider === "openrouter" || settings.provider === "openai" ? settings.provider : "system",
    voiceId: settings.voiceId.trim().slice(0, 300),
    model: settings.model.trim().slice(0, 180) || DEFAULT_VOICE_SETTINGS.model,
    cloudVoice: settings.cloudVoice.trim().slice(0, 100) || DEFAULT_VOICE_SETTINGS.cloudVoice,
    rate: Math.min(1.4, Math.max(.6, Math.round(settings.rate * 20) / 20)),
  };
  localStorage.setItem(VOICE_SETTINGS_KEY, JSON.stringify(normalized));
}

export function loadProfile(): OnyxProfile {
  const fallback: OnyxProfile = {
    firstName: "",
    lastName: "",
    email: "",
    language: "it",
    authMode: "preview",
  };
  try {
    const value = JSON.parse(localStorage.getItem(PROFILE_KEY) ?? "null") as Partial<OnyxProfile> | null;
    if (!value) return fallback;
    return {
      firstName: typeof value.firstName === "string" ? value.firstName.slice(0, 80) : "",
      lastName: typeof value.lastName === "string" ? value.lastName.slice(0, 80) : "",
      email: typeof value.email === "string" ? value.email.slice(0, 180) : "",
      language: typeof value.language === "string" ? value.language.slice(0, 12) : "it",
      authMode: value.authMode === "clerk" ? "clerk" : "preview",
    };
  } catch {
    return fallback;
  }
}

export function saveProfile(profile: OnyxProfile): void {
  localStorage.setItem(PROFILE_KEY, JSON.stringify(profile));
}

export function loadJourneyStage(): JourneyStage {
  const value = localStorage.getItem(JOURNEY_KEY);
  return value === "onboarding" || value === "app" ? value : "auth";
}

export function saveJourneyStage(stage: JourneyStage): void {
  localStorage.setItem(JOURNEY_KEY, stage);
}

export function resetJourney(): void {
  localStorage.removeItem(PROFILE_KEY);
  localStorage.removeItem(JOURNEY_KEY);
}

export function loadRoutes(): CapabilityRoute[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(ROUTES_KEY) ?? "null") as CapabilityRoute[] | null;
    if (!Array.isArray(parsed)) return structuredClone(DEFAULT_ROUTES);
    const byCapability = new Map<CapabilityId, CapabilityRoute>();
    for (const route of parsed) {
      if (!CAPABILITIES.has(route?.capability) || !validSelection(route.primary)) continue;
      byCapability.set(route.capability, {
        capability: route.capability,
        primary: { ...route.primary },
        fallbacks: Array.isArray(route.fallbacks)
          ? route.fallbacks.filter(validSelection).slice(0, 3).map((item) => ({ ...item }))
          : [],
      });
    }
    return DEFAULT_ROUTES.map((fallback) => byCapability.get(fallback.capability) ?? structuredClone(fallback));
  } catch {
    return structuredClone(DEFAULT_ROUTES);
  }
}

function validSelection(value: unknown): value is { provider: ProviderId; model: string } {
  if (!value || typeof value !== "object") return false;
  const selection = value as { provider?: unknown; model?: unknown };
  return typeof selection.provider === "string"
    && PROVIDERS.has(selection.provider as ProviderId)
    && validModel(selection.model);
}

export function saveRoutes(routes: CapabilityRoute[]): void {
  localStorage.setItem(ROUTES_KEY, JSON.stringify(routes));
}

export function loadSearchHistory(): SearchHistoryItem[] {
  try {
    const value = JSON.parse(localStorage.getItem(SEARCH_HISTORY_KEY) ?? "[]") as SearchHistoryItem[];
    return Array.isArray(value) ? value.filter((item) => item && typeof item.query === "string").slice(0, 100) : [];
  } catch {
    return [];
  }
}

export function appendSearchHistory(item: SearchHistoryItem): void {
  const next = [item, ...loadSearchHistory().filter((entry) => entry.id !== item.id)].slice(0, 100);
  localStorage.setItem(SEARCH_HISTORY_KEY, JSON.stringify(next));
}

export function clearSearchHistory(): void {
  localStorage.removeItem(SEARCH_HISTORY_KEY);
}

export function setPreviewConnection(provider: ProviderId, connected: boolean): void {
  const current = getPreviewConnections();
  current[provider] = connected;
  localStorage.setItem(PREVIEW_CONNECTIONS_KEY, JSON.stringify(current));
}

export function getPreviewConnection(provider: ProviderId): boolean {
  if (provider === "local") return true;
  return Boolean(getPreviewConnections()[provider]);
}

function getPreviewConnections(): Partial<Record<ProviderId, boolean>> {
  try {
    const value = JSON.parse(localStorage.getItem(PREVIEW_CONNECTIONS_KEY) ?? "{}") as Partial<Record<ProviderId, boolean>>;
    return value && typeof value === "object" ? value : {};
  } catch {
    return {};
  }
}

export function reportDictationError(message: string): void {
  localStorage.setItem(LAST_ERROR_KEY, message.slice(0, 1_000));
}

export function consumeDictationError(): string | null {
  const message = localStorage.getItem(LAST_ERROR_KEY);
  localStorage.removeItem(LAST_ERROR_KEY);
  return message;
}

export const storageKeys = {
  settings: SETTINGS_KEY,
  legacyHistory: LEGACY_HISTORY_KEY,
  lastError: LAST_ERROR_KEY,
  profile: PROFILE_KEY,
  journey: JOURNEY_KEY,
  routes: ROUTES_KEY,
  searchHistory: SEARCH_HISTORY_KEY,
  voiceSettings: VOICE_SETTINGS_KEY,
} as const;
