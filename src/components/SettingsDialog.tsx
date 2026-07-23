import { createEffect, createSignal, For, onCleanup, Show, type Component } from "solid-js"
import {
  Check,
  ChevronDown,
  Cpu,
  ExternalLink,
  KeyRound,
  Keyboard,
  LoaderCircle,
  Mic2,
  RefreshCw,
  SlidersHorizontal,
  Sparkles,
  UserRound,
  X,
} from "lucide-solid"
import { api } from "../lib/api"
import {
  accountSnapshot,
  initializeAccount,
  openSignIn,
  pushCloudSnapshot,
  signOut,
  subscribeAccount,
} from "../lib/account"
import type { AgentSession, DesktopPreferences, OpenAiStatus, OpenRouterModel, OpenRouterStatus, ProviderId, ProviderModelOption, ProviderStatus, VoiceSettings } from "../lib/types"
import { ProviderBadge } from "./ProviderBadge"

export type ColorScheme = "system" | "light" | "dark"
type SettingsPage = "general" | "shortcuts" | "providers" | "models" | "voice" | "account"

const navItems: Array<{ page: SettingsPage; label: string; icon: typeof SlidersHorizontal }> = [
  { page: "general", label: "General", icon: SlidersHorizontal },
  { page: "shortcuts", label: "Shortcuts", icon: Keyboard },
  { page: "voice", label: "Voice", icon: Mic2 },
  { page: "providers", label: "Runtimes", icon: Cpu },
  { page: "models", label: "Models", icon: Sparkles },
  { page: "account", label: "Account & cloud", icon: UserRound },
]

