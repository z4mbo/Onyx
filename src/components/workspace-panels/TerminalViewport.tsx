import { FitAddon } from "@xterm/addon-fit"
import { Terminal } from "@xterm/xterm"
import "@xterm/xterm/css/xterm.css"
import { onCleanup, onMount, type Component } from "solid-js"
import type { UnlistenFn } from "@tauri-apps/api/event"
import { api } from "../../lib/api"
import type { TerminalEvent } from "../../lib/types"

const MAX_REPLAY_BYTES = 1_048_576
const replayBuffers = new Map<string, string>()
const subscribers = new Map<string, Set<(event: TerminalEvent) => void>>()
const mountedTerminals = new Map<string, Set<Terminal>>()
const closedSessions = new Set<string>()
const discardedSessions = new Map<string, ReturnType<typeof setTimeout>>()
let bridge: Promise<UnlistenFn> | null = null

function trimReplay(value: string) {
  if (value.length <= MAX_REPLAY_BYTES) return value
  return value.slice(value.length - MAX_REPLAY_BYTES)
}

function ensureTerminalBridge() {
  if (bridge) return bridge
  bridge = api.listenTerminal((event) => {
    if (discardedSessions.has(event.sessionId)) {
      if (event.kind === "exit" || event.kind === "error") {
        clearTimeout(discardedSessions.get(event.sessionId))
        discardedSessions.delete(event.sessionId)
      }
      return
    }
    if (event.kind === "data" && event.data) {
      replayBuffers.set(event.sessionId, trimReplay(`${replayBuffers.get(event.sessionId) ?? ""}${event.data}`))
    } else if (event.kind === "exit") {
      closedSessions.add(event.sessionId)
      const suffix = `\r\n\u001b[2m[process exited${event.exitCode === null ? "" : ` with code ${event.exitCode}`}]\u001b[0m\r\n`
      replayBuffers.set(event.sessionId, trimReplay(`${replayBuffers.get(event.sessionId) ?? ""}${suffix}`))
    } else if (event.kind === "error" && event.data) {
      closedSessions.add(event.sessionId)
      const suffix = `\r\n\u001b[31m[terminal] ${event.data}\u001b[0m\r\n`
      replayBuffers.set(event.sessionId, trimReplay(`${replayBuffers.get(event.sessionId) ?? ""}${suffix}`))
    }
    subscribers.get(event.sessionId)?.forEach((subscriber) => subscriber(event))
  })
  return bridge
}

/** Start buffering PTY output before a viewport mounts so fast shell prompts are never lost. */
export async function startTerminalViewportBridge() {
  const unlisten = await ensureTerminalBridge()
  let disposed = false
  return () => {
    if (disposed) return
    disposed = true
    unlisten()
    bridge = null
  }
}

export function clearTerminalViewport(sessionId: string) {
  replayBuffers.set(sessionId, "")
  mountedTerminals.get(sessionId)?.forEach((terminal) => terminal.clear())
}

export function forgetTerminalViewport(sessionId: string) {
  replayBuffers.delete(sessionId)
  subscribers.delete(sessionId)
  mountedTerminals.delete(sessionId)
  closedSessions.delete(sessionId)
  clearTimeout(discardedSessions.get(sessionId))
  discardedSessions.set(sessionId, setTimeout(() => discardedSessions.delete(sessionId), 30_000))
}

export interface TerminalViewportProps {
  sessionId: string
  autofocus?: boolean
  class?: string
}

