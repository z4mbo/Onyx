import {
  Box,
  ChevronDown,
  CloudUpload,
  ExternalLink,
  GitCommit,
  GitPullRequest,
  LoaderCircle,
} from "lucide-solid"
import {
  For,
  Match,
  Show,
  Switch,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  type Component,
  type JSX,
} from "solid-js"
import { PanelLayoutControls, type PanelLayoutControlsProps } from "./PanelLayoutControls"
import type { WorkspaceActionState } from "./types"

export type WorkspaceGitActionName = "commit" | "push" | "create-pr"

export interface WorkspaceOpenOption {
  id: string
  label: string
  available: boolean
}

interface TopbarActionProps {
  name: "open" | WorkspaceGitActionName
  state?: WorkspaceActionState
  defaultLabel: string
  onClick: () => void
  class?: string
  children: JSX.Element
}

const TopbarAction: Component<TopbarActionProps> = (props) => {
  const label = () => props.state?.label ?? props.defaultLabel
  const hint = () => props.state?.hint ?? label()

  return (
    <button
      type="button"
      class={`zai-workspace-action${props.class ? ` ${props.class}` : ""}`}
      data-action={props.name}
      data-busy={props.state?.busy ? "true" : "false"}
      aria-label={label()}
      aria-busy={props.state?.busy || undefined}
      title={hint()}
      disabled={props.state?.disabled || props.state?.busy}
      onClick={props.onClick}
    >
      <Show when={props.state?.busy} fallback={props.children}>
        <LoaderCircle class="zai-workspace-action__spinner" aria-hidden="true" />
      </Show>
      <span>{label()}</span>
    </button>
  )
}

const GitActionIcon: Component<{ action: WorkspaceGitActionName }> = (props) => (
  <Switch>
    <Match when={props.action === "commit"}><GitCommit aria-hidden="true" /></Match>
    <Match when={props.action === "push"}><CloudUpload aria-hidden="true" /></Match>
    <Match when={props.action === "create-pr"}><GitPullRequest aria-hidden="true" /></Match>
  </Switch>
)

export interface WorkspaceTopbarActionsProps extends PanelLayoutControlsProps {
  open?: WorkspaceActionState
  commit?: WorkspaceActionState
  push?: WorkspaceActionState
  createPr?: WorkspaceActionState
  openOptions?: readonly WorkspaceOpenOption[]
  preferredOpenTarget?: string
  primaryGitAction?: WorkspaceGitActionName
  onOpen: () => void
  onOpenTarget?: (target: string) => void
  onCommit: () => void
  onPush: () => void
  onCreatePr: () => void
  onGitMenuOpen?: () => void
}