export const SettingsDialog: Component<{
  open: boolean
  providers: ProviderStatus[]
  openRouter: OpenRouterStatus
  openAi: OpenAiStatus
  openRouterModels?: OpenRouterModel[]
  providerModels?: Partial<Record<ProviderId, ProviderModelOption[]>>
  sessions: AgentSession[]
  colorScheme?: ColorScheme
  onColorScheme?: (scheme: ColorScheme) => void
  onClose: () => void
  onRefresh: () => Promise<void>
  onOpenRouter: (status: OpenRouterStatus) => void
  onOpenAi: (status: OpenAiStatus) => void
  onModels: (models: OpenRouterModel[]) => void
}> = (props) => {
  const [page, setPage] = createSignal<SettingsPage>("general")
  const [key, setKey] = createSignal("")
  const [openAiKey, setOpenAiKey] = createSignal("")
  const [saving, setSaving] = createSignal(false)
  const [message, setMessage] = createSignal<string | null>(null)
  const [voice, setVoice] = createSignal<VoiceSettings | null>(null)
  const [account, setAccount] = createSignal(accountSnapshot())
  const [platform, setPlatform] = createSignal("unknown")
  const [wslDistributions, setWslDistributions] = createSignal<string[]>([])
  const [desktopPreferences, setDesktopPreferences] = createSignal<DesktopPreferences>((() => {
    try { return JSON.parse(localStorage.getItem("onyx.desktop-preferences.v1") ?? "null") as DesktopPreferences ?? { wslMode: "off", wslDistribution: "" } }
    catch { return { wslMode: "off", wslDistribution: "" } }
  })())
  let dialogElement: HTMLElement | undefined
  let returnFocus: HTMLElement | null = null
  let wasOpen = false

  const unsubscribeAccount = subscribeAccount(setAccount)
  void initializeAccount()
  onCleanup(unsubscribeAccount)

  createEffect(() => {
    if (!props.open || voice()) return
    void api.getVoiceSettings().then(setVoice).catch((error) => setMessage(String(error)))
  })

  void api.platform().then((value) => {
    setPlatform(value)
    if (value === "windows") void api.listWslDistributions().then(setWslDistributions).catch(() => setWslDistributions([]))
  })

  const updateDesktopPreferences = (next: DesktopPreferences) => {
    setDesktopPreferences(next)
    localStorage.setItem("onyx.desktop-preferences.v1", JSON.stringify(next))
  }

  const restoreFocus = () => {
    if (!wasOpen) return
    wasOpen = false
    const target = returnFocus
    returnFocus = null
    queueMicrotask(() => {
      if (target?.isConnected) target.focus()
    })
  }

  createEffect(() => {
    if (!props.open) {
      setKey("")
      setOpenAiKey("")
      setMessage(null)
      restoreFocus()
      return
    }

    if (!wasOpen) {
      wasOpen = true
      returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
      queueMicrotask(() => {
        if (props.open) dialogElement?.focus()
      })
    }

    const background = Array.from(
      document.querySelectorAll<HTMLElement>(".zai-titlebar, .zai-main"),
    )
    const backgroundState = background.map((element) => ({
      element,
      inert: element.inert,
      ariaHidden: element.getAttribute("aria-hidden"),
    }))
    background.forEach((element) => {
      element.inert = true
      element.setAttribute("aria-hidden", "true")
    })

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault()
        props.onClose()
        return
      }
      if (event.key !== "Tab" || !dialogElement) return

      const focusable = Array.from(
        dialogElement.querySelectorAll<HTMLElement>(
          'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((element) => {
        const style = window.getComputedStyle(element)
        return style.display !== "none" && style.visibility !== "hidden" && element.getAttribute("aria-hidden") !== "true"
      })

      if (focusable.length === 0) {
        event.preventDefault()
        dialogElement.focus()
        return
      }

      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      const active = document.activeElement
      if (event.shiftKey && (active === first || !dialogElement.contains(active))) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && (active === last || !dialogElement.contains(active))) {
        event.preventDefault()
        first.focus()
      }
    }
    window.addEventListener("keydown", onKeyDown)
    onCleanup(() => {
      window.removeEventListener("keydown", onKeyDown)
      backgroundState.forEach(({ element, inert, ariaHidden }) => {
        element.inert = inert
        if (ariaHidden === null) element.removeAttribute("aria-hidden")
        else element.setAttribute("aria-hidden", ariaHidden)
      })
    })
  })

  onCleanup(restoreFocus)

  const connect = async () => {
    setSaving(true)
    setMessage(null)
    try {
      const status = await api.saveOpenRouterKey(key())
      props.onOpenRouter(status)
      const models = await api.openRouterModels()
      props.onModels(models)
      setKey("")
      setMessage(`Connected. ${models.length} models available.`)
      await props.onRefresh()
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error))
    } finally {
      setSaving(false)
    }
  }

  const disconnect = async () => {
    setSaving(true)
    try {
      props.onOpenRouter(await api.clearOpenRouterKey())
      props.onModels([])
      setMessage("OpenRouter disconnected.")
      await props.onRefresh()
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error))
    } finally {
      setSaving(false)
    }
  }

  const connectOpenAi = async () => {
    setSaving(true)
    setMessage(null)
    try {
      props.onOpenAi(await api.saveOpenAiKey(openAiKey()))
      setOpenAiKey("")
      setMessage("OpenAI API connected. Native GPT Image and audio routes are ready.")
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error))
    } finally {
      setSaving(false)
    }
  }

  const disconnectOpenAi = async () => {
    setSaving(true)
    try {
      props.onOpenAi(await api.clearOpenAiKey())
      setMessage("OpenAI API disconnected.")
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error))
    } finally {
      setSaving(false)
    }
  }

  const providerRow = (provider: ProviderStatus) => (
    <div class="zai-settings-provider-row">
      <ProviderBadge provider={provider.id} />
      <div class="zai-settings-provider-copy">
        <strong>{provider.name}</strong>
        <span>{provider.version ?? provider.transport}</span>
        <Show when={provider.executablePath}><code>{provider.executablePath}</code></Show>
      </div>
      <Show
        when={provider.available}
        fallback={
          <a class="zai-settings-connect" href={provider.installUrl} target="_blank">
            Install <ExternalLink size={12} />
          </a>
        }
      >
        <span class="zai-settings-ready"><Check size={13} /> Ready</span>
      </Show>
    </div>
  )

  const saveVoice = async () => {
    const settings = voice()
    if (!settings) return
    setSaving(true)
    try {
      setVoice(await api.applyVoiceSettings(settings))
      setMessage("Voice settings saved.")
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error))
    } finally {
      setSaving(false)
    }
  }

  type VoiceProviderKey = "agentProvider" | "webProvider" | "filesProvider" | "imageProvider"
  type VoiceModelKey = "agentModel" | "webModel" | "filesModel" | "imageModel"
  const voiceProviderOptions = () => props.providers.filter((provider) => provider.available)
  const voiceModels = (provider: ProviderId, imageOnly = false) => {
    if (provider === "openrouter") {
      return (props.openRouterModels ?? [])
        .filter((model) => !imageOnly || model.outputModalities?.includes("image"))
        .map((model) => ({ id: model.id, name: model.name }))
    }
    return (props.providerModels?.[provider] ?? []).map((model) => ({ id: model.id, name: model.name }))
  }
  const updateVoiceRoute = (providerKey: VoiceProviderKey, modelKey: VoiceModelKey, provider: ProviderId) => {
    const firstModel = voiceModels(provider, providerKey === "imageProvider")[0]?.id ?? (provider === "openrouter" ? "openrouter/auto" : "default")
    setVoice((current) => current ? { ...current, [providerKey]: provider, [modelKey]: firstModel } : current)
  }
  const voiceRoute = (
    title: string,
    description: string,
    providerKey: VoiceProviderKey,
    modelKey: VoiceModelKey,
    imageOnly = false,
  ) => {
    const provider = () => voice()?.[providerKey] ?? "openrouter"
    const model = () => voice()?.[modelKey] ?? ""
    return (
      <div class="zai-setting-row zai-setting-route">
        <div><strong>{title}</strong><span>{description}</span></div>
        <div class="zai-setting-route__controls">
          <label>
            <ProviderBadge provider={provider()} size="sm" />
            <select value={provider()} onChange={(event) => updateVoiceRoute(providerKey, modelKey, event.currentTarget.value as ProviderId)}>
              <For each={voiceProviderOptions()}>{(item) => <option value={item.id}>{item.name}</option>}</For>
            </select>
            <ChevronDown size={13} />
          </label>
          <label>
            <select value={model()} onChange={(event) => setVoice((current) => current ? { ...current, [modelKey]: event.currentTarget.value } : current)}>
              <Show when={!voiceModels(provider(), imageOnly).some((item) => item.id === model()) && model()}>
                <option value={model()}>{model()}</option>
              </Show>
              <For each={voiceModels(provider(), imageOnly)}>{(item) => <option value={item.id}>{item.name}</option>}</For>
            </select>
            <ChevronDown size={13} />
          </label>
        </div>
      </div>
    )
  }

  const syncNow = async () => {
    setSaving(true)
    setMessage(null)
    try {
      await pushCloudSnapshot({
        version: 1,
        exportedAt: new Date().toISOString(),
        sessions: props.sessions,
        chats: JSON.parse(localStorage.getItem("onyx.chat.threads.v1") ?? "[]"),
        voiceHistory: JSON.parse(localStorage.getItem("onyx.voice-history.v1") ?? "[]"),
        preferences: {
          colorScheme: localStorage.getItem("onyx.color-scheme"),
          desktop: JSON.parse(localStorage.getItem("onyx.desktop-preferences.v1") ?? "null"),
        },
      })
      setMessage("Sessions, chats, voice history, and preferences synced.")
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Show when={props.open}>
      <div class="zai-modal-scrim" onMouseDown={(event) => event.target === event.currentTarget && props.onClose()}>
        <section
          ref={dialogElement}
          class="zai-settings-dialog"
          role="dialog"
          aria-modal="true"
          aria-labelledby="zai-settings-page-title"
          tabindex={-1}
        >
          <aside class="zai-settings-sidebar">
            <div>
              <h2>Desktop</h2>
              <nav>
                <For each={navItems.slice(0, 3)}>
                  {(item) => {
                    const Icon = item.icon
                    return (
                      <button classList={{ active: page() === item.page }} onClick={() => setPage(item.page)}>
                        <Icon size={17} stroke-width={1.6} /> {item.label}
                      </button>
                    )
                  }}
                </For>
              </nav>
            </div>
            <div>
              <h2>Agents</h2>
              <nav>
                <For each={navItems.slice(3)}>
                  {(item) => {
                    const Icon = item.icon
                    return (
                      <button classList={{ active: page() === item.page }} onClick={() => setPage(item.page)}>
                        <Icon size={17} stroke-width={1.6} /> {item.label}
                      </button>
                    )
                  }}
                </For>
              </nav>
            </div>
            <div class="zai-settings-version"><strong>Onyx Desktop</strong><span>v0.2.0</span></div>
          </aside>

          <div class="zai-settings-content">
            <button class="zai-settings-close" onClick={props.onClose} aria-label="Close settings"><X size={16} /></button>

            <Show when={page() === "general"}>
              <div class="zai-settings-page">
                <h1 id="zai-settings-page-title">General</h1>
                <section class="zai-settings-card">
                  <div class="zai-setting-row">
                    <div><strong>Language</strong><span>Onyx currently ships with an English interface</span></div>
                    <span class="zai-setting-value">English</span>
                  </div>
                  <div class="zai-setting-row">
                    <div><strong>Permission approvals</strong><span>Supported approval requests stay visible for review</span></div>
                    <span class="zai-setting-value">Review</span>
                  </div>
                  <div class="zai-setting-row">
                    <div><strong>Provider sessions</strong><span>Onyx selects the supported session mode for each provider</span></div>
                    <span class="zai-setting-value">Managed</span>
                  </div>
                  <div class="zai-setting-row">
                    <div><strong>Reasoning summaries</strong><span>Shown in the timeline when a provider supplies them</span></div>
                    <span class="zai-setting-value">When available</span>
                  </div>
                  <div class="zai-setting-row">
                    <div><strong>Shell tool details</strong><span>Open a shell activity in the timeline to inspect its output</span></div>
                    <span class="zai-setting-value">Collapsed</span>
                  </div>
                  <div class="zai-setting-row">
                    <div><strong>Edit tool details</strong><span>Open an edit activity in the timeline to inspect its changes</span></div>
                    <span class="zai-setting-value">Collapsed</span>
                  </div>
                </section>

                <Show when={platform() === "windows"}>
                  <h1>Windows terminal</h1>
                  <section class="zai-settings-card">
                    <div class="zai-setting-row"><div><strong>Terminal environment</strong><span>Use native PowerShell or launch terminal tabs through WSL</span></div><label class="zai-setting-select"><select value={desktopPreferences().wslMode} onChange={(event) => updateDesktopPreferences({ ...desktopPreferences(), wslMode: event.currentTarget.value as DesktopPreferences["wslMode"] })}><option value="off">Windows native</option><option value="default">Default WSL distribution</option><option value="distribution">Specific WSL distribution</option></select><ChevronDown size={13} /></label></div>
                    <Show when={desktopPreferences().wslMode === "distribution"}><div class="zai-setting-row"><div><strong>WSL distribution</strong><span>Installed distributions reported by wsl.exe</span></div><label class="zai-setting-select"><select value={desktopPreferences().wslDistribution} onChange={(event) => updateDesktopPreferences({ ...desktopPreferences(), wslDistribution: event.currentTarget.value })}><option value="">Choose distribution</option><For each={wslDistributions()}>{(distribution) => <option value={distribution}>{distribution}</option>}</For></select><ChevronDown size={13} /></label></div></Show>
                  </section>
                </Show>

                <h1>Appearance</h1>
                <section class="zai-settings-card">
                  <div class="zai-setting-row">
                    <div><strong>Color scheme</strong><span>Choose whether Onyx follows the system, light, or dark theme</span></div>
                    <label class="zai-setting-select">
                      <select
                        aria-label="Color scheme"
                        value={props.colorScheme ?? "system"}
                        onChange={(event) => props.onColorScheme?.(event.currentTarget.value as ColorScheme)}
                      >
                        <option value="system">System</option>
                        <option value="light">Light</option>
                        <option value="dark">Dark</option>
                      </select>
                      <ChevronDown size={13} />
                    </label>
                  </div>
                  <div class="zai-setting-row">
                    <div><strong>Interface style</strong><span>Onyx's OpenCode and T3-informed desktop visual language</span></div>
                    <span class="zai-setting-value">Onyx</span>
                  </div>
                </section>
              </div>
            </Show>

            <Show when={page() === "shortcuts"}>
              <div class="zai-settings-page">
                <h1 id="zai-settings-page-title">Shortcuts</h1>
                <section class="zai-settings-card">
                  <For each={[["New session", platform() === "macos" ? "⌘ N" : "Ctrl N"], ["Settings", platform() === "macos" ? "⌘ ," : "Ctrl ,"], ["Bottom terminal", platform() === "macos" ? "⌘ J" : "Ctrl J"], ["Right panel", platform() === "macos" ? "⌘ ⇧ J" : "Ctrl Shift J"], ["Send message", "↵"], ["New line", "⇧ ↵"], ["Stop agent", "Esc"], ["Hold to dictate", "Ctrl Shift"], ["Hold for voice agent", "Ctrl Alt"]]}>
                    {(shortcut) => <div class="zai-setting-row"><strong>{shortcut[0]}</strong><kbd>{shortcut[1]}</kbd></div>}
                  </For>
                </section>
              </div>
            </Show>

            <Show when={page() === "providers"}>
              <div class="zai-settings-page">
                <div class="zai-settings-title-row">
                  <h1 id="zai-settings-page-title">Runtimes</h1>
                  <button class="zai-neutral-button" disabled={saving()} onClick={() => props.onRefresh()}>
                    <RefreshCw size={13} classList={{ spin: saving() }} /> Refresh
                  </button>
                </div>
                <h3>Local coding agents</h3>
                <section class="zai-settings-provider-card">
                  <For each={props.providers.filter((provider) => provider.id !== "openrouter")}>
                    {providerRow}
                  </For>
                </section>

                <h3>OpenAI API</h3>
                <section class="zai-settings-provider-card">
                  <div class="zai-settings-provider-row zai-openrouter-row">
                    <ProviderBadge brand="openai" />
                    <div class="zai-settings-provider-copy">
                      <strong>OpenAI API</strong>
                      <span>Native GPT Image, transcription, and speech; billed separately from ChatGPT</span>
                    </div>
                    <Show when={props.openAi.connected} fallback={<span class="zai-settings-disconnected">Not connected</span>}>
                      <div class="zai-settings-status-actions">
                        <span class="zai-settings-ready"><Check size={13} /> Connected</span>
                        <button class="zai-danger-link" disabled={saving()} onClick={disconnectOpenAi}>Disconnect</button>
                      </div>
                    </Show>
                  </div>
                  <Show when={!props.openAi.connected}>
                    <div class="zai-openrouter-key">
                      <KeyRound size={15} />
                      <input
                        type="password"
                        autocomplete="off"
                        value={openAiKey()}
                        onInput={(event) => setOpenAiKey(event.currentTarget.value)}
                        placeholder="sk-…"
                        onKeyDown={(event) => event.key === "Enter" && openAiKey().trim() && void connectOpenAi()}
                      />
                      <button class="zai-neutral-button" disabled={!openAiKey().trim() || saving()} onClick={connectOpenAi}>
                        <Show when={saving()} fallback="Connect"><LoaderCircle class="spin" size={14} /></Show>
                      </button>
                    </div>
                  </Show>
                </section>

                <h3>OpenRouter</h3>
                <section class="zai-settings-provider-card">
                  <div class="zai-settings-provider-row zai-openrouter-row">
                    <ProviderBadge provider="openrouter" />
                    <div class="zai-settings-provider-copy">
                      <strong>OpenRouter</strong>
                      <span>Choose from models available to your API key</span>
                    </div>
                    <Show
                      when={props.openRouter.connected}
                      fallback={<span class="zai-settings-disconnected">Not connected</span>}
                    >
                      <div class="zai-settings-status-actions">
                        <span class="zai-settings-ready"><Check size={13} /> Connected</span>
                        <button class="zai-danger-link" disabled={saving()} onClick={disconnect}>Disconnect</button>
                      </div>
                    </Show>
                  </div>
                  <Show when={!props.openRouter.connected}>
                    <div class="zai-openrouter-key">
                      <KeyRound size={15} />
                      <input
                        type="password"
                        autocomplete="off"
                        value={key()}
                        onInput={(event) => setKey(event.currentTarget.value)}
                        placeholder="sk-or-v1-…"
                        onKeyDown={(event) => event.key === "Enter" && key().trim() && void connect()}
                      />
                      <button class="zai-neutral-button" disabled={!key().trim() || saving()} onClick={connect}>
                        <Show when={saving()} fallback="Connect"><LoaderCircle class="spin" size={14} /></Show>
                      </button>
                    </div>
                  </Show>
                </section>
                <Show when={message()}><p class="zai-settings-message">{message()}</p></Show>
              </div>
            </Show>

            <Show when={page() === "models"}>
              <div class="zai-settings-page">
                <h1 id="zai-settings-page-title">Models</h1>
                <p class="zai-settings-intro">CLI agents use their own configured model catalogs. OpenRouter models are loaded from your account.</p>
                <section class="zai-settings-card zai-model-list">
                  <Show when={(props.openRouterModels?.length ?? 0) > 0} fallback={<div class="zai-model-empty">Connect OpenRouter to load its model catalog.</div>}>
                    <For each={(props.openRouterModels ?? []).slice(0, 100)}>
                      {(model) => (
                        <div class="zai-setting-row">
                          <div><strong>{model.name}</strong><code>{model.id}</code></div>
                          <span class="zai-setting-value">{model.contextLength ? `${Math.round(model.contextLength / 1000)}k` : ""}</span>
                        </div>
                      )}
                    </For>
                  </Show>
                </section>
              </div>
            </Show>

            <Show when={page() === "voice"}>
              <div class="zai-settings-page">
                <div class="zai-settings-title-row"><h1 id="zai-settings-page-title">Voice</h1><button class="zai-neutral-button" disabled={!voice() || saving()} onClick={() => void saveVoice()}><Show when={saving()} fallback="Save"><LoaderCircle class="spin" size={14} /></Show></button></div>
                <p class="zai-settings-intro">Dictation and the voice agent stay available from the tray even while the editor is closed.</p>
                <section class="zai-settings-card">
                  <div class="zai-setting-row"><div><strong>Dictation</strong><span>Hold anywhere, release to transcribe and paste</span></div><kbd>{voice()?.dictationShortcut ?? "Ctrl Shift"}</kbd></div>
                  <div class="zai-setting-row"><div><strong>Agentic voice</strong><span>Hold anywhere to ask Onyx about the active app</span></div><kbd>{voice()?.agentShortcut ?? "Ctrl Alt"}</kbd></div>
                  <div class="zai-setting-row"><div><strong>Dictation model</strong><span>Use OpenRouter or a separately billed OpenAI API key</span></div><div class="zai-setting-route__controls"><label class="zai-setting-select"><select value={voice()?.transcriptionProvider ?? "openrouter"} onChange={(event) => setVoice((current) => current ? { ...current, transcriptionProvider: event.currentTarget.value as VoiceSettings["transcriptionProvider"], transcriptionModel: event.currentTarget.value === "openai" ? "gpt-4o-mini-transcribe" : "openai/whisper-large-v3" } : current)}><option value="openrouter">OpenRouter</option><option value="openai" disabled={!props.openAi.connected}>OpenAI API</option></select><ChevronDown size={13} /></label><input class="zai-settings-inline-input" value={voice()?.transcriptionModel ?? ""} onInput={(event) => setVoice((current) => current ? { ...current, transcriptionModel: event.currentTarget.value } : current)} /></div></div>
                  {voiceRoute("General agent", "Answer voice questions with OpenRouter or an authenticated CLI subscription", "agentProvider", "agentModel")}
                  {voiceRoute("Web research", "Used automatically for current information and source requests", "webProvider", "webModel")}
                  {voiceRoute("File tasks", "Preferred coding subscription when a voice request becomes a workspace task", "filesProvider", "filesModel")}
                  {voiceRoute("Image tasks", "OpenRouter image-capable model used by creative workflows", "imageProvider", "imageModel", true)}
                  <div class="zai-setting-row"><div><strong>Speech model</strong><span>Voice used to read agent answers</span></div><div class="zai-setting-route__controls"><label class="zai-setting-select"><select value={voice()?.voiceProvider ?? "openrouter"} onChange={(event) => setVoice((current) => current ? { ...current, voiceProvider: event.currentTarget.value as VoiceSettings["voiceProvider"], voiceModel: event.currentTarget.value === "openai" ? "gpt-4o-mini-tts" : "openai/gpt-4o-mini-tts" } : current)}><option value="openrouter">OpenRouter</option><option value="openai" disabled={!props.openAi.connected}>OpenAI API</option></select><ChevronDown size={13} /></label><input class="zai-settings-inline-input" value={voice()?.voiceModel ?? ""} onInput={(event) => setVoice((current) => current ? { ...current, voiceModel: event.currentTarget.value } : current)} /></div></div>
                  <div class="zai-setting-row"><div><strong>Voice</strong><span>Voice identifier supported by the selected speech model</span></div><input class="zai-settings-inline-input" value={voice()?.voiceId ?? "alloy"} onInput={(event) => setVoice((current) => current ? { ...current, voiceId: event.currentTarget.value } : current)} /></div>
                  <div class="zai-setting-row"><div><strong>Speech rate</strong><span>0.5× to 2×</span></div><input class="zai-settings-inline-input" type="number" min="0.5" max="2" step="0.1" value={voice()?.voiceRate ?? 1} onInput={(event) => setVoice((current) => current ? { ...current, voiceRate: event.currentTarget.valueAsNumber } : current)} /></div>
                  <div class="zai-setting-row"><div><strong>Overlay position</strong><span>Where dictation feedback appears</span></div><label class="zai-setting-select"><select value={voice()?.overlayPosition ?? "bottom_center"} onChange={(event) => setVoice((current) => current ? { ...current, overlayPosition: event.currentTarget.value as VoiceSettings["overlayPosition"] } : current)}><For each={["top_left","top_center","top_right","center","bottom_left","bottom_center","bottom_right"]}>{(position) => <option value={position}>{position.replaceAll("_", " ")}</option>}</For></select><ChevronDown size={13} /></label></div>
                  <div class="zai-setting-row"><div><strong>Speak responses</strong><span>Read agent answers aloud when TTS is configured</span></div><input type="checkbox" checked={voice()?.speakResponses ?? false} onChange={(event) => setVoice((current) => current ? { ...current, speakResponses: event.currentTarget.checked } : current)} /></div>
                </section>
                <Show when={message()}><p class="zai-settings-message">{message()}</p></Show>
              </div>
            </Show>

            <Show when={page() === "account"}>
              <div class="zai-settings-page">
                <h1 id="zai-settings-page-title">Account & cloud</h1>
                <p class="zai-settings-intro">Onyx is local-first. Signing in enables optional encrypted-transport sync through your Clerk and Convex deployment.</p>
                <section class="zai-settings-card">
                  <Show when={account().configured} fallback={<div class="zai-setting-row"><div><strong>Account setup required</strong><span>Copy .env.example to .env.local and add your Clerk publishable key.</span></div><span class="zai-setting-value">Local only</span></div>}>
                    <Show when={account().profile} keyed fallback={<div class="zai-setting-row"><div><strong>Sign in to Onyx</strong><span>Sync sessions, chats, and preferences across your devices</span></div><button class="zai-neutral-button" onClick={() => void openSignIn().catch((error) => setMessage(String(error)))}>Sign in</button></div>}>
                      {(profile) => <><div class="zai-setting-row"><div><strong>{profile.name}</strong><span>{profile.email}</span></div><span class="zai-settings-ready"><Check size={13} /> Signed in</span></div><div class="zai-setting-row"><div><strong>Cloud sync</strong><span>{account().cloud.configured ? "Convex is configured for this build" : "Add VITE_CONVEX_URL to enable sync"}</span></div><button class="zai-neutral-button" disabled={!account().cloud.authenticated || saving()} onClick={() => void syncNow()}><Show when={account().cloud.syncing} fallback="Sync now"><LoaderCircle class="spin" size={14} /></Show></button></div><div class="zai-setting-row"><div><strong>Account session</strong><span>Sign out of Onyx on this device</span></div><button class="zai-danger-link" onClick={() => void signOut()}>Sign out</button></div></>}
                    </Show>
                  </Show>
                </section>
                <Show when={account().error || message()}><p class="zai-settings-message">{account().error ?? message()}</p></Show>
              </div>
            </Show>
          </div>
        </section>
      </div>
    </Show>
  )
}
