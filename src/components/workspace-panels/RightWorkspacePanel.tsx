import { Plus, X } from "lucide-solid"
import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  type Component,
  type JSX,
} from "solid-js"
import { SurfaceIcon } from "./SurfaceIcon"
import {
  ALL_WORKSPACE_SURFACE_KINDS,
  DEFAULT_SURFACE_AVAILABILITY,
  WORKSPACE_SURFACE_COPY,
  type SurfaceRenderer,
  type WorkspaceSurface,
  type WorkspaceSurfaceAvailability,
  type WorkspaceSurfaceKind,
} from "./types"

export interface RightWorkspacePanelProps {
  open: boolean
  surfaces: readonly WorkspaceSurface[]
  activeSurfaceId: string | null
  availability?: Partial<WorkspaceSurfaceAvailability>
  unavailableReasons?: Partial<Record<WorkspaceSurfaceKind, string>>
  class?: string
  style?: JSX.CSSProperties
  renderSurface?: SurfaceRenderer
  onActivate: (surfaceId: string) => void
  onCloseSurface: (surfaceId: string) => void
  onAddSurface: (kind: WorkspaceSurfaceKind) => void
  onClosePanel?: () => void
}

const surfacePanelId = (id: string) => `zai-workspace-surface-${encodeURIComponent(id)}`
const surfaceTabId = (id: string) => `zai-workspace-surface-tab-${encodeURIComponent(id)}`

/**
 * T3 Code-proportioned inspector with independently controlled, multi-instance
 * Browser/Terminal/Files/Diff tabs. Arrow/Home/End use automatic tab activation;
 * Delete closes the focused tab and middle-click mirrors desktop tab behavior.
 */
