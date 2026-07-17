import { beforeEach, describe, expect, it, vi } from "vitest";

import { DEFAULT_ROUTES, DEFAULT_SETTINGS, DEFAULT_VOICE_SETTINGS } from "../types";
import {
  consumeDictationError,
  loadSettings,
  loadRoutes,
  loadVoiceSettings,
  reportDictationError,
  saveSettings,
  saveRoutes,
  saveVoiceSettings,
  storageKeys,
} from "./storage";

const values = new Map<string, string>();

beforeEach(() => {
  values.clear();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
    removeItem: (key: string) => values.delete(key),
  });
});

describe("dictation-only settings", () => {
  it("defaults to OpenRouter Whisper large-v3", () => {
    expect(loadSettings()).toEqual(DEFAULT_SETTINGS);
  });

  it("removes legacy chat, Agent, and local STT fields", () => {
    values.set(storageKeys.legacyHistory, "[{}]");
    values.set(storageKeys.settings, JSON.stringify({
      assistantProvider: "codex",
      agentShortcut: "Super+Shift+X",
      chatModel: "old",
      cloudProvider: "legacy",
      cloudModel: "legacy-model",
      localSttModel: "large-v3",
    }));

    const settings = loadSettings();
    saveSettings(settings);

    expect(values.has(storageKeys.legacyHistory)).toBe(false);
    expect(values.get(storageKeys.settings)).not.toContain("agentShortcut");
    expect(values.get(storageKeys.settings)).not.toContain("assistantProvider");
    expect(values.get(storageKeys.settings)).not.toContain("cloudProvider");
    expect(values.get(storageKeys.settings)).not.toContain("cloudModel");
    expect(values.get(storageKeys.settings)).not.toContain("localSttModel");
  });

  it("migrates the previous cloud model selection", () => {
    values.set(storageKeys.settings, JSON.stringify({ cloudModel: "openai/whisper-1" }));
    expect(loadSettings().sttModel).toBe("openai/whisper-1");
  });

  it("maps the previous local Turbo selection to its OpenRouter model", () => {
    values.set(storageKeys.settings, JSON.stringify({ localSttModel: "large-v3-turbo" }));
    expect(loadSettings().sttModel).toBe("openai/whisper-large-v3-turbo");
  });

  it("ignores invalid legacy model values", () => {
    values.set(storageKeys.settings, JSON.stringify({ cloudModel: "legacy-model" }));
    expect(loadSettings().sttModel).toBe(DEFAULT_SETTINGS.sttModel);
  });

  it("round-trips the remaining dictation settings", () => {
    const settings = {
      ...DEFAULT_SETTINGS,
      sttModel: "openai/whisper-1",
      overlayPosition: "bottom_center" as const,
      overlayMargin: 34,
    };
    saveSettings(settings);
    expect(loadSettings()).toEqual(settings);
  });

  it("surfaces a dictation error only once", () => {
    reportDictationError("Microfono non disponibile");
    expect(consumeDictationError()).toBe("Microfono non disponibile");
    expect(consumeDictationError()).toBeNull();
  });

  it("persists the selected system voice and playback speed", () => {
    saveVoiceSettings({ ...DEFAULT_VOICE_SETTINGS, voiceId: "Microsoft Elsa", rate: 1.15 });
    expect(loadVoiceSettings()).toEqual({ ...DEFAULT_VOICE_SETTINGS, voiceId: "Microsoft Elsa", rate: 1.15 });
  });

  it("uses safe defaults and clamps invalid voice speeds", () => {
    expect(loadVoiceSettings()).toEqual(DEFAULT_VOICE_SETTINGS);
    saveVoiceSettings({ ...DEFAULT_VOICE_SETTINGS, rate: 4 });
    expect(loadVoiceSettings().rate).toBe(1.4);
  });

  it("persists an OpenRouter TTS model and cloud voice", () => {
    saveVoiceSettings({
      ...DEFAULT_VOICE_SETTINGS,
      provider: "openrouter",
      model: "openai/gpt-4o-mini-tts-2025-12-15",
      cloudVoice: "coral",
      rate: .9,
    });
    expect(loadVoiceSettings()).toMatchObject({
      provider: "openrouter",
      model: "openai/gpt-4o-mini-tts-2025-12-15",
      cloudVoice: "coral",
      rate: .9,
    });
  });

  it("accepts ChatGPT/Codex as a persisted web-search provider", () => {
    const routes = DEFAULT_ROUTES.map((route) => route.capability === "web_search"
      ? { ...route, primary: { provider: "chatgpt_codex" as const, model: "codex/default" } }
      : route);
    saveRoutes(routes);
    expect(loadRoutes().find((route) => route.capability === "web_search")?.primary).toEqual({
      provider: "chatgpt_codex",
      model: "codex/default",
    });
  });
});
