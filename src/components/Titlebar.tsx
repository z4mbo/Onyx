import { For, Show, type Component, type JSX } from "solid-js"
import { Icon } from "@opencode-ai/ui/v2/icon"
import { ZaiAppIcon } from "./ZaiAppIcon"

/** The presentation-only tab shape consumed by the zAI desktop titlebar. */
export interface TitlebarTab {
  /** Stable session or draft identifier. */
  id: string
  /** Human-readable title shown inside the tab pill. */
  label: string
  /** Whether this is the currently selected tab. */
  active: boolean
  /** Whether the tab's agent is producing a response. */
  running?: boolean
}

/**
 * Self-contained callbacks and state required by the zAI desktop titlebar.
 *
 * The component intentionally knows nothing about routing, Tauri commands, or
 * provider protocols. Its parent owns navigation and session lifecycle state.
 */
export interface TitlebarProps {
  /** Tabs in their visual order. */
  tabs: readonly TitlebarTab[]
  /** Select a tab by its stable identifier. */
  onSelect: (id: string) => void
  /** Close a tab by its stable identifier. */
  onClose: (id: string) => void
  /** Create a new draft/session tab. */
  onNew: () => void
  /** Open the project/session home view. */
  onHome: () => void
  /** Open zAI settings. */
  onOpenSettings: () => void
}

const CONTROL_RESET: JSX.CSSProperties = {
  "-webkit-app-region": "no-drag",
  appearance: "none",
  border: "0",
  margin: "0",
  padding: "0",
}

const MIDDLE_MOUSE_BUTTON = 1
const IS_MACOS = (() => {
  if (typeof navigator === "undefined") return false
  const navigatorWithPlatform = navigator as Navigator & {
    userAgentData?: { platform?: string }
  }
  const platform = navigatorWithPlatform.userAgentData?.platform || navigator.platform || navigator.userAgent
  return /mac|iphone|ipad/i.test(platform)
})()
const RUNNING_DOTS = Array.from({ length: 25 }, (_, index) => ({
  index,
  x: 1.5 + (index % 5) * 3,
  y: 1.5 + Math.floor(index / 5) * 3,
}))

function RunningIndicator() {
  return (
    <svg
      class="zai-titlebar__running-indicator"
      width="16"
      height="16"
      viewBox="0 0 16 16"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <For each={RUNNING_DOTS}>
        {(dot) => <rect data-dot={dot.index} x={dot.x} y={dot.y} width="2" height="2" fill="currentColor" />}
      </For>
    </svg>
  )
}

