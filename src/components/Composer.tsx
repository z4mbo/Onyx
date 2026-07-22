import { createMemo, createSignal, For, Match, Show, Switch, type Component } from "solid-js"
import {
  Bot,
  BrainCircuit,
  ChevronDown,
  LoaderCircle,
  Lock,
  LockOpen,
  PenLine,
  ShieldAlert,
} from "lucide-solid"
import { providerMeta } from "../lib/providers"
import type {
  ApprovalRequest,
  OpenRouterModel,
  ProviderId,
  ProviderStatus,
} from "../lib/types"
import { ProviderBadge } from "./ProviderBadge"
import "./composer.css"

export interface ComposerProps {
  provider: ProviderId
  model: string
  workspace: string
  providers: ProviderStatus[]
  openRouterModels: OpenRouterModel[]
  /** Locks provider, model, and workspace selection for an existing session. */
  locked?: boolean
  /** Switches the primary action from send to stop. */
  running?: boolean
  /** Defaults to true for an unlocked/new-session composer. */
  hero?: boolean
  /** Replaces the editor with an OpenCode-style permission dock. */
  approval?: ApprovalRequest | null
  approvalBusy?: boolean
  placeholder?: string
  autofocus?: boolean
  onProvider: (provider: ProviderId) => void
  onModel: (model: string) => void
  onWorkspace: (workspace: string) => void
  onSubmit: (content: string) => Promise<void>
  onCancel?: () => void | Promise<void>
  onApproval?: (allow: boolean) => void | Promise<void>
}

function modelLabel(model: string) {
  if (model === "default") return "Default model"
  return model
    .split(/[-_/]/g)
    .filter(Boolean)
    .map((part) => part[0]?.toUpperCase() + part.slice(1))
    .join(" ")
}

function compactTokenCount(tokens: number) {
  if (tokens >= 1_000_000) return `${Number((tokens / 1_000_000).toFixed(1))}M`
  if (tokens >= 1_000) return `${Number((tokens / 1_000).toFixed(0))}K`
  return `${tokens}`
}

