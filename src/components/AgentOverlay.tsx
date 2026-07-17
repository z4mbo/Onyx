import { useEffect, useMemo, useRef, useState } from "react";

import type { SearchReply } from "../types";
import {
  applyBackendSettings,
  errorMessage,
  hideWindow,
  isTauri,
  onHold,
  openExternal,
  saveTtsConfig,
  searchWeb,
  setAgentExpanded,
  speakTts,
  transcribe,
} from "../lib/api";
import { SpeechCapture } from "../lib/audio";
import { appendSearchHistory, loadRoutes, loadSettings, loadVoiceSettings } from "../lib/storage";
import { speakText, stopSpeech, toBackendTtsConfig } from "../lib/voice";
import { emptyWaveHistory, pushWaveLevel, waveBarHeights } from "../lib/waveform";

type AgentPhase = "idle" | "starting" | "listening" | "transcribing" | "searching" | "answer" | "error";
type RunState = "idle" | "starting" | "recording" | "processing";

const PHASE_COPY: Record<AgentPhase, string> = {
  idle: "Tieni premuto Ctrl + Alt",
  starting: "Attivo il microfono…",
  listening: "Ti ascolto",
  transcribing: "Trascrivo la richiesta…",
  searching: "Cerco e confronto le fonti…",
  answer: "Ricerca completata",
  error: "Qualcosa non ha funzionato",
};