/** xterm.js viewport connected to Onyx's native Rust PTY session. */
export const TerminalViewport: Component<TerminalViewportProps> = (props) => {
  let mount: HTMLDivElement | undefined

  onMount(() => {
    if (!mount) return
    let disposed = false
    let resizeFrame = 0
    const styles = getComputedStyle(mount)
    const terminal = new Terminal({
      allowTransparency: true,
      convertEol: false,
      cursorBlink: true,
      cursorStyle: "block",
      fontFamily: '"JetBrains Mono", "SFMono-Regular", Menlo, Consolas, monospace',
      fontSize: 12,
      letterSpacing: 0,
      lineHeight: 1.25,
      minimumContrastRatio: 4.5,
      scrollback: 5_000,
      theme: {
        background: "transparent",
        foreground: styles.color || "#e8e8e8",
        cursor: styles.color || "#e8e8e8",
        cursorAccent: styles.backgroundColor || "#111",
        selectionBackground: "rgba(120, 150, 255, 0.28)",
        black: "#202020",
        red: "#ff6b7a",
        green: "#77d991",
        yellow: "#e8c56b",
        blue: "#7db4ff",
        magenta: "#c79cff",
        cyan: "#69d9df",
        white: "#d7d7d7",
        brightBlack: "#777",
        brightRed: "#ff9aa5",
        brightGreen: "#a7e9b7",
        brightYellow: "#f6dc9b",
        brightBlue: "#a8ccff",
        brightMagenta: "#dfc0ff",
        brightCyan: "#9cebef",
        brightWhite: "#fff",
      },
    })
    const fit = new FitAddon()
    terminal.loadAddon(fit)
    terminal.open(mount)

    const terminalSet = mountedTerminals.get(props.sessionId) ?? new Set<Terminal>()
    terminalSet.add(terminal)
    mountedTerminals.set(props.sessionId, terminalSet)

    const replay = replayBuffers.get(props.sessionId)
    if (replay) terminal.write(replay)
    if (closedSessions.has(props.sessionId)) terminal.options.disableStdin = true

    const receive = (event: TerminalEvent) => {
      if (event.kind === "data" && event.data) terminal.write(event.data)
      else if (event.kind === "exit") {
        terminal.options.disableStdin = true
        terminal.write(`\r\n\u001b[2m[process exited${event.exitCode === null ? "" : ` with code ${event.exitCode}`}]\u001b[0m\r\n`)
      } else if (event.kind === "error" && event.data) {
        terminal.options.disableStdin = true
        terminal.write(`\r\n\u001b[31m[terminal] ${event.data}\u001b[0m\r\n`)
      }
    }
    const sessionSubscribers = subscribers.get(props.sessionId) ?? new Set()
    sessionSubscribers.add(receive)
    subscribers.set(props.sessionId, sessionSubscribers)
    void ensureTerminalBridge().catch((error) => {
      if (!disposed) terminal.write(`\r\n\u001b[31m[terminal] ${String(error)}\u001b[0m\r\n`)
    })

    const dataSubscription = terminal.onData((data) => {
      if (closedSessions.has(props.sessionId)) return
      void api.terminalWrite(props.sessionId, data).catch((error) => {
        terminal.write(`\r\n\u001b[31m[terminal] ${String(error)}\u001b[0m\r\n`)
      })
    })
    const resizeTerminal = () => {
      cancelAnimationFrame(resizeFrame)
      resizeFrame = requestAnimationFrame(() => {
        if (disposed || !mount?.isConnected) return
        try {
          fit.fit()
          if (terminal.cols > 0 && terminal.rows > 0) {
            void api.terminalResize(props.sessionId, terminal.cols, terminal.rows).catch(() => undefined)
          }
        } catch {
          // The panel may be between layout states; the observer will retry.
        }
      })
    }
    const observer = new ResizeObserver(resizeTerminal)
    observer.observe(mount)
    resizeTerminal()
    if (props.autofocus) queueMicrotask(() => terminal.focus())

    onCleanup(() => {
      disposed = true
      cancelAnimationFrame(resizeFrame)
      observer.disconnect()
      dataSubscription.dispose()
      sessionSubscribers.delete(receive)
      if (sessionSubscribers.size === 0) subscribers.delete(props.sessionId)
      terminalSet.delete(terminal)
      if (terminalSet.size === 0) mountedTerminals.delete(props.sessionId)
      terminal.dispose()
    })
  })

  return (
    <div
      ref={mount}
      class={`zai-xterm-viewport${props.class ? ` ${props.class}` : ""}`}
      data-terminal-session={props.sessionId}
      aria-label="Interactive terminal"
    />
  )
}
