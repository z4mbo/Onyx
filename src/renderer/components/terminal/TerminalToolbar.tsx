import { useCallback, useState, useEffect, useMemo, useRef } from 'react'
import { useTerminalStore, generateTerminalId } from '@/stores/terminal-store'
import { useProjectStore } from '@/stores/project-store'
import { useSettingsStore } from '@/stores/settings-store'
import { ENGINE_NAMES, ENGINE_COLORS, ENGINE_MD_FILES, ENGINE_COMPACT_CMD, type EngineId } from '@/lib/constants'
import * as api from '@/lib/api'
import type { AIEngineInfo } from '@/lib/api'
import ClearSessionDialog from './ClearSessionDialog'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'

/** Install URLs for each engine */
const ENGINE_INSTALL_URLS: Partial<Record<EngineId, string>> = {
  claude: 'https://docs.anthropic.com/en/docs/claude-code/overview',
  gemini: 'https://github.com/google-gemini/gemini-cli?tab=readme-ov-file#-installation',
  codex: 'https://github.com/openai/codex',
  kimi: 'https://www.kimi.com/code/docs/en/kimi-code-cli/guides/getting-started.html'
}

interface OpenRouterModelInfo {
  id: string
  name: string
  description?: string
}

/**
 * Modal that displays the engine's memory file (CLAUDE.md or GEMINI.md).
 */
