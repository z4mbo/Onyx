import { useCallback } from 'react'
import { useTerminalStore, generateTerminalId } from '@/stores/terminal-store'
import { useSettingsStore } from '@/stores/settings-store'
import { useProjectStore } from '@/stores/project-store'
import { ENGINE_NAMES, ENGINE_TEXT_COLORS, type EngineId } from '@/lib/constants'
import * as api from '@/lib/api'

/**
 * Horizontal tab bar for chat sessions.
 * Windows 11-style tabs with rounded selected indicator.
 */
export default function TerminalTabs() {
  const terminals = useTerminalStore((s) => s.terminals)
  const activeTerminalId = useTerminalStore((s) => s.activeTerminalId)
  const addTerminal = useTerminalStore((s) => s.addTerminal)
  const removeTerminal = useTerminalStore((s) => s.removeTerminal)
  const setActive = useTerminalStore((s) => s.setActive)
  const defaultEngine = useSettingsStore((s) => s.defaultEngine)
  const setShowSettings = useSettingsStore((s) => s.setShowSettingsDialog)
  const activeProject = useProjectStore((s) => s.activeProject)

  const tabList = Array.from(terminals.values())

  const handleNew = useCallback(async () => {
    if (defaultEngine === 'openrouter') {
      try {
        const [status, detected] = await Promise.all([
          api.openRouterGetStatus(),
          api.detectEngines()
        ])
        const openRouterAvailable = detected
          .some((engine) => engine.id === 'openrouter' && engine.isAvailable)
        if (!status.hasApiKey || !status.selectedModelId || !openRouterAvailable) {
          setShowSettings(true)
          return
        }
      } catch {
        setShowSettings(true)
        return
      }
    }
    const id = generateTerminalId()
    addTerminal({
      id,
      ptyId: null,
      name: `Chat ${tabList.length + 1}`,
      engine: defaultEngine,
      isActive: true,
      cwd: activeProject?.path || '',
      isLoading: true
    })
  }, [addTerminal, defaultEngine, tabList.length, activeProject, setShowSettings])

  const handleClose = useCallback(
    (e: React.MouseEvent, id: string) => {
      e.stopPropagation()
      api.ptyKill(id)
      removeTerminal(id)
    },
    [removeTerminal, terminals]
  )

  const handleSelect = useCallback(
    (id: string) => {
      setActive(id)
    },
    [setActive]
  )

  return (
    <div className="flex h-10 shrink-0 items-center border-b border-win-border bg-win-bg">
      {/* Tabs */}
      <div className="flex flex-1 items-center overflow-x-auto px-1 gap-0.5">
        {tabList.map((term) => {
          const isActive = term.id === activeTerminalId
          return (
            <button
              key={term.id}
              onClick={() => handleSelect(term.id)}
              className={`group relative flex h-8 items-center gap-1.5 rounded-md px-3 text-sm transition-all ${
                isActive
                  ? 'bg-win-card text-win-text'
                  : 'text-win-text-secondary hover:bg-win-hover hover:text-win-text'
              }`}
            >
              {/* Active indicator - Windows 11 style bottom pill */}
              {isActive && (
                <div className="absolute bottom-0 left-3 right-3 h-[2px] rounded-full bg-win-accent" />
              )}

              {/* Chat bubble icon */}
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={`shrink-0 ${ENGINE_TEXT_COLORS[term.engine]}`}>
                <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
              </svg>

              <span className="truncate max-w-[120px]">{term.name}</span>

              <span className="text-xs text-win-text-tertiary">
                {ENGINE_NAMES[term.engine]}
              </span>

              {/* Close button */}
              <span
                onClick={(e) => handleClose(e, term.id)}
                className="ml-1 flex h-4 w-4 shrink-0 items-center justify-center rounded text-win-text-tertiary hover:bg-win-pressed hover:text-win-text opacity-0 group-hover:opacity-100 transition-opacity"
                role="button"
                aria-label={`Close ${term.name}`}
              >
                <svg width="8" height="8" viewBox="0 0 8 8" fill="none" stroke="currentColor" strokeWidth="1.5">
                  <line x1="1" y1="1" x2="7" y2="7" />
                  <line x1="7" y1="1" x2="1" y2="7" />
                </svg>
              </span>
            </button>
          )
        })}
      </div>

      {/* Add new chat */}
      <button
        onClick={handleNew}
        className="flex h-10 w-10 shrink-0 items-center justify-center text-win-text-tertiary hover:bg-win-hover hover:text-win-text transition-colors"
        aria-label="New chat"
      >
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5">
          <line x1="7" y1="2" x2="7" y2="12" />
          <line x1="2" y1="7" x2="12" y2="7" />
        </svg>
      </button>
    </div>
  )
}