export const RightWorkspacePanel: Component<RightWorkspacePanelProps> = (props) => {
  const [addMenuOpen, setAddMenuOpen] = createSignal(false)
  const [addMenuPosition, setAddMenuPosition] = createSignal({ top: 0, left: 0 })
  const tabElements = new Map<string, HTMLButtonElement>()
  const menuItemElements = new Map<WorkspaceSurfaceKind, HTMLButtonElement>()
  let addButton: HTMLButtonElement | undefined
  let menu: HTMLDivElement | undefined

  const availability = (): WorkspaceSurfaceAvailability => ({
    ...DEFAULT_SURFACE_AVAILABILITY,
    ...props.availability,
  })
  const activeSurface = createMemo(
    () => props.surfaces.find((surface) => surface.id === props.activeSurfaceId) ?? null,
  )

  const focusAndActivate = (index: number) => {
    const surface = props.surfaces[index]
    if (!surface) return
    props.onActivate(surface.id)
    queueMicrotask(() => {
      const element = tabElements.get(surface.id)
      element?.focus()
      element?.scrollIntoView({ block: "nearest", inline: "nearest" })
    })
  }

  const handleTabKeyDown = (event: KeyboardEvent, surface: WorkspaceSurface) => {
    const index = props.surfaces.findIndex((entry) => entry.id === surface.id)
    if (index < 0) return

    if (event.key === "Delete") {
      event.preventDefault()
      const focusAfterClose = Math.min(index, Math.max(props.surfaces.length - 2, 0))
      props.onCloseSurface(surface.id)
      queueMicrotask(() => {
        const nextSurface = props.surfaces[focusAfterClose]
        if (nextSurface) tabElements.get(nextSurface.id)?.focus()
      })
      return
    }

    let nextIndex: number | null = null
    if (event.key === "ArrowLeft") {
      nextIndex = (index - 1 + props.surfaces.length) % props.surfaces.length
    } else if (event.key === "ArrowRight") {
      nextIndex = (index + 1) % props.surfaces.length
    } else if (event.key === "Home") {
      nextIndex = 0
    } else if (event.key === "End") {
      nextIndex = props.surfaces.length - 1
    }
    if (nextIndex === null) return
    event.preventDefault()
    focusAndActivate(nextIndex)
  }

  const openAddMenu = () => {
    const bounds = addButton?.getBoundingClientRect()
    if (bounds) {
      const menuWidth = 176
      setAddMenuPosition({
        top: Math.min(bounds.bottom + 6, window.innerHeight - 132),
        left: Math.max(8, Math.min(bounds.left, window.innerWidth - menuWidth - 8)),
      })
    }
    setAddMenuOpen(true)
    queueMicrotask(() => {
      const firstAvailable = ALL_WORKSPACE_SURFACE_KINDS.find((kind) => availability()[kind])
      if (firstAvailable) menuItemElements.get(firstAvailable)?.focus()
    })
  }

  const closeAddMenu = (restoreFocus = false) => {
    setAddMenuOpen(false)
    if (restoreFocus) queueMicrotask(() => addButton?.focus())
  }

  const addSurface = (kind: WorkspaceSurfaceKind) => {
    if (!availability()[kind]) return
    closeAddMenu()
    props.onAddSurface(kind)
  }

  const handleMenuKeyDown = (event: KeyboardEvent, kind: WorkspaceSurfaceKind) => {
    const enabled = ALL_WORKSPACE_SURFACE_KINDS.filter((entry) => availability()[entry])
    const index = enabled.indexOf(kind)
    if (event.key === "Escape") {
      event.preventDefault()
      closeAddMenu(true)
      return
    }
    let nextIndex: number | null = null
    if (event.key === "ArrowDown") nextIndex = (index + 1) % enabled.length
    else if (event.key === "ArrowUp") nextIndex = (index - 1 + enabled.length) % enabled.length
    else if (event.key === "Home") nextIndex = 0
    else if (event.key === "End") nextIndex = enabled.length - 1
    if (nextIndex === null) return
    event.preventDefault()
    const nextKind = enabled[nextIndex]
    if (nextKind) menuItemElements.get(nextKind)?.focus()
  }

  onMount(() => {
    const handlePointerDown = (event: PointerEvent) => {
      if (!addMenuOpen()) return
      const target = event.target
      if (!(target instanceof Node)) return
      if (menu?.contains(target) || addButton?.contains(target)) return
      closeAddMenu()
    }
    const handleWindowKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && addMenuOpen()) closeAddMenu(true)
    }
    document.addEventListener("pointerdown", handlePointerDown)
    window.addEventListener("keydown", handleWindowKeyDown)
    onCleanup(() => {
      document.removeEventListener("pointerdown", handlePointerDown)
      window.removeEventListener("keydown", handleWindowKeyDown)
    })
  })

  createEffect(() => {
    const id = props.activeSurfaceId
    if (!id) return
    queueMicrotask(() => tabElements.get(id)?.scrollIntoView({ block: "nearest", inline: "nearest" }))
  })

  return (
    <Show when={props.open}>
      <aside
        class={`zai-right-workspace-panel${props.class ? ` ${props.class}` : ""}`}
        style={props.style}
        data-slot="right-workspace-panel"
        aria-label="Workspace tools"
      >
        <header class="zai-right-workspace-panel__tabbar">
          <div class="zai-right-workspace-panel__tabs" role="tablist" aria-label="Workspace tools">
            <For each={props.surfaces}>
              {(surface) => {
                const active = () => surface.id === props.activeSurfaceId
                return (
                  <div
                    class="zai-surface-tab"
                    data-active={active() ? "true" : "false"}
                    data-pending={surface.pending ? "true" : "false"}
                    onMouseDown={(event) => {
                      if (event.button === 1) event.preventDefault()
                    }}
                    onAuxClick={(event) => {
                      if (event.button !== 1) return
                      event.preventDefault()
                      props.onCloseSurface(surface.id)
                    }}
                  >
                    <button
                      ref={(element) => tabElements.set(surface.id, element)}
                      type="button"
                      id={surfaceTabId(surface.id)}
                      class="zai-surface-tab__select"
                      role="tab"
                      aria-controls={surfacePanelId(surface.id)}
                      aria-selected={active()}
                      tabIndex={active() ? 0 : -1}
                      title={surface.title}
                      onClick={() => props.onActivate(surface.id)}
                      onKeyDown={(event) => handleTabKeyDown(event, surface)}
                    >
                      <SurfaceIcon kind={surface.kind} class="zai-surface-icon" />
                      <span>{surface.title}</span>
                      <Show when={surface.dirty}>
                        <span class="zai-surface-tab__dirty" aria-label="Unsaved changes" />
                      </Show>
                    </button>
                    <button
                      type="button"
                      class="zai-surface-tab__close"
                      aria-label={`Close ${surface.title}`}
                      title={`Close ${surface.title}`}
                      onClick={() => props.onCloseSurface(surface.id)}
                    >
                      <Show
                        when={surface.pending}
                        fallback={<X class="zai-surface-tab__close-icon" aria-hidden="true" />}
                      >
                        <span class="zai-surface-tab__pending" aria-label="Loading" />
                        <X class="zai-surface-tab__close-icon" aria-hidden="true" />
                      </Show>
                    </button>
                  </div>
                )
              }}
            </For>

            <Show when={props.surfaces.length > 0}>
              <div class="zai-add-surface">
                <button
                  ref={addButton}
                  type="button"
                  class="zai-add-surface__button"
                  aria-label="Add panel surface"
                  aria-haspopup="menu"
                  aria-expanded={addMenuOpen()}
                  title="Add panel surface"
                  onClick={() => (addMenuOpen() ? closeAddMenu() : openAddMenu())}
                >
                  <Plus aria-hidden="true" />
                </button>
                <Show when={addMenuOpen()}>
                  <div
                    ref={menu}
                    class="zai-add-surface__menu"
                    role="menu"
                    aria-label="Add panel surface"
                    style={{
                      top: `${addMenuPosition().top}px`,
                      left: `${addMenuPosition().left}px`,
                    }}
                  >
                    <For each={ALL_WORKSPACE_SURFACE_KINDS}>
                      {(kind) => (
                        <button
                          ref={(element) => menuItemElements.set(kind, element)}
                          type="button"
                          role="menuitem"
                          disabled={!availability()[kind]}
                          title={
                            availability()[kind]
                              ? WORKSPACE_SURFACE_COPY[kind].label
                              : props.unavailableReasons?.[kind]
                          }
                          onClick={() => addSurface(kind)}
                          onKeyDown={(event) => handleMenuKeyDown(event, kind)}
                        >
                          <SurfaceIcon kind={kind} class="zai-surface-icon" />
                          <span>{WORKSPACE_SURFACE_COPY[kind].label}</span>
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            </Show>
          </div>
          <Show when={props.onClosePanel}>
            <button
              type="button"
              class="zai-right-workspace-panel__close"
              aria-label="Close right panel"
              title="Close right panel"
              onClick={() => props.onClosePanel?.()}
            >
              <X aria-hidden="true" />
            </button>
          </Show>
        </header>

        <Show
          when={activeSurface()}
          fallback={
            <div class="zai-surface-empty">
              <div class="zai-surface-empty__intro">
                <strong>Open a surface</strong>
                <span>Choose what to show in the right panel.</span>
              </div>
              <div class="zai-surface-empty__grid">
                <For each={ALL_WORKSPACE_SURFACE_KINDS}>
                  {(kind) => (
                    <button
                      type="button"
                      disabled={!availability()[kind]}
                      title={
                        availability()[kind]
                          ? WORKSPACE_SURFACE_COPY[kind].label
                          : props.unavailableReasons?.[kind]
                      }
                      onClick={() => props.onAddSurface(kind)}
                    >
                      <SurfaceIcon kind={kind} class="zai-surface-empty__icon" />
                      <strong>{WORKSPACE_SURFACE_COPY[kind].label}</strong>
                      <span>{WORKSPACE_SURFACE_COPY[kind].description}</span>
                    </button>
                  )}
                </For>
              </div>
            </div>
          }
        >
          {(surface) => (
            <section
              id={surfacePanelId(surface().id)}
              class="zai-right-workspace-panel__content"
              role="tabpanel"
              aria-labelledby={surfaceTabId(surface().id)}
              tabIndex={0}
            >
              <Show
                when={props.renderSurface}
                fallback={
                  <div class="zai-surface-placeholder">
                    <SurfaceIcon kind={surface().kind} class="zai-surface-placeholder__icon" />
                    <strong>{surface().title}</strong>
                    <span>Connect this surface to its native workspace renderer.</span>
                  </div>
                }
              >
                {(render) => render()(surface())}
              </Show>
            </section>
          )}
        </Show>
      </aside>
    </Show>
  )
}
