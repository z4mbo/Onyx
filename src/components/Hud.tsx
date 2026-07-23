import { createSignal, For, onCleanup, onMount, type Component } from "solid-js"
import { listen } from "@tauri-apps/api/event"
import { api } from "../lib/api"
import { SpeechCapture } from "../lib/audio"
import { appendVoiceHistory } from "../lib/voice-history"
import type { ActiveAppContext, HoldPayload } from "../lib/types"
import { OnyxOrb } from "./OnyxOrb"
import "./overlay.css"

export const Hud: Component = () => {
  const [phase, setPhase] = createSignal("Ready")
  const [level, setLevel] = createSignal(0)
  const [app, setApp] = createSignal<ActiveAppContext>({ name: "Active app", process: "", accent: "#7165e8", symbol: "O" })
  const capture = new SpeechCapture()
  let starting = false
  let finishing = false
  let pendingStop = false

  const start = async () => {
    if (starting || finishing || capture.isRecording) return
    starting = true
    pendingStop = false
    setPhase("Starting microphone")
    let shouldFinish = false
    try {
      setApp(await api.activeAppContext())
      await capture.start(setLevel)
      setPhase("Listening")
      shouldFinish = pendingStop
    } catch (error) {
      setPhase(error instanceof Error ? error.message : String(error))
      window.setTimeout(() => void api.hideWindow("hud"), 3500)
    } finally {
      starting = false
    }
    if (shouldFinish) await finish()
  }

  const finish = async () => {
    if (starting) { pendingStop = true; return }
    if (finishing || !capture.isRecording) return
    finishing = true
    setPhase("Transcribing")
    try {
      const audio = await capture.stop()
      const result = await api.transcribeAudio(audio.audioBase64, audio.format)
      await api.injectText(result.text)
      appendVoiceHistory({ id: crypto.randomUUID(), createdAt: new Date().toISOString(), kind: "dictation", text: result.text, appName: app().name, model: result.model })
      setPhase("Inserted")
      window.setTimeout(() => void api.hideWindow("hud"), 650)
    } catch (error) {
      setPhase(error instanceof Error ? error.message : String(error))
      window.setTimeout(() => void api.hideWindow("hud"), 3500)
    } finally {
      finishing = false
    }
  }

  onMount(() => {
    let unlisten: () => void = () => undefined
    void listen<HoldPayload>("onyx://hold", (event) => {
      if (event.payload.mode !== "dictation") return
      if (event.payload.phase === "pressed") void start()
      else void finish()
    }).then((dispose) => { unlisten = dispose })
    onCleanup(() => { unlisten(); capture.cancel() })
  })

  return <main class="onyx-hud" style={{ "--app-accent": app().accent }} title={`${phase()} · ${app().name}`}><OnyxOrb class="onyx-hud__app" /><i /><div class="onyx-hud__wave"><For each={[.35,.6,.85,1,.78,.5,.3]}>{(weight) => <b style={{ height: `${4 + level() * 23 * weight}px` }} />}</For></div><span class="onyx-hud__phase">{phase()}</span></main>
}
