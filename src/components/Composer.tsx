import { createMemo, createSignal, For, Match, Show, Switch, type Component } from "solid-js"
import {
  Bot,
  BrainCircuit,
  Check,
  ChevronDown,
  Gauge,
  LoaderCircle,
  Lock,
  LockOpen,
  PenLine,
  PencilRuler,
  Plus,
  ShieldAlert,
  TimerReset,
  Zap,
} from "lucide-solid"
import {
  accessModes,
  brandStatus,
  modelsForBrand,
  providerBrands,
} from "../lib/providers"
import type {
  AccessMode,
  ApprovalRequest,
  ContextUsage,
  InteractionMode,
  OpenRouterModel,
  ProviderBrand,
  ProviderId,
  ProviderModelOption,
  ProviderStatus,
  ProviderUsage,
  ReasoningEffort,
  SpeedMode,
} from "../lib/types"
import { ProviderBadge } from "./ProviderBadge"
import "./composer.css"

export interface ComposerProps {
  provider: ProviderId
  brand: ProviderBrand
  model: string
  reasoning: ReasoningEffort | null
  speedMode: SpeedMode
  interactionMode: InteractionMode
  accessMode: AccessMode
  workspace: string
  providers: ProviderStatus[]
  providerModels: Partial<Record<ProviderId, ProviderModelOption[]>>
  openRouterModels: OpenRouterModel[]
  contextUsage?: ContextUsage | null
  providerUsage?: ProviderUsage | null
  locked?: boolean
  running?: boolean
  hero?: boolean
  approval?: ApprovalRequest | null
  approvalBusy?: boolean
  placeholder?: string
  autofocus?: boolean
  onBrand: (brand: ProviderBrand) => void
  onModel: (model: string) => void
  onReasoning: (reasoning: ReasoningEffort) => void
  onSpeedMode: (mode: SpeedMode) => void
  onInteractionMode: (mode: InteractionMode) => void
  onAccessMode: (mode: AccessMode) => void
  onWorkspace: (workspace: string) => void
  onAttach?: () => Promise<string[]>
  onSubmit: (content: string) => Promise<void>
  onCancel?: () => void | Promise<void>
  onApproval?: (allow: boolean) => void | Promise<void>
}

function compactTokenCount(tokens: number) {
  if (tokens >= 1_000_000) return `${Number((tokens / 1_000_000).toFixed(1))}M`
  if (tokens >= 1_000) return `${Number((tokens / 1_000).toFixed(0))}K`
  return `${tokens}`
}

function titleCase(value: string) {
  if (value === "xhigh") return "Extra high"
  return value.charAt(0).toUpperCase() + value.slice(1)
}

