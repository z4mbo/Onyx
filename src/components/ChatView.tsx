import { createMemo, createSignal, For, Show, type Component } from "solid-js"
import {
  ChevronDown,
  ExternalLink,
  Image as ImageIcon,
  LoaderCircle,
  Menu,
  MessageSquare,
  MoreHorizontal,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Search,
  Send,
  Star,
  Trash2,
  Video,
} from "lucide-solid"
import { api } from "../lib/api"
import { modelsForBrand, providerBrands, runtimeForBrand } from "../lib/providers"
import type {
  ChatMessage,
  ChatThread,
  OpenRouterModel,
  OpenAiStatus,
  ProviderBrand,
  ProviderId,
  ProviderModelOption,
  ProviderStatus,
  AccountProfile,
} from "../lib/types"
import { ProviderBadge } from "./ProviderBadge"
import "./chat.css"

const THREADS_KEY = "onyx.chat.threads.v1"
const FAVORITES_KEY = "onyx.chat.favorite-models.v1"

function readJson<T>(key: string, fallback: T): T {
  try {
    const value = localStorage.getItem(key)
    return value ? JSON.parse(value) as T : fallback
  } catch {
    return fallback
  }
}

function makeThread(provider: ProviderId, model: string): ChatThread {
  const now = new Date().toISOString()
  return {
    id: crypto.randomUUID(),
    title: "New chat",
    provider,
    model,
    mode: "chat",
    messages: [],
    createdAt: now,
    updatedAt: now,
  }
}

function modeIcon(mode: ChatThread["mode"]) {
  return mode === "image" ? ImageIcon : mode === "video" ? Video : MessageSquare
}

const subscriptionApps: Array<{
  id: "chatgpt" | "claude" | "gemini" | "grok"
  name: string
  brand: ProviderBrand
}> = [
  { id: "chatgpt", name: "ChatGPT", brand: "openai" },
  { id: "claude", name: "Claude", brand: "anthropic" },
  { id: "gemini", name: "Gemini", brand: "google" },
  { id: "grok", name: "Grok", brand: "xai" },
]

