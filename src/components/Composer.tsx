import { createMemo, createSignal, For, Show, type Component } from "solid-js"
import { ArrowUp, ChevronDown, FolderOpen, LoaderCircle, Square } from "lucide-solid"
import { api } from "../lib/api"
import { providerMeta, workspaceName } from "../lib/providers"
import type { OpenRouterModel, ProviderId, ProviderStatus } from "../lib/types"
import { ProviderBadge } from "./ProviderBadge"

export const Composer: Component<{
  provider: ProviderId
  model: string
  workspace: string
  providers: ProviderStatus[]
  openRouterModels: OpenRouterModel[]
  locked?: boolean
  running?: boolean
  onProvider: (provider: ProviderId) => void
  onModel: (model: string) => void
  onWorkspace: (workspace: string) => void
  onSubmit: (content: string) => Promise<void>
  onCancel?: () => void
}> = (props) => {
  const [content, setContent] = createSignal("")
  const [submitting, setSubmitting] = createSignal(false)
  let textarea!: HTMLTextAreaElement

  const providerStatus = createMemo(() => props.providers.find((item) => item.id === props.provider))
  const modelOptions = createMemo(() =>
    props.provider === "openrouter"
      ? props.openRouterModels.map((model) => ({ value: model.id, label: model.name }))
      : providerMeta[props.provider].models.map((model) => ({
          value: model,
          label: model === "default" ? "Default model" : model[0].toUpperCase() + model.slice(1),
        })),
  )

  const resize = () => {
    textarea.style.height = "0px"
    textarea.style.height = `${Math.min(textarea.scrollHeight, 180)}px`
  }

  const submit = async () => {
    const value = content().trim()
    if (!value || submitting() || props.running) return
    setSubmitting(true)
    try {
      await props.onSubmit(value)
      setContent("")
      queueMicrotask(resize)
    } catch {
      // The application surfaces provider errors in its shared toast.
    } finally {
      setSubmitting(false)
    }
  }

  const chooseWorkspace = async () => {
    const workspace = await api.chooseWorkspace()
    if (workspace) props.onWorkspace(workspace)
  }

  return (
    <div class="composer-shell">
      <textarea
        ref={textarea}
        class="composer-input"
        rows={1}
        value={content()}
        placeholder={props.workspace ? "Ask zAI to build, explain, or fix something…" : "Choose a workspace, then tell zAI what to build…"}
        onInput={(event) => {
          setContent(event.currentTarget.value)
          resize()
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
            event.preventDefault()
            void submit()
          }
        }}
      />
      <div class="composer-footer">
        <div class="composer-controls">
          <button class="composer-workspace" disabled={props.locked} onClick={chooseWorkspace} title={props.workspace || "Choose workspace"}>
            <FolderOpen size={14} />
            <span>{props.workspace ? workspaceName(props.workspace) : "Choose folder"}</span>
          </button>

          <label class="select-control" classList={{ unavailable: providerStatus()?.available === false }}>
            <ProviderBadge provider={props.provider} size="sm" />
            <select
              aria-label="Provider"
              value={props.provider}
              disabled={props.locked}
              onChange={(event) => props.onProvider(event.currentTarget.value as ProviderId)}
            >
              <For each={props.providers}>
                {(provider) => <option value={provider.id}>{provider.name}{provider.available ? "" : " — unavailable"}</option>}
              </For>
            </select>
            <ChevronDown size={12} />
          </label>

          <Show when={modelOptions().length > 0}>
            <label class="select-control model-control">
              <select aria-label="Model" value={props.model} disabled={props.locked} onChange={(event) => props.onModel(event.currentTarget.value)}>
                <Show when={props.provider === "openrouter" && !props.model}>
                  <option value="">Choose model</option>
                </Show>
                <For each={modelOptions()}>{(model) => <option value={model.value}>{model.label}</option>}</For>
              </select>
              <ChevronDown size={12} />
            </label>
          </Show>
        </div>

        <Show
          when={props.running}
          fallback={
            <button
              class="send-button"
              disabled={!content().trim() || submitting() || !props.workspace || providerStatus()?.available === false}
              onClick={submit}
              aria-label="Send message"
            >
              <Show when={submitting()} fallback={<ArrowUp size={16} stroke-width={2.4} />}>
                <LoaderCircle class="spin" size={15} />
              </Show>
            </button>
          }
        >
          <button class="send-button stop-button" onClick={props.onCancel} aria-label="Stop agent"><Square size={12} fill="currentColor" /></button>
        </Show>
      </div>
    </div>
  )
}
