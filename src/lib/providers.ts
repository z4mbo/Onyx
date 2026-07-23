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
    models: ["fable", "opus", "sonnet", "haiku"],
  },
  codex: {
    name: "OpenAI",
    color: "#10a37f",
    models: ["default"],
  },
  gemini: {
    name: "Google",
    color: "#4285f4",
    models: ["default"],
  },
  kimi: {
    name: "Moonshot AI",
    color: "#5b5bd6",
    models: ["default"],
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
    namedModel("fable", "Claude Fable", { isDefault: true, reasoning: ["low", "medium", "high", "xhigh", "max"] }),
    namedModel("opus", "Claude Opus", { reasoning: ["low", "medium", "high", "xhigh", "max"] }),
    namedModel("sonnet", "Claude Sonnet", { reasoning: ["low", "medium", "high", "xhigh", "max"] }),
    namedModel("haiku", "Claude Haiku", { reasoning: ["low", "medium", "high"] }),
  ],
  gemini: [
    namedModel("default", "Gemini CLI default", { isDefault: true, reasoning: [], defaultReasoning: null }),
  ],
  kimi: [
    namedModel("default", "Kimi configured default", { isDefault: true, reasoning: [], defaultReasoning: null }),
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
        reasoning: [],
        defaultReasoning: null,
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
