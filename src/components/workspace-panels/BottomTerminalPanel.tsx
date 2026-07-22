import {
  Eraser,
  Plus,
  SquareSplitHorizontal,
  SquareSplitVertical,
  TerminalSquare,
  Trash2,
  X,
} from "lucide-solid"
import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  type Component,
} from "solid-js"
import type { TerminalRenderer, WorkspaceTerminal } from "./types"

export interface BottomTerminalPanelProps {
  open: boolean
  terminals: readonly WorkspaceTerminal[]
  activeTerminalId: string | null
  height?: number
  minHeight?: number
  maxHeight?: number
  renderTerminal?: TerminalRenderer
  onActivate: (terminalId: string) => void
  onCloseTerminal: (terminalId: string) => void
  onNewTerminal: () => void
  onSplitHorizontal?: () => void
  onSplitVertical?: () => void
  onClear?: (terminalId: string) => void
  onHeightChange?: (height: number) => void
  onClosePanel?: () => void
}

const terminalTabId = (id: string) => `zai-bottom-terminal-tab-${encodeURIComponent(id)}`
const terminalPanelId = (id: string) => `zai-bottom-terminal-${encodeURIComponent(id)}`

/** Resizable T3 Code-style terminal drawer with a backend-neutral render hook. */
export const BottomTerminalPanel: Component<BottomTerminalPanelProps> = (props) => {
  const [drawerHeight, setDrawerHeight] = createSignal(props.height ?? 280)
  const tabElements = new Map<string, HTMLButtonElement>()
  let resizeStart: { pointerId: number; y: number; height: number } | null = null

  const activeTerminal = createMemo(
    () => props.terminals.find((terminal) => terminal.id === props.activeTerminalId) ?? null,
  )
  const clampedHeight = (height: number) => {
    const minimum = props.minHeight ?? 180
    const maximum = props.maxHeight ?? Math.max(minimum, Math.floor(window.innerHeight * 0.75))
    return Math.min(Math.max(Math.round(height), minimum), maximum)
  }
  const setHeight = (height: number) => {
    const next = clampedHeight(height)
    setDrawerHeight(next)
    props.onHeightChange?.(next)
  }

  createEffect(() => {
    if (props.height === undefined) return
    setDrawerHeight(clampedHeight(props.height))
  })

  onCleanup(() => {
    delete document.documentElement.dataset.workspacePanelResizing
  })

  const focusAndActivate = (index: number) => {
    const terminal = props.terminals[index]
    if (!terminal) return
    props.onActivate(terminal.id)
    queueMicrotask(() => {
      const element = tabElements.get(terminal.id)
      element?.focus()
      element?.scrollIntoView({ block: "nearest", inline: "nearest" })
    })
  }

  const handleTabKeyDown = (event: KeyboardEvent, terminal: WorkspaceTerminal) => {
    const index = props.terminals.findIndex((entry) => entry.id === terminal.id)
    if (index < 0) return
    if (event.key === "Delete") {
      event.preventDefault()
      props.onCloseTerminal(terminal.id)
      return
    }
    let nextIndex: number | null = null
    if (event.key === "ArrowLeft") {
      nextIndex = (index - 1 + props.terminals.length) % props.terminals.length
    } else if (event.key === "ArrowRight") {
      nextIndex = (index + 1) % props.terminals.length
    } else if (event.key === "Home") {
      nextIndex = 0
    } else if (event.key === "End") {
      nextIndex = props.terminals.length - 1
    }
    if (nextIndex === null) return
    event.preventDefault()
    focusAndActivate(nextIndex)
  }

  const handleResizeStart = (event: PointerEvent & { currentTarget: HTMLDivElement }) => {
    resizeStart = { pointerId: event.pointerId, y: event.clientY, height: drawerHeight() }
    event.currentTarget.setPointerCapture(event.pointerId)
    document.documentElement.dataset.workspacePanelResizing = "true"
  }
  const handleResizeMove = (event: PointerEvent & { currentTarget: HTMLDivElement }) => {
    if (!resizeStart || resizeStart.pointerId !== event.pointerId) return
    setHeight(resizeStart.height + resizeStart.y - event.clientY)
  }
  const handleResizeEnd = (event: PointerEvent & { currentTarget: HTMLDivElement }) => {
    if (!resizeStart || resizeStart.pointerId !== event.pointerId) return
    resizeStart = null
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
    delete document.documentElement.dataset.workspacePanelResizing
  }

  return (
    <Show when={props.open}>
      <aside
        class="zai-bottom-terminal"
        data-slot="bottom-terminal-panel"
        aria-label="Terminal drawer"
        style={{ height: `${drawerHeight()}px` }}
      >
        <div
          class="zai-bottom-terminal__resize-handle"
          role="separator"
          aria-label="Resize terminal drawer"
          aria-orientation="horizontal"
          aria-valuemin={props.minHeight ?? 180}
          aria-valuemax={props.maxHeight ?? Math.floor(window.innerHeight * 0.75)}
          aria-valuenow={drawerHeight()}
          tabIndex={0}
          onPointerDown={handleResizeStart}
          onPointerMove={handleResizeMove}
          onPointerUp={handleResizeEnd}
          onPointerCancel={handleResizeEnd}
          onKeyDown={(event) => {
            if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return
            event.preventDefault()
            const increment = event.shiftKey ? 40 : 10
            setHeight(drawerHeight() + (event.key === "ArrowUp" ? increment : -increment))
          }}
        />

        <header class="zai-bottom-terminal__header">
          <div class="zai-bottom-terminal__tabs" role="tablist" aria-label="Terminal sessions">
            <For each={props.terminals}>
              {(terminal) => {
                const active = () => terminal.id === props.activeTerminalId
                return (
                  <div class="zai-terminal-tab" data-active={active() ? "true" : "false"}>
                    <button
                      ref={(element) => tabElements.set(terminal.id, element)}
                      type="button"
                      id={terminalTabId(terminal.id)}
                      role="tab"
                      aria-selected={active()}
                      aria-controls={terminalPanelId(terminal.id)}
                      tabIndex={active() ? 0 : -1}
                      title={terminal.cwd ? `${terminal.title} — ${terminal.cwd}` : terminal.title}
                      onClick={() => props.onActivate(terminal.id)}
                      onKeyDown={(event) => handleTabKeyDown(event, terminal)}
                    >
                      <TerminalSquare aria-hidden="true" />
                      <span>{terminal.title}</span>
                      <Show when={terminal.status === "starting"}>
                        <span class="zai-terminal-tab__status" aria-label="Starting" />
                      </Show>
                    </button>
                    <button
                      type="button"
                      class="zai-terminal-tab__close"
                      aria-label={`Close ${terminal.title}`}
                      title={`Close ${terminal.title}`}
                      onClick={() => props.onCloseTerminal(terminal.id)}
                    >
                      <X aria-hidden="true" />
                    </button>
                  </div>
                )
              }}
            </For>
          </div>

          <div class="zai-terminal-actions" role="toolbar" aria-label="Terminal actions">
            <Show when={props.onSplitHorizontal}>
              <button
                type="button"
                aria-label="Split terminal horizontally"
                title="Split terminal horizontally"
                disabled={!activeTerminal()}
                onClick={() => props.onSplitHorizontal?.()}
              >
                <SquareSplitHorizontal aria-hidden="true" />
              </button>
            </Show>
            <Show when={props.onSplitVertical}>
              <button
                type="button"
                aria-label="Split terminal vertically"
                title="Split terminal vertically"
                disabled={!activeTerminal()}
                onClick={() => props.onSplitVertical?.()}
              >
                <SquareSplitVertical aria-hidden="true" />
              </button>
            </Show>
            <button
              type="button"
              aria-label="New terminal"
              title="New terminal"
              onClick={props.onNewTerminal}
            >
              <Plus aria-hidden="true" />
            </button>
            <Show when={props.onClear && activeTerminal()}>
              <button
                type="button"
                aria-label="Clear terminal"
                title="Clear terminal"
                onClick={() => {
                  const terminal = activeTerminal()
                  if (terminal) props.onClear?.(terminal.id)
                }}
              >
                <Eraser aria-hidden="true" />
              </button>
            </Show>
            <button
              type="button"
              aria-label="Close terminal"
              title="Close terminal"
              disabled={!activeTerminal()}
              onClick={() => {
                const terminal = activeTerminal()
                if (terminal) props.onCloseTerminal(terminal.id)
              }}
            >
              <Trash2 aria-hidden="true" />
            </button>
            <Show when={props.onClosePanel}>
              <button
                type="button"
                aria-label="Close terminal drawer"
                title="Close terminal drawer"
                onClick={() => props.onClosePanel?.()}
              >
                <X aria-hidden="true" />
              </button>
            </Show>
          </div>
        </header>

        <Show
          when={activeTerminal()}
          fallback={
            <div class="zai-bottom-terminal__empty">
              <TerminalSquare aria-hidden="true" />
              <span>No terminal sessions for this workspace yet.</span>
              <button type="button" onClick={props.onNewTerminal}>
                New terminal
              </button>
            </div>
          }
        >
          {(terminal) => (
            <section
              id={terminalPanelId(terminal().id)}
              class="zai-bottom-terminal__viewport"
              role="tabpanel"
              aria-labelledby={terminalTabId(terminal().id)}
            >
              <Show
                when={props.renderTerminal}
                fallback={
                  <pre role="log" aria-label={`${terminal().title} output`} tabIndex={0}>
                    <For each={terminal().lines ?? []}>{(line) => <>{line}{"\n"}</>}</For>
                    <Show when={(terminal().lines?.length ?? 0) === 0}>
                      <span class="zai-bottom-terminal__prompt">$ </span>
                    </Show>
                  </pre>
                }
              >
                {(render) => render()(terminal())}
              </Show>
            </section>
          )}
        </Show>
      </aside>
    </Show>
  )
}