export const Composer: Component<ComposerProps> = (props) => {
  const [content, setContent] = createSignal("")
  const [submitting, setSubmitting] = createSignal(false)
  const [respondingApproval, setRespondingApproval] = createSignal(false)
  const [usageOpen, setUsageOpen] = createSignal(false)
  let textarea: HTMLTextAreaElement | undefined

  const hero = createMemo(() => props.hero ?? !props.locked)
  const selectedBrand = createMemo(() => providerBrands.find((item) => item.id === props.brand)!)
  const providerStatus = createMemo(() => brandStatus(props.brand, props.providers))
  const modelOptions = createMemo(() => modelsForBrand(props.brand, props.providerModels, props.openRouterModels))
  const selectedModel = createMemo(() => modelOptions().find((item) => item.id === props.model) ?? modelOptions()[0])
  const reasoningOptions = createMemo(() => selectedModel()?.reasoning ?? [])
  const speedOptions = createMemo(() => selectedModel()?.speeds ?? ["standard"])
  const selectedAccess = createMemo(() => accessModes.find((item) => item.id === props.accessMode) ?? accessModes[0])
  const accessIcon = createMemo(() => props.accessMode === "full_access" ? LockOpen : props.accessMode === "auto_accept_edits" ? PenLine : Lock)
  const contextLimit = createMemo(() => props.contextUsage?.maxTokens ?? selectedModel()?.contextLength ?? null)
  const contextPercent = createMemo(() => {
    const limit = contextLimit()
    return limit ? Math.min(100, Math.max(0, ((props.contextUsage?.usedTokens ?? 0) / limit) * 100)) : 0
  })
  const placeholder = createMemo(() => props.placeholder?.trim() || (props.workspace ? "Tell Onyx what to build…" : "Choose a project, then tell Onyx what to build…"))
  const approvalPending = createMemo(() => props.approvalBusy || respondingApproval())
  const sendDisabled = createMemo(() =>
    !content().trim() || submitting() || props.running || !!props.approval ||
    providerStatus()?.available === false || !props.model,
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
    } finally {
      setSubmitting(false)
    }
  }

  const decideApproval = async (allow: boolean) => {
    if (!props.approval || !props.onApproval || approvalPending()) return
    setRespondingApproval(true)
    try {
      await props.onApproval(allow)
    } finally {
      setRespondingApproval(false)
    }
  }

  const attachFiles = async () => {
    if (!props.onAttach || submitting() || props.running) return
    const paths = await props.onAttach()
    if (!paths.length) return
    const references = paths.map((path) => `@${path.includes(" ") ? `"${path}"` : path}`).join(" ")
    setContent((value) => `${value}${value.trim() ? "\n" : ""}${references}`)
    queueMicrotask(() => {
      resize()
      textarea?.focus()
    })
  }

  const changeBrand = (brand: ProviderBrand) => {
    if (props.locked) return
    props.onBrand(brand)
  }

  return (
    <div classList={{
      "zai-composer": true,
      "t3-composer": true,
      "zai-composer--hero": hero(),
      "zai-composer--docked": !hero(),
      "zai-composer--running": !!props.running,
      "zai-composer--approval": !!props.approval,
    }} data-component="onyx-composer" data-layout={hero() ? "hero" : "docked"} data-provider={props.provider}>
      <Show when={props.approval} keyed fallback={
        <form class="zai-composer__frame" data-component="prompt-input-v2" onSubmit={(event) => { event.preventDefault(); void submit() }}>
          <div class="zai-composer__surface">
            <div class="zai-composer__editor-region">
              <textarea
                ref={textarea}
                class="zai-composer__editor"
                rows={1}
                value={content()}
                aria-label="Prompt"
                autocomplete="off"
                spellcheck
                autofocus={props.autofocus ?? hero()}
                placeholder={placeholder()}
                onInput={(event) => { setContent(event.currentTarget.value); resize() }}
                onKeyDown={(event) => {
                  if (event.key === "Escape" && props.running && props.onCancel) { event.preventDefault(); void props.onCancel(); return }
                  if (event.key !== "Enter" || event.shiftKey || event.isComposing) return
                  event.preventDefault()
                  if (!event.repeat && !props.running) void submit()
                }}
              />
            </div>

            <div class="zai-composer__footer">
              <div class="zai-composer__controls">
                <button type="button" class="zai-composer__control zai-composer__attach" disabled={!props.onAttach || submitting() || props.running} onClick={() => void attachFiles()} aria-label="Attach files" title="Attach files">
                  <Plus aria-hidden="true" size={18} />
                </button>

                <label class="zai-composer__control zai-composer__select-control zai-composer__provider-tile" title={providerStatus()?.available === false ? `${selectedBrand().name} is unavailable` : selectedBrand().name}>
                  <ProviderBadge brand={props.brand} size="sm" />
                  <span class="zai-composer__control-label">{selectedBrand().name}</span>
                  <ChevronDown aria-hidden="true" size={12} />
                  <select class="zai-composer__native-select" aria-label="Provider" value={props.brand} disabled={props.locked} onChange={(event) => changeBrand(event.currentTarget.value as ProviderBrand)}>
                    <For each={providerBrands}>{(brand) => {
                      const status = () => brandStatus(brand.id, props.providers)
                      return <option value={brand.id} disabled={!status()?.available}>{brand.name}{!status()?.available ? " — unavailable" : ""}</option>
                    }}</For>
                  </select>
                </label>

                <label class="zai-composer__control zai-composer__select-control zai-composer__model-tile" title={selectedModel()?.description ?? selectedModel()?.name}>
                  <span class="zai-composer__control-label">{selectedModel()?.name ?? "Choose model"}</span>
                  <ChevronDown aria-hidden="true" size={12} />
                  <select class="zai-composer__native-select" aria-label="Model" value={props.model} disabled={props.locked || modelOptions().length === 0} onChange={(event) => props.onModel(event.currentTarget.value)}>
                    <For each={modelOptions()}>{(model) => <option value={model.id}>{model.name}</option>}</For>
                  </select>
                </label>

                <Show when={reasoningOptions().length > 0}>
                  <label class="zai-composer__control zai-composer__select-control" title="Reasoning effort advertised for this model">
                    <BrainCircuit aria-hidden="true" size={15} />
                    <span>{titleCase(props.reasoning ?? selectedModel()?.defaultReasoning ?? "medium")}</span>
                    <ChevronDown aria-hidden="true" size={12} />
                    <select class="zai-composer__native-select" aria-label="Reasoning" value={props.reasoning ?? selectedModel()?.defaultReasoning ?? reasoningOptions()[0]} disabled={props.locked} onChange={(event) => props.onReasoning(event.currentTarget.value as ReasoningEffort)}>
                      <For each={reasoningOptions()}>{(effort) => <option value={effort}>{titleCase(effort)}</option>}</For>
                    </select>
                  </label>
                </Show>

                <Show when={speedOptions().length > 1}>
                  <label class="zai-composer__control zai-composer__select-control" title="Model service tier">
                    <Zap aria-hidden="true" size={14} />
                    <span>{props.speedMode === "fast" ? "Fast" : "Standard"}</span>
                    <ChevronDown aria-hidden="true" size={12} />
                    <select class="zai-composer__native-select" aria-label="Speed" value={props.speedMode} disabled={props.locked} onChange={(event) => props.onSpeedMode(event.currentTarget.value as SpeedMode)}>
                      <For each={speedOptions()}>{(speed) => <option value={speed}>{speed === "fast" ? "Fast" : "Standard"}</option>}</For>
                    </select>
                  </label>
                </Show>

                <button type="button" classList={{ "zai-composer__control": true, "zai-composer__mode-toggle": true, active: props.interactionMode === "plan" }} onClick={() => !props.locked && props.onInteractionMode(props.interactionMode === "plan" ? "build" : "plan")} aria-label={props.interactionMode === "plan" ? "Plan mode. Click for Build mode" : "Build mode. Click for Plan mode"}>
                  <Show when={props.interactionMode === "plan"} fallback={<Bot aria-hidden="true" size={15} />}><PencilRuler aria-hidden="true" size={15} /></Show>
                  <span>{props.interactionMode === "plan" ? "Plan" : "Build"}</span>
                </button>

                <label class="zai-composer__control zai-composer__select-control" title={selectedAccess().description}>
                  {(() => { const Icon = accessIcon(); return <Icon aria-hidden="true" size={15} /> })()}
                  <span>{selectedAccess().name}</span>
                  <ChevronDown aria-hidden="true" size={12} />
                  <select class="zai-composer__native-select" aria-label="Access" value={props.accessMode} disabled={props.locked} onChange={(event) => props.onAccessMode(event.currentTarget.value as AccessMode)}>
                    <For each={accessModes}>{(mode) => <option value={mode.id}>{mode.name}</option>}</For>
                  </select>
                </label>
              </div>

              <div class="zai-composer__primary-actions">
                <div class="zai-context-popover-wrap">
                  <button type="button" class="zai-composer__context-meter" style={{ "--context-progress": `${contextPercent() * 3.6}deg` }} aria-label="Context and usage" aria-expanded={usageOpen()} onClick={() => setUsageOpen((value) => !value)}>
                    <Gauge aria-hidden="true" size={15} />
                  </button>
                  <button type="button" class="zai-composer__usage-meter" aria-label="Subscription usage limits" aria-expanded={usageOpen()} onClick={() => setUsageOpen((value) => !value)}>
                    <TimerReset aria-hidden="true" size={13} />
                    <span>{props.providerUsage?.windows?.[0] ? `${props.providerUsage.windows[0].label} ${Math.round(props.providerUsage.windows[0].usedPercent)}%` : "Usage —"}</span>
                  </button>
                  <Show when={usageOpen()}>
                    <div class="zai-context-popover" role="dialog" aria-label="Context and usage limits">
                      <div class="zai-context-popover__header"><strong>Context & usage</strong><span>{props.providerUsage?.plan ?? selectedBrand().name}</span></div>
                      <div class="zai-context-popover__row"><span>Context window</span><strong>{props.contextUsage ? `${compactTokenCount(props.contextUsage.usedTokens)} / ${contextLimit() ? compactTokenCount(contextLimit()!) : "—"}` : contextLimit() ? `0 / ${compactTokenCount(contextLimit()!)}` : "Not reported"}</strong></div>
                      <Show when={props.providerUsage?.windows?.length} fallback={<p>Usage limits are shown only when the connected subscription reports them.</p>}>
                        <For each={props.providerUsage!.windows}>{(window) => (
                          <div class="zai-usage-window">
                            <div><span>{window.label}</span><strong>{Math.round(window.usedPercent)}%</strong></div>
                            <i><b style={{ width: `${Math.min(100, window.usedPercent)}%` }} /></i>
                          </div>
                        )}</For>
                      </Show>
                    </div>
                  </Show>
                </div>

                <Show when={props.running} fallback={
                  <button type="submit" class="zai-composer__submit" disabled={sendDisabled()} aria-label="Send message" title="Send (Enter)">
                    <Show when={submitting()} fallback={<svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true"><path d="M7 11.5V2.5M7 2.5L3 6.5M7 2.5L11 6.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg>}>
                      <LoaderCircle aria-hidden="true" class="zai-composer__spinner" size={15} />
                    </Show>
                  </button>
                }>
                  <button type="button" class="zai-composer__submit zai-composer__stop" disabled={!props.onCancel} onClick={() => void props.onCancel?.()} aria-label="Stop generation" title="Stop (Esc)"><svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true"><rect x="2" y="2" width="8" height="8" rx="1.5" /></svg></button>
                </Show>
              </div>
            </div>
          </div>
        </form>
      }>
        {(approval) => (
          <div class="zai-composer__frame zai-composer__frame--permission">
            <div class="zai-composer__permission" role="group" aria-labelledby={`onyx-approval-${approval.id}`} aria-busy={approvalPending()}>
              <div class="zai-composer__permission-body">
                <div class="zai-composer__permission-header"><span class="zai-composer__permission-icon" aria-hidden="true"><ShieldAlert size={17} /></span><div><span class="zai-composer__eyebrow">Permission required</span><strong id={`onyx-approval-${approval.id}`}>{approval.title}</strong></div></div>
                <pre class="zai-composer__permission-detail">{approval.detail}</pre>
              </div>
              <div class="zai-composer__permission-tray"><span class="zai-composer__permission-risk">{approval.risk}</span><div class="zai-composer__permission-actions">
                <button type="button" class="zai-composer__permission-button" disabled={!props.onApproval || approvalPending()} onClick={() => void decideApproval(false)}>Deny</button>
                <button type="button" class="zai-composer__permission-button zai-composer__permission-button--allow" disabled={!props.onApproval || approvalPending()} onClick={() => void decideApproval(true)}><Show when={approvalPending()} fallback={<><Check size={14} /> Allow once</>}><LoaderCircle class="zai-composer__spinner" size={14} /> Responding…</Show></button>
              </div></div>
            </div>
          </div>
        )}
      </Show>
    </div>
  )
}
