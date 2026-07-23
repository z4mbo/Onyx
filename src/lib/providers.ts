import type {
  AccessMode,
  InteractionMode,
  OpenRouterModel,
  ProviderBrand,
  ProviderId,
  ProviderModelOption,
  ProviderStatus,
  ReasoningEffort,
  SpeedMode,
} from "./types"

export interface ProviderBrandMeta {
  id: ProviderBrand
  name: string
  runtime: ProviderId
  color: string
  modelPrefix?: string
}

export const providerBrands: ProviderBrandMeta[] = [
  { id: "openai", name: "OpenAI", runtime: "codex", color: "#10a37f" },
  { id: "anthropic", name: "Anthropic", runtime: "claude", color: "#d97757" },
  { id: "google", name: "Google", runtime: "gemini", color: "#4285f4" },
  { id: "xai", name: "xAI", runtime: "openrouter", color: "#111111", modelPrefix: "x-ai/" },
  { id: "moonshot", name: "Moonshot AI", runtime: "kimi", color: "#5b5bd6" },
  { id: "openrouter", name: "OpenRouter", runtime: "openrouter", color: "#6d5dfc" },
]

export const providerMeta: Record<ProviderId, { name: string; color: string; models: string[] }> = {
  claude: {
    name: "Anthropic",
    color: "#d97757",
    models: ["claude-fable-5", "claude-opus-4-8", "claude-opus-4-7", "claude-sonnet-5", "claude-sonnet-4-6", "claude-haiku-4-5"],
  },
  codex: {
    name: "OpenAI",
    color: "#10a37f",
    models: ["default"],
  },
  gemini: {
    name: "Google",
    color: "#4285f4",
    models: ["default", "gemini-3.1-pro", "gemini-3-flash"],
  },
  kimi: {
    name: "Moonshot AI",
    color: "#5b5bd6",
    models: ["default", "kimi-k2.5", "kimi-k2-thinking"],
  },
  openrouter: {
    name: "OpenRouter",
    color: "#6d5dfc",
    models: [],
  },
}

const DEFAULT_REASONING: ReasoningEffort[] = ["low", "medium", "high"]

function namedModel(id: string, name: string, overrides: Partial<ProviderModelOption> = {}): ProviderModelOption {
  return {
    id,
    name,
    description: null,
    isDefault: false,
    reasoning: DEFAULT_REASONING,
    defaultReasoning: "medium",
    speeds: ["standard"],
    defaultSpeed: "standard",
    contextLength: null,
    ...overrides,
  }
}

export const fallbackModels: Record<Exclude<ProviderId, "openrouter">, ProviderModelOption[]> = {
  codex: [namedModel("default", "Codex default", { isDefault: true })],
  claude: [
    namedModel("claude-fable-5", "Claude Fable 5", { isDefault: true, reasoning: ["low", "medium", "high", "xhigh", "max"] }),
    namedModel("claude-opus-4-8", "Claude Opus 4.8", { reasoning: ["low", "medium", "high", "xhigh", "max"], speeds: ["standard", "fast"] }),
    namedModel("claude-opus-4-7", "Claude Opus 4.7", { reasoning: ["low", "medium", "high", "xhigh", "max"], speeds: ["standard", "fast"] }),
    namedModel("claude-sonnet-5", "Claude Sonnet 5", { reasoning: ["low", "medium", "high", "xhigh", "max"] }),
    namedModel("claude-sonnet-4-6", "Claude Sonnet 4.6", { reasoning: ["low", "medium", "high", "xhigh", "max"] }),
    namedModel("claude-haiku-4-5", "Claude Haiku 4.5", { reasoning: ["low", "medium", "high"] }),
  ],
  gemini: [
    namedModel("default", "Gemini default", { isDefault: true }),
    namedModel("gemini-3.1-pro", "Gemini 3.1 Pro", { reasoning: ["low", "medium", "high"] }),
    namedModel("gemini-3-flash", "Gemini 3 Flash", { reasoning: ["minimal", "low", "medium", "high"], speeds: ["standard", "fast"] }),
  ],
  kimi: [
    namedModel("default", "Kimi default", { isDefault: true, reasoning: ["medium", "high"] }),
    namedModel("kimi-k2.5", "Kimi K2.5", { reasoning: ["medium", "high"] }),
    namedModel("kimi-k2-thinking", "Kimi K2 Thinking", { reasoning: ["high"] }),
  ],
}

export const accessModes: Array<{ id: AccessMode; name: string; description: string }> = [
  { id: "approval_required", name: "Supervised", description: "Ask before commands and file changes." },
  { id: "auto_accept_edits", name: "Auto-accept edits", description: "Apply file edits automatically; ask before riskier actions." },
  { id: "full_access", name: "Full access", description: "Allow commands and edits without prompts." },
]

export const interactionModes: Array<{ id: InteractionMode; name: string }> = [
  { id: "build", name: "Build" },
  { id: "plan", name: "Plan" },
]

export function runtimeForBrand(brand: ProviderBrand): ProviderId {
  return providerBrands.find((item) => item.id === brand)?.runtime ?? "openrouter"
}

export function brandForRuntime(provider: ProviderId): ProviderBrand {
  return ({ codex: "openai", claude: "anthropic", gemini: "google", kimi: "moonshot", openrouter: "openrouter" } as const)[provider]
}

export function brandStatus(brand: ProviderBrand, statuses: ProviderStatus[]) {
  const meta = providerBrands.find((item) => item.id === brand)!
  return statuses.find((status) => status.id === meta.runtime)
}

export function modelsForBrand(
  brand: ProviderBrand,
  catalogs: Partial<Record<ProviderId, ProviderModelOption[]>>,
  openRouterModels: OpenRouterModel[],
): ProviderModelOption[] {
  const meta = providerBrands.find((item) => item.id === brand)!
  if (meta.runtime === "openrouter") {
    return openRouterModels
      .filter((model) => !meta.modelPrefix || model.id.startsWith(meta.modelPrefix))
      .map((model, index) => ({
        id: model.id,
        name: model.name,
        description: model.description ?? null,
        isDefault: index === 0,
        reasoning: DEFAULT_REASONING,
        defaultReasoning: "medium",
        speeds: ["standard"] as SpeedMode[],
        defaultSpeed: "standard" as SpeedMode,
        contextLength: model.contextLength,
      }))
  }
  return catalogs[meta.runtime] ?? fallbackModels[meta.runtime]
}

export function workspaceName(path: string) {
  const segments = path.replaceAll("\\", "/").split("/").filter(Boolean)
  return segments.at(-1) ?? path
}
