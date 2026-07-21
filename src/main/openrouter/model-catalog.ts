export interface OpenRouterModelInfo {
  id: string
  name: string
  description: string
  contextLength: number
  inputModalities: string[]
  supportsThinking: boolean
  supportsImages: boolean
}

interface RawOpenRouterModel {
  id?: unknown
  name?: unknown
  description?: unknown
  context_length?: unknown
  supported_parameters?: unknown
  architecture?: { input_modalities?: unknown } | null
}

function sanitizeText(value: unknown, maxLength: number): string {
  if (typeof value !== 'string') return ''
  return value
    .replace(/[\u0000-\u001f\u007f]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, maxLength)
}

export function sanitizeModelId(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const id = value.trim()
  if (id.length === 0 || id.length > 256) return null
  return /^[a-zA-Z0-9][a-zA-Z0-9._~:+/-]*$/.test(id) ? id : null
}

function sanitizeStringArray(value: unknown, maxItems = 32): string[] {
  if (!Array.isArray(value)) return []
  const result: string[] = []
  const seen = new Set<string>()
  for (const item of value.slice(0, maxItems)) {
    const text = sanitizeText(item, 64).toLowerCase()
    if (!text || seen.has(text)) continue
    seen.add(text)
    result.push(text)
  }
  return result
}

function sanitizeModel(value: unknown): OpenRouterModelInfo | null {
  if (!value || typeof value !== 'object') return null
  const raw = value as RawOpenRouterModel
  const id = sanitizeModelId(raw.id)
  const contextLength = typeof raw.context_length === 'number' &&
    Number.isFinite(raw.context_length) && raw.context_length > 0
    ? Math.min(Math.floor(raw.context_length), Number.MAX_SAFE_INTEGER)
    : null
  const supportedParameters = sanitizeStringArray(raw.supported_parameters)

  // Kimi Code is an agent, so models without native tool calling are excluded.
  if (!id || !contextLength || !supportedParameters.includes('tools')) return null

  const inputModalities = sanitizeStringArray(raw.architecture?.input_modalities, 12)
  const name = sanitizeText(raw.name, 160) || id
  const supportsThinking =
    supportedParameters.includes('reasoning') ||
    supportedParameters.includes('include_reasoning') ||
    /(?:^|[\s/_.:-])(thinking|reasoning|deepseek-r1|o[134](?:-|$))/i.test(`${id} ${name}`)

  return {
    id,
    name,
    description: sanitizeText(raw.description, 800),
    contextLength,
    inputModalities,
    supportsThinking,
    supportsImages: inputModalities.includes('image')
  }
}

export function parseModelsPayload(payload: unknown): OpenRouterModelInfo[] {
  if (!payload || typeof payload !== 'object') return []
  const data = (payload as { data?: unknown }).data
  if (!Array.isArray(data)) return []

  const byId = new Map<string, OpenRouterModelInfo>()
  for (const rawModel of data) {
    const model = sanitizeModel(rawModel)
    if (model) byId.set(model.id, model)
  }
  return Array.from(byId.values()).sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }) || a.id.localeCompare(b.id)
  )
}

export function buildKimiModelEnvironment(
  apiKey: string,
  model: OpenRouterModelInfo,
  baseUrl = 'https://openrouter.ai/api/v1'
): Record<string, string> {
  const capabilities = ['tool_use']
  if (model.supportsThinking) capabilities.push('thinking')
  if (model.supportsImages) capabilities.push('image_in')

  return {
    KIMI_MODEL_PROVIDER_TYPE: 'openai',
    KIMI_MODEL_BASE_URL: baseUrl,
    KIMI_MODEL_API_KEY: apiKey,
    KIMI_MODEL_NAME: model.id,
    KIMI_MODEL_MAX_CONTEXT_SIZE: String(model.contextLength),
    KIMI_MODEL_CAPABILITIES: capabilities.join(',')
  }
}