export const ChatView: Component<{
  providers: ProviderStatus[]
  providerModels: Partial<Record<ProviderId, ProviderModelOption[]>>
  openRouterModels: OpenRouterModel[]
  openAi: OpenAiStatus
  profile?: AccountProfile | null
  onOpenSettings: () => void
}> = (props) => {
  const initialModels = modelsForBrand("anthropic", props.providerModels, props.openRouterModels)
  const [threads, setThreads] = createSignal<ChatThread[]>(readJson<ChatThread[]>(THREADS_KEY, []).slice(0, 80))
  const [activeId, setActiveId] = createSignal<string | null>(threads()[0]?.id ?? null)
  const [draft, setDraft] = createSignal("")
  const [busy, setBusy] = createSignal(false)
  const [sidebarOpen, setSidebarOpen] = createSignal(true)
  const [modelMenuOpen, setModelMenuOpen] = createSignal(false)
  const [modelQuery, setModelQuery] = createSignal("")
  const [chatQuery, setChatQuery] = createSignal("")
  const [favorites, setFavorites] = createSignal<string[]>(readJson<string[]>(FAVORITES_KEY, []))
  const [brand, setBrand] = createSignal<ProviderBrand>("anthropic")
  const [selectedModel, setSelectedModel] = createSignal(initialModels.find((model) => model.isDefault)?.id ?? initialModels[0]?.id ?? "default")
  const [mode, setMode] = createSignal<ChatThread["mode"]>("chat")
  const [webSearch, setWebSearch] = createSignal(false)
  const [error, setError] = createSignal<string | null>(null)

  const active = createMemo(() => threads().find((thread) => thread.id === activeId()) ?? null)
  const availableModels = (selectedBrand: ProviderBrand, selectedMode: ChatThread["mode"]) => {
    if (selectedMode === "image" && selectedBrand === "openai") {
      return [{
        id: "gpt-image-2",
        name: "GPT Image 2",
        description: "OpenAI's current native image generation model",
        isDefault: true,
        reasoning: [],
        defaultReasoning: null,
        speeds: ["standard" as const],
        defaultSpeed: "standard" as const,
        contextLength: null,
      }]
    }
    return modelsForBrand(selectedBrand, props.providerModels, props.openRouterModels)
  }
  const modelOptions = createMemo(() => {
    const models = availableModels(brand(), mode())
    if (mode() === "image" && brand() === "openai") return models
    if (mode() === "chat") return models
    return models.filter((model) => {
      const source = props.openRouterModels.find((item) => item.id === model.id)
      return source?.outputModalities?.includes(mode())
    })
  })
  const selectedModelName = createMemo(() => modelOptions().find((model) => model.id === selectedModel())?.name ?? (selectedModel() || "Choose model"))
  const filteredBrands = createMemo(() => providerBrands.filter((item) => {
    if (mode() === "image") return item.id === "openrouter" || (item.id === "openai" && props.openAi.connected)
    if (mode() === "video") return item.id === "openrouter"
    const status = props.providers.find((provider) => provider.id === item.runtime)
    return status?.available
  }))
  const filteredModels = createMemo(() => {
    const needle = modelQuery().trim().toLowerCase()
    const models = modelOptions()
    return needle ? models.filter((model) => `${model.name} ${model.id}`.toLowerCase().includes(needle)) : models
  })
  const filteredThreads = createMemo(() => {
    const needle = chatQuery().trim().toLowerCase()
    return needle ? threads().filter((thread) => thread.title.toLowerCase().includes(needle)) : threads()
  })

  const persistThreads = (next: ChatThread[]) => {
    const trimmed = [...next].sort((a, b) => Date.parse(b.updatedAt) - Date.parse(a.updatedAt)).slice(0, 80)
    setThreads(trimmed)
    localStorage.setItem(THREADS_KEY, JSON.stringify(trimmed))
    window.dispatchEvent(new Event("onyx:cloud-data-changed"))
  }

  const updateThread = (id: string, update: (thread: ChatThread) => ChatThread) => {
    persistThreads(threads().map((thread) => thread.id === id ? update(thread) : thread))
  }

  const newChat = () => {
    const thread = makeThread(runtimeForBrand(brand()), selectedModel())
    thread.mode = mode()
    persistThreads([thread, ...threads()])
    setActiveId(thread.id)
    setDraft("")
    setError(null)
  }

  const ensureThread = () => {
    const current = active()
    if (current) return current
    const thread = makeThread(runtimeForBrand(brand()), selectedModel())
    thread.mode = mode()
    persistThreads([thread, ...threads()])
    setActiveId(thread.id)
    return thread
  }

  const selectThread = (thread: ChatThread) => {
    setActiveId(thread.id)
    setSelectedModel(thread.model)
    setMode(thread.mode)
    const inferred = providerBrands.find((item) => item.runtime === thread.provider) ?? providerBrands.at(-1)!
    setBrand(inferred.id)
    setError(null)
  }

  const deleteThread = (id: string) => {
    const next = threads().filter((thread) => thread.id !== id)
    persistThreads(next)
    if (activeId() === id) setActiveId(next[0]?.id ?? null)
  }

  const chooseBrand = (value: ProviderBrand) => {
    setBrand(value)
    const models = availableModels(value, mode())
    const next = models.find((model) => model.isDefault) ?? models[0]
    setSelectedModel(next?.id ?? "")
  }

  const chooseMode = (value: ChatThread["mode"]) => {
    setMode(value)
    if (value === "image") chooseBrand(props.openAi.connected ? "openai" : "openrouter")
    else if (value === "video") chooseBrand("openrouter")
    else if (brand() === "openrouter" && !props.providers.find((provider) => provider.id === "openrouter")?.available) chooseBrand("anthropic")
  }

  const toggleFavorite = (id: string) => {
    const next = favorites().includes(id) ? favorites().filter((item) => item !== id) : [...favorites(), id]
    setFavorites(next)
    localStorage.setItem(FAVORITES_KEY, JSON.stringify(next))
  }

  const submit = async () => {
    const prompt = draft().trim()
    if (!prompt || busy() || !selectedModel()) return
    const thread = ensureThread()
    const now = new Date().toISOString()
    const user: ChatMessage = { id: crypto.randomUUID(), role: "user", content: prompt, media: [], createdAt: now }
    const nextThread: ChatThread = {
      ...thread,
      title: thread.messages.length ? thread.title : prompt.slice(0, 52),
      provider: runtimeForBrand(brand()),
      model: selectedModel(),
      mode: mode(),
      messages: [...thread.messages, user],
      updatedAt: now,
    }
    persistThreads(threads().map((item) => item.id === thread.id ? nextThread : item))
    setDraft("")
    setBusy(true)
    setError(null)
    try {
      let reply
      if (mode() === "image") {
        reply = await api.generateImage(selectedModel(), prompt, "1:1", brand() === "openai" ? "openai" : "openrouter")
      } else if (mode() === "video") {
        let job = await api.startVideo(selectedModel(), prompt, "16:9")
        for (let attempt = 0; attempt < 120 && !["completed", "failed"].includes(job.status); attempt += 1) {
          await new Promise((resolve) => window.setTimeout(resolve, 2500))
          job = await api.pollVideo(job.id)
        }
        if (job.status !== "completed" || !job.contentUrl) throw new Error(job.error ?? "Video generation did not complete")
        reply = { content: "Video generated", model: selectedModel(), media: [{ kind: "video" as const, url: job.contentUrl, mimeType: "video/mp4" }] }
      } else {
        reply = await api.chatSend({
          provider: runtimeForBrand(brand()),
          model: selectedModel(),
          webSearch: webSearch(),
          messages: nextThread.messages.map((message) => ({ role: message.role, content: message.content })),
        })
      }
      const assistant: ChatMessage = { id: crypto.randomUUID(), role: "assistant", content: reply.content, media: reply.media, createdAt: new Date().toISOString() }
      updateThread(thread.id, (item) => ({ ...item, messages: [...item.messages, assistant], updatedAt: assistant.createdAt }))
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section class="onyx-chat" data-sidebar={sidebarOpen() ? "open" : "closed"}>
      <aside class="onyx-chat__sidebar">
        <div class="onyx-chat__sidebar-head">
          <button class="onyx-chat__new" onClick={newChat}><Plus size={16} /> New chat</button>
          <button class="onyx-chat__icon-button" onClick={() => setSidebarOpen(false)} aria-label="Close chat history"><PanelLeftClose size={17} /></button>
        </div>
        <label class="onyx-chat__search"><Search size={14} /><input value={chatQuery()} onInput={(event) => setChatQuery(event.currentTarget.value)} placeholder="Search chats" /></label>
        <div class="onyx-chat__history">
          <Show when={filteredThreads().length} fallback={<p class="onyx-chat__empty-history">{chatQuery() ? "No chats match your search." : "Your conversations stay local unless cloud sync is enabled."}</p>}>
            <For each={filteredThreads()}>{(thread) => {
              const Icon = modeIcon(thread.mode)
              return (
                <div classList={{ "onyx-chat__history-row": true, active: activeId() === thread.id }}>
                  <button onClick={() => selectThread(thread)}><Icon size={14} /><span>{thread.title}</span></button>
                  <button onClick={() => deleteThread(thread.id)} aria-label={`Delete ${thread.title}`}><Trash2 size={13} /></button>
                </div>
              )
            }}</For>
          </Show>
        </div>
        <button class="onyx-chat__account" onClick={props.onOpenSettings}>
          <Show when={props.profile?.imageUrl} fallback={<span class="onyx-chat__account-fallback">{props.profile?.name?.slice(0, 1).toUpperCase() ?? "O"}</span>}>
            {(imageUrl) => <img src={imageUrl()} alt="" referrerpolicy="no-referrer" />}
          </Show>
          <div><strong>{props.profile?.name ?? "Onyx account"}</strong><small>{props.profile?.email || "Account & cloud"}</small></div><MoreHorizontal size={16} />
        </button>
      </aside>

      <main class="onyx-chat__main">
        <header class="onyx-chat__topbar">
          <Show when={!sidebarOpen()}><button class="onyx-chat__icon-button" onClick={() => setSidebarOpen(true)} aria-label="Open chat history"><PanelLeftOpen size={18} /></button></Show>
          <div><strong>{active()?.title ?? "New chat"}</strong><span>Onyx Chat</span></div>
          <button class="onyx-chat__icon-button" onClick={props.onOpenSettings} aria-label="Open chat settings"><Menu size={17} /></button>
        </header>

        <div class="onyx-chat__scroll">
          <Show when={active()?.messages.length} fallback={
            <div class="onyx-chat__welcome">
              <h1>Onyx</h1>
              <p>How can I help?</p>
              <div class="onyx-chat__web-apps" aria-label="Subscription web apps">
                <For each={subscriptionApps}>{(app) => (
                  <button onClick={() => void api.openProviderWeb(app.id).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause)))}>
                    <ProviderBadge brand={app.brand} size="sm" />
                    <span>{app.name}</span>
                    <ExternalLink size={12} />
                  </button>
                )}</For>
              </div>
              <small class="onyx-chat__web-note">Open the provider’s signed-in web app for subscription-only tools.</small>
            </div>
          }>
            <div class="onyx-chat__messages">
              <For each={active()?.messages}>{(message) => (
                <article classList={{ "onyx-chat__message": true, user: message.role === "user" }}>
                  <div class="onyx-chat__message-copy">{message.content}</div>
                  <For each={message.media}>{(media) => media.kind === "image"
                    ? <img src={media.url} alt="Generated media" />
                    : <video src={media.url} controls />
                  }</For>
                </article>
              )}</For>
              <Show when={busy()}><div class="onyx-chat__thinking"><LoaderCircle class="spin" size={15} /> Onyx is thinking…</div></Show>
            </div>
          </Show>
        </div>

        <div class="onyx-chat__dock">
          <Show when={error()}><button class="onyx-chat__error" onClick={() => setError(null)}>{error()}</button></Show>
          <form class="onyx-chat__composer" onSubmit={(event) => { event.preventDefault(); void submit() }}>
            <textarea value={draft()} onInput={(event) => setDraft(event.currentTarget.value)} placeholder={mode() === "chat" ? "Message Onyx…" : `Describe the ${mode()} you want…`} rows={1} onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey && !event.isComposing) { event.preventDefault(); void submit() }
            }} />
            <div class="onyx-chat__composer-row">
              <div class="onyx-chat__modes">
                <For each={(["chat", "image", "video"] as const)}>{(item) => {
                  const Icon = modeIcon(item)
                  return <button type="button" classList={{ active: mode() === item }} onClick={() => chooseMode(item)}><Icon size={14} />{item}</button>
                }}</For>
                <button type="button" classList={{ active: webSearch() }} disabled={mode() !== "chat"} onClick={() => setWebSearch((value) => !value)} title="Let the selected provider search the web"><Search size={14} />web</button>
              </div>
              <div class="onyx-chat__composer-actions">
                <button type="button" class="onyx-chat__model-trigger" onClick={() => setModelMenuOpen((value) => !value)}>
                  <ProviderBadge brand={brand()} size="sm" /><span>{selectedModelName()}</span><ChevronDown size={12} />
                </button>
                <button class="onyx-chat__send" type="submit" disabled={!draft().trim() || busy()}><Show when={busy()} fallback={<Send size={15} />}><LoaderCircle class="spin" size={15} /></Show></button>
              </div>
            </div>
          </form>

          <Show when={modelMenuOpen()}>
            <div class="onyx-chat__model-menu">
              <label><Search size={14} /><input autofocus value={modelQuery()} onInput={(event) => setModelQuery(event.currentTarget.value)} placeholder="Search models" /></label>
              <div class="onyx-chat__provider-tabs">
                <For each={filteredBrands()}>{(item) => <button classList={{ active: brand() === item.id }} onClick={() => chooseBrand(item.id)}><ProviderBadge brand={item.id} size="sm" />{item.name}</button>}</For>
              </div>
              <div class="onyx-chat__model-list">
                <Show when={filteredModels().length} fallback={<p>No compatible models were reported for this mode.</p>}>
                  <For each={filteredModels().sort((a, b) => Number(favorites().includes(b.id)) - Number(favorites().includes(a.id)))}>{(model) => (
                    <button classList={{ selected: selectedModel() === model.id }} onClick={() => { setSelectedModel(model.id); setModelMenuOpen(false) }}>
                      <ProviderBadge brand={brand()} size="sm" /><span><strong>{model.name}</strong><small>{model.description ?? model.id}</small></span>
                      <i role="button" tabindex={0} onClick={(event) => { event.stopPropagation(); toggleFavorite(model.id) }}><Star size={14} fill={favorites().includes(model.id) ? "currentColor" : "none"} /></i>
                    </button>
                  )}</For>
                </Show>
              </div>
            </div>
          </Show>
          <p class="onyx-chat__disclaimer">Models can make mistakes. Verify important information.</p>
        </div>
      </main>
    </section>
  )
}
