import { useState, useEffect, useLayoutEffect, useRef, useCallback } from 'react'
import { useProjectStore } from '@/stores/project-store'
import { useTerminalStore } from '@/stores/terminal-store'
import { useSettingsStore, type CanvasMode } from '@/stores/settings-store'
import * as api from '@/lib/api'

/**
 * Canvas scripts may prefill a terminal, but may not submit commands. Removing
 * every control character means the user must review the text and press Enter.
 */
function sanitizeTerminalText(text: string): string {
  // eslint-disable-next-line no-control-regex
  return text.replace(/[\x00-\x1f\x7f-\x9f]/g, ' ').replace(/\s+/g, ' ').slice(0, 4096)
}

interface CanvasPanelProps {
  /** Layout mode — affects close button visibility and empty state */
  mode?: CanvasMode
}

interface CanvasSource {
  html: string
  projectPath: string
}

interface BoundCanvasDocument extends api.CanvasDocumentInfo {
  projectPath: string
}

interface CanvasSession {
  token: string
  projectPath: string
  source: Window
}

export default function CanvasPanel({ mode = 'panel' }: CanvasPanelProps) {
  const activeProject = useProjectStore((s) => s.activeProject)
  const [source, setSource] = useState<CanvasSource | null>(null)
  const [canvasDocument, setCanvasDocument] = useState<BoundCanvasDocument | null>(null)
  const [documentError, setDocumentError] = useState(false)
  const [loading, setLoading] = useState(false)
  const iframeRef = useRef<HTMLIFrameElement>(null)
  const sessionRef = useRef<CanvasSession | null>(null)

  // Listen for postMessage from iframe
  useLayoutEffect(() => {
    const handler = (event: MessageEvent) => {
      const iframe = iframeRef.current
      const session = sessionRef.current
      const sourceWindow = iframe?.contentWindow
      const currentProjectPath = useProjectStore.getState().activeProject?.path
      if (
        !iframe ||
        !session ||
        !sourceWindow ||
        event.source !== sourceWindow ||
        session.source !== sourceWindow ||
        session.projectPath !== currentProjectPath
      ) return
      const data = event.data
      if (!data || typeof data !== 'object') return

      const sessionIsCurrent = () => {
        const currentSession = sessionRef.current
        return currentSession?.token === session.token &&
          currentSession.projectPath === session.projectPath &&
          currentSession.source === sourceWindow &&
          iframeRef.current?.contentWindow === sourceWindow &&
          useProjectStore.getState().activeProject?.path === session.projectPath
      }
      const respond = (payload: Record<string, unknown>) => {
        if (sessionIsCurrent()) sourceWindow.postMessage(payload, '*')
      }

      switch (data.type) {
        case 'send_to_terminal': {
          const terminalState = useTerminalStore.getState()
          const termId = terminalState.activeTerminalId
          const terminal = termId ? terminalState.terminals.get(termId) : null
          // OpenRouter credentials live only in that PTY's process environment;
          // project-controlled Canvas scripts never get a write bridge to it.
          if (termId && terminal?.engine !== 'openrouter' && typeof data.text === 'string') {
            api.ptyWrite(termId, sanitizeTerminalText(data.text))
          }
          break
        }
        case 'switch_tab': {
          if (typeof data.tab === 'string' && ['tips', 'agents', 'skills', 'mcps', 'canvas'].includes(data.tab)) {
            useSettingsStore.getState().setRightPanelActiveTab(data.tab as never)
          }
          break
        }
        case 'set_canvas_mode': {
          if (typeof data.mode === 'string' && ['panel', 'full', 'bottom'].includes(data.mode)) {
            useSettingsStore.getState().setCanvasMode(data.mode as CanvasMode)
          }
          break
        }
        case 'read_file': {
          if (typeof data.path === 'string' && Number.isSafeInteger(data.reqId)) {
            const reqId = data.reqId
            api.canvasReadFile(session.projectPath, data.path).then((content) => {
              respond({ type: 'yft_response', reqId, content })
            }).catch(() => {
              respond({ type: 'yft_response', reqId, error: 'not_found' })
            })
          }
          break
        }
        case 'read_dir': {
          if (typeof data.path === 'string' && Number.isSafeInteger(data.reqId)) {
            const reqId = data.reqId
            api.canvasReadDir(session.projectPath, data.path).then((entries) => {
              respond({ type: 'yft_response', reqId, entries })
            }).catch(() => {
              respond({ type: 'yft_response', reqId, error: 'not_found' })
            })
          }
          break
        }
      }
    }

    window.addEventListener('message', handler)
    return () => window.removeEventListener('message', handler)
  }, [])

  // Load canvas.html and watch for changes
  useEffect(() => {
    if (!activeProject) {
      setSource(null)
      return
    }

    let cancelled = false
    const projectPath = activeProject.path
    const canvasPath = `${projectPath}/canvas.html`

    const load = async () => {
      setLoading(true)
      setSource(null)
      try {
        const text = await api.readFile(canvasPath)
        if (!cancelled) setSource(text === null ? null : { html: text, projectPath })
      } catch {
        if (!cancelled) setSource(null)
      } finally {
        if (!cancelled) setLoading(false)
      }
    }

    load()

    void api.fsWatch(projectPath)

    const unsub = api.onFsChanged((rootPath, changedDir) => {
      if (rootPath !== projectPath) return
      const normalizedChanged = changedDir.replace(/\\/g, '/')
      const normalizedRoot = projectPath.replace(/\\/g, '/')
      if (normalizedChanged === normalizedRoot || changedDir === projectPath) {
        api.readFile(canvasPath).then((text) => {
          if (cancelled) return
          setSource(text === null ? null : { html: text, projectPath })
          if (text !== null) {
            const store = useSettingsStore.getState()
            // Only auto-switch to canvas tab when in panel mode
            if (store.canvasMode === 'panel') {
              store.setRightPanelActiveTab('canvas')
            }
          }
        }).catch(() => { if (!cancelled) setSource(null) })
      }
    })

    return () => {
      cancelled = true
      unsub()
      void api.fsUnwatch(projectPath)
    }
  }, [activeProject])

  // Project HTML runs from a dedicated custom-protocol origin with its own
  // restrictive CSP. It never inherits or weakens the privileged renderer CSP.
  useEffect(() => {
    let cancelled = false
    let token: string | null = null
    setCanvasDocument(null)
    setDocumentError(false)
    if (source === null) return

    void api.canvasCreateDocument(source.html)
      .then((document) => {
        token = document.token
        if (cancelled) {
          void api.canvasDisposeDocument(document.token)
          return
        }
        setCanvasDocument({ ...document, projectPath: source.projectPath })
      })
      .catch(() => {
        if (!cancelled) setDocumentError(true)
      })

    return () => {
      cancelled = true
      if (token) void api.canvasDisposeDocument(token)
    }
  }, [source])

  useLayoutEffect(() => {
    const sourceWindow = iframeRef.current?.contentWindow
    if (
      !canvasDocument ||
      !sourceWindow ||
      canvasDocument.projectPath !== activeProject?.path
    ) {
      sessionRef.current = null
      return
    }
    const session: CanvasSession = {
      token: canvasDocument.token,
      projectPath: canvasDocument.projectPath,
      source: sourceWindow
    }
    sessionRef.current = session
    return () => {
      if (sessionRef.current === session) sessionRef.current = null
    }
  }, [activeProject?.path, canvasDocument])

  const handleClose = useCallback(() => {
    useSettingsStore.getState().setCanvasMode('panel')
    useSettingsStore.getState().setRightPanelActiveTab('canvas')
  }, [])

  if (!activeProject) {
    return (
      <div className="p-4 text-sm text-win-text-tertiary">
        Select a project to use Canvas.
      </div>
    )
  }

  const activeSource = source?.projectPath === activeProject.path ? source : null
  const activeDocument = canvasDocument?.projectPath === activeProject.path ? canvasDocument : null

  if (loading || (activeSource !== null && !activeDocument && !documentError)) {
    return (
      <div className="p-4 text-sm text-win-text-tertiary">Loading canvas...</div>
    )
  }

  if (activeSource === null) {
    // In full/bottom mode, show a minimal empty state
    if (mode !== 'panel') {
      return (
        <div className="flex flex-1 items-center justify-center text-sm text-win-text-tertiary">
          <p>No canvas.html found. Ask your AI assistant to create one.</p>
        </div>
      )
    }
    return (
      <div className="flex flex-col items-center gap-3 p-6 text-center">
        <div className="text-win-text-tertiary">
          <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M18.37 2.63 14 7l-1.59-1.59a2 2 0 0 0-2.82 0L8 7l9 9 1.59-1.59a2 2 0 0 0 0-2.82L17 10l4.37-4.37a2.12 2.12 0 1 0-3-3Z" />
            <path d="M9 8c-2 3-4 3.5-7 4l8 10c2-1 6-5 6-7" />
            <path d="M14.5 17.5 4.5 15" />
          </svg>
        </div>
        <p className="text-sm text-win-text-secondary font-medium">Canvas</p>
        <p className="text-xs text-win-text-tertiary leading-relaxed max-w-[240px]">
          Ask your AI assistant to create a dashboard, form, visualization, or any custom UI.
          It will appear here as an interactive HTML page.
        </p>
      </div>
    )
  }

  if (documentError || !activeDocument) {
    return (
      <div className="flex flex-1 items-center justify-center p-4 text-sm text-red-600">
        Canvas could not be rendered safely.
      </div>
    )
  }

  return (
    <div className="relative flex flex-1 flex-col h-full overflow-hidden">
      {/* Close button for full/bottom modes */}
      {mode !== 'panel' && (
        <button
          onClick={handleClose}
          className="absolute top-2 right-2 z-10 flex items-center gap-1.5 rounded-md border border-win-border bg-win-surface/90 backdrop-blur-sm px-2.5 py-1.5 text-xs font-medium text-win-text-secondary hover:bg-win-hover hover:text-win-text transition-colors shadow-sm"
          title="Move canvas to panel"
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      )}
      <iframe
        key={activeDocument.token}
        ref={iframeRef}
        src={activeDocument.url}
        sandbox="allow-scripts"
        className="w-full h-full border-0 flex-1"
        title="Canvas"
      />
    </div>
  )
}
