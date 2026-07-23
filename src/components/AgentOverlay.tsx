import { createEffect, createSignal, For, onCleanup, onMount, Show, type Component } from "solid-js"
import { listen } from "@tauri-apps/api/event"
import { ArrowUp, Mic, Square, X } from "lucide-solid"
import { api } from "../lib/api"
import { SpeechCapture, withDeadline } from "../lib/audio"
import { appendVoiceHistory } from "../lib/voice-history"
import { applyDocumentTheme } from "../lib/theme"
import type { HoldPayload } from "../lib/types"
import { MarkdownContent } from "./Transcript"
import { OnyxOrb } from "./OnyxOrb"
import "./overlay.css"

type OverlayMode = "inactive" | "listening" | "expanded"
type IslandMessage = { id: string; role: "user" | "assistant"; content: string }

export const AgentOverlay: Component = () => {
  const [mode, setMode] = createSignal<OverlayMode>("inactive")
  const [phase, setPhase] = createSignal("Ask Onyx")
  const [level, setLevel] = createSignal(0)
  const [draft, setDraft] = createSignal("")
  const [messages, setMessages] = createSignal<IslandMessage[]>([])
  const [busy, setBusy] = createSignal(false)
  const [recording, setRecording] = createSignal(false)
  const capture = new SpeechCapture()
  let conversation: HTMLDivElement | undefined
  let starting = false
  let finishing = false
  let pendingStop = false
  let leaveTimer: number | undefined

  const changeMode = async (next: OverlayMode) => {
    setMode(next)
    await api.setAgentOverlayMode(next).catch(() => undefined)
  }

  const expand = async () => {
    if (mode() !== "inactive") return
    if (leaveTimer !== undefined) window.clearTimeout(leaveTimer)
    applyDocumentTheme()
    await changeMode("expanded")
  }

  const collapse = async () => {
    if (leaveTimer !== undefined) window.clearTimeout(leaveTimer)
    capture.cancel()
    setRecording(false)
    setLevel(0)
    setPhase("Ask Onyx")
    await changeMode("inactive")
  }

  const appendError = (error: unknown) => {
    const content = error instanceof Error ? error.message : String(error)
    setMessages((current) => [
      ...current,
      { id: crypto.randomUUID(), role: "assistant", content },
    ])
  }

  const ask = async (text: string, source: "voice" | "typed") => {
    const prompt = text.trim()
    if (!prompt || busy()) return
    const userMessage: IslandMessage = {
      id: crypto.randomUUID(),
      role: "user",
      content: prompt,
    }
    const history = [...messages(), userMessage].slice(-20)
    setMessages(history)
    setDraft("")
    setPhase("Thinking")
    setBusy(true)
    await changeMode("expanded")
    try {
      const settings = await api.getVoiceSettings()
      const active = await api.activeAppContext()
      const needsWeb = /\b(search|look up|latest|today|news|weather|web|online|source|price)\b/i.test(prompt)
      const route = needsWeb
        ? { provider: settings.webProvider, model: settings.webModel, label: "web" }
        : { provider: settings.agentProvider, model: settings.agentModel, label: "general" }
      const requestMessages = history.map((message, index) => ({
        role: message.role,
        content: index === history.length - 1
          ? `The active application is ${active.name}. This is a ${route.label} ${source} request. Answer in concise Markdown and only use read-only tools when needed:\n\n${message.content}`
          : message.content,
      }))
      const reply = await api.chatSend({
        provider: route.provider,
        model: route.model,
        webSearch: needsWeb,
        messages: requestMessages,
      })
      setMessages((current) => [
        ...current,
        { id: crypto.randomUUID(), role: "assistant", content: reply.content },
      ])
      setPhase("Ready")
      appendVoiceHistory({
        id: crypto.randomUUID(),
        createdAt: new Date().toISOString(),
        kind: "agent",
        text: prompt,
        answer: reply.content,
        appName: active.name,
        model: reply.model,
      })
      if (settings.speakResponses && source === "voice") {
        void api.speakText(reply.content)
          .then((audioSource) => audioSource ? new Audio(audioSource).play() : undefined)
          .catch(() => undefined)
      }
    } catch (error) {
      setPhase("Couldn’t answer")
      appendError(error)
    } finally {
      setBusy(false)
    }
  }

  const start = async (fromPanel = false) => {
    if (starting || finishing || capture.isRecording) return
    applyDocumentTheme()
    starting = true
    pendingStop = false
    setPhase("Starting microphone")
    if (!fromPanel) await changeMode("listening")
    let shouldFinish = false
    try {
      await capture.start(setLevel)
      setRecording(true)
      setPhase("Listening")
      shouldFinish = pendingStop
    } catch (error) {
      await changeMode("expanded")
      setPhase("Microphone unavailable")
      appendError(error)
    } finally {
      starting = false
    }
    if (shouldFinish) await finish()
  }

  const finish = async () => {
    if (starting) {
      pendingStop = true
      return
    }
    if (finishing || !capture.isRecording) return
    finishing = true
    setPhase("Transcribing")
    try {
      const audio = await capture.stop()
      setRecording(false)
      const result = await withDeadline(api.transcribeAudio(audio.audioBase64, audio.format), 75, "Transcription")
      await ask(result.text, "voice")
    } catch (error) {
      await changeMode("expanded")
      setPhase("Something went wrong")
      appendError(error)
    } finally {
      finishing = false
      setRecording(false)
      setLevel(0)
    }
  }

  const submit = (event: SubmitEvent) => {
    event.preventDefault()
    void ask(draft(), "typed")
  }

  const composerKeyDown = (event: KeyboardEvent) => {
    if (event.key !== "Enter" || event.shiftKey) return
    event.preventDefault()
    void ask(draft(), "typed")
  }

  const pointerLeft = () => {
    if (mode() !== "expanded" || document.hasFocus()) return
    leaveTimer = window.setTimeout(() => void collapse(), 180)
  }

  createEffect(() => {
    messages()
    busy()
    requestAnimationFrame(() => {
      if (conversation) conversation.scrollTop = conversation.scrollHeight
    })
  })

  onMount(() => {
    applyDocumentTheme()
    let unlisten: () => void = () => undefined
    const blurred = () => {
      if (mode() === "expanded") void collapse()
    }
    const keyed = (event: KeyboardEvent) => {
      if (event.key === "Escape" && mode() !== "inactive") void collapse()
    }
    window.addEventListener("blur", blurred)
    window.addEventListener("keydown", keyed)
    void listen<HoldPayload>("onyx://hold", (event) => {
      if (event.payload.mode !== "agent") return
      if (event.payload.phase === "pressed") void start()
      else void finish()
    }).then((dispose) => { unlisten = dispose })
    onCleanup(() => {
      unlisten()
      capture.cancel()
      if (leaveTimer !== undefined) window.clearTimeout(leaveTimer)
      window.removeEventListener("blur", blurred)
      window.removeEventListener("keydown", keyed)
    })
  })

  return (
    <main
      class="onyx-agent"
      data-state={mode()}
      onMouseEnter={() => void expand()}
      onMouseLeave={pointerLeft}
    >
      <Show when={mode() === "inactive"}>
        <div class="onyx-agent__hotspot" aria-label="Open Onyx Agent"><i /></div>
      </Show>

      <Show when={mode() === "listening"}>
        <section class="onyx-agent__listening">
          <OnyxOrb class="onyx-agent__orb" />
          <div class="onyx-agent__wave" aria-hidden="true">
            <For each={[.35, .62, 1, .74, .44]}>{(weight) => (
              <i style={{ height: `${4 + level() * 18 * weight}px` }} />
            )}</For>
          </div>
          <span>{phase()}</span>
        </section>
      </Show>

      <Show when={mode() === "expanded"}>
        <section class="onyx-agent__panel">
          <header class="onyx-agent__header">
            <OnyxOrb class="onyx-agent__orb" />
            <div><strong>Onyx Agent</strong><span>{phase()}</span></div>
            <button onClick={() => void collapse()} aria-label="Close"><X size={15} /></button>
          </header>

          <div ref={conversation} class="onyx-agent__conversation" aria-live="polite">
            <Show
              when={messages().length}
              fallback={
                <div class="onyx-agent__empty">
                  <strong>What can I help with?</strong>
                  <span>Type below, tap the mic, or hold Control + Option.</span>
                </div>
              }
            >
              <For each={messages()}>{(message) => (
                <article class="onyx-agent__message" data-role={message.role}>
                  <Show
                    when={message.role === "assistant"}
                    fallback={<p>{message.content}</p>}
                  >
                    <MarkdownContent content={message.content} />
                  </Show>
                </article>
              )}</For>
            </Show>
            <Show when={busy()}>
              <div class="onyx-agent__typing" aria-label="Onyx is thinking"><i /><i /><i /></div>
            </Show>
          </div>

          <form class="onyx-agent__composer" onSubmit={submit}>
            <textarea
              aria-label="Message Onyx"
              rows={1}
              placeholder="Ask or search anything…"
              value={draft()}
              onInput={(event) => setDraft(event.currentTarget.value)}
              onKeyDown={composerKeyDown}
            />
            <button
              type="button"
              class="onyx-agent__voice"
              aria-label={recording() ? "Stop recording" : "Ask with voice"}
              onClick={() => void (recording() ? finish() : start(true))}
            >
              {recording() ? <Square size={13} /> : <Mic size={15} />}
            </button>
            <button
              type="submit"
              class="onyx-agent__send"
              aria-label="Send"
              disabled={busy() || !draft().trim()}
            >
              <ArrowUp size={15} />
            </button>
          </form>
        </section>
      </Show>
    </main>
  )
}
