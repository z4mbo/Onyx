import { createSignal, For, onCleanup, onMount, Show, type Component } from "solid-js"
import { listen } from "@tauri-apps/api/event"
import { api } from "../lib/api"
import { SpeechCapture } from "../lib/audio"
import { appendVoiceHistory } from "../lib/voice-history"
import type { HoldPayload } from "../lib/types"
import { OnyxOrb } from "./OnyxOrb"
import "./overlay.css"

export const AgentOverlay: Component = () => {
  const [phase, setPhase] = createSignal("Hold Ctrl + Alt")
  const [level, setLevel] = createSignal(0)
  const [question, setQuestion] = createSignal("")
  const [answer, setAnswer] = createSignal("")
  const [expanded, setExpanded] = createSignal(false)
  const capture = new SpeechCapture()
  let starting = false
  let pendingStop = false

  const ask = async (text: string) => {
    const settings = await api.getVoiceSettings()
    const active = await api.activeAppContext()
    const needsWeb = /\b(search|look up|latest|today|news|weather|web|online|source|price)\b/i.test(text)
    const route = needsWeb
      ? { provider: settings.webProvider, model: settings.webModel, label: "web" }
      : { provider: settings.agentProvider, model: settings.agentModel, label: "general" }
    setPhase("Thinking")
    setExpanded(true)
    await api.setAgentExpanded(true)
    const reply = await api.chatSend({
      provider: route.provider,
      model: route.model,
      webSearch: needsWeb,
      messages: [{ role: "user", content: `The active application is ${active.name}. This is a ${route.label} voice request. Answer concisely and only use read-only tools when the request needs them:\n\n${text}` }],
    })
    setAnswer(reply.content)
    setPhase("Onyx")
    appendVoiceHistory({ id: crypto.randomUUID(), createdAt: new Date().toISOString(), kind: "agent", text, answer: reply.content, appName: active.name, model: reply.model })
    if (settings.speakResponses) {
      void api.speakText(reply.content)
        .then((source) => source ? new Audio(source).play() : undefined)
        .catch(() => undefined)
    }
  }

  const start = async () => {
    if (starting || capture.isRecording) return
    starting = true; pendingStop = false; setExpanded(false); setAnswer(""); setQuestion(""); setPhase("Starting microphone")
    await api.setAgentExpanded(false).catch(() => undefined)
    try { await capture.start(setLevel); setPhase("Listening"); if (pendingStop) await finish() }
    catch (error) { setPhase(error instanceof Error ? error.message : String(error)) }
    finally { starting = false }
  }

  const finish = async () => {
    if (starting) { pendingStop = true; return }
    if (!capture.isRecording) return
    setPhase("Transcribing")
    try { const audio = await capture.stop(); const result = await api.transcribeAudio(audio.audioBase64, audio.format); setQuestion(result.text); await ask(result.text) }
    catch (error) { setExpanded(true); await api.setAgentExpanded(true).catch(() => undefined); setPhase("Something went wrong"); setAnswer(error instanceof Error ? error.message : String(error)) }
  }

  const close = async () => { capture.cancel(); setExpanded(false); setAnswer(""); setPhase("Hold Ctrl + Alt"); await api.setAgentExpanded(false).catch(() => undefined); await api.hideWindow("agent") }

  onMount(() => {
    let unlisten: () => void = () => undefined
    void listen<HoldPayload>("onyx://hold", (event) => { if (event.payload.mode !== "agent") return; if (event.payload.phase === "pressed") void start(); else void finish() }).then((dispose) => { unlisten = dispose })
    onCleanup(() => { unlisten(); capture.cancel() })
  })

  return <main class="onyx-agent" data-expanded={expanded()}><section class="onyx-agent__island"><OnyxOrb class="onyx-agent__orb" /><div><strong>{phase()}</strong><Show when={question()} fallback={<div class="onyx-agent__wave"><For each={[.4,.7,1,.65,.35]}>{(weight) => <i style={{ height: `${3 + level() * 15 * weight}px` }} />}</For></div>}><small>{question()}</small></Show></div><Show when={expanded()}><button onClick={() => void close()} aria-label="Close">×</button></Show></section><Show when={expanded()}><section class="onyx-agent__result"><p>{answer()}</p><button onClick={() => void api.showMainWindow()}>Open Onyx</button></section></Show></main>
}