export const Composer: Component<ComposerProps> = (props) => {
  const [content, setContent] = createSignal("")
  const [submitting, setSubmitting] = createSignal(false)
  const [respondingApproval, setRespondingApproval] = createSignal(false)
  let textarea: HTMLTextAreaElement | undefined

  const hero = createMemo(() => props.hero ?? !props.locked)
  const providerStatus = createMemo(() =>
    props.providers.find((item) => item.id === props.provider),
  )
  const providerName = createMemo(
    () => providerStatus()?.name ?? providerMeta[props.provider].name,
  )
  const modelOptions = createMemo(() =>
    props.provider === "openrouter"
      ? props.openRouterModels.map((model) => ({ value: model.id, label: model.name }))
      : providerMeta[props.provider].models.map((model) => ({
          value: model,
          label: modelLabel(model),
        })),
  )
  const selectedModelLabel = createMemo(
    () => modelOptions().find((option) => option.value === props.model)?.label ?? modelLabel(props.model),
  )
  const selectedOpenRouterModel = createMemo(() =>
    props.provider === "openrouter"
      ? props.openRouterModels.find((model) => model.id === props.model)
      : undefined,
  )
  const providerModelOptions = createMemo(() => props.providers.flatMap((provider) => {
    const models = provider.id === "openrouter"
      ? props.openRouterModels.map((model) => ({ value: model.id, label: model.name }))
      : providerMeta[provider.id].models.map((model) => ({ value: model, label: modelLabel(model) }))
    const options = models.length > 0 ? models : [{ value: "default", label: "Default model" }]
    return options.map((model) => ({
      value: `${provider.id}\u0000${model.value}`,
      provider: provider.id,
      model: model.value,
      label: model.value === "default" ? provider.name : `${provider.name} — ${model.label}`,
      disabled: !provider.available || (provider.id === "openrouter" && props.openRouterModels.length === 0),
    }))
  }))
  const providerModelValue = () => `${props.provider}\u0000${props.model || "default"}`
  const providerModelLabel = createMemo(() =>
    props.model && props.model !== "default" ? selectedModelLabel() : providerName(),
  )
  const accessMode = createMemo(() => {
    if (props.provider === "gemini") {
      return {
        label: "Auto edit",
        kind: "edit" as const,
        hint: "Gemini CLI can edit files in the trusted workspace without a zAI prompt.",
      }
    }
    if (props.provider === "kimi") {
      return {
        label: "CLI access",
        kind: "open" as const,
        hint: "Kimi Code controls permissions using its non-interactive CLI policy.",
      }
    }
    return {
      label: "Ask access",
      kind: "locked" as const,
      hint: `${providerName()} requests supported permissions through zAI; CLI fallback policy still applies.`,
    }
  })
  const reasoningMode = createMemo(() => {
    if (props.provider === "claude") {
      return {
        label: "Effort · CLI",
        hint: "Claude Code owns the effort setting in its CLI configuration; zAI does not override it yet.",
      }
    }
    if (props.provider === "codex") {
      return {
        label: "Reasoning · CLI",
        hint: "Codex owns reasoning effort in its CLI configuration; zAI does not override it yet.",
      }
    }
    if (props.provider === "gemini") {
      return {
        label: "Thinking · CLI",
        hint: "Gemini CLI and the selected model own thinking behavior; zAI does not override it yet.",
      }
    }
    if (props.provider === "kimi") {
      return {
        label: "Reasoning · CLI",
        hint: "Kimi Code and the selected model own reasoning behavior; zAI does not override it yet.",
      }
    }
    return {
      label: "Reasoning · model",
      hint: "The selected OpenRouter model owns reasoning behavior; zAI does not send a reasoning override yet.",
    }
  })
  const contextWindow = createMemo(() => {
    const maxTokens = selectedOpenRouterModel()?.contextLength ?? null
    if (maxTokens && maxTokens > 0) {
      return {
        label: `${compactTokenCount(maxTokens)} token context limit; live usage unavailable`,
        hint: `This model advertises a ${compactTokenCount(maxTokens)} token context limit. zAI's normalized provider events do not include live token usage yet, so the meter remains empty.`,
        maxTokens,
      }
    }
    return {
      label: "Context usage unavailable",
      hint: `${providerName()} does not report normalized live token usage to zAI yet, so the context meter remains empty.`,
      maxTokens: null,
    }
  })
  const placeholder = createMemo(() => {
    const requested = props.placeholder?.trim()
    const advertisesUnavailableCommands = requested?.includes("/ for commands") || requested?.includes("@ for context")
    if (requested && !advertisesUnavailableCommands) return requested
    return props.workspace
      ? "Tell zAI what to build…"
      : "Choose a workspace, then tell zAI what to build…"
  })
  const approvalPending = createMemo(() => props.approvalBusy || respondingApproval())
  const sendDisabled = createMemo(
    () =>
      !content().trim() ||
      submitting() ||
      props.running ||
      !!props.approval ||
      !props.workspace ||
      providerStatus()?.available === false ||
      (props.provider === "openrouter" && !props.model),
  )

  const resize = () => {
    if (!textarea) return
    textarea.style.height = "0px"
    textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`
  }

  const submit = async () => {
    const value = content().trim()
    if (!value || sendDisabled()) return
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

  const decideApproval = async (allow: boolean) => {
    if (!props.approval || !props.onApproval || approvalPending()) return
    setRespondingApproval(true)
    try {
      await props.onApproval(allow)
    } catch {
      // The application owns approval error presentation.
    } finally {
      setRespondingApproval(false)
    }
  }

  return (
    <div
      classList={{
        "zai-composer": true,
        "t3-composer": true,
        "zai-composer--hero": hero(),
        "zai-composer--docked": !hero(),
        "zai-composer--running": !!props.running,
        "zai-composer--approval": !!props.approval,
      }}
      data-component="zai-composer"
      data-layout={hero() ? "hero" : "docked"}
      data-provider={props.provider}
    >
      <Show
        when={props.approval}
        keyed
        fallback={
          <form
            class="zai-composer__frame"
            data-component="prompt-input-v2"
            data-dock-border-underlay="v2"
            data-chat-composer-form="true"
            aria-busy={submitting()}
            onSubmit={(event) => {
              event.preventDefault()
              void submit()
            }}
          >
            <div class="zai-composer__surface">
              <div class="zai-composer__editor-region">
                <textarea
                  ref={textarea}
                  class="zai-composer__editor"
                  data-component="prompt-input"
                  rows={1}
                  value={content()}
                  aria-label="Prompt"
                  aria-multiline="true"
                  aria-keyshortcuts="Enter Shift+Enter Escape"
                  autocomplete="off"
                  autocapitalize="sentences"
                  spellcheck
                  autofocus={props.autofocus ?? hero()}
                  placeholder={placeholder()}
                  onInput={(event) => {
                    setContent(event.currentTarget.value)
                    resize()
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Escape" && props.running && props.onCancel) {
                      event.preventDefault()
                      void props.onCancel()
                      return
                    }
                    if (event.key !== "Enter" || event.shiftKey || event.isComposing) return
                    event.preventDefault()
                    if (event.repeat || props.running) return
                    void submit()
                  }}
                />
              </div>

              <div class="zai-composer__footer">
                <div class="zai-composer__controls">
                  <label
                    class="zai-composer__control zai-composer__select-control zai-composer__provider-model"
                    data-unavailable={providerStatus()?.available === false ? "true" : undefined}
                    title={
                      providerStatus()?.available === false
                        ? `${providerName()} is unavailable`
                        : `${providerName()} · ${selectedModelLabel()}`
                    }
                  >
                    <ProviderBadge provider={props.provider} size="sm" />
                    <span class="zai-composer__control-label">{providerModelLabel()}</span>
                    <ChevronDown aria-hidden="true" class="zai-composer__chevron" size={12} />
                    <select
                      class="zai-composer__native-select"
                      aria-label="Provider and model"
                      value={providerModelValue()}
                      disabled={props.locked}
                      onChange={(event) => {
                        const option = providerModelOptions().find((item) => item.value === event.currentTarget.value)
                        if (!option) return
                        props.onProvider(option.provider)
                        props.onModel(option.model)
                      }}
                    >
                      <For each={providerModelOptions()}>
                        {(option) => (
                          <option value={option.value} disabled={option.disabled}>
                            {option.label}{option.disabled ? " — unavailable" : ""}
                          </option>
                        )}
                      </For>
                    </select>
                  </label>

                  <span class="zai-composer__separator" aria-hidden="true" />

                  <span
                    class="zai-composer__control zai-composer__mode zai-composer__reasoning"
                    role="button"
                    aria-disabled="true"
                    aria-label={`${reasoningMode().label}. ${reasoningMode().hint}`}
                    title={reasoningMode().hint}
                  >
                    <BrainCircuit aria-hidden="true" size={16} />
                    <span>{reasoningMode().label}</span>
                    <ChevronDown aria-hidden="true" class="zai-composer__chevron" size={12} />
                  </span>

                  <span class="zai-composer__separator" aria-hidden="true" />

                  <span
                    class="zai-composer__control zai-composer__mode"
                    title="Build mode"
                    aria-label="Build mode"
                  >
                    <Bot aria-hidden="true" size={16} />
                    <span>Build</span>
                  </span>

                  <span class="zai-composer__separator" aria-hidden="true" />

                  <span
                    class="zai-composer__control zai-composer__mode"
                    title={accessMode().hint}
                    aria-label={`${accessMode().label}. ${accessMode().hint}`}
                  >
                    <Switch>
                      <Match when={accessMode().kind === "edit"}>
                        <PenLine aria-hidden="true" size={16} />
                      </Match>
                      <Match when={accessMode().kind === "open"}>
                        <LockOpen aria-hidden="true" size={16} />
                      </Match>
                      <Match when={true}>
                        <Lock aria-hidden="true" size={16} />
                      </Match>
                    </Switch>
                    <span>{accessMode().label}</span>
                  </span>
                </div>

                <div class="zai-composer__primary-actions">
                  <span
                    class="zai-composer__context-meter"
                    data-telemetry="unavailable"
                    data-context-limit={contextWindow().maxTokens ?? undefined}
                    role="img"
                    aria-label={contextWindow().label}
                    title={contextWindow().hint}
                  >
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <circle cx="12" cy="12" r="9.75" />
                      <path d="M9 12h6" />
                    </svg>
                  </span>

                  <Show
                    when={props.running}
                    fallback={
                      <button
                        type="submit"
                        class="zai-composer__submit"
                        disabled={sendDisabled()}
                        aria-label="Send message"
                        title="Send (Enter)"
                      >
                        <Show
                          when={submitting()}
                          fallback={
                            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
                              <path
                                d="M7 11.5V2.5M7 2.5L3 6.5M7 2.5L11 6.5"
                                stroke="currentColor"
                                stroke-width="1.8"
                                stroke-linecap="round"
                                stroke-linejoin="round"
                              />
                            </svg>
                          }
                        >
                          <LoaderCircle aria-hidden="true" class="zai-composer__spinner" size={15} />
                        </Show>
                      </button>
                    }
                  >
                    <button
                      type="button"
                      class="zai-composer__submit zai-composer__stop"
                      disabled={!props.onCancel}
                      onClick={() => void props.onCancel?.()}
                      aria-label="Stop generation"
                      title="Stop (Esc)"
                    >
                      <svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
                        <rect x="2" y="2" width="8" height="8" rx="1.5" />
                      </svg>
                    </button>
                  </Show>
                </div>
              </div>
            </div>
          </form>
        }
      >
        {(approval) => (
          <div class="zai-composer__frame zai-composer__frame--permission">
            <div
              class="zai-composer__permission"
              data-component="dock-prompt"
              data-kind="permission"
              role="group"
              aria-labelledby={`zai-approval-${approval.id}`}
              aria-busy={approvalPending()}
            >
              <div
                class="zai-composer__permission-body"
                data-dock-surface="shell"
                data-dock-border-underlay="v2"
              >
                <div class="zai-composer__permission-header" data-slot="permission-header">
                  <span class="zai-composer__permission-icon" aria-hidden="true">
                    <ShieldAlert size={17} />
                  </span>
                  <div>
                    <span class="zai-composer__eyebrow">Permission required</span>
                    <strong id={`zai-approval-${approval.id}`}>{approval.title}</strong>
                  </div>
                </div>
                <pre class="zai-composer__permission-detail">{approval.detail}</pre>
              </div>
              <div
                class="zai-composer__permission-tray"
                data-dock-surface="tray"
                data-dock-attach="top"
              >
                <span class="zai-composer__permission-risk">{approval.risk}</span>
                <div class="zai-composer__permission-actions">
                  <button
                    type="button"
                    class="zai-composer__permission-button zai-composer__permission-button--deny"
                    disabled={!props.onApproval || approvalPending()}
                    onClick={() => void decideApproval(false)}
                  >
                    Deny
                  </button>
                  <button
                    type="button"
                    class="zai-composer__permission-button zai-composer__permission-button--allow"
                    disabled={!props.onApproval || approvalPending()}
                    onClick={() => void decideApproval(true)}
                  >
                    <Show when={approvalPending()} fallback="Allow once">
                      <LoaderCircle aria-hidden="true" class="zai-composer__spinner" size={14} />
                      Responding…
                    </Show>
                  </button>
                </div>
              </div>
            </div>
          </div>
        )}
      </Show>
    </div>
  )
}