export function AgentOverlay() {
  const [phase, setPhase] = useState<AgentPhase>("idle");
  const [query, setQuery] = useState("");
  const [draft, setDraft] = useState("");
  const [reply, setReply] = useState<SearchReply | null>(null);
  const [error, setError] = useState("");
  const [waveHistory, setWaveHistory] = useState<number[]>(emptyWaveHistory);
  const capture = useRef(new SpeechCapture());
  const stateRef = useRef<RunState>("idle");
  const pendingStop = useRef(false);
  const runToken = useRef(0);
  const draftRef = useRef("");
  const phaseRef = useRef<AgentPhase>("idle");
  const heights = waveBarHeights(waveHistory);
  const expanded = phase === "transcribing" || phase === "searching" || phase === "answer" || phase === "error";

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let timeout: number | undefined;

    async function begin() {
      if (stateRef.current !== "idle") return;
      const token = ++runToken.current;
      stopSpeech();
      stateRef.current = "starting";
      pendingStop.current = false;
      setError("");
      setReply(null);
      setQuery("");
      setWaveHistory(emptyWaveHistory());
      setPhase("starting");
      await setAgentExpanded(false).catch(() => undefined);
      if (disposed || token !== runToken.current) return;
      try {
        const persisted = loadSettings();
        const routes = loadRoutes();
        const stt = routes.find((route) => route.capability === "stt")?.primary;
        const agent = routes.find((route) => route.capability === "web_search")?.primary;
        await applyBackendSettings({
          ...persisted,
          sttProvider: stt?.provider ?? persisted.sttProvider,
          sttModel: stt?.model ?? persisted.sttModel,
          agentProvider: agent?.provider ?? persisted.agentProvider,
          agentModel: agent?.model ?? persisted.agentModel,
        });
        if (disposed || token !== runToken.current) return;
        await capture.current.start((level) => {
          if (!disposed && token === runToken.current) setWaveHistory((history) => pushWaveLevel(history, level));
        });
        if (disposed || token !== runToken.current) {
          capture.current.cancel();
          return;
        }
        stateRef.current = "recording";
        setPhase("listening");
        timeout = window.setTimeout(() => void finish(), 90_000);
        if (pendingStop.current) void finish();
      } catch (cause) {
        if (!disposed && token === runToken.current) fail(cause);
      }
    }

    async function finish() {
      if (stateRef.current === "starting") {
        pendingStop.current = true;
        return;
      }
      if (stateRef.current !== "recording" || !capture.current.isRecording) return;
      const token = runToken.current;
      stateRef.current = "processing";
      pendingStop.current = false;
      if (timeout) window.clearTimeout(timeout);
      setWaveHistory(emptyWaveHistory());
      setPhase("transcribing");
      await setAgentExpanded(true).catch(() => undefined);
      if (disposed || token !== runToken.current) return;
      try {
        const audio = await capture.current.stop();
        if (disposed || token !== runToken.current) return;
        const transcription = await transcribe(audio.audioBase64, audio.format);
        if (disposed || token !== runToken.current) return;
        const spokenQuery = transcription.text.trim();
        setQuery(spokenQuery);
        setDraft(spokenQuery);
        await runSearch(spokenQuery, token);
      } catch (cause) {
        if (!disposed && token === runToken.current) fail(cause);
      }
    }

    async function runSearch(text: string, token: number) {
      const settings = loadSettings();
      const route = loadRoutes().find((item) => item.capability === "web_search")?.primary;
      setPhase("searching");
      const result = await searchWeb({
        query: text,
        provider: route?.provider ?? settings.agentProvider,
        model: route?.model ?? settings.agentModel,
        reasoning: settings.reasoning,
      });
      if (disposed || token !== runToken.current) return;
      setReply(result);
      setPhase("answer");
      stateRef.current = "idle";
      appendSearchHistory({
        id: globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`,
        createdAt: new Date().toISOString(),
        query: text,
        answer: result.answer,
        model: result.model,
        sources: result.sources,
        usage: result.usage,
      });
      if (settings.speakResponses) {
        try {
          const voice = loadVoiceSettings();
          if (!isTauri) {
            speakText(result.answer, voice, settings.language, settings.voicePreset);
          } else {
            await saveTtsConfig(toBackendTtsConfig(voice));
            const spoken = await speakTts(result.answer);
            if (spoken.warning) console.warn("Onyx TTS:", spoken.warning);
          }
        } catch (voiceError) {
          console.warn("Onyx TTS non disponibile:", errorMessage(voiceError));
        }
      }
    }

    function fail(cause: unknown) {
      capture.current.cancel();
      stateRef.current = "idle";
      pendingStop.current = false;
      setWaveHistory(emptyWaveHistory());
      setError(errorMessage(cause));
      setPhase("error");
      void setAgentExpanded(true);
    }

    async function submitTyped() {
      const text = draftRef.current.trim();
      if (!text || phaseRef.current === "searching" || phaseRef.current === "transcribing") return;
      const token = ++runToken.current;
      stopSpeech();
      setQuery(text);
      setReply(null);
      setError("");
      stateRef.current = "processing";
      try {
        await runSearch(text, token);
      } catch (cause) {
        if (!disposed && token === runToken.current) fail(cause);
      }
    }

    // Expose typed search to the form without creating a second event listener.
    typedSearchRef.current = submitTyped;

    void onHold((event) => {
      if (event.mode !== "agent") return;
      if (event.phase === "pressed") void begin();
      else void finish();
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });

    return () => {
      disposed = true;
      runToken.current += 1;
      if (timeout) window.clearTimeout(timeout);
      unlisten?.();
      capture.current.cancel();
      stopSpeech();
    };
  }, []);

  const typedSearchRef = useRef<() => Promise<void>>(async () => undefined);
  useEffect(() => { draftRef.current = draft; }, [draft]);
  useEffect(() => { phaseRef.current = phase; }, [phase]);
  const sourceCount = reply?.sources.length ?? 0;
  const usageLabel = useMemo(() => {
    if (!reply) return "";
    const total = (reply.usage.inputTokens ?? 0) + (reply.usage.outputTokens ?? 0);
    return total > 0 ? `${total.toLocaleString("it-IT")} token` : reply.model;
  }, [reply]);

  async function close() {
    runToken.current += 1;
    stopSpeech();
    capture.current.cancel();
    stateRef.current = "idle";
    setPhase("idle");
    setReply(null);
    setError("");
    await setAgentExpanded(false).catch(() => undefined);
    if (isTauri) await hideWindow("agent").catch(() => undefined);
  }

  function previewPress() {
    if (isTauri || phase !== "idle") return;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Control" }));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Alt" }));
  }

  function previewRelease() {
    if (isTauri) return;
    window.dispatchEvent(new KeyboardEvent("keyup", { key: "Alt" }));
    window.dispatchEvent(new KeyboardEvent("keyup", { key: "Control" }));
  }

  return (
    <main className="agent-shell" data-phase={phase} data-expanded={expanded ? "true" : "false"}>
      <section
        className="agent-island"
        aria-live="polite"
        onPointerDown={previewPress}
        onPointerUp={previewRelease}
        onPointerCancel={previewRelease}
      >
        <span className="agent-orb" aria-hidden="true"><i /><i /></span>
        <div className="agent-island-copy">
          <strong>{phase === "answer" ? "Onyx" : PHASE_COPY[phase]}</strong>
          {phase === "listening" || phase === "starting" ? (
            <div className="agent-mini-wave" aria-hidden="true">
              {heights.slice(2, 11).map((height, index) => <i key={index} style={{ height: `${Math.max(2, height * .62)}px` }} />)}
            </div>
          ) : (
            <span>{query || (isTauri ? "Assistente di ricerca" : "Tieni premuto o clicca")}</span>
          )}
        </div>
        {expanded && (
          <button className="agent-close" type="button" aria-label="Chiudi" onClick={(event) => { event.stopPropagation(); void close(); }}>×</button>
        )}
      </section>

      {expanded && (
        <section className="agent-result-card" onPointerDown={(event) => event.stopPropagation()}>
          {query && <p className="agent-query">{query}</p>}

          {(phase === "transcribing" || phase === "searching") && (
            <div className="agent-worklog" role="status">
              <WorkMessage done={phase === "searching"}>Audio ricevuto</WorkMessage>
              <WorkMessage active={phase === "transcribing"} done={phase === "searching"}>Trascrivo la richiesta</WorkMessage>
              <WorkMessage active={phase === "searching"}>Cerco e verifico le fonti web</WorkMessage>
            </div>
          )}

          {phase === "error" && (
            <div className="agent-error" role="alert">
              <strong>Non riesco a completare la ricerca</strong>
              <p>{error}</p>
            </div>
          )}

          {reply && (
            <div className="agent-answer">
              <AnswerText text={reply.answer} />
              <div className="agent-answer-meta">
                <span>{reply.model}</span>
                {usageLabel && <span>{usageLabel}</span>}
              </div>
              {sourceCount > 0 && (
                <div className="agent-sources">
                  <div className="agent-sources-title"><span>Fonti</span><b>{sourceCount}</b></div>
                  {reply.sources.map((source, index) => (
                    <button type="button" className="source-card" key={`${source.url}-${index}`} onClick={() => void openExternal(source.url)}>
                      <span className="source-index">{index + 1}</span>
                      <span className="source-copy"><strong>{source.title}</strong><small>{domainOf(source.url)}</small></span>
                      <span aria-hidden="true">↗</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}

          <form className="agent-compose" onSubmit={(event) => { event.preventDefault(); void typedSearchRef.current(); }}>
            <input
              value={draft}
              onChange={(event) => { draftRef.current = event.target.value; setDraft(event.target.value); }}
              placeholder="Chiedi altro a Onyx…"
              aria-label="Domanda per Onyx"
              disabled={phase === "transcribing" || phase === "searching"}
            />
            <button type="submit" aria-label="Invia" disabled={!draft.trim() || phase === "transcribing" || phase === "searching"}>↑</button>
          </form>
          <p className="agent-hint">Tieni premuto <kbd>Ctrl</kbd> <span>+</span> <kbd>Alt</kbd> per una nuova domanda</p>
        </section>
      )}
    </main>
  );
}

function WorkMessage({ children, active = false, done = false }: { children: string; active?: boolean; done?: boolean }) {
  return (
    <div className={`work-message ${active ? "is-active" : ""} ${done ? "is-done" : ""}`}>
      <i aria-hidden="true">{done ? "✓" : ""}</i><span>{children}</span>
    </div>
  );
}

function AnswerText({ text }: { text: string }) {
  return (
    <div className="answer-text">
      {text.split(/\n{2,}/).filter(Boolean).map((paragraph, index) => (
        <p key={index}>{paragraph.replace(/^#+\s*/g, "")}</p>
      ))}
    </div>
  );
}

function domainOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}
