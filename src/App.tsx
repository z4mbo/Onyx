import {
  createEffect,
  createMemo,
  createSignal,
  Match,
  onCleanup,
  onMount,
  Show,
  Switch,
  type Component,
} from "solid-js"
import { Gauge, GitBranch, TimerReset } from "lucide-solid"
import { openUrl } from "@tauri-apps/plugin-opener"
import { api } from "./lib/api"
import {
  accountSnapshot,
  initializeAccount,
  signOut,
  subscribeAccount,
} from "./lib/account"
import {
  brandForRuntime,
  fallbackModels,
  modelsForBrand,
  runtimeForBrand,
  workspaceName,
} from "./lib/providers"
import { deriveWorkspaceGitActions } from "./lib/workspace-actions"
import {
  applySessionEvent,
  mergeCommandSession,
  replaceSession,
  sessionEventId,
  sortSessions,
} from "./lib/session-state"
import type {
  AgentSession,
  ApprovalRequest,
  EditorTarget,
  OpenRouterModel,
  OpenRouterStatus,
  OpenAiStatus,
  ProviderBrand,
  ProviderId,
  ProviderModelOption,
  ProviderStatus,
  ProviderUsage,
  ReasoningEffort,
  RepoSummary,
  SessionEvent,
} from "./lib/types"
import { Composer } from "./components/Composer"
import { ChatView } from "./components/ChatView"
import { HomeView } from "./components/HomeView"
import { SettingsDialog, type ColorScheme } from "./components/SettingsDialog"
import { Titlebar, type TitlebarTab } from "./components/Titlebar"
import { Transcript } from "./components/Transcript"
import { VoiceHistoryView } from "./components/VoiceHistoryView"
import { ZaiWordmark } from "./components/ZaiWordmark"
import {
  BottomTerminalPanel,
  GitCommitDialog,
  RightWorkspacePanel,
  TerminalViewport,
  WorkspaceSurfaceView,
  WorkspaceTopbarActions,
  clearTerminalViewport,
  forgetTerminalViewport,
  startTerminalViewportBridge,
  type WorkspaceGitActionName,
  type WorkspaceSurface,
  type WorkspaceSurfaceKind,
  type WorkspaceTerminal,
} from "./components/workspace-panels"

type Page = "home" | "draft" | "session" | "chat" | "voice"
const DRAFT_TAB_ID = "onyx:draft"
const LAST_WORKSPACE_KEY = "onyx.last-workspace"
const PREFERRED_EDITOR_KEY = "onyx.preferred-editor"
const DESKTOP_PREFERENCES_KEY = "onyx.desktop-preferences.v1"

