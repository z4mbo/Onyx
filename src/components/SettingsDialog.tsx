import { createEffect, createSignal, For, Show, type Component } from "solid-js"
import { Check, ExternalLink, KeyRound, LoaderCircle, RefreshCw, X } from "lucide-solid"
import { api } from "../lib/api"
import { ProviderBadge } from "./ProviderBadge"
import type { OpenRouterModel, OpenRouterStatus, ProviderStatus } from "../lib/types"

export const SettingsDialog: Component<{
  open: boolean
  providers: ProviderStatus[]
  openRouter: OpenRouterStatus
  onClose: () => void
  onRefresh: () => Promise<void>
  onOpenRouter: (status: OpenRouterStatus) => void
  onModels: (models: OpenRouterModel[]) => void
}> = (props) => {
  const [key, setKey] = createSignal("")
  const [saving, setSaving] = createSignal(false)
  const [message, setMessage] = createSignal<string | null>(null)

  createEffect(() => {
    if (!props.open) {
      setKey("")
      setMessage(null)
    }
  })

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

  return (
    <Show when={props.open}>
      <div class="modal-backdrop" onMouseDown={(event) => event.target === event.currentTarget && props.onClose()}>
        <section class="settings-dialog" role="dialog" aria-modal="true" aria-label="Settings">
          <header class="settings-header">
            <div>
              <h2>Settings</h2>
              <p>Local agents and provider credentials</p>
            </div>
            <button class="icon-button" onClick={props.onClose} aria-label="Close settings"><X size={17} /></button>
          </header>
          <div class="settings-body">
            <div class="settings-section-heading">
              <div>
                <h3>Coding agents</h3>
                <p>zAI uses your existing CLI login and configuration.</p>
              </div>
              <button class="secondary-button" onClick={() => props.onRefresh()}>
                <RefreshCw size={13} /> Refresh
              </button>
            </div>
            <div class="provider-settings-list">
              <For each={props.providers.filter((provider) => provider.id !== "openrouter")}>
                {(provider) => (
                  <div class="provider-settings-row">
                    <ProviderBadge provider={provider.id} />
                    <div class="provider-settings-copy">
                      <strong>{provider.name}</strong>
                      <span>{provider.version ?? provider.transport}</span>
                      <Show when={provider.executablePath}><code>{provider.executablePath}</code></Show>
                    </div>
                    <Show
                      when={provider.available}
                      fallback={<a class="install-link" href={provider.installUrl} target="_blank">Install <ExternalLink size={12} /></a>}
                    >
                      <span class="connected-pill"><Check size={12} /> Ready</span>
                    </Show>
                  </div>
                )}
              </For>
            </div>

            <div class="settings-divider" />
            <div class="settings-section-heading">
              <div>
                <h3>OpenRouter</h3>
                <p>The API key is stored in your operating system keychain and never returned to this window.</p>
              </div>
              <Show when={props.openRouter.connected}><span class="connected-pill"><Check size={12} /> Connected</span></Show>
            </div>
            <Show
              when={!props.openRouter.connected}
              fallback={
                <div class="openrouter-connected">
                  <KeyRound size={17} />
                  <div><strong>API key connected</strong><span>Models are loaded directly from OpenRouter.</span></div>
                  <button class="danger-text-button" disabled={saving()} onClick={disconnect}>Disconnect</button>
                </div>
              }
            >
              <div class="key-field">
                <KeyRound size={15} />
                <input
                  type="password"
                  autocomplete="off"
                  value={key()}
                  onInput={(event) => setKey(event.currentTarget.value)}
                  placeholder="sk-or-v1-…"
                  onKeyDown={(event) => event.key === "Enter" && key().trim() && connect()}
                />
                <button class="primary-button" disabled={!key().trim() || saving()} onClick={connect}>
                  <Show when={saving()} fallback="Connect"><LoaderCircle class="spin" size={14} /></Show>
                </button>
              </div>
            </Show>
            <Show when={message()}><p class="settings-message">{message()}</p></Show>

            <div class="settings-divider" />
            <div class="about-zai">
              <img src="/zai.svg" alt="" />
              <div>
                <h3>About zAI</h3>
                <p>
                  An independent MIT-licensed project. Interface direction inspired by
                  {" "}<a href="https://github.com/anomalyco/opencode" target="_blank">OpenCode</a>;
                  provider architecture inspired by
                  {" "}<a href="https://github.com/pingdotgg/t3code" target="_blank">T3 Code</a>.
                </p>
              </div>
            </div>
          </div>
        </section>
      </div>
    </Show>
  )
}
