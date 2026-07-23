import { openUrl } from "@tauri-apps/plugin-opener"
import {
  ArrowLeft,
  ArrowRight,
  ExternalLink,
  File,
  Folder,
  Globe2,
  MessageSquare,
  RefreshCw,
} from "lucide-solid"
import {
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  type Component,
} from "solid-js"
import { api } from "../../lib/api"
import {
  boundsForProviderSidebar,
  focusProviderSidebar,
  hideProviderSidebar,
  officialProviders,
  positionProviderSidebar,
  showProviderSidebar,
  type OfficialProviderId,
} from "../../lib/provider-sidebar"
import type { WorkspaceEntry, WorkspaceFile } from "../../lib/types"
import { ProviderBadge } from "../ProviderBadge"
import { normalizeBrowserUrl } from "./browser-url"
import { TerminalViewport } from "./TerminalViewport"
import type { WorkspaceSurface } from "./types"
import "./surface-views.css"

const browserState = new Map<string, { entries: string[]; index: number }>()
const selectedFiles = new Map<string, string>()
function displayPath(workspace: string, path: string) {
  const prefix = workspace.endsWith("/") ? workspace : `${workspace}/`
  return path.startsWith(prefix) ? path.slice(prefix.length) : path
}

const ChatSurface: Component<{ suspended?: boolean; onError?: (error: unknown) => void }> = (props) => {
  const stored = localStorage.getItem("onyx.official-provider")
  const initial = officialProviders.some((provider) => provider.id === stored)
    ? stored as OfficialProviderId
    : null
  const [selected, setSelected] = createSignal<OfficialProviderId | null>(initial)
  const [loading, setLoading] = createSignal(false)
  let host: HTMLDivElement | undefined
  let resizeObserver: ResizeObserver | undefined
  let frame = 0
  let disposed = false
  let previousSuspended = props.suspended ?? false

  const bounds = () => host ? boundsForProviderSidebar(host) : null
  const syncBounds = () => {
    window.cancelAnimationFrame(frame)
    frame = window.requestAnimationFrame(() => {
      const provider = selected()
      const next = bounds()
      if (provider && next && next.width > 1 && next.height > 1) {
        void positionProviderSidebar(provider, next).catch((error) => props.onError?.(error))
      }
    })
  }

  const open = async (provider: OfficialProviderId) => {
    setSelected(provider)
    localStorage.setItem("onyx.official-provider", provider)
    setLoading(true)
    try {
      await new Promise<void>((resolve) => window.requestAnimationFrame(() => resolve()))
      if (disposed || props.suspended) return
      const next = bounds()
      if (!next) throw new Error("The provider sidebar is not ready.")
      await showProviderSidebar(provider, next)
      await focusProviderSidebar(provider)
    } catch (error) {
      props.onError?.(error)
    } finally {
      setLoading(false)
    }
  }

  onMount(() => {
    if (host) {
      resizeObserver = new ResizeObserver(syncBounds)
      resizeObserver.observe(host)
    }
    window.addEventListener("resize", syncBounds)
    window.addEventListener("scroll", syncBounds, true)
    const visibility = () => {
      const provider = selected()
      if (document.hidden) void hideProviderSidebar(provider ?? undefined)
      else if (provider && !props.suspended) void open(provider)
    }
    document.addEventListener("visibilitychange", visibility)
    const provider = selected()
    if (provider && !props.suspended) void open(provider)
    onCleanup(() => {
      disposed = true
      window.cancelAnimationFrame(frame)
      resizeObserver?.disconnect()
      window.removeEventListener("resize", syncBounds)
      window.removeEventListener("scroll", syncBounds, true)
      document.removeEventListener("visibilitychange", visibility)
      void hideProviderSidebar(selected() ?? undefined).catch(() => undefined)
    })
  })

  createEffect(() => {
    const suspended = props.suspended ?? false
    if (suspended === previousSuspended) return
    previousSuspended = suspended
    const provider = selected()
    if (suspended) void hideProviderSidebar(provider ?? undefined)
    else if (provider && !disposed) void open(provider)
  })

  return (
    <div class="zai-provider-sidebar">
      <nav aria-label="Official chat provider">
        <For each={officialProviders}>
          {(provider) => (
            <button
              type="button"
              data-active={selected() === provider.id ? "true" : "false"}
              aria-pressed={selected() === provider.id}
              title={provider.name}
              onClick={() => void open(provider.id)}
            >
              <ProviderBadge brand={provider.brand} size="sm" />
              <span>{provider.name}</span>
            </button>
          )}
        </For>
      </nav>
      <div ref={host} class="zai-provider-sidebar__host">
        <Show
          when={selected()}
          fallback={
            <div class="zai-provider-sidebar__empty">
              <MessageSquare aria-hidden="true" />
              <strong>Choose an official chat</strong>
              <span>It will stay inside this sidebar and reuse its signed-in session.</span>
            </div>
          }
        >
          {(providerId) => {
            const provider = () => officialProviders.find((item) => item.id === providerId())
            return (
              <div class="zai-provider-sidebar__loading">
                <ProviderBadge brand={provider()!.brand} />
                <strong>{loading() ? `Opening ${provider()!.name}…` : provider()!.name}</strong>
                <span>{provider()!.detail}</span>
              </div>
            )
          }}
        </Show>
      </div>
      <footer>
        Official provider site · account and subscription stay with the provider
      </footer>
    </div>
  )
}

