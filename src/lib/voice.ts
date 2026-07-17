import type { SystemVoiceOption, TtsConfig, VoiceSettings } from "../types";

let activeAudioSource: AudioBufferSourceNode | null = null;
let activeAudioContext: AudioContext | null = null;

export interface SpeechCallbacks {
  onStart?: () => void;
  onEnd?: () => void;
  onError?: () => void;
}

export function toBackendTtsConfig(settings: VoiceSettings): TtsConfig {
  return {
    provider: settings.provider,
    model: settings.model,
    voice: settings.provider === "system"
      ? settings.voiceId || "default"
      : settings.cloudVoice,
    speed: settings.rate,
    instructions: "Parla in modo naturale, caldo e chiaro. Mantieni la lingua del testo.",
    fallbackToSystem: true,
    systemVoice: settings.voiceId || null,
  };
}

export function systemVoices(): SystemVoiceOption[] {
  if (typeof window === "undefined" || !("speechSynthesis" in window)) return [];
  return window.speechSynthesis.getVoices()
    .map((voice) => ({
      id: voice.voiceURI || `${voice.name}|${voice.lang}`,
      name: voice.name,
      language: voice.lang,
      local: voice.localService,
    }))
    .sort((left, right) => {
      if (left.local !== right.local) return left.local ? -1 : 1;
      const language = left.language.localeCompare(right.language);
      return language || left.name.localeCompare(right.name);
    });
}

export function stopSpeech(): void {
  if (typeof window !== "undefined" && "speechSynthesis" in window) {
    window.speechSynthesis.cancel();
  }
  try { activeAudioSource?.stop(); } catch { /* The source may already have ended. */ }
  activeAudioSource?.disconnect();
  activeAudioSource = null;
  const context = activeAudioContext;
  activeAudioContext = null;
  if (context && context.state !== "closed") void context.close().catch(() => undefined);
}

export async function playSynthesizedAudio(audioBase64: string, callbacks: SpeechCallbacks = {}): Promise<void> {
  const AudioContextConstructor = window.AudioContext
    ?? (window as typeof window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!AudioContextConstructor) throw new Error("Riproduzione audio non disponibile su questo sistema.");
  stopSpeech();
  const bytes = Uint8Array.from(atob(audioBase64), (character) => character.charCodeAt(0));
  const context = new AudioContextConstructor({ latencyHint: "interactive" });
  activeAudioContext = context;
  try {
    const decoded = await context.decodeAudioData(bytes.buffer.slice(0));
    const source = context.createBufferSource();
    source.buffer = decoded;
    source.connect(context.destination);
    activeAudioSource = source;
    source.onended = () => {
      if (activeAudioSource === source) activeAudioSource = null;
      if (activeAudioContext === context) activeAudioContext = null;
      void context.close().catch(() => undefined);
      callbacks.onEnd?.();
    };
    callbacks.onStart?.();
    source.start();
  } catch (error) {
    if (activeAudioContext === context) activeAudioContext = null;
    await context.close().catch(() => undefined);
    callbacks.onError?.();
    throw error;
  }
}

export function speakText(
  text: string,
  settings: VoiceSettings,
  language: string | null,
  preset = "sky",
  callbacks: SpeechCallbacks = {},
): boolean {
  if (settings.provider !== "system"
    || typeof window === "undefined"
    || !("speechSynthesis" in window)
    || typeof SpeechSynthesisUtterance === "undefined") {
    return false;
  }
  const clean = text
    .replace(/https?:\/\/\S+/g, "")
    .replace(/[*_#`>]/g, "")
    .slice(0, 3_000)
    .trim();
  if (!clean) return false;

  const utterance = new SpeechSynthesisUtterance(clean);
  utterance.lang = normalizedLanguage(language);
  utterance.rate = Math.min(1.4, Math.max(.6, settings.rate));
  utterance.pitch = ({ sky: 1.08, dawn: 1, dusk: .88, jarvis: .74 } as Record<string, number>)[preset] ?? 1;

  const voices = window.speechSynthesis.getVoices();
  const selected = settings.voiceId
    ? voices.find((voice) => (voice.voiceURI || `${voice.name}|${voice.lang}`) === settings.voiceId)
    : voices.find((voice) => voice.lang.toLowerCase().startsWith(utterance.lang.slice(0, 2).toLowerCase()));
  if (selected) utterance.voice = selected;

  utterance.onstart = () => callbacks.onStart?.();
  utterance.onend = () => callbacks.onEnd?.();
  utterance.onerror = () => callbacks.onError?.();
  window.speechSynthesis.cancel();
  window.speechSynthesis.speak(utterance);
  return true;
}

function normalizedLanguage(language: string | null): string {
  if (!language || language === "it") return "it-IT";
  if (language === "en") return "en-US";
  if (language === "es") return "es-ES";
  if (language === "fr") return "fr-FR";
  return language;
}
