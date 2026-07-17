import { useEffect, useRef, useState, type CSSProperties } from "react";

import type { ActiveAppContext, DictationPhase } from "../types";
import {
  applyBackendSettings,
  errorMessage,
  getActiveApp,
  hideWindow,
  injectText,
  isTauri,
  onHold,
  providerConnectionStatus,
  showMainWindow,
  transcribe,
} from "../lib/api";
import { SpeechCapture } from "../lib/audio";
import { loadRoutes, loadSettings, reportDictationError } from "../lib/storage";
import { emptyWaveHistory, pushWaveLevel, waveBarHeights } from "../lib/waveform";

type RunState = "idle" | "starting" | "recording" | "processing";

const EMPTY_APP: ActiveAppContext = {
  name: "App attiva",
  process: "active-app",
  accent: "#8f9bab",
  symbol: "•",
};

const PHASE_LABELS: Record<DictationPhase, string> = {
  idle: "pronto",
  starting: "attivazione microfono",
  listening: "in ascolto",
  transcribing: "trascrizione in corso",
  success: "testo inserito",
  error: "errore di dettatura",
};

export function Hud() {
  const [phase, setPhase] = useState<DictationPhase>("idle");
  const [waveHistory, setWaveHistory] = useState<number[]>(emptyWaveHistory);
  const [activeApp, setActiveApp] = useState<ActiveAppContext>(EMPTY_APP);
  const [errorDetail, setErrorDetail] = useState("");
  const capture = useRef(new SpeechCapture());
  const heights = waveBarHeights(waveHistory);

  useEffect(() => {
    let disposed = false;
    let runState: RunState = "idle";
    let pendingStop = false;
    let unlisten: (() => void) | undefined;
    let autoStopTimer: number | undefined;
    let hideTimer: number | undefined;
    let openSettingsTimer: number | undefined;

    function clearTimers() {
      if (autoStopTimer) window.clearTimeout(autoStopTimer);
      if (hideTimer) window.clearTimeout(hideTimer);
      if (openSettingsTimer) window.clearTimeout(openSettingsTimer);
      autoStopTimer = undefined;
      hideTimer = undefined;
      openSettingsTimer = undefined;
    }

    function scheduleHide(delay: number) {
      if (!isTauri) return;
      if (hideTimer) window.clearTimeout(hideTimer);
      hideTimer = window.setTimeout(() => {
        if (runState === "idle") void hideWindow("hud");
      }, delay);
    }

    function showFailure(error: unknown, openSettings = true) {
      const message = errorMessage(error);
      capture.current.cancel();
      runState = "idle";
      pendingStop = false;
      setWaveHistory(emptyWaveHistory());
      setErrorDetail(message);
      setPhase("error");
      reportDictationError(message);
      scheduleHide(4_000);
      if (isTauri && openSettings) {
        openSettingsTimer = window.setTimeout(() => void showMainWindow(), 750);
      }
    }

    async function start() {
      if (runState !== "idle") return;
      runState = "starting";
      pendingStop = false;
      clearTimers();
      setErrorDetail("");
      setWaveHistory(emptyWaveHistory());
      setPhase("starting");
      try {
        const persisted = loadSettings();
        const route = loadRoutes().find((item) => item.capability === "stt")?.primary;
        const routedSettings = {
          ...persisted,
          sttProvider: route?.provider ?? persisted.sttProvider,
          sttModel: route?.model ?? persisted.sttModel,
        };
        const [settings, app] = await Promise.all([
          applyBackendSettings(routedSettings),
          getActiveApp().catch(() => EMPTY_APP),
        ]);
        if (!disposed) setActiveApp(app);
        if (isTauri && (settings.sttProvider === "openrouter" || settings.sttProvider === "openai")) {
          const connected = await providerConnectionStatus(settings.sttProvider);
          if (!connected) throw new Error(`Collega ${settings.sttProvider === "openai" ? "OpenAI" : "OpenRouter"} dalla sezione Modelli.`);
        }
        await capture.current.start((level) => {
          if (!disposed) setWaveHistory((history) => pushWaveLevel(history, level));
        });
        if (disposed) {
          capture.current.cancel();
          return;
        }
        runState = "recording";
        setPhase("listening");
        autoStopTimer = window.setTimeout(() => void finish(), 90_000);
        if (pendingStop) void finish();
      } catch (error) {
        if (!disposed) showFailure(error);
      }
    }

    async function finish() {
      if (runState === "starting") {
        pendingStop = true;
        return;
      }
      if (runState !== "recording" || !capture.current.isRecording) return;
      runState = "processing";
      pendingStop = false;
      if (autoStopTimer) window.clearTimeout(autoStopTimer);
      autoStopTimer = undefined;
      setWaveHistory(emptyWaveHistory());
      setPhase("transcribing");
      try {
        const audio = await capture.current.stop();
        const result = await transcribe(audio.audioBase64, audio.format);
        if (disposed) return;
        await injectText(result.text);
        runState = "idle";
        setPhase("success");
        scheduleHide(720);
      } catch (error) {
        if (!disposed) showFailure(error);
      }
    }

    void onHold((event) => {
      if (event.mode !== "dictation") return;
      if (event.phase === "pressed") void start();
      else void finish();
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });

    return () => {
      disposed = true;
      clearTimers();
      unlisten?.();
      capture.current.cancel();
    };
  }, []);

  const ariaLabel = errorDetail
    ? `Onyx: ${PHASE_LABELS[phase]}. ${errorDetail}`
    : `Onyx: ${PHASE_LABELS[phase]}`;

  return (
    <main
      className="hud"
      data-phase={phase}
      aria-label={ariaLabel}
      aria-live="polite"
      aria-atomic="true"
      role="status"
      title={errorDetail || activeApp.name}
      style={{ "--app-accent": activeApp.accent } as CSSProperties}
    >
      <div className="voice-pill">
        <span className="active-app-badge" aria-hidden="true">{activeApp.symbol}</span>
        <span className="voice-divider" aria-hidden="true" />
        <div className="voice-wave" aria-hidden="true">
          {heights.map((height, index) => (
            <i
              className="voice-wave__bar"
              key={index}
              style={{
                "--wave-height": `${height}px`,
                "--wave-delay": `${index * 46 - 600}ms`,
              } as CSSProperties}
            />
          ))}
        </div>
        <span className="voice-state-dot" aria-hidden="true" />
      </div>
    </main>
  );
}