const BrowserSurface: Component<{ surface: WorkspaceSurface; onError?: (error: unknown) => void }> = (props) => {
  const initial = browserState.get(props.surface.id) ?? {
    entries: props.surface.resourceId ? [props.surface.resourceId] : [],
    index: props.surface.resourceId ? 0 : -1,
  }
  const [history, setHistory] = createSignal(initial.entries)
  const [historyIndex, setHistoryIndex] = createSignal(initial.index)
  const [input, setInput] = createSignal(initial.entries[initial.index] ?? "")
  let frame: HTMLIFrameElement | undefined
  const currentUrl = createMemo(() => history()[historyIndex()] ?? "")

  createEffect(() => {
    browserState.set(props.surface.id, { entries: history(), index: historyIndex() })
  })

  const navigate = (raw: string) => {
    try {
      const next = normalizeBrowserUrl(raw)
      const entries = history().slice(0, historyIndex() + 1)
      entries.push(next)
      setHistory(entries)
      setHistoryIndex(entries.length - 1)
      setInput(next)
    } catch (error) {
      props.onError?.(error)
    }
  }

  const move = (delta: number) => {
    const next = Math.max(0, Math.min(history().length - 1, historyIndex() + delta))
    if (next === historyIndex()) return
    setHistoryIndex(next)
    setInput(history()[next] ?? "")
  }

  const openExternal = async () => {
    try {
      if (!currentUrl()) return
      if (api.isTauri) await openUrl(currentUrl())
      else window.open(currentUrl(), "_blank", "noopener,noreferrer")
    } catch (error) {
      props.onError?.(error)
    }
  }

  return (
    <div class="zai-browser-surface">
      <form
        class="zai-browser-toolbar"
        onSubmit={(event) => {
          event.preventDefault()
          navigate(input())
        }}
      >
        <button type="button" aria-label="Back" title="Back" disabled={historyIndex() <= 0} onClick={() => move(-1)}>
          <ArrowLeft aria-hidden="true" />
        </button>
        <button
          type="button"
          aria-label="Forward"
          title="Forward"
          disabled={historyIndex() >= history().length - 1}
          onClick={() => move(1)}
        >
          <ArrowRight aria-hidden="true" />
        </button>
        <button
          type="button"
          aria-label="Reload"
          title="Reload"
          disabled={!currentUrl()}
          onClick={() => {
            if (frame) frame.src = currentUrl()
          }}
        >
          <RefreshCw aria-hidden="true" />
        </button>
        <label>
          <Globe2 aria-hidden="true" />
          <span class="sr-only">URL</span>
          <input
            value={input()}
            spellcheck={false}
            autocomplete="off"
            onInput={(event) => setInput(event.currentTarget.value)}
          />
        </label>
        <button
          type="button"
          aria-label="Open in default browser"
          title="Open in default browser"
          disabled={!currentUrl()}
          onClick={() => void openExternal()}
        >
          <ExternalLink aria-hidden="true" />
        </button>
      </form>
      <Show
        when={currentUrl()}
        fallback={
          <div class="zai-browser-empty">
            <Globe2 aria-hidden="true" />
            <strong>Open a preview</strong>
            <span>Enter a localhost or HTTPS URL above.</span>
          </div>
        }
      >
        <iframe
          ref={frame}
          src={currentUrl()}
          title={`Browser: ${currentUrl()}`}
          sandbox="allow-downloads allow-forms allow-modals allow-popups allow-same-origin allow-scripts"
          referrerpolicy="strict-origin-when-cross-origin"
        />
      </Show>
      <Show when={currentUrl()}>
        <div class="zai-browser-hint">
          Some sites block embedding; use the external-open icon when a page stays blank.
        </div>
      </Show>
    </div>
  )
}

