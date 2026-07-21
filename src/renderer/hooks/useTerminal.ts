import { useEffect, useRef, useCallback } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebglAddon } from '@xterm/addon-webgl'
import { DEFAULT_TERMINAL_OPTIONS } from '@/lib/constants'
import { useTerminalStore } from '@/stores/terminal-store'
import { useSettingsStore } from '@/stores/settings-store'
import { useSplitViewStore } from '@/stores/split-view-store'
import * as api from '@/lib/api'

import '@xterm/xterm/css/xterm.css'

interface UseTerminalOptions {
  terminalId: string
  cwd?: string
}

/**
 * Tracks which PTY IDs have been spawned so that React StrictMode
 * double-mount doesn't spawn a second time for the same ID.
 */
const spawnedPtys = new Set<string>()

/**
 * Tracks which PTY IDs have had the engine command sent,
 * so we only auto-start once even across StrictMode remounts.
 */
const engineStartedPtys = new Set<string>()

export function useTerminal({ terminalId, cwd }: UseTerminalOptions) {
  const containerRef = useRef<HTMLDivElement>(null)
  const terminalRef = useRef<Terminal | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)
  const disposeDataRef = useRef<(() => void) | null>(null)
  const disposeExitRef = useRef<(() => void) | null>(null)
  const ptyReadyRef = useRef(false)
  const engineCommandSentRef = useRef(false)
  const engineReadyRef = useRef(false)
  const loadingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const updateTerminal = useTerminalStore((s) => s.updateTerminal)
  const setFocusedTerminalId = useSplitViewStore((s) => s.setFocusedTerminalId)

  const fit = useCallback(() => {
    const fitAddon = fitAddonRef.current
    const terminal = terminalRef.current
    if (!fitAddon || !terminal) return
    try {
      fitAddon.fit()
      if (ptyReadyRef.current) {
        api.ptyResize(terminalId, terminal.cols, terminal.rows)
      }
    } catch {
      // Container may not be visible yet
    }
  }, [terminalId])

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    // Capture the engine type for auto-starting (captured once at effect start)
    const terminalEntry = useTerminalStore.getState().terminals.get(terminalId)
    const engineId = terminalEntry?.engine ?? 'claude'
    const launchIntent = terminalEntry?.launchIntent ?? 'start-session'
    const isMac = navigator.userAgent.includes('Mac')

    const terminal = new Terminal({
      ...DEFAULT_TERMINAL_OPTIONS,
      theme: useSettingsStore.getState().getResolvedTheme()
    })
    terminal.attachCustomKeyEventHandler((event: KeyboardEvent) => {
      if (event.type !== 'keydown') return true
      const key = event.key.toLowerCase()
      const primaryModifier = isMac ? event.metaKey : event.ctrlKey
      const hasConflictingModifier =
        event.altKey ||
        event.shiftKey ||
        event.getModifierState('AltGraph') ||
        (isMac ? event.ctrlKey : event.metaKey)

      // Cmd+C on macOS and Ctrl+C on Windows/Linux copy a selection. Without
      // a selection, let xterm forward the key so the PTY can send SIGINT.
      if (primaryModifier && !hasConflictingModifier && key === 'c' && terminal.hasSelection()) {
        void navigator.clipboard.writeText(terminal.getSelection())
        terminal.clearSelection()
        return false
      }
      if (primaryModifier && !hasConflictingModifier && key === 'v') {
        event.preventDefault()
        const text = api.clipboardReadText()
        if (text && ptyReadyRef.current) terminal.paste(text)
        return false
      }
      return true
    })
    terminalRef.current = terminal

    const fitAddon = new FitAddon()
    fitAddonRef.current = fitAddon
    terminal.loadAddon(fitAddon)

    terminal.open(container)

    // Track which terminal has focus for cross-panel clipboard paste routing
    // AND update activeTerminalId so sidebar context buttons target the right terminal
    const focusHandler = () => {
      useSplitViewStore.getState().setFocusedTerminalId(terminalId)
      // Set this terminal as active so file attach, tips, and toolbar target it
      const panel = useSplitViewStore.getState().getPanelByTerminalId(terminalId)
      if (panel && panel.activeTerminalId !== terminalId) {
        useSplitViewStore.getState().setActiveTerminal(panel.panelId, terminalId)
      }
    }
    container.addEventListener('focusin', focusHandler)

    // Right-click context menu (Copy / Paste)
    const contextMenuHandler = (e: MouseEvent) => {
      e.preventDefault()
      api.showTerminalContextMenu(terminal.hasSelection()).then((action) => {
        if (action === 'copy' && terminal.hasSelection()) {
          navigator.clipboard.writeText(terminal.getSelection())
          terminal.clearSelection()
        } else if (action === 'paste') {
          const text = api.clipboardReadText()
          if (text && ptyReadyRef.current) {
            terminal.paste(text)
          }
        }
      })
    }
    container.addEventListener('contextmenu', contextMenuHandler)

    // Subscribe to theme changes for live updates
    let prevThemeId = useSettingsStore.getState().terminalTheme
    let prevCustom = useSettingsStore.getState().terminalThemeCustom
    const unsubTheme = useSettingsStore.subscribe((state) => {
      if (state.terminalTheme !== prevThemeId || state.terminalThemeCustom !== prevCustom) {
        prevThemeId = state.terminalTheme
        prevCustom = state.terminalThemeCustom
        terminal.options.theme = state.getResolvedTheme()
      }
    })

    try {
      const webglAddon = new WebglAddon()
      webglAddon.onContextLoss(() => {
        webglAddon.dispose()
      })
      terminal.loadAddon(webglAddon)
    } catch {
      console.warn('[useTerminal] WebGL addon not available, falling back to canvas renderer.')
    }

    fitAddon.fit()

    let disposed = false
    ptyReadyRef.current = false

    async function initPty() {
      // Skip spawn if this PTY was already created (StrictMode remount)
      const alreadySpawned = spawnedPtys.has(terminalId)

      try {
        const [startCommand, platform] = await Promise.all([
          api.getCommand(engineId, launchIntent),
          api.getPlatform()
        ])

        if (!alreadySpawned) {
          spawnedPtys.add(terminalId)
          const result = await api.ptySpawn(terminalId, {
            cols: terminal.cols,
            rows: terminal.rows,
            cwd: cwd ?? undefined,
            engineId
          })
          if (!result.success) {
            throw new Error(result.error || 'The terminal process could not be started.')
          }
        }

        if (disposed) {
          // Don't kill — the next mount will reuse this PTY
          return
        }

        ptyReadyRef.current = true
        // Mark the terminal as having a live PTY so toolbar buttons (Clear, Compact) enable
        updateTerminal(terminalId, { ptyId: terminalId })

        // Sends clear + engine command as soon as the shell is ready
        const clearCmd = platform === 'win32' ? 'cls' : 'clear'
        let awaitingShellPrompt = false

        const sendEngineCommand = () => {
          if (!ptyReadyRef.current || engineCommandSentRef.current) return
          console.log('[useTerminal] 📤 Sending engine command:', startCommand, 'to terminal:', terminalId)
          api.ptyWrite(terminalId, `${clearCmd}\r`)
          api.ptyWrite(terminalId, `${startCommand}\r`)
          engineCommandSentRef.current = true

          // Fallback timeout — if engine doesn't show a prompt within 15s, clear loading anyway
          loadingTimeoutRef.current = setTimeout(() => {
            if (!engineReadyRef.current) {
              console.log('[useTerminal] ⏱️ Fallback timeout: setting isLoading=false for', terminalId)
              updateTerminal(terminalId, { isLoading: false })
              engineReadyRef.current = true
            }
          }, 15000)
        }

        // PTY → Terminal
        disposeDataRef.current = api.onPtyData((id, data) => {
          if (id === terminalId) {
            terminal.write(data)

            // Phase 1: Detect shell prompt → immediately send engine command
            // Instead of a blind 1s delay, we watch for the shell's first prompt.
            if (awaitingShellPrompt && !engineCommandSentRef.current) {
              const clean = data.replace(/\x1b(?:\[[0-9;?]*[a-zA-Z]|\][^\x07]*\x07|\(B)/g, '')
              // PowerShell: "PS path> ", bash: "user@host:~$ ", zsh: "% "
              if (clean.includes('> ') || clean.trimEnd().endsWith('>') ||
                  clean.includes('$ ') || clean.trimEnd().endsWith('$') ||
                  clean.includes('% ')) {
                awaitingShellPrompt = false
                sendEngineCommand()
              }
            }

            // Phase 2: Detect engine readiness AFTER the engine command has been sent
            if (engineCommandSentRef.current && !engineReadyRef.current) {
              // Strip all ANSI escape sequences (SGR, cursor, erase, OSC, etc.)
              const cleanData = data.replace(/\x1b(?:\[[0-9;?]*[a-zA-Z]|\][^\x07]*\x07|\(B)/g, '')

              // Claude Code shows '>' or '❯' prompt when ready
              // Gemini CLI shows '❯' prompt when ready
              if (cleanData.includes('> ') || cleanData.includes('❯') || cleanData.trim().endsWith('>')) {
                console.log('[useTerminal] ✅ Engine ready detected! Setting isLoading=false for', terminalId)
                updateTerminal(terminalId, { isLoading: false })
                engineReadyRef.current = true
                if (loadingTimeoutRef.current) {
                  clearTimeout(loadingTimeoutRef.current)
                  loadingTimeoutRef.current = null
                }
              }
            }
          }
        })

        // PTY exit
        disposeExitRef.current = api.onPtyExit((id, exitCode) => {
          if (id === terminalId) {
            ptyReadyRef.current = false
            spawnedPtys.delete(terminalId)
            engineStartedPtys.delete(terminalId)
            updateTerminal(terminalId, { ptyId: null })
            terminal.write(`\r\n[Process exited with code ${exitCode}]\r\n`)
          }
        })

        // Terminal → PTY (only after spawn is done)
        terminal.onData((data) => {
          if (ptyReadyRef.current) {
            api.ptyWrite(terminalId, data)
          }
        })

        // Sync size now that PTY exists
        fitAddon.fit()
        api.ptyResize(terminalId, terminal.cols, terminal.rows)

        // Auto-start the AI engine command (separate from PTY spawn guard)
        if (!engineStartedPtys.has(terminalId) && terminalEntry) {
          engineStartedPtys.add(terminalId)
          console.log('[useTerminal] 🚀 Will auto-start engine:', engineId, 'for terminal:', terminalId)
          // Watch PTY output for the shell prompt instead of a blind delay.
          // The PTY already starts in the correct directory via the cwd spawn option.
          awaitingShellPrompt = true

          // Fallback: if shell prompt isn't detected within 3s, send command anyway
          setTimeout(() => {
            if (!awaitingShellPrompt) return
            awaitingShellPrompt = false
            console.log('[useTerminal] ⏱️ Shell prompt fallback for', terminalId)
            sendEngineCommand()
          }, 3000)
        } else {
          console.log('[useTerminal] ⏭️ Skipping auto-start (already started or no terminal entry)', {
            alreadyStarted: engineStartedPtys.has(terminalId),
            hasTerminalEntry: !!terminalEntry,
            terminalId
          })
        }
      } catch (err) {
        console.error('[useTerminal] Failed to create PTY:', err)
        spawnedPtys.delete(terminalId)
        updateTerminal(terminalId, { ptyId: null, isLoading: false })
        const message = err instanceof Error ? err.message : String(err)
        if (engineId === 'openrouter') {
          terminal.write(`\r\n[OpenRouter is not ready: ${message}]\r\n`)
          terminal.write('[Open Settings → Providers, connect your API key, and choose a model before starting a new OpenRouter chat.]\r\n')
        } else {
          terminal.write(`\r\n[Failed to start terminal process: ${message}]\r\n`)
        }
      }
    }

    initPty()

    // ResizeObserver — only forward resize to PTY if it's ready
    const resizeObserver = new ResizeObserver(() => {
      requestAnimationFrame(() => {
        try {
          fitAddon.fit()
          if (ptyReadyRef.current) {
            api.ptyResize(terminalId, terminal.cols, terminal.rows)
          }
        } catch {
          // ignore
        }
      })
    })
    resizeObserver.observe(container)

    return () => {
      disposed = true
      ptyReadyRef.current = false
      resizeObserver.disconnect()
      container.removeEventListener('focusin', focusHandler)
      container.removeEventListener('contextmenu', contextMenuHandler)
      unsubTheme()
      disposeDataRef.current?.()
      disposeExitRef.current?.()
      if (loadingTimeoutRef.current) {
        clearTimeout(loadingTimeoutRef.current)
        loadingTimeoutRef.current = null
      }
      // Don't kill PTY here — StrictMode will remount and reuse it.
      // PTY gets killed when the tab is explicitly closed (via TerminalTabs).
      terminal.dispose()
      terminalRef.current = null
      fitAddonRef.current = null
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [terminalId, cwd])

  return { containerRef, terminalRef, fit }
}
