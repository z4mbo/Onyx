import { net, safeStorage } from 'electron'
import { createHash } from 'crypto'
import { settingsStore } from '../settings/settings-store'
import {
  buildKimiModelEnvironment,
  parseModelsPayload,
  sanitizeModelId,
  type OpenRouterModelInfo
} from './model-catalog'

export type { OpenRouterModelInfo } from './model-catalog'

const OPENROUTER_MODELS_URL = 'https://openrouter.ai/api/v1/models/user'
const OPENROUTER_BASE_URL = 'https://openrouter.ai/api/v1'
const MODELS_CACHE_TTL_MS = 5 * 60 * 1000
const REQUEST_TIMEOUT_MS = 15_000

export interface OpenRouterOperationResult {
  success: boolean
  error?: string
}

export interface OpenRouterModelsResult extends OpenRouterOperationResult {
  models: OpenRouterModelInfo[]
}

export interface OpenRouterStatus {
  hasApiKey: boolean
  selectedModelId: string | null
}

interface ModelsCache {
  keyFingerprint: string
  expiresAt: number
  models: OpenRouterModelInfo[]
}

let modelsCache: ModelsCache | null = null
let credentialMutationGeneration = 0

function apiKeyFingerprint(apiKey: string): string {
  return createHash('sha256').update(apiKey).digest('hex')
}

function operationError(error: string): OpenRouterOperationResult {
  return { success: false, error }
}

function requestErrorForStatus(status: number): string {
  if (status === 401 || status === 403) return 'The OpenRouter API key is invalid or unauthorized.'
  if (status === 429) return 'OpenRouter is rate limiting requests. Please try again shortly.'
  if (status >= 500) return 'OpenRouter is temporarily unavailable. Please try again later.'
  return 'OpenRouter could not validate this API key.'
}

async function requestModels(apiKey: string): Promise<OpenRouterModelInfo[]> {
  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS)

  try {
    const response = await net.fetch(OPENROUTER_MODELS_URL, {
      method: 'GET',
      headers: {
        Accept: 'application/json',
        Authorization: `Bearer ${apiKey}`,
        'HTTP-Referer': 'https://github.com/z4mbo/zAI',
        'X-OpenRouter-Title': 'zAI'
      },
      signal: controller.signal
    })

    if (!response.ok) throw new OpenRouterServiceError(requestErrorForStatus(response.status))

    const payload = await response.json() as unknown
    const models = parseModelsPayload(payload)
    if (models.length === 0) {
      throw new OpenRouterServiceError(
        'No tool-capable OpenRouter models are available for this account.'
      )
    }
    return models
  } catch (error) {
    if (error instanceof OpenRouterServiceError) throw error
    if (controller.signal.aborted) {
      throw new OpenRouterServiceError('OpenRouter did not respond in time. Please try again.')
    }
    throw new OpenRouterServiceError('Could not connect to OpenRouter. Check your connection and try again.')
  } finally {
    clearTimeout(timeout)
  }
}

class OpenRouterServiceError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'OpenRouterServiceError'
  }
}

function safeErrorMessage(error: unknown): string {
  return error instanceof OpenRouterServiceError
    ? error.message
    : 'OpenRouter settings could not be updated.'
}

function encryptApiKey(apiKey: string): string {
  if (!safeStorage.isEncryptionAvailable()) {
    throw new OpenRouterServiceError(
      'Secure credential storage is unavailable on this device.'
    )
  }
  return safeStorage.encryptString(apiKey).toString('base64')
}

function readApiKey(): string | null {
  const encrypted = settingsStore.get('openRouterApiKeyEncrypted')
  if (!encrypted || !safeStorage.isEncryptionAvailable()) return null

  try {
    const apiKey = safeStorage.decryptString(Buffer.from(encrypted, 'base64')).trim()
    return apiKey || null
  } catch {
    return null
  }
}

export async function saveOpenRouterApiKey(apiKeyInput: string): Promise<OpenRouterOperationResult> {
  const mutationGeneration = ++credentialMutationGeneration
  const apiKey = typeof apiKeyInput === 'string' ? apiKeyInput.trim() : ''
  if (!apiKey || apiKey.length > 8192) return operationError('Enter a valid OpenRouter API key.')

  try {
    // Ensure secure storage is usable before transmitting or persisting a key.
    if (!safeStorage.isEncryptionAvailable()) {
      return operationError('Secure credential storage is unavailable on this device.')
    }

    const models = await requestModels(apiKey)
    if (mutationGeneration !== credentialMutationGeneration) {
      return operationError('OpenRouter credentials changed while this key was being validated.')
    }
    const encryptedApiKey = encryptApiKey(apiKey)
    settingsStore.set('openRouterApiKeyEncrypted', encryptedApiKey)
    modelsCache = {
      keyFingerprint: apiKeyFingerprint(apiKey),
      models,
      expiresAt: Date.now() + MODELS_CACHE_TTL_MS
    }

    const selectedModelId = getSelectedOpenRouterModel()
    if (selectedModelId && !models.some((model) => model.id === selectedModelId)) {
      settingsStore.set('openRouterSelectedModel', '')
    }
    return { success: true }
  } catch (error) {
    return operationError(safeErrorMessage(error))
  }
}