/** OpenCode-v2-proportioned desktop chrome, rebranded and decoupled for zAI. */
export const Titlebar: Component<TitlebarProps> = (props) => {
  const tabElements = new Map<string, HTMLButtonElement>()

  const closeTab = (event: MouseEvent, id: string) => {
    event.preventDefault()
    event.stopPropagation()
    props.onClose(id)
  }

  const moveTabFocus = (event: KeyboardEvent, id: string) => {
    const index = props.tabs.findIndex((tab) => tab.id === id)
    if (index < 0 || props.tabs.length === 0) return

    let nextIndex: number | undefined
    switch (event.key) {
      case "ArrowLeft":
        nextIndex = (index - 1 + props.tabs.length) % props.tabs.length
        break
      case "ArrowRight":
        nextIndex = (index + 1) % props.tabs.length
        break
      case "Home":
        nextIndex = 0
        break
      case "End":
        nextIndex = props.tabs.length - 1
        break
      default:
        return
    }

    event.preventDefault()
    const next = props.tabs[nextIndex]
    props.onSelect(next.id)
    queueMicrotask(() => {
      const element = tabElements.get(next.id)
      element?.focus()
      element?.scrollIntoView({ block: "nearest", inline: "nearest" })
    })
  }

  return (
    <header
      class="zai-titlebar"
      data-slot="zai-titlebar"
      data-platform={IS_MACOS ? "macos" : "other"}
      data-tauri-drag-region
      style={{
        "-webkit-app-region": "drag",
        "box-sizing": "border-box",
        display: "flex",
        height: "36px",
        "min-height": "36px",
        width: "100%",
        "padding-left": IS_MACOS ? "84px" : "0",
        overflow: "visible",
      }}
    >
      <div
        class="zai-titlebar__inner"
        data-tauri-drag-region
        style={{
          "box-sizing": "border-box",
          display: "flex",
          "align-items": "center",
          gap: "6px",
          height: "36px",
          width: "100%",
          padding: "8px 12px 0 8px",
          overflow: "hidden",
        }}
      >
        <button
          type="button"
          class="zai-titlebar__control zai-titlebar__home"
          style={{
            ...CONTROL_RESET,
            display: "inline-flex",
            "align-items": "center",
            "justify-content": "center",
            width: "36px",
            height: "28px",
            "min-width": "36px",
            "border-radius": "6px",
            background: "transparent",
          }}
          onClick={props.onHome}
          aria-label="Home"
          title="Home"
        >
          <ZaiAppIcon />
        </button>

        <div
          class="zai-titlebar__tabs"
          data-slot="zai-titlebar-tabs"
          style={{ position: "relative", "min-width": "0", "max-width": "100%", overflow: "hidden" }}
        >
          <div
            class="zai-titlebar__tabs-scroll"
            data-slot="zai-titlebar-tabs-scroll"
            role="tablist"
            aria-label="Open sessions"
            style={{
              "-webkit-app-region": "no-drag",
              display: "flex",
              "align-items": "center",
              "min-width": "0",
              "max-width": "100%",
              "overflow-x": "auto",
              "scrollbar-width": "none",
            }}
          >
            <For each={props.tabs}>
              {(tab) => (
                <div
                  class="zai-titlebar__tab-slot"
                  data-tab-id={tab.id}
                  data-active={tab.active ? "true" : "false"}
                  data-running={tab.running ? "true" : "false"}
                  style={{
                    display: "flex",
                    position: "relative",
                    flex: "0 1 224px",
                    width: "224px",
                    "min-width": "28px",
                    "max-width": "224px",
                    height: "28px",
                  }}
                  onMouseDown={(event) => {
                    if (event.button !== MIDDLE_MOUSE_BUTTON) return
                    event.preventDefault()
                    event.stopPropagation()
                  }}
                  onAuxClick={(event) => {
                    if (event.button !== MIDDLE_MOUSE_BUTTON) return
                    closeTab(event, tab.id)
                  }}
                >
                  <div
                    class="zai-titlebar__tab"
                    data-slot="zai-titlebar-tab-item"
                    data-active={tab.active ? "true" : "false"}
                    data-running={tab.running ? "true" : "false"}
                    style={{
                      display: "flex",
                      "align-items": "center",
                      gap: "6px",
                      width: "100%",
                      height: "28px",
                      "min-width": "0",
                      padding: "0 6px",
                      overflow: "hidden",
                      "border-radius": "6px",
                      "white-space": "nowrap",
                    }}
                  >
                    <button
                      ref={(element) => tabElements.set(tab.id, element)}
                      type="button"
                      class="zai-titlebar__tab-select"
                      role="tab"
                      aria-selected={tab.active}
                      aria-label={tab.running ? `${tab.label}, running` : tab.label}
                      tabIndex={tab.active ? 0 : -1}
                      style={{
                        ...CONTROL_RESET,
                        display: "flex",
                        "align-items": "center",
                        gap: "6px",
                        flex: "1",
                        height: "100%",
                        "min-width": "0",
                        background: "transparent",
                        "text-align": "left",
                      }}
                      onMouseDown={(event) => {
                        if (event.button !== 0) return
                        props.onSelect(tab.id)
                      }}
                      onClick={(event) => {
                        // Pointer navigation happens on mouse-down; detail 0 is keyboard activation.
                        if (event.detail !== 0) return
                        props.onSelect(tab.id)
                      }}
                      onKeyDown={(event) => moveTabFocus(event, tab.id)}
                    >
                      <span
                        class="zai-titlebar__tab-icon"
                        style={{
                          display: "inline-flex",
                          "align-items": "center",
                          "justify-content": "center",
                          width: "16px",
                          height: "16px",
                          "min-width": "16px",
                        }}
                      >
                        <Show when={tab.running} fallback={<Icon name="edit" />}>
                          <RunningIndicator />
                        </Show>
                      </span>
                      <span
                        class="zai-titlebar__tab-label"
                        style={{
                          flex: "1",
                          "min-width": "0",
                          overflow: "hidden",
                          "text-overflow": "clip",
                          "white-space": "nowrap",
                          "font-size": "13px",
                          "font-weight": "500",
                          "line-height": "16px",
                        }}
                      >
                        {tab.label}
                      </span>
                    </button>

                    <button
                      type="button"
                      class="zai-titlebar__tab-close"
                      data-slot="zai-titlebar-tab-close"
                      style={{
                        ...CONTROL_RESET,
                        display: "inline-flex",
                        "align-items": "center",
                        "justify-content": "center",
                        width: "20px",
                        height: "20px",
                        "min-width": "20px",
                        "border-radius": "4px",
                        background: "transparent",
                      }}
                      onPointerDown={(event) => {
                        event.preventDefault()
                        event.stopPropagation()
                      }}
                      onMouseDown={(event) => {
                        event.preventDefault()
                        event.stopPropagation()
                      }}
                      onClick={(event) => closeTab(event, tab.id)}
                      aria-label={`Close ${tab.label}`}
                      title={`Close ${tab.label}`}
                    >
                      <Icon name="xmark-small" />
                    </button>
                  </div>
                </div>
              )}
            </For>
          </div>
        </div>

        <button
          type="button"
          class="zai-titlebar__control zai-titlebar__new"
          style={{
            ...CONTROL_RESET,
            display: "inline-flex",
            "align-items": "center",
            "justify-content": "center",
            width: "28px",
            height: "28px",
            "min-width": "28px",
            "border-radius": "6px",
            background: "transparent",
          }}
          onClick={props.onNew}
          aria-label="New session"
          title="New session"
        >
          <Icon name="plus" />
        </button>

        <div class="zai-titlebar__drag-space" data-tauri-drag-region style={{ flex: "1", height: "100%" }} />

        <button
          type="button"
          class="zai-titlebar__control zai-titlebar__settings"
          style={{
            ...CONTROL_RESET,
            display: "inline-flex",
            "align-items": "center",
            "justify-content": "center",
            width: "28px",
            height: "28px",
            "min-width": "28px",
            "border-radius": "6px",
            background: "transparent",
          }}
          onClick={props.onOpenSettings}
          aria-label="Settings"
          title="Settings"
        >
          <Icon name="settings-gear" />
        </button>
      </div>
    </header>
  )
}
