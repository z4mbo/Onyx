import { useCallback, useEffect, useMemo, useState } from 'react'
import * as api from '@/lib/api'
import { useSettingsStore } from '@/stores/settings-store'

interface OpenRouterModelInfo {
  id: string
  name: string
  description?: string
  contextLength?: number
  inputModalities?: string[]
  supportsThinking?: boolean
  supportsImages?: boolean
}

function formatContextLength(value?: number): string | null {
  if (!value) return null
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(value % 1_000_000 === 0 ? 0 : 1)}M context`
  if (value >= 1_000) return `${Math.round(value / 1_000)}K context`
  return `${value} context`
}

export default function OpenRouterProviderSection() {
  const defaultEngine = useSettingsStore((state) => state.defaultEngine)
  const updateSetting = useSettingsStore((state) => state.updateSetting)
  const [apiKey, setApiKey] = useState('')
  const [hasApiKey, setHasApiKey] = useState(false)
  const [models, setModels] = useState<OpenRouterModelInfo[]>([])
  const [selectedModelId, setSelectedModelId] = useState<string | null>(null)
  const [search, setSearch] = useState('')
  const [loadingStatus, setLoadingStatus] = useState(true)
  const [loadingModels, setLoadingModels] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const loadModels = useCallback(async (forceRefresh = false) => {
    setLoadingModels(true)
    setError(null)
    try {
      const result = await api.openRouterListModels(forceRefresh)
      if (!result.success) throw new Error(result.error || 'Could not load OpenRouter models.')
      setModels(result.models)
      setSelectedModelId(await api.openRouterGetSelectedModel())
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      setModels([])
    } finally {
      setLoadingModels(false)
    }
  }, [])

  const loadProvider = useCallback(async () => {
    setLoadingStatus(true)
    setError(null)
    try {
      const [status, selectedModel] = await Promise.all([
        api.openRouterGetStatus(),
        api.openRouterGetSelectedModel()
      ])
      setHasApiKey(status.hasApiKey)
      setSelectedModelId(selectedModel ?? status.selectedModelId)
      if (status.hasApiKey) await loadModels(false)
      else setModels([])
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoadingStatus(false)
    }
  }, [loadModels])

  useEffect(() => {
    void loadProvider()
  }, [loadProvider])

  const handleConnect = useCallback(async () => {
    const trimmed = apiKey.trim()
    if (!trimmed) {
      setError('Enter an OpenRouter API key first.')
      return
    }

    setSaving(true)
    setError(null)
    try {
      const result = await api.openRouterSaveApiKey(trimmed)
      if (!result.success) throw new Error(result.error || 'OpenRouter rejected the API key.')
      setApiKey('')
      setHasApiKey(true)
      await loadModels(false)
      setSelectedModelId(await api.openRouterGetSelectedModel())
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSaving(false)
    }
  }, [apiKey, loadModels])

  const handleDisconnect = useCallback(async () => {
    setSaving(true)
    setError(null)
    try {
      const result = await api.openRouterClearApiKey()
      if (!result.success) throw new Error(result.error || 'Could not disconnect OpenRouter.')
      setApiKey('')
      setHasApiKey(false)
      setModels([])
      setSelectedModelId(null)
      setSearch('')
      if (defaultEngine === 'openrouter') {
        const kimiAvailable = await api.detectEngines()
          .then((detected) => (detected as Array<{ id: string; isAvailable: boolean }>)
            .some((engine) => engine.id === 'kimi' && engine.isAvailable))
          .catch(() => false)
        await updateSetting('defaultEngine', kimiAvailable ? 'kimi' : 'claude')
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSaving(false)
    }
  }, [defaultEngine, updateSetting])

  const handleModelChange = useCallback(async (modelId: string) => {
    if (!modelId) return
    setSaving(true)
    setError(null)
    try {
      const result = await api.openRouterSetSelectedModel(modelId)
      if (!result.success) throw new Error(result.error || 'Could not save the selected model.')
      setSelectedModelId(modelId)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSaving(false)
    }
  }, [])

  const filteredModels = useMemo(() => {
    const query = search.trim().toLowerCase()
    if (!query) return models
    return models.filter((model) =>
      model.name.toLowerCase().includes(query) ||
      model.id.toLowerCase().includes(query) ||
      model.description?.toLowerCase().includes(query)
    )
  }, [models, search])

  const selectedModel = models.find((model) => model.id === selectedModelId)
  const selectedIsFilteredOut = !!selectedModelId && !filteredModels.some((model) => model.id === selectedModelId)

  return (
    <div className="space-y-5">
      <div>
        <div className="flex items-center justify-between gap-3">
          <div>
            <h3 className="text-sm font-semibold text-win-text">OpenRouter</h3>
            <p className="mt-0.5 text-[11px] text-win-text-tertiary">
              Use OpenRouter models through the Kimi Code CLI.
            </p>
          </div>
          <div className="flex items-center gap-2 text-xs text-win-text-secondary">
            <span className={`h-2 w-2 rounded-full ${hasApiKey ? 'bg-green-400' : 'bg-neutral-300'}`} />
            {loadingStatus ? 'Checking…' : hasApiKey ? 'Connected' : 'Not connected'}
          </div>
        </div>
      </div>

      <div className="rounded-lg border border-win-border bg-win-surface p-4 space-y-3">
        <label className="block text-xs font-medium text-win-text-secondary">
          API key
        </label>
        <div className="flex gap-2">
          <input
            type="password"
            value={apiKey}
            onChange={(event) => setApiKey(event.target.value)}
            onKeyDown={(event) => { if (event.key === 'Enter') void handleConnect() }}
            placeholder={hasApiKey ? '••••••••••••••••  (connected)' : 'sk-or-v1-…'}
            autoComplete="off"
            className="min-w-0 flex-1 rounded border border-win-border bg-win-card px-3 py-2 text-sm text-win-text placeholder:text-win-text-tertiary outline-none focus:border-win-accent"
          />
          <button
            onClick={() => void handleConnect()}
            disabled={saving || !apiKey.trim()}
            className="rounded bg-win-accent px-3 py-2 text-xs font-medium text-white hover:bg-win-accent-dark disabled:cursor-not-allowed disabled:opacity-40 transition-colors"
          >
            {saving ? 'Saving…' : hasApiKey ? 'Replace key' : 'Connect'}
          </button>
          {hasApiKey && (
            <button
              onClick={() => void handleDisconnect()}
              disabled={saving}
              className="rounded border border-win-border bg-win-card px-3 py-2 text-xs font-medium text-win-text-secondary hover:bg-win-hover hover:text-win-text disabled:opacity-40 transition-colors"
            >
              Disconnect
            </button>
          )}
        </div>
        <p className="text-[10px] text-win-text-tertiary">
          Your key is stored by zAI and is never shown again after saving.
        </p>
      </div>

      {hasApiKey && (
        <div className="rounded-lg border border-win-border bg-win-surface p-4 space-y-3">
          <div className="flex items-center justify-between gap-3">
            <label className="text-xs font-medium text-win-text-secondary">Default OpenRouter model</label>
            <button
              onClick={() => void loadModels(true)}
              disabled={loadingModels}
              className="text-[11px] font-medium text-win-accent hover:underline disabled:opacity-40"
            >
              {loadingModels ? 'Refreshing…' : 'Refresh models'}
            </button>
          </div>

          <input
            type="search"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Search model names or IDs…"
            className="w-full rounded border border-win-border bg-win-card px-3 py-2 text-sm text-win-text placeholder:text-win-text-tertiary outline-none focus:border-win-accent"
          />

          <select
            value={selectedModelId ?? ''}
            onChange={(event) => void handleModelChange(event.target.value)}
            disabled={loadingModels || saving || models.length === 0}
            className="w-full rounded border border-win-border bg-win-card px-3 py-2 text-sm text-win-text outline-none focus:border-win-accent disabled:opacity-50"
          >
            <option value="">Choose a model…</option>
            {selectedIsFilteredOut && selectedModelId && (
              <option value={selectedModelId}>{selectedModel?.name || selectedModelId}</option>
            )}
            {filteredModels.map((model) => (
              <option key={model.id} value={model.id}>{model.name} — {model.id}</option>
            ))}
          </select>

          {!loadingModels && models.length > 0 && filteredModels.length === 0 && (
            <p className="text-xs text-win-text-tertiary">No models match “{search}”.</p>
          )}

          {selectedModel && (
            <div className="rounded border border-win-border bg-win-card px-3 py-2.5">
              <p className="text-xs font-medium text-win-text">{selectedModel.name}</p>
              <p className="mt-0.5 break-all font-mono text-[10px] text-win-text-tertiary">{selectedModel.id}</p>
              {selectedModel.description && (
                <p className="mt-2 line-clamp-2 text-[11px] leading-relaxed text-win-text-secondary">{selectedModel.description}</p>
              )}
              <div className="mt-2 flex flex-wrap gap-1.5">
                {formatContextLength(selectedModel.contextLength) && (
                  <span className="rounded bg-win-hover px-1.5 py-0.5 text-[10px] text-win-text-secondary">{formatContextLength(selectedModel.contextLength)}</span>
                )}
                {selectedModel.supportsThinking && (
                  <span className="rounded bg-win-hover px-1.5 py-0.5 text-[10px] text-win-text-secondary">Reasoning</span>
                )}
                {selectedModel.supportsImages && (
                  <span className="rounded bg-win-hover px-1.5 py-0.5 text-[10px] text-win-text-secondary">Images</span>
                )}
              </div>
            </div>
          )}
        </div>
      )}

      {error && (
        <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700">
          {error}
        </div>
      )}

      <p className="text-[10px] leading-relaxed text-win-text-tertiary">
        OpenRouter sessions require Kimi Code 0.6.0 or newer. zAI passes the selected model and provider credentials only to that local OpenRouter terminal process tree.
      </p>
    </div>
  )
}
