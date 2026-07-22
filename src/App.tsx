import { createMemo, createSignal, onCleanup, onMount, Show, type Component } from "solid-js"
import { FolderOpen, PanelRightOpen, Trash2 } from "lucide-solid"
import { api } from "./lib/api"
import { workspaceName } from "./lib/providers"
import type { AgentSession, ApprovalRequest, OpenRouterModel, OpenRouterStatus, ProviderId, ProviderStatus, SessionEvent } from "./lib/types"
import { ApprovalBar } from "./components/ApprovalBar"
import { Composer } from "./components/Composer"
import { FileTree } from "./components/FileTree"
import { ProviderBadge } from "./components/ProviderBadge"
import { SettingsDialog } from "./components/SettingsDialog"
import { Sidebar } from "./components/Sidebar"
import { Transcript } from "./components/Transcript"

const App: Component = () => {
  const [providers, setProviders] = createSignal<ProviderStatus[]>([])
  const [sessions, setSessions] = createSignal<AgentSession[]>([])
  const [currentId, setCurrentId] = createSignal<string | null>(null)
  const [newProvider, setNewProvider] = createSignal<ProviderId>("claude")
  const [newModel, setNewModel] = createSignal("default")
  const [newWorkspace, setNewWorkspace] = createSignal("")
  const [settingsOpen, setSettingsOpen] = createSignal(false)
  const [filesOpen, setFilesOpen] = createSignal(false)
  const [openRouter, setOpenRouter] = createSignal<OpenRouterStatus>({ connected: false })
  const [openRouterModels, setOpenRouterModels] = createSignal<OpenRouterModel[]>([])
  const [approvals, setApprovals] = createSignal<ApprovalRequest[]>([])
  const [approvalBusy, setApprovalBusy] = createSignal(false)
  const [notice, setNotice] = createSignal<string | null>(null)

  const current = createMemo(() => sessions().find((session) => session.id === currentId()) ?? null)
  const activeApproval = createMemo(() => approvals().find((request) => request.sessionId === currentId()) ?? approvals()[0])

  const sortSessions = (items: AgentSession[]) =>
    [...items].sort((left, right) => Date.parse(right.updatedAt) - Date.parse(left.updatedAt))

  const putSession = (session: AgentSession) => {
    setSessions((items) => sortSessions([session, ...items.filter((item) => item.id !== session.id)]))
  }

  const handleSessionEvent = (event: SessionEvent) => {
    if (event.type === "snapshot") {
      putSession(event.session)
      if (event.session.status !== "running" && event.session.status !== "waiting_approval") {
        setApprovals((items) => items.filter((request) => request.sessionId !== event.session.id))
      }
      return
    }
    if (event.type === "removed") {
      setSessions((items) => items.filter((session) => session.id !== event.sessionId))
      setApprovals((items) => items.filter((request) => request.sessionId !== event.sessionId))
      if (currentId() === event.sessionId) setCurrentId(null)
      return
    }
    setSessions((items) => items.map((session) => {
      if (session.id !== event.sessionId) return session
      if (event.type === "activity") {
        if (session.messages.some((message) => message.id === event.message.id)) return session
        return { ...session, messages: [...session.messages, event.message] }
      }
      const existing = session.messages.find((message) => message.id === event.messageId)
      const messages = existing
        ? session.messages.map((message) => message.id === event.messageId ? { ...message, content: message.content + event.delta } : message)
        : [...session.messages, {
            id: event.messageId,
            role: "assistant" as const,
            kind: "text" as const,
            content: event.delta,
            createdAt: new Date().toISOString(),
          }]
      return { ...session, messages }
    }))
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

  const selectNewProvider = (provider: ProviderId) => {
    setNewProvider(provider)
    setNewModel(provider === "openrouter" ? (openRouterModels()[0]?.id ?? "") : "default")
  }

  const load = async () => {
    try {
      const [providerList, sessionList, routerStatus] = await Promise.all([
        api.listProviders(), api.listSessions(), api.openRouterStatus(),
      ])
      setProviders(providerList)
      setSessions(sortSessions(sessionList))
      setOpenRouter(routerStatus)
      if (routerStatus.connected) {
        const models = await api.openRouterModels().catch(() => [])
        setOpenRouterModels(models)
      }
      const firstAvailable = providerList.find((provider) => provider.available)
      if (firstAvailable) selectNewProvider(firstAvailable.id)
    } catch (error) {
      showError(error)
    }
  }

  onMount(() => {
    void load()
    let unlisten: () => void = () => {}
    void api.listen(handleSessionEvent, (request) => setApprovals((items) => [...items, request])).then((dispose) => { unlisten = dispose })
    const keydown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "n") {
        event.preventDefault()
        setCurrentId(null)
      }
    }
    window.addEventListener("keydown", keydown)
    onCleanup(() => {
      unlisten()
      window.removeEventListener("keydown", keydown)
    })
  })

  const showError = (error: unknown) => {
    setNotice(error instanceof Error ? error.message : String(error))
    window.setTimeout(() => setNotice(null), 6500)
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
      const session = await api.createSession(newProvider(), newModel() || null, workspace)
      putSession(session)
      setCurrentId(session.id)
      const running = await api.sendMessage(session.id, content)
      putSession(running)
    } catch (error) {
      showError(error)
      throw error
    }
  }

  const continueSession = async (content: string) => {
    const session = current()
    if (!session) return
    try {
      putSession(await api.sendMessage(session.id, content))
    } catch (error) {
      showError(error)
      throw error
    }
  }

  const cancel = async () => {
    const session = current()
    if (!session) return
    try { await api.cancelTurn(session.id) } catch (error) { showError(error) }
  }

  const removeCurrent = async () => {
    const session = current()
    if (!session || !window.confirm(`Delete “${session.title}”?`)) return
    try {
      await api.deleteSession(session.id)
      setSessions((items) => items.filter((item) => item.id !== session.id))
      setCurrentId(null)
      setFilesOpen(false)
    } catch (error) { showError(error) }
  }

  const respondApproval = async (allow: boolean) => {
    const request = activeApproval()
    if (!request) return
    setApprovalBusy(true)
    try {
      await api.respondApproval(request.id, allow)
      setApprovals((items) => items.filter((item) => item.id !== request.id))
    } catch (error) { showError(error) } finally { setApprovalBusy(false) }
  }

  return (
    <div class="app-shell">
      <Sidebar
        sessions={sessions()}
        currentId={currentId()}
        onSelect={(id) => setCurrentId(id)}
        onNew={() => { setCurrentId(null); setFilesOpen(false) }}
        onSettings={() => setSettingsOpen(true)}
      />
      <main class="main-shell">
        <Show
          when={current()}
          fallback={
            <div class="welcome-view">
              <div class="window-drag" data-tauri-drag-region />
              <div class="welcome-content">
                <div class="welcome-mark"><img src="/zai.svg" alt="" /></div>
                <h1>What do you want to build?</h1>
                <p>One workspace for Claude Code, Codex, Gemini CLI, Kimi Code, and OpenRouter.</p>
                <Composer
                  provider={newProvider()}
                  model={newModel()}
                  workspace={newWorkspace()}
                  providers={providers()}
                  openRouterModels={openRouterModels()}
                  onProvider={selectNewProvider}
                  onModel={setNewModel}
                  onWorkspace={setNewWorkspace}
                  onSubmit={startSession}
                />
                <div class="welcome-hints"><span>↵ send</span><span>⇧↵ new line</span><span>Local CLI credentials stay local</span></div>
              </div>
            </div>
          }
        >
          {(session) => (
            <div class="session-layout">
              <header class="session-header" data-tauri-drag-region>
                <div class="session-heading">
                  <ProviderBadge provider={session().provider} />
                  <div><h1>{session().title}</h1><span><FolderOpen size={12} /> {workspaceName(session().workspace)}</span></div>
                </div>
                <div class="session-actions">
                  <button class="icon-button" onClick={() => setFilesOpen(!filesOpen())} aria-label="Toggle workspace files"><PanelRightOpen size={16} /></button>
                  <button class="icon-button" onClick={removeCurrent} aria-label="Delete session"><Trash2 size={15} /></button>
                </div>
              </header>
              <div class="session-body">
                <section class="conversation-pane">
                  <Transcript session={session()} />
                  <Show when={activeApproval() && activeApproval()!.sessionId === session().id}>
                    <ApprovalBar request={activeApproval()!} busy={approvalBusy()} onRespond={respondApproval} />
                  </Show>
                  <div class="active-composer">
                    <Composer
                      provider={session().provider}
                      model={session().model ?? "default"}
                      workspace={session().workspace}
                      providers={providers()}
                      openRouterModels={openRouterModels()}
                      locked
                      running={session().status === "running" || session().status === "waiting_approval"}
                      onProvider={() => undefined}
                      onModel={() => undefined}
                      onWorkspace={() => undefined}
                      onSubmit={continueSession}
                      onCancel={cancel}
                    />
                  </div>
                </section>
                <Show when={filesOpen()}><FileTree workspace={session().workspace} onClose={() => setFilesOpen(false)} /></Show>
              </div>
            </div>
          )}
        </Show>
      </main>

      <SettingsDialog
        open={settingsOpen()}
        providers={providers()}
        openRouter={openRouter()}
        onClose={() => setSettingsOpen(false)}
        onRefresh={refreshProviders}
        onOpenRouter={setOpenRouter}
        onModels={(models) => {
          setOpenRouterModels(models)
          if (newProvider() === "openrouter" && !newModel()) setNewModel(models[0]?.id ?? "")
        }}
      />
      <Show when={notice()}><div class="toast" role="status">{notice()}</div></Show>
    </div>
  )
}

export default App
