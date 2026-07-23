import { createSignal, For, onCleanup, onMount, Show, type Component } from "solid-js"
import { Bot, Clipboard, Copy, Mic2, Trash2 } from "lucide-solid"
import { clearVoiceHistory, loadVoiceHistory } from "../lib/voice-history"

export const VoiceHistoryView: Component<{ onSettings: () => void }> = (props) => {
  const [items, setItems] = createSignal(loadVoiceHistory())
  onMount(() => {
    const refresh = () => setItems(loadVoiceHistory())
    window.addEventListener("onyx:voice-history", refresh)
    onCleanup(() => window.removeEventListener("onyx:voice-history", refresh))
  })
  return <section class="zai-page-frame onyx-voice-history"><header><div><h1>Voice history</h1><p>Your local clipboard for dictation and agent conversations.</p></div><div><button class="zai-neutral-button" onClick={props.onSettings}>Voice settings</button><button class="zai-neutral-button" disabled={!items().length} onClick={() => { clearVoiceHistory(); setItems([]) }}><Trash2 size={14} /> Clear</button></div></header><Show when={items().length} fallback={<div class="zai-home-empty"><Mic2 size={23} /><strong>No voice history yet</strong><span>Hold Ctrl+Shift to dictate or Ctrl+Alt to ask Onyx.</span></div>}><div class="onyx-voice-history__list"><For each={items()}>{(item) => <article><span>{item.kind === "dictation" ? <Clipboard size={16} /> : <Bot size={16} />}</span><div><small>{item.kind === "dictation" ? "Dictation" : "Agent"} · {new Date(item.createdAt).toLocaleString()} · {item.appName ?? "Active app"}</small><p>{item.text}</p><Show when={item.answer}><blockquote>{item.answer}</blockquote></Show></div><button aria-label="Copy" onClick={() => void navigator.clipboard.writeText(item.answer ?? item.text)}><Copy size={14} /></button></article>}</For></div></Show></section>
}