const FilesSurface: Component<{ surface: WorkspaceSurface; workspace: string; onError?: (error: unknown) => void }> = (props) => {
  const [entries, setEntries] = createSignal<WorkspaceEntry[]>([])
  const [selected, setSelected] = createSignal(selectedFiles.get(props.surface.id) ?? "")
  const [file, setFile] = createSignal<WorkspaceFile | null>(null)
  const [loading, setLoading] = createSignal(false)

  const loadEntries = async () => {
    setLoading(true)
    try {
      setEntries(await api.workspaceEntries(props.workspace))
    } catch (error) {
      props.onError?.(error)
    } finally {
      setLoading(false)
    }
  }

  const openFile = async (path: string) => {
    setSelected(path)
    selectedFiles.set(props.surface.id, path)
    setLoading(true)
    try {
      setFile(await api.readWorkspaceFile(props.workspace, path))
    } catch (error) {
      setFile(null)
      props.onError?.(error)
    } finally {
      setLoading(false)
    }
  }

  onMount(() => {
    void loadEntries().then(() => {
      const path = selected()
      if (path) void openFile(path)
    })
  })

  return (
    <div class="zai-files-surface" aria-busy={loading()}>
      <aside>
        <header>
          <span>Files</span>
          <button type="button" aria-label="Refresh files" title="Refresh files" onClick={() => void loadEntries()}>
            <RefreshCw classList={{ spin: loading() }} aria-hidden="true" />
          </button>
        </header>
        <div class="zai-files-list">
          <For each={entries()} fallback={<div class="zai-surface-note">No files to show.</div>}>
            {(entry) => (
              <button
                type="button"
                data-selected={selected() === entry.path ? "true" : "false"}
                disabled={entry.isDirectory}
                title={entry.path}
                style={{ "padding-left": `${10 + Math.min(entry.depth, 8) * 14}px` }}
                onClick={() => void openFile(entry.path)}
              >
                <Show when={entry.isDirectory} fallback={<File aria-hidden="true" />}>
                  <Folder aria-hidden="true" />
                </Show>
                <span>{entry.name}</span>
              </button>
            )}
          </For>
        </div>
      </aside>
      <section class="zai-file-preview">
        <Show
          when={file()}
          fallback={<div class="zai-surface-placeholder"><File aria-hidden="true" /><span>Select a text file to preview it.</span></div>}
        >
          {(value) => (
            <>
              <header title={value().path}>{displayPath(props.workspace, value().path)}</header>
              <pre><code>{value().content}</code></pre>
              <Show when={value().truncated}><div class="zai-surface-note">Preview truncated at the safe read limit.</div></Show>
            </>
          )}
        </Show>
      </section>
    </div>
  )
}

const DiffSurface: Component<{ workspace: string; onError?: (error: unknown) => void }> = (props) => {
  const [diff, setDiff] = createSignal("")
  const [loading, setLoading] = createSignal(false)
  const load = async () => {
    setLoading(true)
    try {
      setDiff(await api.gitDiff(props.workspace))
    } catch (error) {
      setDiff("")
      props.onError?.(error)
    } finally {
      setLoading(false)
    }
  }
  onMount(() => void load())

  const lineClass = (line: string) => {
    if (line.startsWith("+") && !line.startsWith("+++")) return "addition"
    if (line.startsWith("-") && !line.startsWith("---")) return "deletion"
    if (line.startsWith("@@")) return "hunk"
    if (line.startsWith("diff --git")) return "heading"
    return ""
  }

  return (
    <div class="zai-diff-surface" aria-busy={loading()}>
      <header>
        <span>Working tree diff</span>
        <button type="button" aria-label="Refresh diff" title="Refresh diff" onClick={() => void load()}>
          <RefreshCw classList={{ spin: loading() }} aria-hidden="true" />
        </button>
      </header>
      <Show
        when={diff()}
        fallback={<div class="zai-surface-placeholder"><span>{loading() ? "Loading diff…" : "No working tree changes."}</span></div>}
      >
        <pre><code><For each={diff().split("\n")}>{(line) => <span class={lineClass(line)}>{line}{"\n"}</span>}</For></code></pre>
      </Show>
    </div>
  )
}

export interface WorkspaceSurfaceViewProps {
  surface: WorkspaceSurface
  workspace: string
  suspended?: boolean
  onError?: (error: unknown) => void
}

/** Functional renderer for each tab kind accepted by RightWorkspacePanel. */
export const WorkspaceSurfaceView: Component<WorkspaceSurfaceViewProps> = (props) => (
  <Switch>
    <Match when={props.surface.kind === "chat"}>
      <ChatSurface suspended={props.suspended} onError={props.onError} />
    </Match>
    <Match when={props.surface.kind === "browser"}>
      <BrowserSurface surface={props.surface} onError={props.onError} />
    </Match>
    <Match when={props.surface.kind === "terminal" && props.surface.resourceId}>
      <TerminalViewport sessionId={props.surface.resourceId!} autofocus />
    </Match>
    <Match when={props.surface.kind === "files"}>
      <FilesSurface surface={props.surface} workspace={props.workspace} onError={props.onError} />
    </Match>
    <Match when={props.surface.kind === "diff"}>
      <DiffSurface workspace={props.workspace} onError={props.onError} />
    </Match>
    <Match when={true}>
      <div class="zai-surface-placeholder"><span>Unable to open this workspace surface.</span></div>
    </Match>
  </Switch>
)