export function clearOpenRouterApiKey(): OpenRouterOperationResult {
  credentialMutationGeneration += 1
  try {
    settingsStore.set('openRouterApiKeyEncrypted', '')
    settingsStore.set('openRouterSelectedModel', '')
    modelsCache = null
    return { success: true }
  } catch {
    return operationError('OpenRouter credentials could not be cleared.')
  }
}

export function getSelectedOpenRouterModel(): string | null {
  return sanitizeModelId(settingsStore.get('openRouterSelectedModel'))
}

export function getOpenRouterStatus(): OpenRouterStatus {
  return {
    hasApiKey: readApiKey() !== null,
    selectedModelId: getSelectedOpenRouterModel()
  }
}

export async function listOpenRouterModels(forceRefresh = false): Promise<OpenRouterModelsResult> {
  const apiKey = readApiKey()
  if (!apiKey) {
    return { success: false, models: [], error: 'Add an OpenRouter API key first.' }
  }

  const keyFingerprint = apiKeyFingerprint(apiKey)
  if (
    !forceRefresh &&
    modelsCache &&
    modelsCache.keyFingerprint === keyFingerprint &&
    modelsCache.expiresAt > Date.now()
  ) {
    return { success: true, models: modelsCache.models }
  }

  try {
    const models = await requestModels(apiKey)
    // Do not let an older in-flight request replace a newer key's cache.
    if (apiKeyFingerprint(readApiKey() || '') !== keyFingerprint) {
      return { success: false, models: [], error: 'OpenRouter credentials changed during refresh.' }
    }
    modelsCache = {
      keyFingerprint,
      models,
      expiresAt: Date.now() + MODELS_CACHE_TTL_MS
    }
    const selectedModelId = getSelectedOpenRouterModel()
    if (selectedModelId && !models.some((model) => model.id === selectedModelId)) {
      settingsStore.set('openRouterSelectedModel', '')
    }
    return { success: true, models }
  } catch (error) {
    return { success: false, models: [], error: safeErrorMessage(error) }
  }
}

export async function setSelectedOpenRouterModel(
  modelIdInput: string
): Promise<OpenRouterOperationResult> {
  const modelId = sanitizeModelId(modelIdInput)
  if (!modelId) return operationError('Select a valid OpenRouter model.')
  const apiKey = readApiKey()
  if (!apiKey) return operationError('Add an OpenRouter API key first.')
  const keyFingerprint = apiKeyFingerprint(apiKey)

  const modelsResult = await listOpenRouterModels(false)
  if (!modelsResult.success) return operationError(modelsResult.error || 'Models are unavailable.')
  if (apiKeyFingerprint(readApiKey() || '') !== keyFingerprint) {
    return operationError('OpenRouter credentials changed while the model was being selected.')
  }
  if (!modelsResult.models.some((model) => model.id === modelId)) {
    return operationError('That model is not available for this OpenRouter account.')
  }

  try {
    settingsStore.set('openRouterSelectedModel', modelId)
    return { success: true }
  } catch {
    return operationError('The selected OpenRouter model could not be saved.')
  }
}

/**
 * Builds the private environment used only in the main process when launching
 * Kimi Code with an OpenRouter model. The API key is never returned over IPC.
 */
export async function getOpenRouterPtyEnvironment(): Promise<Record<string, string>> {
  const apiKey = readApiKey()
  if (!apiKey) throw new OpenRouterServiceError('Add an OpenRouter API key in Settings first.')

  const selectedModelId = getSelectedOpenRouterModel()
  if (!selectedModelId) {
    throw new OpenRouterServiceError('Select an OpenRouter model in Settings first.')
  }

  const modelsResult = await listOpenRouterModels(false)
  if (!modelsResult.success) {
    throw new OpenRouterServiceError(modelsResult.error || 'OpenRouter models are unavailable.')
  }
  const selectedModel = modelsResult.models.find((model) => model.id === selectedModelId)
  if (!selectedModel) {
    throw new OpenRouterServiceError('The selected OpenRouter model is no longer available.')
  }

  return buildKimiModelEnvironment(apiKey, selectedModel, OPENROUTER_BASE_URL)
}

export function sanitizeOpenRouterError(error: unknown): string {
  return safeErrorMessage(error)
}