/** T3 Code-style split Open/Git controls plus the paired panel toggles. */
export const WorkspaceTopbarActions: Component<WorkspaceTopbarActionsProps> = (props) => {
  const [openMenu, setOpenMenu] = createSignal(false)
  const [gitMenu, setGitMenu] = createSignal(false)
  let root: HTMLDivElement | undefined
  let openChevron: HTMLButtonElement | undefined
  let gitChevron: HTMLButtonElement | undefined
  const openMenuItems = new Map<string, HTMLButtonElement>()
  const gitMenuItems = new Map<WorkspaceGitActionName, HTMLButtonElement>()

  const stateFor = (action: WorkspaceGitActionName) => {
    if (action === "commit") return props.commit
    if (action === "push") return props.push
    return props.createPr
  }
  const labelFor = (action: WorkspaceGitActionName) => {
    if (action === "commit") return "Commit"
    if (action === "push") return "Push"
    return "Create PR"
  }
  const runGitAction = (action: WorkspaceGitActionName) => {
    setGitMenu(false)
    if (action === "commit") props.onCommit()
    else if (action === "push") props.onPush()
    else props.onCreatePr()
  }
  const primaryGitAction = createMemo<WorkspaceGitActionName>(() => {
    if (props.primaryGitAction) return props.primaryGitAction
    if (!props.commit?.disabled) return "commit"
    if (!props.push?.disabled) return "push"
    return "create-pr"
  })
  const availableOpenOptions = () => props.openOptions?.filter((option) => option.available) ?? []
  const availableGitActions = () => (["commit", "push", "create-pr"] as const)
    .filter((action) => !stateFor(action)?.disabled && !stateFor(action)?.busy)

  const focusMenuItem = <T extends string>(items: readonly T[], index: number, elements: Map<T, HTMLButtonElement>) => {
    const item = items[index]
    if (item) queueMicrotask(() => elements.get(item)?.focus())
  }

  const handleMenuKeyDown = <T extends string>(
    event: KeyboardEvent,
    current: T,
    items: readonly T[],
    elements: Map<T, HTMLButtonElement>,
    close: () => void,
  ) => {
    if (event.key === "Escape") {
      event.preventDefault()
      close()
      return
    }
    const index = items.indexOf(current)
    if (index < 0 || items.length === 0) return
    let next: number | null = null
    if (event.key === "ArrowDown") next = (index + 1) % items.length
    else if (event.key === "ArrowUp") next = (index - 1 + items.length) % items.length
    else if (event.key === "Home") next = 0
    else if (event.key === "End") next = items.length - 1
    if (next === null) return
    event.preventDefault()
    focusMenuItem(items, next, elements)
  }

  onMount(() => {
    const closeMenus = (event: PointerEvent) => {
      if (event.target instanceof Node && root?.contains(event.target)) return
      setOpenMenu(false)
      setGitMenu(false)
    }
    const escapeMenus = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return
      if (openMenu()) {
        setOpenMenu(false)
        queueMicrotask(() => openChevron?.focus())
      }
      if (gitMenu()) {
        setGitMenu(false)
        queueMicrotask(() => gitChevron?.focus())
      }
    }
    document.addEventListener("pointerdown", closeMenus)
    window.addEventListener("keydown", escapeMenus)
    onCleanup(() => {
      document.removeEventListener("pointerdown", closeMenus)
      window.removeEventListener("keydown", escapeMenus)
    })
  })

  return (
    <div
      ref={root}
      class="zai-workspace-topbar-actions"
      data-slot="workspace-topbar-actions"
      onFocusOut={() => queueMicrotask(() => {
        if (root?.contains(document.activeElement)) return
        setOpenMenu(false)
        setGitMenu(false)
      })}
    >
      <div class="zai-workspace-split-control" role="group" aria-label="Open workspace">
        <TopbarAction name="open" state={props.open} defaultLabel="Open" class="zai-workspace-split-control__primary" onClick={props.onOpen}>
          <Box aria-hidden="true" />
        </TopbarAction>
        <Show when={(props.openOptions?.length ?? 0) > 0}>
          <button
            ref={openChevron}
            type="button"
            class="zai-workspace-split-control__chevron"
            aria-label="Choose where to open"
            aria-haspopup="menu"
            aria-expanded={openMenu()}
            onClick={() => {
              setGitMenu(false)
              const next = !openMenu()
              setOpenMenu(next)
              if (next) {
                const first = availableOpenOptions()[0]
                if (first) focusMenuItem([first.id], 0, openMenuItems)
              }
            }}
          >
            <ChevronDown aria-hidden="true" />
          </button>
          <Show when={openMenu()}>
            <div class="zai-workspace-action-menu" role="menu" aria-label="Open workspace with">
              <For each={props.openOptions}>
                {(option) => (
                  <button
                    ref={(element) => openMenuItems.set(option.id, element)}
                    type="button"
                    role="menuitem"
                    disabled={!option.available}
                    data-selected={option.id === props.preferredOpenTarget ? "true" : "false"}
                    onClick={() => {
                      setOpenMenu(false)
                      props.onOpenTarget?.(option.id)
                    }}
                    onKeyDown={(event) => handleMenuKeyDown(
                      event,
                      option.id,
                      availableOpenOptions().map((item) => item.id),
                      openMenuItems,
                      () => {
                        setOpenMenu(false)
                        queueMicrotask(() => openChevron?.focus())
                      },
                    )}
                  >
                    <ExternalLink aria-hidden="true" />
                    <span>{option.label}</span>
                  </button>
                )}
              </For>
            </div>
          </Show>
        </Show>
      </div>

      <div class="zai-workspace-split-control" role="group" aria-label="Git actions">
        <TopbarAction
          name={primaryGitAction()}
          state={stateFor(primaryGitAction())}
          defaultLabel={labelFor(primaryGitAction())}
          class="zai-workspace-split-control__primary"
          onClick={() => runGitAction(primaryGitAction())}
        >
          <GitActionIcon action={primaryGitAction()} />
        </TopbarAction>
        <button
          ref={gitChevron}
          type="button"
          class="zai-workspace-split-control__chevron"
          aria-label="Git action options"
          aria-haspopup="menu"
          aria-expanded={gitMenu()}
          onClick={() => {
            setOpenMenu(false)
            const next = !gitMenu()
            setGitMenu(next)
            if (next) {
              props.onGitMenuOpen?.()
              focusMenuItem(availableGitActions(), 0, gitMenuItems)
            }
          }}
        >
          <ChevronDown aria-hidden="true" />
        </button>
        <Show when={gitMenu()}>
          <div class="zai-workspace-action-menu" role="menu" aria-label="Git actions">
            <For each={(["commit", "push", "create-pr"] as const)}>
              {(action) => (
                <button
                  ref={(element) => gitMenuItems.set(action, element)}
                  type="button"
                  role="menuitem"
                  disabled={stateFor(action)?.disabled || stateFor(action)?.busy}
                  title={stateFor(action)?.hint}
                  onClick={() => runGitAction(action)}
                  onKeyDown={(event) => handleMenuKeyDown(
                    event,
                    action,
                    availableGitActions(),
                    gitMenuItems,
                    () => {
                      setGitMenu(false)
                      queueMicrotask(() => gitChevron?.focus())
                    },
                  )}
                >
                  <GitActionIcon action={action} />
                  <span>{stateFor(action)?.label ?? labelFor(action)}</span>
                </button>
              )}
            </For>
          </div>
        </Show>
      </div>

      <PanelLayoutControls
        bottomPanelOpen={props.bottomPanelOpen}
        rightPanelOpen={props.rightPanelOpen}
        bottomPanelAvailable={props.bottomPanelAvailable}
        rightPanelAvailable={props.rightPanelAvailable}
        bottomPanelShortcut={props.bottomPanelShortcut}
        rightPanelShortcut={props.rightPanelShortcut}
        onToggleBottomPanel={props.onToggleBottomPanel}
        onToggleRightPanel={props.onToggleRightPanel}
      />
    </div>
  )
}