function compactTokens(tokens: number) {
  if (tokens >= 1_000_000) return `${Number((tokens / 1_000_000).toFixed(1))}M`
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}K`
  return String(tokens)
}

function selectedWslDistribution(): string | null {
  try {
    const preferences = JSON.parse(localStorage.getItem(DESKTOP_PREFERENCES_KEY) ?? "null") as { wslMode?: string; wslDistribution?: string } | null
    if (preferences?.wslMode === "default") return ""
    if (preferences?.wslMode === "distribution" && preferences.wslDistribution?.trim()) return preferences.wslDistribution.trim()
  } catch { /* native terminal is the safe fallback */ }
  return null
}

interface SessionWorkspaceUi {
  rightPanelOpen: boolean
  bottomPanelOpen: boolean
  surfaces: WorkspaceSurface[]
  activeSurfaceId: string | null
  terminals: WorkspaceTerminal[]
  activeTerminalId: string | null
  terminalHeight: number
}

const emptyWorkspaceUi = (): SessionWorkspaceUi => ({
  rightPanelOpen: false,
  bottomPanelOpen: false,
  surfaces: [],
  activeSurfaceId: null,
  terminals: [],
  activeTerminalId: null,
  terminalHeight: 280,
})

function storedColorScheme(): ColorScheme {
  const value = localStorage.getItem("onyx.color-scheme") ?? localStorage.getItem("zai.color-scheme")
  return value === "light" || value === "dark" ? value : "system"
}

function storedWorkspace(): string {
  return localStorage.getItem(LAST_WORKSPACE_KEY)?.trim() ?? ""
}

const App: Component = () => {
  const [providers, setProviders] = createSignal<ProviderStatus[]>([])
  const [sessions, setSessions] = createSignal<AgentSession[]>([])
  const [page, setPage] = createSignal<Page>("draft")
  const [currentId, setCurrentId] = createSignal<string | null>(null)
  const [draftOpen, setDraftOpen] = createSignal(true)
  const [openSessionIds, setOpenSessionIds] = createSignal<string[]>([])
  const [newProvider, setNewProvider] = createSignal<ProviderId>("claude")
  const [newBrand, setNewBrand] = createSignal<ProviderBrand>("anthropic")
  const [newModel, setNewModel] = createSignal("fable")
  const [newReasoning, setNewReasoning] = createSignal<ReasoningEffort | null>("medium")
  const [newSpeedMode, setNewSpeedMode] = createSignal<"standard" | "fast">("standard")
  const [newInteractionMode, setNewInteractionMode] = createSignal<"build" | "plan">("build")
  const [newAccessMode, setNewAccessMode] = createSignal<"approval_required" | "auto_accept_edits" | "full_access">("auto_accept_edits")
  const [providerModels, setProviderModels] = createSignal<Partial<Record<ProviderId, ProviderModelOption[]>>>({ ...fallbackModels })
  const [currentProviderUsage, setCurrentProviderUsage] = createSignal<ProviderUsage | null>(null)
  const [draftProviderUsage, setDraftProviderUsage] = createSignal<ProviderUsage | null>(null)
  const [newWorkspace, setNewWorkspace] = createSignal(storedWorkspace())
  const [settingsOpen, setSettingsOpen] = createSignal(false)
  const [openRouter, setOpenRouter] = createSignal<OpenRouterStatus>({ connected: false })
  const [openAi, setOpenAi] = createSignal<OpenAiStatus>({ connected: false })
  const [openRouterModels, setOpenRouterModels] = createSignal<OpenRouterModel[]>([])
  const [approvals, setApprovals] = createSignal<ApprovalRequest[]>([])
  const [approvalBusy, setApprovalBusy] = createSignal(false)
  const [notice, setNotice] = createSignal<string | null>(null)
  const [noticeKind, setNoticeKind] = createSignal<"error" | "success">("error")
  const [colorScheme, setColorScheme] = createSignal<ColorScheme>(storedColorScheme())
  const [workspaceUiBySession, setWorkspaceUiBySession] = createSignal<Record<string, SessionWorkspaceUi>>({})
  const [repoSummary, setRepoSummary] = createSignal<RepoSummary | null>(null)
  const [repoLoading, setRepoLoading] = createSignal(false)
  const [editors, setEditors] = createSignal<EditorTarget[]>([])
  const [preferredEditor, setPreferredEditor] = createSignal(localStorage.getItem(PREFERRED_EDITOR_KEY) ?? "")
  const [gitAction, setGitAction] = createSignal<WorkspaceGitActionName | null>(null)
  const [commitOpen, setCommitOpen] = createSignal(false)
  const [draftGitReady, setDraftGitReady] = createSignal(false)
  const [account, setAccount] = createSignal(accountSnapshot())

  const current = createMemo(() => sessions().find((session) => session.id === currentId()) ?? null)
  const draftSelectedModel = createMemo(() =>
    modelsForBrand(newBrand(), providerModels(), openRouterModels()).find((model) => model.id === newModel()),
  )
  const activeApproval = createMemo(() => approvals().find((request) => request.sessionId === currentId()) ?? null)
  const activeWorkspaceUi = createMemo(() => {
    const id = currentId()
    return id ? (workspaceUiBySession()[id] ?? emptyWorkspaceUi()) : emptyWorkspaceUi()
  })
  const tombstones = new Set<string>()
  const pendingEvents = new Map<string, SessionEvent[]>()
  let eventsReady = false
  let queuedEvents: SessionEvent[] = []
  let lastCloudSessionVersion = ""

  const unsubscribeAccount = subscribeAccount(setAccount)
  void initializeAccount()
  onCleanup(unsubscribeAccount)

  const applyTheme = () => {
    const scheme = colorScheme()
    const resolved = scheme === "system"
      ? (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light")
      : scheme
    document.documentElement.dataset.colorScheme = resolved
    document.documentElement.dataset.theme = "oc-2"
    document.body.dataset.newLayout = ""
  }

  createEffect(() => {
    const scheme = colorScheme()
    applyTheme()
    void api.setWindowTheme(scheme === "system" ? null : scheme).catch(() => undefined)
  })

  createEffect(() => {
    const version = sessions().map((session) => `${session.id}:${session.updatedAt}:${session.status}`).join("|")
    if (!version || version === lastCloudSessionVersion) return
    lastCloudSessionVersion = version
    window.dispatchEvent(new Event("onyx:cloud-data-changed"))
  })

  createEffect(() => {
    const workspace = newWorkspace().trim()
    if (workspace) localStorage.setItem(LAST_WORKSPACE_KEY, workspace)
  })

  const changeColorScheme = (scheme: ColorScheme) => {
    setColorScheme(scheme)
    localStorage.setItem("onyx.color-scheme", scheme)
  }

  const updateWorkspaceUi = (id: string, update: (value: SessionWorkspaceUi) => SessionWorkspaceUi) => {
    setWorkspaceUiBySession((items) => ({
      ...items,
      [id]: update(items[id] ?? emptyWorkspaceUi()),
    }))
  }

  const updateActiveWorkspaceUi = (update: (value: SessionWorkspaceUi) => SessionWorkspaceUi) => {
    const id = currentId()
    if (id) updateWorkspaceUi(id, update)
  }

  const mergeReturnedSession = (session: AgentSession) => {
    if (tombstones.has(session.id)) return
    setSessions((items) => mergeCommandSession(items, session))
  }

  const consumeSessionEvent = (event: SessionEvent) => {
    const id = sessionEventId(event)
    if (event.type === "removed") {
      tombstones.add(id)
      pendingEvents.delete(id)
      disposeWorkspaceUi(id)
      setSessions((items) => applySessionEvent(items, event))
      setApprovals((items) => items.filter((request) => request.sessionId !== id))
      setOpenSessionIds((items) => items.filter((item) => item !== id))
      if (currentId() === id) {
        setCurrentId(null)
        setPage(draftOpen() ? "draft" : "home")
      }
      return
    }
    if (tombstones.has(id)) return

    if (event.type !== "snapshot" && !sessions().some((session) => session.id === id)) {
      pendingEvents.set(id, [...(pendingEvents.get(id) ?? []), event])
      return
    }

    setSessions((items) => applySessionEvent(items, event))
    if (event.type === "snapshot") {
      const pending = pendingEvents.get(id) ?? []
      pendingEvents.delete(id)
      for (const buffered of pending) setSessions((items) => applySessionEvent(items, buffered))
      if (event.session.status !== "running" && event.session.status !== "waiting_approval") {
        setApprovals((items) => items.filter((request) => request.sessionId !== id))
      }
    }
  }

  const handleSessionEvent = (event: SessionEvent) => {
    if (!eventsReady) {
      queuedEvents.push(event)
      return
    }
    consumeSessionEvent(event)
  }

  const selectNewProvider = (provider: ProviderId) => {
    setNewProvider(provider)
    setNewBrand(brandForRuntime(provider))
    const models = modelsForBrand(brandForRuntime(provider), providerModels(), openRouterModels())
    const selected = models.find((model) => model.isDefault) ?? models[0]
    setNewModel(selected?.id ?? "")
    setNewReasoning(selected?.defaultReasoning ?? selected?.reasoning[0] ?? null)
    setNewSpeedMode(selected?.defaultSpeed ?? "standard")
  }

  const selectNewBrand = (brand: ProviderBrand) => {
    setNewBrand(brand)
    const provider = runtimeForBrand(brand)
    setNewProvider(provider)
    const models = modelsForBrand(brand, providerModels(), openRouterModels())
    const selected = models.find((model) => model.isDefault) ?? models[0]
    setNewModel(selected?.id ?? "")
    setNewReasoning(selected?.defaultReasoning ?? selected?.reasoning[0] ?? null)
    setNewSpeedMode(selected?.defaultSpeed ?? "standard")
  }

  const showNotice = (message: string, kind: "error" | "success") => {
    setNoticeKind(kind)
    setNotice(message)
    window.setTimeout(() => setNotice(null), 6500)
  }

  const showError = (error: unknown) => {
    showNotice(error instanceof Error ? error.message : String(error), "error")
  }

  const load = async () => {
    const [providerList, sessionList, routerStatus, openAiStatus, editorList] = await Promise.all([
      api.listProviders(), api.listSessions(), api.openRouterStatus(), api.openAiStatus(), api.workspaceEditors(),
    ])
    setProviders(providerList)
    setSessions(sortSessions(sessionList))
    setOpenRouter(routerStatus)
    setOpenAi(openAiStatus)
    setEditors(editorList)
    if (!editorList.some((editor) => editor.id === preferredEditor() && editor.available)) {
      const available = editorList.find((editor) => editor.available)
      if (available) setPreferredEditor(available.id)
    }
    if (routerStatus.connected) {
      void api.openRouterModels()
        .then((models) => {
          setOpenRouterModels(models)
          if (newProvider() === "openrouter" && !newModel()) setNewModel(models[0]?.id ?? "")
        })
        .catch(() => setOpenRouterModels([]))
    }
    const codex = providerList.find((provider) => provider.id === "codex" && provider.available)
    if (codex) {
      void api.providerModels("codex").then((models) => {
        if (models.length) setProviderModels((catalogs) => ({ ...catalogs, codex: models }))
      }).catch(() => undefined)
    }
    const selected = providerList.find((provider) => provider.id === newProvider() && provider.available)
    const available = selected ?? providerList.find((provider) => provider.available)
    if (available) selectNewProvider(available.id)
  }

  onMount(() => {
    let disposed = false
    let unlisten: () => void = () => undefined

    void (async () => {
      try {
        const disposeEvents = await api.listen(
          handleSessionEvent,
          (request) => setApprovals((items) =>
            items.some((item) => item.id === request.id) ? items : [...items, request],
          ),
        )
        let disposeTerminal: () => void
        let disposeTerminalViewport: () => void = () => undefined
        try {
          disposeTerminalViewport = await startTerminalViewportBridge()
          disposeTerminal = await api.listenTerminal((event) => {
            if (event.kind !== "exit" && event.kind !== "error") return
            setWorkspaceUiBySession((items) => Object.fromEntries(
              Object.entries(items).map(([id, ui]) => [id, {
                ...ui,
                terminals: ui.terminals.map((terminal) =>
                  terminal.id === event.sessionId ? { ...terminal, status: "exited" as const } : terminal,
                ),
              }]),
            ))
          })
        } catch (error) {
          disposeTerminalViewport()
          disposeEvents()
          throw error
        }
        if (disposed) {
          disposeEvents()
          disposeTerminal()
          disposeTerminalViewport()
          return
        }
        unlisten = () => {
          disposeTerminal()
          disposeTerminalViewport()
          disposeEvents()
        }
        await load()
      } catch (error) {
        showError(error)
      } finally {
        eventsReady = true
        const replay = queuedEvents
        queuedEvents = []
        replay.forEach(consumeSessionEvent)
      }
    })()

    const newSessionKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === ",") {
        event.preventDefault()
        if (commitOpen()) return
        setSettingsOpen(true)
        return
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "j") {
        event.preventDefault()
        if (settingsOpen() || commitOpen() || page() !== "session") return
        if (event.shiftKey) toggleRightPanel()
        else void toggleBottomPanel()
        return
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "n") {
        event.preventDefault()
        if (settingsOpen() || commitOpen()) return
        openDraft()
        return
      }
      if (event.key === "Escape" && page() === "session" && current()?.status === "running") {
        event.preventDefault()
        void cancel()
      }
    }
    const media = window.matchMedia("(prefers-color-scheme: dark)")
    const mediaChange = () => colorScheme() === "system" && applyTheme()
    window.addEventListener("keydown", newSessionKey)
    media.addEventListener("change", mediaChange)
    onCleanup(() => {
      disposed = true
      unlisten()
      Object.keys(workspaceUiBySession()).forEach(disposeWorkspaceUi)
      window.removeEventListener("keydown", newSessionKey)
      media.removeEventListener("change", mediaChange)
    })
  })

  const openDraft = (workspace?: string) => {
    if (workspace) setNewWorkspace(workspace)
    setDraftOpen(true)
    setCurrentId(null)
    setPage("draft")
  }

  const openSession = (id: string) => {
    if (!sessions().some((session) => session.id === id)) return
    setOpenSessionIds((items) => items.includes(id) ? items : [...items, id])
    setCurrentId(id)
    setPage("session")
  }

  const disposeWorkspaceUi = (id: string) => {
    const ui = workspaceUiBySession()[id]
    if (!ui) return
    const terminalIds = new Set([
      ...ui.terminals.map((terminal) => terminal.id),
      ...ui.surfaces
        .filter((surface) => surface.kind === "terminal" && surface.resourceId)
        .map((surface) => surface.resourceId!),
    ])
    terminalIds.forEach((terminalId) => {
      forgetTerminalViewport(terminalId)
      void api.terminalClose(terminalId).catch(() => undefined)
    })
    setWorkspaceUiBySession((items) => {
      const next = { ...items }
      delete next[id]
      return next
    })
  }

  const closeTab = (id: string) => {
    if (id === DRAFT_TAB_ID) {
      setDraftOpen(false)
      if (page() !== "draft") return
      const fallback = openSessionIds().at(-1)
      if (fallback) openSession(fallback)
      else setPage("home")
      return
    }
    disposeWorkspaceUi(id)
    setOpenSessionIds((items) => items.filter((item) => item !== id))
    if (currentId() !== id || page() !== "session") return
    const fallback = openSessionIds().filter((item) => item !== id).at(-1)
    if (fallback) openSession(fallback)
    else if (draftOpen()) openDraft()
    else {
      setCurrentId(null)
      setPage("home")
    }
  }

  const removeSession = async (id: string) => {
    const session = sessions().find((item) => item.id === id)
    if (!session) return
    const warning = session.status === "running" || session.status === "waiting_approval"
      ? " This also stops its running agent."
      : ""
    if (!window.confirm(`Delete “${session.title}”? This removes its local conversation history.${warning}`)) return
    disposeWorkspaceUi(id)
    try {
      await api.deleteSession(id)
    } catch (error) {
      showError(error)
    }
  }

  const tabs = createMemo<TitlebarTab[]>(() => {
    const result: TitlebarTab[] = []
    if (draftOpen()) result.push({ id: DRAFT_TAB_ID, label: "New session", active: page() === "draft", projectInitial: newWorkspace() ? workspaceName(newWorkspace()).slice(0, 1).toUpperCase() : "+" })
    for (const id of openSessionIds()) {
      const session = sessions().find((item) => item.id === id)
      if (!session) continue
      result.push({
        id,
        label: session.title || "Untitled session",
        active: page() === "session" && currentId() === id,
        running: session.status === "running" || session.status === "waiting_approval",
        projectInitial: workspaceName(session.workspace).slice(0, 1).toUpperCase(),
      })
    }
    return result
  })

  const selectTab = (id: string) => id === DRAFT_TAB_ID ? openDraft() : openSession(id)

  const chooseDraftWorkspace = async () => {
    try {
      const workspace = await api.chooseWorkspace()
      if (workspace) { setNewWorkspace(workspace); setDraftGitReady(false) }
    } catch (error) {
      showError(error)
    }
  }

  const initializeDraftGit = async () => {
    if (!newWorkspace()) return
    try {
      const result = await api.initGit(newWorkspace())
      setDraftGitReady(true)
      showNotice(result.message, "success")
    } catch (error) { showError(error) }
  }

  const refreshProviders = async () => {
    try {
      const result = await api.listProviders()
      setProviders(result)
      const selected = result.find((provider) => provider.id === newProvider())
      if (!selected?.available) {
        const available = result.find((provider) => provider.available)
        if (available) selectNewProvider(available.id)
      }
    } catch (error) {
      showError(error)
    }
  }

  const startSession = async (content: string) => {
    try {
      let workspace = newWorkspace()
      if (!workspace) {
        workspace = (await api.chooseWorkspace()) ?? ""
        if (!workspace) throw new Error("Choose a workspace to start a session.")
        setNewWorkspace(workspace)
      }
      if (newProvider() === "openrouter" && !newModel()) throw new Error("Choose an OpenRouter model first.")
      const session = await api.createSession({
        provider: newProvider(),
        providerBrand: newBrand(),
        model: newModel() || null,
        reasoning: newReasoning(),
        speedMode: newSpeedMode(),
        interactionMode: newInteractionMode(),
        accessMode: newAccessMode(),
        workspace,
      })
      tombstones.delete(session.id)
      setSessions((items) => replaceSession(items, session))
      setDraftOpen(false)
      setOpenSessionIds((items) => items.includes(session.id) ? items : [...items, session.id])
      setCurrentId(session.id)
      setPage("session")
      mergeReturnedSession(await api.sendMessage(session.id, content))
    } catch (error) {
      showError(error)
      throw error
    }
  }

  const continueSession = async (content: string) => {
    const session = current()
    if (!session) return
    try {
      mergeReturnedSession(await api.sendMessage(session.id, content))
    } catch (error) {
      showError(error)
      throw error
    }
  }

  const updateCurrentOptions = async (patch: Partial<Pick<AgentSession, "provider" | "providerBrand" | "model" | "reasoning" | "speedMode" | "interactionMode" | "accessMode">>) => {
    const session = current()
    if (!session) return
    try {
      mergeReturnedSession(await api.updateSessionOptions({
        id: session.id,
        provider: patch.provider ?? session.provider,
        providerBrand: patch.providerBrand ?? session.providerBrand,
        model: patch.model === undefined ? session.model : patch.model,
        reasoning: patch.reasoning === undefined ? session.reasoning : patch.reasoning,
        speedMode: patch.speedMode ?? session.speedMode,
        interactionMode: patch.interactionMode ?? session.interactionMode,
        accessMode: patch.accessMode ?? session.accessMode,
      }))
    } catch (error) { showError(error) }
  }

  const cancel = async () => {
    const session = current()
    if (!session) return
    try { await api.cancelTurn(session.id) } catch (error) { showError(error) }
  }

  const respondApproval = async (allow: boolean) => {
    const request = activeApproval()
    if (!request) return
    setApprovalBusy(true)
    try {
      await api.respondApproval(request.id, allow)
      setApprovals((items) => items.filter((item) => item.id !== request.id))
    } catch (error) {
      showError(error)
      throw error
    } finally {
      setApprovalBusy(false)
    }
  }

  let repoRequest = 0
  const refreshRepo = async (background = false) => {
    const session = current()
    if (!session) {
      setRepoSummary(null)
      return
    }
    const request = ++repoRequest
    if (!background) setRepoLoading(true)
    try {
      const summary = await api.repoSummary(session.workspace)
      if (request === repoRequest && currentId() === session.id) setRepoSummary(summary)
    } catch (error) {
      if (request === repoRequest && currentId() === session.id) {
        if (!background) {
          setRepoSummary(null)
          showError(error)
        }
      }
    } finally {
      if (!background) setRepoLoading(false)
    }
  }

  createEffect(() => {
    const session = current()
    if (!session) {
      setRepoSummary(null)
      return
    }
    const status = session.status
    session.updatedAt
    if (status === "running" || status === "waiting_approval") return
    const timer = window.setTimeout(() => void refreshRepo(), 220)
    onCleanup(() => window.clearTimeout(timer))
  })

  createEffect(() => {
    const provider = newProvider()
    let disposed = false
    void api.providerUsage(provider)
      .then((usage) => { if (!disposed && newProvider() === provider) setDraftProviderUsage(usage) })
      .catch(() => { if (!disposed && newProvider() === provider) setDraftProviderUsage(null) })
    onCleanup(() => { disposed = true })
  })

  createEffect(() => {
    const session = current()
    if (!session) {
      setCurrentProviderUsage(null)
      return
    }
    let disposed = false
    const refresh = () => void api.providerUsage(session.provider)
      .then((usage) => { if (!disposed && currentId() === session.id) setCurrentProviderUsage(usage) })
      .catch(() => { if (!disposed && currentId() === session.id) setCurrentProviderUsage(null) })
    refresh()
    const timer = window.setInterval(refresh, 60_000)
    onCleanup(() => {
      disposed = true
      window.clearInterval(timer)
    })
  })

  onMount(() => {
    const refreshAfterExternalWork = () => {
      if (page() === "session" && document.visibilityState === "visible") void refreshRepo(true)
    }
    window.addEventListener("focus", refreshAfterExternalWork)
    document.addEventListener("visibilitychange", refreshAfterExternalWork)
    onCleanup(() => {
      window.removeEventListener("focus", refreshAfterExternalWork)
      document.removeEventListener("visibilitychange", refreshAfterExternalWork)
    })
  })

  const addRightSurface = async (kind: WorkspaceSurfaceKind) => {
    const session = current()
    if (!session) return
    try {
      let resourceId: string | undefined
      let title = kind === "browser" ? "Browser" : kind === "terminal" ? "Terminal" : kind === "files" ? "Files" : "Diff"
      if (kind === "terminal") {
        const terminal = await api.terminalOpen(session.workspace, 100, 30, selectedWslDistribution())
        resourceId = terminal.id
        const count = (workspaceUiBySession()[session.id]?.surfaces ?? [])
          .filter((surface) => surface.kind === "terminal").length + 1
        title = `Terminal ${count}`
      }
      const surface: WorkspaceSurface = {
        id: crypto.randomUUID(),
        kind,
        title,
        ...(resourceId ? { resourceId } : {}),
      }
      updateWorkspaceUi(session.id, (ui) => ({
        ...ui,
        rightPanelOpen: true,
        surfaces: [...ui.surfaces, surface],
        activeSurfaceId: surface.id,
      }))
    } catch (error) {
      showError(error)
    }
  }

  const closeRightSurface = (surfaceId: string) => {
    const session = current()
    if (!session) return
    const ui = workspaceUiBySession()[session.id] ?? emptyWorkspaceUi()
    const index = ui.surfaces.findIndex((surface) => surface.id === surfaceId)
    const surface = ui.surfaces[index]
    if (!surface) return
    if (surface.kind === "terminal" && surface.resourceId) {
      forgetTerminalViewport(surface.resourceId)
      void api.terminalClose(surface.resourceId).catch(showError)
    }
    const nextSurfaces = ui.surfaces.filter((item) => item.id !== surfaceId)
    const nextActive = ui.activeSurfaceId === surfaceId
      ? (nextSurfaces[Math.min(index, Math.max(nextSurfaces.length - 1, 0))]?.id ?? null)
      : ui.activeSurfaceId
    updateWorkspaceUi(session.id, (value) => ({ ...value, surfaces: nextSurfaces, activeSurfaceId: nextActive }))
  }

  const toggleRightPanel = () => {
    const session = current()
    if (!session) return
    updateWorkspaceUi(session.id, (value) => ({ ...value, rightPanelOpen: !value.rightPanelOpen }))
  }

  const newBottomTerminal = async () => {
    const session = current()
    if (!session) return
    try {
      const opened = await api.terminalOpen(session.workspace, 120, 24, selectedWslDistribution())
      updateWorkspaceUi(session.id, (ui) => {
        const terminal: WorkspaceTerminal = {
          id: opened.id,
          title: `Terminal ${ui.terminals.length + 1}`,
          cwd: opened.cwd,
          status: "running",
        }
        return {
          ...ui,
          bottomPanelOpen: true,
          terminals: [...ui.terminals, terminal],
          activeTerminalId: terminal.id,
        }
      })
    } catch (error) {
      showError(error)
    }
  }

  const closeBottomTerminal = (terminalId: string) => {
    const session = current()
    if (!session) return
    const ui = workspaceUiBySession()[session.id] ?? emptyWorkspaceUi()
    const index = ui.terminals.findIndex((terminal) => terminal.id === terminalId)
    forgetTerminalViewport(terminalId)
    void api.terminalClose(terminalId).catch(showError)
    const nextTerminals = ui.terminals.filter((terminal) => terminal.id !== terminalId)
    const nextActive = ui.activeTerminalId === terminalId
      ? (nextTerminals[Math.min(index, Math.max(nextTerminals.length - 1, 0))]?.id ?? null)
      : ui.activeTerminalId
    updateWorkspaceUi(session.id, (value) => ({
      ...value,
      terminals: nextTerminals,
      activeTerminalId: nextActive,
      bottomPanelOpen: nextTerminals.length > 0 && value.bottomPanelOpen,
    }))
  }

  const toggleBottomPanel = async () => {
    const session = current()
    if (!session) return
    const ui = workspaceUiBySession()[session.id] ?? emptyWorkspaceUi()
    if (ui.bottomPanelOpen) {
      updateWorkspaceUi(session.id, (value) => ({ ...value, bottomPanelOpen: false }))
      return
    }
    updateWorkspaceUi(session.id, (value) => ({ ...value, bottomPanelOpen: true }))
    if (ui.terminals.length === 0) await newBottomTerminal()
  }

  const choosePreferredEditor = (target: string) => {
    setPreferredEditor(target)
    localStorage.setItem(PREFERRED_EDITOR_KEY, target)
    const session = current()
    if (session) void api.openWorkspace(session.workspace, target).catch(showError)
  }

  const openWorkspace = () => {
    const session = current()
    if (!session) return
    const target = editors().find((editor) => editor.id === preferredEditor() && editor.available)
      ?? editors().find((editor) => editor.available)
    if (!target) {
      showError(new Error("No supported editor or file manager is available."))
      return
    }
    void api.openWorkspace(session.workspace, target.id).catch(showError)
  }

  const finishGitAction = async (action: WorkspaceGitActionName, operation: () => Promise<{ message: string; url: string | null }>) => {
    if (gitAction()) return
    setGitAction(action)
    try {
      const result = await operation()
      showNotice(result.message, "success")
      await refreshRepo()
      if (result.url) {
        if (api.isTauri) await openUrl(result.url)
        else window.open(result.url, "_blank", "noopener,noreferrer")
      }
    } catch (error) {
      showError(error)
      throw error
    } finally {
      setGitAction(null)
    }
  }

  const commitWorkspace = async (message: string | null) => {
    const session = current()
    if (!session) return
    try {
      await finishGitAction("commit", () => api.commitWorkspace(session.workspace, message))
      setCommitOpen(false)
    } catch {
      // Keep the dialog open so the user can edit the message and retry.
    }
  }

  const pushWorkspace = () => {
    const session = current()
    if (session) void finishGitAction("push", () => api.pushWorkspace(session.workspace)).catch(() => undefined)
  }

  const createPullRequest = () => {
    const session = current()
    if (session) void finishGitAction("create-pr", () => api.createPullRequest(session.workspace)).catch(() => undefined)
  }

  const workspaceGitActions = createMemo(() => deriveWorkspaceGitActions(repoSummary(), repoLoading()))

  return (
    <div class="zai-shell" data-page={page()}>
      <Titlebar
        tabs={tabs()}
        onSelect={selectTab}
        onClose={closeTab}
        onNew={() => openDraft()}
        sessions={sessions().filter((session) => !openSessionIds().includes(session.id)).map((session) => ({ id: session.id, label: session.title, project: workspaceName(session.workspace) }))}
        onOpenSession={openSession}
        onHome={() => setPage("home")}
        onChat={() => setPage("chat")}
        onOpenSettings={() => setSettingsOpen(true)}
        profile={account().profile}
        onSignOut={() => signOut().catch(showError)}
      />

      <main class="zai-main">
        <Switch>
          <Match when={page() === "home"}>
            <HomeView
              sessions={sessions()}
              draftWorkspace={newWorkspace()}
              onNew={openDraft}
              onSelect={openSession}
              onDelete={(id) => void removeSession(id)}
              onChooseWorkspace={chooseDraftWorkspace}
              onSettings={() => setSettingsOpen(true)}
              onChat={() => setPage("chat")}
              onVoice={() => setPage("voice")}
            />
          </Match>

          <Match when={page() === "chat"}>
            <ChatView
              providers={providers()}
              providerModels={providerModels()}
              openRouterModels={openRouterModels()}
              openAi={openAi()}
              profile={account().profile}
              onOpenSettings={() => setSettingsOpen(true)}
            />
          </Match>

          <Match when={page() === "voice"}>
            <VoiceHistoryView onSettings={() => setSettingsOpen(true)} />
          </Match>

          <Match when={page() === "session" && current()}>
            <section class="zai-session-page">
              <div class="zai-session-panel">
                <div class="zai-session-workspace">
                  <div class="zai-session-body">
                    <section class="zai-conversation-pane">
                      <header class="zai-workspace-header">
                        <div class="zai-workspace-header__identity">
                          <span class="zai-project-avatar">
                            {workspaceName(current()!.workspace).slice(0, 1).toUpperCase()}
                          </span>
                          <span class="zai-workspace-header__project">{workspaceName(current()!.workspace)}</span>
                          <span aria-hidden="true" class="zai-workspace-header__slash">/</span>
                          <h2 title={current()!.title}>{current()!.title}</h2>
                          <Show when={repoSummary()?.branch}>
                            <span class="zai-workspace-header__branch"><GitBranch aria-hidden="true" />{repoSummary()!.branch}</span>
                          </Show>
                        </div>
                        <WorkspaceTopbarActions
                          open={{
                            disabled: !editors().some((editor) => editor.available),
                            hint: "Open this workspace in the preferred editor",
                          }}
                          openOptions={editors()}
                          preferredOpenTarget={preferredEditor()}
                          commit={{
                            disabled: workspaceGitActions().commit.disabled,
                            busy: gitAction() === "commit",
                            hint: workspaceGitActions().commit.hint,
                          }}
                          push={{
                            disabled: workspaceGitActions().push.disabled,
                            busy: gitAction() === "push",
                            hint: workspaceGitActions().push.hint,
                          }}
                          createPr={{
                            disabled: workspaceGitActions().createPr.disabled,
                            busy: gitAction() === "create-pr",
                            label: workspaceGitActions().createPr.label,
                            hint: workspaceGitActions().createPr.hint,
                          }}
                          primaryGitAction={workspaceGitActions().primary}
                          bottomPanelOpen={activeWorkspaceUi().bottomPanelOpen}
                          rightPanelOpen={activeWorkspaceUi().rightPanelOpen}
                          bottomPanelShortcut="⌘J"
                          rightPanelShortcut="⌘⇧J"
                          onOpen={openWorkspace}
                          onOpenTarget={choosePreferredEditor}
                          onCommit={() => setCommitOpen(true)}
                          onPush={pushWorkspace}
                          onCreatePr={createPullRequest}
                          onGitMenuOpen={() => void refreshRepo(true)}
                          onToggleBottomPanel={() => void toggleBottomPanel()}
                          onToggleRightPanel={toggleRightPanel}
                        />
                      </header>

                      <Transcript session={current()!} />
                      <div class="zai-session-composer">
                        <Composer
                          provider={current()!.provider}
                          brand={current()!.providerBrand}
                          model={current()!.model ?? "default"}
                          reasoning={current()!.reasoning}
                          speedMode={current()!.speedMode}
                          interactionMode={current()!.interactionMode}
                          accessMode={current()!.accessMode}
                          workspace={current()!.workspace}
                          providers={providers()}
                          providerModels={providerModels()}
                          openRouterModels={openRouterModels()}
                          contextUsage={current()!.contextUsage}
                          providerUsage={currentProviderUsage()}
                          hero={false}
                          placeholder="Ask anything…"
                          running={current()!.status === "running" || current()!.status === "waiting_approval"}
                          approval={activeApproval()}
                          approvalBusy={approvalBusy()}
                          onBrand={(brand) => {
                            const models = modelsForBrand(brand, providerModels(), openRouterModels())
                            const model = models.find((item) => item.isDefault) ?? models[0]
                            void updateCurrentOptions({ provider: runtimeForBrand(brand), providerBrand: brand, model: model?.id ?? null, reasoning: model?.defaultReasoning ?? model?.reasoning[0] ?? null, speedMode: model?.defaultSpeed ?? "standard" })
                          }}
                          onModel={(modelId) => {
                            const model = modelsForBrand(current()!.providerBrand, providerModels(), openRouterModels()).find((item) => item.id === modelId)
                            void updateCurrentOptions({ model: modelId, reasoning: model?.defaultReasoning ?? model?.reasoning[0] ?? null, speedMode: model?.defaultSpeed ?? "standard" })
                          }}
                          onReasoning={(reasoning) => void updateCurrentOptions({ reasoning })}
                          onSpeedMode={(speedMode) => void updateCurrentOptions({ speedMode })}
                          onInteractionMode={(interactionMode) => void updateCurrentOptions({ interactionMode })}
                          onAccessMode={(accessMode) => void updateCurrentOptions({ accessMode })}
                          onWorkspace={() => undefined}
                          onAttach={api.chooseFiles}
                          onSubmit={continueSession}
                          onCancel={cancel}
                          onApproval={respondApproval}
                        />
                      </div>
                    </section>

                    <RightWorkspacePanel
                      open={activeWorkspaceUi().rightPanelOpen}
                      surfaces={activeWorkspaceUi().surfaces}
                      activeSurfaceId={activeWorkspaceUi().activeSurfaceId}
                      availability={{ diff: repoSummary()?.isRepo ?? false }}
                      unavailableReasons={{ diff: "Diff is available inside a Git repository." }}
                      onActivate={(surfaceId) => updateActiveWorkspaceUi((ui) => ({ ...ui, activeSurfaceId: surfaceId }))}
                      onCloseSurface={closeRightSurface}
                      onAddSurface={(kind) => void addRightSurface(kind)}
                      onClosePanel={() => updateActiveWorkspaceUi((ui) => ({ ...ui, rightPanelOpen: false }))}
                      renderSurface={(surface) => (
                        <WorkspaceSurfaceView surface={surface} workspace={current()!.workspace} onError={showError} />
                      )}
                    />
                  </div>

                  <BottomTerminalPanel
                    open={activeWorkspaceUi().bottomPanelOpen}
                    terminals={activeWorkspaceUi().terminals}
                    activeTerminalId={activeWorkspaceUi().activeTerminalId}
                    height={activeWorkspaceUi().terminalHeight}
                    onActivate={(terminalId) => updateActiveWorkspaceUi((ui) => ({ ...ui, activeTerminalId: terminalId }))}
                    onCloseTerminal={closeBottomTerminal}
                    onNewTerminal={() => void newBottomTerminal()}
                    onClear={clearTerminalViewport}
                    onHeightChange={(terminalHeight) => updateActiveWorkspaceUi((ui) => ({ ...ui, terminalHeight }))}
                    onClosePanel={() => updateActiveWorkspaceUi((ui) => ({ ...ui, bottomPanelOpen: false }))}
                    renderTerminal={(terminal) => <TerminalViewport sessionId={terminal.id} autofocus />}
                  />
                </div>
              </div>
            </section>
          </Match>

          <Match when={true}>
            <section class="zai-new-session">
              <div class="zai-new-session__stage">
                <div class="zai-new-session__content">
                  <ZaiWordmark aria-label="Onyx" />
                  <div class="zai-new-session__composer">
                    <Composer
                      provider={newProvider()}
                      brand={newBrand()}
                      model={newModel()}
                      reasoning={newReasoning()}
                      speedMode={newSpeedMode()}
                      interactionMode={newInteractionMode()}
                      accessMode={newAccessMode()}
                      workspace={newWorkspace()}
                      providers={providers()}
                      providerModels={providerModels()}
                      openRouterModels={openRouterModels()}
                      hero
                      autofocus
                      placeholder="Ask anything…"
                      onBrand={selectNewBrand}
                      onModel={(model) => {
                        setNewModel(model)
                        const selected = modelsForBrand(newBrand(), providerModels(), openRouterModels()).find((item) => item.id === model)
                        setNewReasoning(selected?.defaultReasoning ?? selected?.reasoning[0] ?? null)
                        setNewSpeedMode(selected?.defaultSpeed ?? "standard")
                      }}
                      onReasoning={setNewReasoning}
                      onSpeedMode={setNewSpeedMode}
                      onInteractionMode={setNewInteractionMode}
                      onAccessMode={setNewAccessMode}
                      onWorkspace={setNewWorkspace}
                      onAttach={api.chooseFiles}
                      onSubmit={startSession}
                    />
                  </div>
                  <div class="zai-new-session__workspace-row">
                    <button onClick={chooseDraftWorkspace} title={newWorkspace() || "Choose a project"}>
                      <span class="zai-project-avatar">
                        {(newWorkspace() ? workspaceName(newWorkspace()) : "Project").slice(0, 1).toUpperCase()}
                      </span>
                      <span>{newWorkspace() ? workspaceName(newWorkspace()) : "Choose project"}</span>
                      <span class="zai-workspace-chevron">⌄</span>
                    </button>
                    <span class="zai-workspace-divider">/</span>
                    <button class="zai-git-status" onClick={() => void initializeDraftGit()} title={draftGitReady() ? "Git repository ready" : "Initialize a Git repository"}><GitBranch size={14} /> {draftGitReady() ? "main" : "No Git"}</button>
                    <span class="zai-workspace-divider">/</span>
                    <span class="zai-draft-usage" title="Selected model context window">
                      <Gauge size={14} />
                      Context {draftSelectedModel()?.contextLength ? compactTokens(draftSelectedModel()!.contextLength!) : "—"}
                    </span>
                    <span class="zai-workspace-divider">/</span>
                    <span class="zai-draft-usage" title="Usage reported by the selected CLI subscription">
                      <TimerReset size={14} />
                      <Show when={draftProviderUsage()?.windows?.length} fallback="Usage not reported">
                        {draftProviderUsage()!.windows.map((window) => `${window.label} ${Math.round(window.usedPercent)}%`).join(" · ")}
                      </Show>
                    </span>
                  </div>
                </div>
              </div>
            </section>
          </Match>
        </Switch>
      </main>

      <SettingsDialog
        open={settingsOpen()}
        sessions={sessions()}
        providers={providers()}
        providerModels={providerModels()}
        openRouter={openRouter()}
        openAi={openAi()}
        openRouterModels={openRouterModels()}
        colorScheme={colorScheme()}
        onColorScheme={changeColorScheme}
        onClose={() => setSettingsOpen(false)}
        onRefresh={refreshProviders}
        onOpenRouter={setOpenRouter}
        onOpenAi={setOpenAi}
        onModels={(models) => {
          setOpenRouterModels(models)
          if (newProvider() === "openrouter" && !newModel()) setNewModel(models[0]?.id ?? "")
        }}
      />
      <GitCommitDialog
        open={commitOpen()}
        summary={repoSummary()}
        busy={gitAction() === "commit"}
        onClose={() => setCommitOpen(false)}
        onCommit={commitWorkspace}
      />
      <Show when={notice()}><div class="zai-toast" data-kind={noticeKind()} role="status">{notice()}</div></Show>
    </div>
  )
}

export default App
