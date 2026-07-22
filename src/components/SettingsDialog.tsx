import { createEffect, createSignal, For, onCleanup, Show, type Component } from "solid-js"
import {
  Check,
  ChevronDown,
  Cpu,
  ExternalLink,
  KeyRound,
  Keyboard,
  LoaderCircle,
  RefreshCw,
  Server,
  SlidersHorizontal,
  Sparkles,
  X,
} from "lucide-solid"
import { api } from "../lib/api"
import type { OpenRouterModel, OpenRouterStatus, ProviderStatus } from "../lib/types"
import { ProviderBadge } from "./ProviderBadge"

export type ColorScheme = "system" | "light" | "dark"
type SettingsPage = "general" | "shortcuts" | "providers" | "models"

const navItems: Array<{ page: SettingsPage; label: string; icon: typeof SlidersHorizontal }> = [
  { page: "general", label: "General", icon: SlidersHorizontal },
  { page: "shortcuts", label: "Shortcuts", icon: Keyboard },
  { page: "providers", label: "Providers", icon: Cpu },
  { page: "models", label: "Models", icon: Sparkles },
]

export const SettingsDialog: Component<{
  open: boolean
  providers: ProviderStatus[]
  openRouter: OpenRouterStatus
  openRouterModels?: OpenRouterModel[]
  colorScheme?: ColorScheme
  onColorScheme?: (scheme: ColorScheme) => void
  onClose: () => void
  onRefresh: () => Promise<void>
  onOpenRouter: (status: OpenRouterStatus) => void
  onModels: (models: OpenRouterModel[]) => void
}> = (props) => {
  const [page, setPage] = createSignal<SettingsPage>("general")
  const [key, setKey] = createSignal("")
  const [saving, setSaving] = createSignal(false)
  const [message, setMessage] = createSignal<string | null>(null)
  let dialogElement: HTMLElement | undefined
  let returnFocus: HTMLElement | null = null
  let wasOpen = false

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
                <For each={navItems.slice(0, 2)}>
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
                <button disabled><Server size={17} stroke-width={1.6} /> Runtimes</button>
                <For each={navItems.slice(2)}>
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
            <div class="zai-settings-version"><strong>zAI Desktop</strong><span>v0.1.0</span></div>
          </aside>

          <div class="zai-settings-content">
            <button class="zai-settings-close" onClick={props.onClose} aria-label="Close settings"><X size={16} /></button>

            <Show when={page() === "general"}>
              <div class="zai-settings-page">
                <h1 id="zai-settings-page-title">General</h1>
                <section class="zai-settings-card">
                  <div class="zai-setting-row">
                    <div><strong>Language</strong><span>zAI currently ships with an English interface</span></div>
                    <span class="zai-setting-value">English</span>
                  </div>
                  <div class="zai-setting-row">
                    <div><strong>Permission approvals</strong><span>Supported approval requests stay visible for review</span></div>
                    <span class="zai-setting-value">Review</span>
                  </div>
                  <div class="zai-setting-row">
                    <div><strong>Provider sessions</strong><span>zAI selects the supported session mode for each provider</span></div>
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

                <h1>Appearance</h1>
                <section class="zai-settings-card">
                  <div class="zai-setting-row">
                    <div><strong>Color scheme</strong><span>Choose whether zAI follows the system, light, or dark theme</span></div>
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
                    <div><strong>Interface style</strong><span>zAI's OpenCode-informed desktop visual language</span></div>
                    <span class="zai-setting-value">zAI</span>
                  </div>
                </section>
              </div>
            </Show>

            <Show when={page() === "shortcuts"}>
              <div class="zai-settings-page">
                <h1 id="zai-settings-page-title">Shortcuts</h1>
                <section class="zai-settings-card">
                  <For each={[["New session", "⌘ N"], ["Send message", "↵"], ["New line", "⇧ ↵"], ["Stop agent", "Esc"]]}>
                    {(shortcut) => <div class="zai-setting-row"><strong>{shortcut[0]}</strong><kbd>{shortcut[1]}</kbd></div>}
                  </For>
                </section>
              </div>
            </Show>

            <Show when={page() === "providers"}>
              <div class="zai-settings-page">
                <div class="zai-settings-title-row">
                  <h1 id="zai-settings-page-title">Providers</h1>
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
          </div>
        </section>
      </div>
    </Show>
  )
}