function MemoryModal({
  projectPath,
  engine,
  onClose
}: {
  projectPath: string
  engine: EngineId
  onClose: () => void
}) {
  const overlayRef = useRef<HTMLDivElement>(null)
  const [content, setContent] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  const fileName = ENGINE_MD_FILES[engine]

  useEffect(() => {
    const filePath = `${projectPath}/${fileName}`
    api.readFile(filePath)
      .then((text) => {
        setContent(text)
        setLoading(false)
      })
      .catch(() => {
        setContent(null)
        setLoading(false)
      })
  }, [projectPath, fileName])

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [onClose])

  const handleOverlayClick = useCallback(
    (e: React.MouseEvent) => {
      if (e.target === overlayRef.current) onClose()
    },
    [onClose]
  )

  return (
    <div
      ref={overlayRef}
      onClick={handleOverlayClick}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm"
    >
      <div className="w-full max-w-2xl max-h-[80vh] rounded-lg border border-win-border bg-win-card flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-win-border shrink-0">
          <div className="flex items-center gap-3">
            <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-win-accent-subtle">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-win-accent">
                <path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" />
                <path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" />
              </svg>
            </div>
            <div>
              <h2 className="text-base font-semibold text-win-text">Project Memory</h2>
              <p className="text-xs text-win-text-tertiary">{fileName}</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="flex h-8 w-8 items-center justify-center rounded-md text-win-text-tertiary hover:bg-win-hover hover:text-win-text transition-colors"
          >
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5">
              <line x1="1" y1="1" x2="13" y2="13" />
              <line x1="13" y1="1" x2="1" y2="13" />
            </svg>
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto px-6 py-5">
          {loading ? (
            <div className="flex items-center gap-3 text-sm text-win-text-tertiary">
              <div className="h-5 w-5 animate-spin rounded-full border-2 border-win-border border-t-win-accent" />
              Loading memory file...
            </div>
          ) : content === null || content.trim() === '' ? (
            <div className="flex flex-col items-center gap-3 py-10 text-center">
              <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="text-win-text-tertiary">
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                <polyline points="14 2 14 8 20 8" />
              </svg>
              <p className="text-sm text-win-text-secondary">No memory file found</p>
              <p className="text-xs text-win-text-tertiary">
                The AI assistant will create <code className="rounded bg-win-hover px-1.5 py-0.5 border border-win-border">{fileName}</code> as it learns about your project.
              </p>
            </div>
          ) : (
            <div className="prose prose-sm max-w-none
              prose-headings:text-win-text prose-headings:font-semibold
              prose-h1:text-lg prose-h1:mb-3 prose-h1:mt-0
              prose-h2:text-base prose-h2:mb-2 prose-h2:mt-5
              prose-h3:text-sm prose-h3:mb-1.5 prose-h3:mt-4
              prose-p:text-sm prose-p:leading-relaxed prose-p:text-win-text-secondary prose-p:mb-2
              prose-a:text-win-accent prose-a:no-underline hover:prose-a:underline
              prose-code:text-xs prose-code:bg-win-hover prose-code:px-1.5 prose-code:py-0.5 prose-code:rounded prose-code:text-win-accent-dark prose-code:font-mono prose-code:border prose-code:border-win-border
              prose-pre:bg-win-bg prose-pre:border prose-pre:border-win-border prose-pre:rounded-lg prose-pre:text-xs prose-pre:p-4
              prose-ul:text-sm prose-ul:text-win-text-secondary prose-ol:text-sm prose-ol:text-win-text-secondary
              prose-li:text-sm prose-li:text-win-text-secondary prose-li:my-0.5
              prose-strong:text-win-text
              prose-em:text-win-text-secondary
              prose-hr:border-win-border">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{content}</ReactMarkdown>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end px-6 py-4 border-t border-win-border shrink-0">
          <button
            onClick={onClose}
            className="rounded-md bg-win-accent px-5 py-2 text-sm font-medium text-white hover:bg-win-accent-dark transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  )
}

/**
 * Toolbar above the terminal area.
 * Contains: AI engine selector, session action buttons, and project memory.
 */
export default function TerminalToolbar() {
  const terminals = useTerminalStore((s) => s.terminals)
  const activeTerminalId = useTerminalStore((s) => s.activeTerminalId)
  const addTerminal = useTerminalStore((s) => s.addTerminal)
  const activeProject = useProjectStore((s) => s.activeProject)
  const defaultEngine = useSettingsStore((s) => s.defaultEngine)
  const settingsOpen = useSettingsStore((s) => s.showSettingsDialog)
  const setShowSettings = useSettingsStore((s) => s.setShowSettingsDialog)

  const minifiedView = useSettingsStore((s) => s.minifiedView)
  const toggleMinifiedView = useSettingsStore((s) => s.toggleMinifiedView)

  const activeTerminal = activeTerminalId ? terminals.get(activeTerminalId) : undefined
  const currentEngine = activeTerminal?.engine ?? defaultEngine

  const [selectedEngine, setSelectedEngine] = useState<EngineId>(defaultEngine)
  const [showClearDialog, setShowClearDialog] = useState(false)
  const [showMemory, setShowMemory] = useState(false)
  const [engines, setEngines] = useState<AIEngineInfo[]>([])
  const [engineDropdownOpen, setEngineDropdownOpen] = useState(false)
  const [modelDropdownOpen, setModelDropdownOpen] = useState(false)
  const [openRouterHasKey, setOpenRouterHasKey] = useState(false)
  const [openRouterModelId, setOpenRouterModelId] = useState<string | null>(null)
  const [openRouterModels, setOpenRouterModels] = useState<OpenRouterModelInfo[]>([])
  const [openRouterLoading, setOpenRouterLoading] = useState(false)
  const [modelSearch, setModelSearch] = useState('')
  const [providerGuide, setProviderGuide] = useState<string | null>(null)
  const engineDropdownRef = useRef<HTMLDivElement>(null)
  const modelDropdownRef = useRef<HTMLDivElement>(null)

  // Detect installed engines on mount
  useEffect(() => {
    api.detectEngines().then((result) => setEngines(result as AIEngineInfo[]))
  }, [])

  const loadOpenRouterState = useCallback(async (forceRefresh = false) => {
    setOpenRouterLoading(true)
    try {
      const status = await api.openRouterGetStatus()
      setOpenRouterHasKey(status.hasApiKey)
      setOpenRouterModelId(status.selectedModelId)
      if (status.hasApiKey) {
        const result = await api.openRouterListModels(forceRefresh)
        if (result.success) {
          setOpenRouterModels(result.models)
          setOpenRouterModelId(await api.openRouterGetSelectedModel())
        } else setOpenRouterModels([])
      } else {
        setOpenRouterModels([])
      }
    } catch (err) {
      console.error('[TerminalToolbar] Failed to load OpenRouter state:', err)
    } finally {
      setOpenRouterLoading(false)
    }
  }, [])

  // Refresh after Settings closes so changes made in Providers immediately
  // appear in the toolbar.
  useEffect(() => {
    if (!settingsOpen) void loadOpenRouterState(false)
  }, [settingsOpen, loadOpenRouterState])

  // The selector represents the active session when one exists, otherwise the
  // configured default for the next session.
  useEffect(() => {
    setSelectedEngine(activeTerminal?.engine ?? defaultEngine)
  }, [activeTerminal?.engine, defaultEngine])

  // Close either selector on outside click.
  useEffect(() => {
    if (!engineDropdownOpen && !modelDropdownOpen) return
    const handler = (e: MouseEvent) => {
      if (engineDropdownRef.current && !engineDropdownRef.current.contains(e.target as Node)) {
        setEngineDropdownOpen(false)
      }
      if (modelDropdownRef.current && !modelDropdownRef.current.contains(e.target as Node)) {
        setModelDropdownOpen(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [engineDropdownOpen, modelDropdownOpen])

  const atMaxTerminals = terminals.size >= 4

  const createSession = useCallback(
    (engine: EngineId, launchIntent: 'start-session' | 'continue-session' = 'start-session') => {
    const id = generateTerminalId()
    const count = terminals.size + 1
    addTerminal({
      id,
      ptyId: null,
      name: `${ENGINE_NAMES[engine]} #${count}`,
      engine,
      isActive: true,
      cwd: activeProject?.path ?? '',
      isLoading: true,
      launchIntent
    })
  }, [addTerminal, terminals.size, activeProject])

  const ensureOpenRouterReady = useCallback(async (): Promise<boolean> => {
    try {
      const status = await api.openRouterGetStatus()
      setOpenRouterHasKey(status.hasApiKey)
      setOpenRouterModelId(status.selectedModelId)

      if (!status.hasApiKey) {
        setProviderGuide('Connect an OpenRouter API key in Settings → Providers before starting an OpenRouter chat.')
        return false
      }
      if (!status.selectedModelId) {
        setProviderGuide('Choose an OpenRouter model in Settings → Providers before starting an OpenRouter chat.')
        return false
      }
      const kimiInstalled = engines.find((engine) => engine.id === 'kimi')?.isAvailable ?? false
      if (!kimiInstalled) {
        setProviderGuide('OpenRouter runs through Kimi Code. Install Kimi Code before starting this chat.')
        return false
      }
      const openRouterEngine = engines.find((engine) => engine.id === 'openrouter')
      if (!openRouterEngine?.isAvailable) {
        setProviderGuide(
          openRouterEngine?.availabilityReason ||
          'OpenRouter requires Kimi Code 0.6.0 or newer. Upgrade Kimi Code and restart zAI.'
        )
        return false
      }
      setProviderGuide(null)
      return true
    } catch (err) {
      setProviderGuide(err instanceof Error ? err.message : 'Could not check the OpenRouter connection.')
      return false
    }
  }, [engines])

  const handleEngineChange = useCallback(
    async (engine: EngineId) => {
      setEngineDropdownOpen(false)

      if (engine === 'openrouter' && !(await ensureOpenRouterReady())) return

      setSelectedEngine(engine)
      // If already at max, don't create a new terminal
      if (atMaxTerminals) return
      createSession(engine)
    },
    [atMaxTerminals, createSession, ensureOpenRouterReady]
  )

  const handleNewSession = useCallback(async () => {
    if (selectedEngine === 'openrouter' && !(await ensureOpenRouterReady())) return
    createSession(selectedEngine)
  }, [selectedEngine, createSession, ensureOpenRouterReady])

  const handleContinueSession = useCallback(async () => {
    if (!activeTerminal) return
    try {
      if (currentEngine === 'openrouter' && !(await ensureOpenRouterReady())) return
      if (atMaxTerminals) {
        setProviderGuide('Close a chat before resuming; zAI supports up to four terminals.')
        return
      }
      createSession(currentEngine, 'continue-session')
    } catch (err) {
      setProviderGuide(err instanceof Error ? err.message : `Could not resume ${ENGINE_NAMES[currentEngine]}.`)
    }
  }, [activeTerminal, currentEngine, ensureOpenRouterReady, atMaxTerminals, createSession])

  const handleSelectOpenRouterModel = useCallback(async (modelId: string) => {
    try {
      const result = await api.openRouterSetSelectedModel(modelId)
      if (!result.success) throw new Error(result.error || 'Could not save the selected model.')
      setOpenRouterModelId(modelId)
      setModelDropdownOpen(false)
      setModelSearch('')
      setProviderGuide(null)
    } catch (err) {
      setProviderGuide(err instanceof Error ? err.message : 'Could not save the selected model.')
    }
  }, [])

  const filteredOpenRouterModels = useMemo(() => {
    const query = modelSearch.trim().toLowerCase()
    if (!query) return openRouterModels
    return openRouterModels.filter((model) =>
      model.name.toLowerCase().includes(query) || model.id.toLowerCase().includes(query)
    )
  }, [modelSearch, openRouterModels])

  const selectedOpenRouterModel = openRouterModels.find((model) => model.id === openRouterModelId)

  // ---- Clear session handlers ----

  const handleClearClick = useCallback(() => {
    if (!activeTerminal?.ptyId) return
    setShowClearDialog(true)
  }, [activeTerminal])

  const handleClear = useCallback(() => {
    if (!activeTerminal) return
    api.ptyWrite(activeTerminal.id, '/clear\n')
    setShowClearDialog(false)
  }, [activeTerminal])

  const handleSaveAndClear = useCallback(async () => {
    if (!activeTerminal || !activeProject) return

    const engineMdFile = ENGINE_MD_FILES[currentEngine]

    const summarizePrompt =
      `Before clearing, summarize this session using the /summarizer skill format and append the summary to \`${engineMdFile}\` in the project root. ` +
      `Add a "## Session - ${new Date().toLocaleDateString()}" heading before the summary. ` +
      `After saving, confirm with "Summary saved."`

    api.ptyWrite(activeTerminal.id, summarizePrompt + '\n')
    setShowClearDialog(false)

    setTimeout(() => {
      api.ptyWrite(activeTerminal.id, '/clear\n')
    }, 30000)
  }, [activeTerminal, activeProject, currentEngine])

  // ---- Compact handler ----

  const handleCompact = useCallback(() => {
    if (!activeTerminal?.ptyId) return
    api.ptyWrite(activeTerminal.id, ENGINE_COMPACT_CMD[currentEngine] + '\n')
  }, [activeTerminal, currentEngine])

  const hasPty = !!activeTerminal?.ptyId

  return (
    <>
      <div className="flex h-12 shrink-0 items-center gap-3 border-b border-win-border bg-win-surface px-4">
        {/* AI assistant selector */}
        <div className="relative flex items-center gap-2" ref={engineDropdownRef}>
          <label className="text-sm font-medium text-win-text-secondary">
            Assistant
          </label>
          <button
            onClick={() => setEngineDropdownOpen((v) => !v)}
            className="flex items-center gap-2 rounded-md border border-win-border bg-win-card px-3 py-2 text-sm text-win-text hover:bg-win-hover transition-colors cursor-pointer"
          >
            <span className={`h-2 w-2 rounded-full ${ENGINE_COLORS[selectedEngine]}`} />
            {ENGINE_NAMES[selectedEngine]}
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-win-text-tertiary">
              <path d="M2 4l3 3 3-3" />
            </svg>
          </button>

          {engineDropdownOpen && (
            <div className="absolute left-0 top-full mt-1 z-50 min-w-[200px] rounded-lg border border-win-border bg-win-card shadow-lg overflow-hidden">
              {(Object.entries(ENGINE_NAMES) as [EngineId, string][]).map(([key, name]) => {
                const info = engines.find((e) => e.id === key)
                const kimiInstalled = engines.find((e) => e.id === 'kimi')?.isAvailable ?? false
                const openRouterCapable = engines.find((e) => e.id === 'openrouter')?.isAvailable ?? false
                const isInstalled = key === 'openrouter'
                  ? openRouterHasKey && !!openRouterModelId && openRouterCapable
                  : info?.isAvailable ?? false
                const isCurrent = key === selectedEngine

                if (isInstalled) {
                  return (
                    <button
                      key={key}
                      onClick={() => void handleEngineChange(key)}
                      disabled={atMaxTerminals && !isCurrent}
                      className={`flex w-full items-center gap-2.5 px-3 py-2.5 text-sm transition-colors ${
                        isCurrent
                          ? 'bg-win-accent-subtle text-win-accent font-medium'
                          : 'text-win-text hover:bg-win-hover disabled:opacity-40 disabled:cursor-not-allowed'
                      }`}
                    >
                      <span className={`h-2 w-2 shrink-0 rounded-full ${ENGINE_COLORS[key]}`} />
                      <span className="flex-1 text-left">{name}</span>
                      {isCurrent && (
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" className="shrink-0">
                          <polyline points="20 6 9 17 4 12" />
                        </svg>
                      )}
                      {atMaxTerminals && !isCurrent && (
                        <span className="text-[10px] text-win-text-tertiary">4/4</span>
                      )}
                    </button>
                  )
                }

                if (key === 'openrouter') {
                  const reason = !kimiInstalled
                    ? 'Needs Kimi Code'
                    : !openRouterCapable
                      ? 'Upgrade Kimi Code'
                    : !openRouterHasKey
                      ? 'API key required'
                      : 'Choose a model'
                  return (
                    <div
                      key={key}
                      className="flex w-full items-center gap-2.5 px-3 py-2.5 text-sm text-win-text-tertiary"
                    >
                      <span className="h-2 w-2 shrink-0 rounded-full bg-neutral-300" />
                      <span className="min-w-0 flex-1 text-left">
                        <span className="block text-win-text-secondary">{name}</span>
                        <span className="block text-[10px]">{reason}</span>
                      </span>
                      {!kimiInstalled || !openRouterCapable ? (
                        <a
                          href={ENGINE_INSTALL_URLS.kimi}
                          rel="noreferrer"
                          onClick={() => setEngineDropdownOpen(false)}
                          className="text-xs font-medium text-win-accent hover:underline"
                        >
                          {kimiInstalled ? 'Upgrade Kimi' : 'Install Kimi'}
                        </a>
                      ) : (
                        <button
                          onClick={() => {
                            setEngineDropdownOpen(false)
                            setShowSettings(true)
                          }}
                          className="text-xs font-medium text-win-accent hover:underline"
                        >
                          Configure
                        </button>
                      )}
                    </div>
                  )
                }

                return (
                  <div
                    key={key}
                    className="flex w-full items-center gap-2.5 px-3 py-2.5 text-sm text-win-text-tertiary"
                  >
                    <span className="h-2 w-2 shrink-0 rounded-full bg-neutral-300" />
                    <span className="flex-1 text-left">{name}</span>
                    <a
                      href={ENGINE_INSTALL_URLS[key]}
                      rel="noreferrer"
                      onClick={(e) => {
                        e.stopPropagation()
                        setEngineDropdownOpen(false)
                      }}
                      className="text-xs font-medium text-win-accent hover:underline"
                    >
                      Install
                    </a>
                  </div>
                )
              })}
            </div>
          )}
        </div>

        {/* OpenRouter model selector. Model changes apply to new chats. */}
        {(selectedEngine === 'openrouter' || currentEngine === 'openrouter') && (
          <div className="relative flex items-center gap-2" ref={modelDropdownRef}>
            <label className="text-xs font-medium text-win-text-secondary">New chat model</label>
            <button
              onClick={() => {
                setModelDropdownOpen((open) => !open)
                if (openRouterModels.length === 0) void loadOpenRouterState(false)
              }}
              className="flex max-w-[220px] items-center gap-2 rounded-md border border-win-border bg-win-card px-2.5 py-1.5 text-xs text-win-text hover:bg-win-hover transition-colors"
              title={openRouterModelId || 'Choose an OpenRouter model'}
            >
              <span className="truncate">{selectedOpenRouterModel?.name || openRouterModelId || 'Choose model'}</span>
              <svg width="9" height="9" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5" className="shrink-0 text-win-text-tertiary">
                <path d="M2 4l3 3 3-3" />
              </svg>
            </button>

            {modelDropdownOpen && (
              <div className="absolute left-0 top-full z-50 mt-1 w-80 rounded-lg border border-win-border bg-win-card p-2 shadow-lg">
                <div className="mb-2 flex gap-1.5">
                  <input
                    type="search"
                    value={modelSearch}
                    onChange={(event) => setModelSearch(event.target.value)}
                    placeholder="Search models…"
                    autoFocus
                    className="min-w-0 flex-1 rounded border border-win-border bg-win-surface px-2.5 py-1.5 text-xs text-win-text placeholder:text-win-text-tertiary outline-none focus:border-win-accent"
                  />
                  <button
                    onClick={() => void loadOpenRouterState(true)}
                    disabled={openRouterLoading}
                    title="Refresh OpenRouter models"
                    className="rounded border border-win-border px-2 text-win-text-secondary hover:bg-win-hover disabled:opacity-40"
                  >
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={openRouterLoading ? 'animate-spin' : ''}>
                      <polyline points="23 4 23 10 17 10" />
                      <polyline points="1 20 1 14 7 14" />
                      <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
                    </svg>
                  </button>
                </div>
                <div className="max-h-64 overflow-y-auto">
                  {filteredOpenRouterModels.map((model) => (
                    <button
                      key={model.id}
                      onClick={() => void handleSelectOpenRouterModel(model.id)}
                      className={`w-full rounded px-2.5 py-2 text-left transition-colors ${
                        model.id === openRouterModelId
                          ? 'bg-win-accent-subtle text-win-accent'
                          : 'text-win-text-secondary hover:bg-win-hover hover:text-win-text'
                      }`}
                    >
                      <span className="block truncate text-xs font-medium">{model.name}</span>
                      <span className="mt-0.5 block truncate font-mono text-[9px] text-win-text-tertiary">{model.id}</span>
                    </button>
                  ))}
                  {!openRouterLoading && filteredOpenRouterModels.length === 0 && (
                    <div className="px-2.5 py-4 text-center text-xs text-win-text-tertiary">
                      {openRouterHasKey ? 'No matching models.' : 'Connect OpenRouter in Settings → Providers.'}
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>
        )}

        {/* Action buttons */}
        <div className="flex items-center gap-1.5">
          <button
            onClick={handleNewSession}
            disabled={terminals.size >= 4}
            className="flex items-center gap-2 rounded-md bg-win-accent px-2 py-1 text-sm font-medium text-white hover:bg-win-accent-dark disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="12" y1="5" x2="12" y2="19" />
              <line x1="5" y1="12" x2="19" y2="12" />
            </svg>
            New Chat
          </button>

          {/* Engines with a backend-defined continuation command. */}
          {(['claude', 'codex', 'kimi', 'openrouter'] as EngineId[]).includes(currentEngine) && (
            <button
              onClick={handleContinueSession}
              disabled={atMaxTerminals}
              className="flex items-center gap-2 rounded-md border border-win-border bg-win-card px-2 py-1 text-sm text-win-text-secondary hover:bg-win-hover hover:text-win-text disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            >
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <polygon points="5 3 19 12 5 21 5 3" />
              </svg>
              Resume
            </button>
          )}

          {/* Separator */}
          <div className="mx-0.5 h-4 w-px bg-win-border" />

          {/* Reset Chat button */}
          <button
            onClick={handleClearClick}
            disabled={!hasPty}
            title="Reset chat — start a fresh conversation"
            className="flex items-center gap-2 rounded-md border border-win-border bg-win-card px-2 py-1 text-sm text-win-text-secondary hover:bg-win-hover hover:text-win-text disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="1 4 1 10 7 10" />
              <path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10" />
            </svg>
            Reset
          </button>

          {/* Summarize button */}
          <button
            onClick={handleCompact}
            disabled={!hasPty}
            title="Summarize the conversation to free up context"
            className="flex items-center gap-2 rounded-md border border-win-border bg-win-card px-2 py-1 text-sm text-win-text-secondary hover:bg-win-hover hover:text-win-text disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="4 14 10 14 10 20" />
              <polyline points="20 10 14 10 14 4" />
              <line x1="14" y1="10" x2="21" y2="3" />
              <line x1="3" y1="21" x2="10" y2="14" />
            </svg>
            Summarize
          </button>
        </div>

        {/* Spacer */}
        <div className="flex-1" />

        {/* Project Memory button */}
        {activeProject && (
          <button
            onClick={() => setShowMemory(true)}
            title="View project memory file"
            className="flex items-center gap-2 rounded-md border border-win-border bg-win-card px-2 py-1 text-sm text-win-text-secondary hover:bg-win-hover hover:text-win-text transition-colors"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" />
              <path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" />
            </svg>
            Project Memory
          </button>
        )}

        {/* Minified view toggle */}
        <button
          onClick={toggleMinifiedView}
          title={minifiedView ? 'Exit focus mode' : 'Focus mode — hide sidebar and panels'}
          className={`flex items-center justify-center h-8 w-8 rounded-md border transition-colors ${
            minifiedView
              ? 'border-win-accent bg-win-accent-subtle text-win-accent'
              : 'border-win-border bg-win-card text-win-text-secondary hover:bg-win-hover hover:text-win-text'
          }`}
        >
          {minifiedView ? (
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="15 3 21 3 21 9" />
              <polyline points="9 21 3 21 3 15" />
              <line x1="21" y1="3" x2="14" y2="10" />
              <line x1="3" y1="21" x2="10" y2="14" />
            </svg>
          ) : (
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="4 14 10 14 10 20" />
              <polyline points="20 10 14 10 14 4" />
              <line x1="14" y1="10" x2="21" y2="3" />
              <line x1="3" y1="21" x2="10" y2="14" />
            </svg>
          )}
        </button>
      </div>

      {providerGuide && (
        <div className="fixed left-1/2 top-14 z-[60] flex max-w-xl -translate-x-1/2 items-center gap-3 rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 shadow-lg">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="shrink-0 text-amber-600">
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
          <p className="flex-1 text-xs leading-relaxed text-amber-800">{providerGuide}</p>
          {providerGuide.includes('Kimi Code') ? (
            <a
              href={ENGINE_INSTALL_URLS.kimi}
              rel="noreferrer"
              className="shrink-0 text-xs font-semibold text-amber-800 underline"
            >
              Kimi setup
            </a>
          ) : (
            <button
              onClick={() => setShowSettings(true)}
              className="shrink-0 text-xs font-semibold text-amber-800 underline"
            >
              Open Settings
            </button>
          )}
          <button
            onClick={() => setProviderGuide(null)}
            aria-label="Dismiss"
            className="flex h-5 w-5 shrink-0 items-center justify-center rounded text-amber-600 hover:bg-amber-100"
          >
            ×
          </button>
        </div>
      )}

      {/* Clear session confirmation dialog */}
      {showClearDialog && (
        <ClearSessionDialog
          engineName={ENGINE_NAMES[currentEngine]}
          onClose={() => setShowClearDialog(false)}
          onClear={handleClear}
          onSaveAndClear={handleSaveAndClear}
        />
      )}

      {/* Memory modal */}
      {showMemory && activeProject && (
        <MemoryModal
          projectPath={activeProject.path}
          engine={currentEngine}
          onClose={() => setShowMemory(false)}
        />
      )}
    </>
  )
}
