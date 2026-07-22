import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { getCurrentWindow, type Theme } from "@tauri-apps/api/window"
import { open } from "@tauri-apps/plugin-dialog"
import type {
  AgentSession,
  ApprovalRequest,
  EditorTarget,
  GitActionResult,
  OpenRouterModel,
  OpenRouterStatus,
  ProviderId,
  ProviderStatus,
  RepoSummary,
  SessionEvent,
  TerminalEvent,
  TerminalSession,
  WorkspaceFile,
  WorkspaceEntry,
} from "./types"

const tauri = "__TAURI_INTERNALS__" in window

const demoProviders: ProviderStatus[] = [
  ["claude", "Claude Code", true, "2.1.217", "stream-json"],
  ["codex", "Codex", true, "codex-cli 0.144.6", "JSONL (app-server compatible)"],
  ["gemini", "Gemini CLI", false, null, "stream-json / ACP capable"],
  ["kimi", "Kimi Code", true, "0.28.1", "stream-json / ACP capable"],
  ["openrouter", "OpenRouter", true, null, "HTTPS + SSE"],
].map(([id, name, available, version, transport]) => ({
  id: id as ProviderId,
  name: String(name),
  available: Boolean(available),
  executablePath: available && id !== "openrouter" ? `/usr/local/bin/${id}` : null,
  version: version ? String(version) : null,
  installUrl: "https://github.com/z4mbo/zAI#providers",
  transport: String(transport),
}))

const mockSessions: AgentSession[] = []
let mockSessionListener: ((event: SessionEvent) => void) | undefined
let mockApprovalListener: ((request: ApprovalRequest) => void) | undefined
const mockTerminalListeners = new Set<(event: TerminalEvent) => void>()
const mockTurns = new Map<string, number>()

const cloneSession = (session: AgentSession): AgentSession => structuredClone(session)
const putMockSession = (session: AgentSession) => {
  const index = mockSessions.findIndex((item) => item.id === session.id)
  if (index >= 0) mockSessions[index] = cloneSession(session)
  else mockSessions.push(cloneSession(session))
}

const createMockSession = (provider: ProviderId, model: string | null, workspace: string) => {
  const now = new Date().toISOString()
  const session: AgentSession = {
    id: crypto.randomUUID(),
    title: "New session",
    provider,
    model,
    workspace,
    providerSessionId: null,
    status: "idle",
    messages: [],
    createdAt: now,
    updatedAt: now,
  }
  putMockSession(session)
  return cloneSession(session)
}

const sendMockMessage = async (sessionId: string, content: string) => {
  const session = mockSessions.find((item) => item.id === sessionId)
  if (!session) throw new Error("Demo session not found.")
  const now = new Date().toISOString()
  const assistantId = crypto.randomUUID()
  const running: AgentSession = {
    ...session,
    title: session.messages.length === 0 ? content.trim().slice(0, 56) || "New session" : session.title,
    status: "running",
    updatedAt: now,
    messages: [
      ...session.messages,
      { id: crypto.randomUUID(), role: "user", kind: "text", content, createdAt: now },
      { id: assistantId, role: "assistant", kind: "text", content: "", createdAt: now },
    ],
  }
  putMockSession(running)
  mockSessionListener?.({ type: "snapshot", session: cloneSession(running) })

  const turn = (mockTurns.get(sessionId) ?? 0) + 1
  mockTurns.set(sessionId, turn)
  const response = "This is zAI's browser-preview transport. The native app will stream this turn from your selected CLI or OpenRouter model."
  const chunks = response.match(/[\s\S]{1,9}/g) ?? [response]
  void (async () => {
    for (const delta of chunks) {
      await new Promise((resolve) => window.setTimeout(resolve, 45))
      if (mockTurns.get(sessionId) !== turn) return
      const current = mockSessions.find((item) => item.id === sessionId)
      const target = current?.messages.find((message) => message.id === assistantId)
      if (!current || !target) return
      target.content += delta
      current.updatedAt = new Date().toISOString()
      mockSessionListener?.({ type: "delta", sessionId, messageId: assistantId, delta })
    }
    const current = mockSessions.find((item) => item.id === sessionId)
    if (!current || mockTurns.get(sessionId) !== turn) return
    current.status = "idle"
    current.updatedAt = new Date().toISOString()
    mockSessionListener?.({ type: "snapshot", session: cloneSession(current) })
  })()
  return cloneSession(running)
}

export const api = {
  isTauri: tauri,

  listProviders: () => (tauri ? invoke<ProviderStatus[]>("list_providers") : Promise.resolve(demoProviders)),
  listSessions: () => (tauri ? invoke<AgentSession[]>("list_sessions") : Promise.resolve(mockSessions.map(cloneSession))),

  createSession: (provider: ProviderId, model: string | null, workspace: string) =>
    tauri
      ? invoke<AgentSession>("create_session", { input: { provider, model, workspace } })
      : Promise.resolve(createMockSession(provider, model, workspace)),

  deleteSession: (sessionId: string) => {
    if (tauri) return invoke<void>("delete_session", { sessionId })
    const index = mockSessions.findIndex((session) => session.id === sessionId)
    if (index >= 0) mockSessions.splice(index, 1)
    mockTurns.delete(sessionId)
    mockSessionListener?.({ type: "removed", sessionId })
    return Promise.resolve()
  },
  sendMessage: (sessionId: string, content: string) =>
    tauri
      ? invoke<AgentSession>("send_message", { input: { sessionId, content } })
      : sendMockMessage(sessionId, content),
  cancelTurn: (sessionId: string) => {
    if (tauri) return invoke<void>("cancel_turn", { sessionId })
    mockTurns.set(sessionId, (mockTurns.get(sessionId) ?? 0) + 1)
    const session = mockSessions.find((item) => item.id === sessionId)
    if (session) {
      session.status = "idle"
      session.updatedAt = new Date().toISOString()
      mockSessionListener?.({ type: "snapshot", session: cloneSession(session) })
    }
    return Promise.resolve()
  },

  chooseWorkspace: async () => {
    if (!tauri) return "/Users/you/Developer/project"
    const value = await open({ directory: true, multiple: false, title: "Choose a workspace" })
    return typeof value === "string" ? value : null
  },
  workspaceEntries: (workspace: string) =>
    tauri ? invoke<WorkspaceEntry[]>("workspace_entries", { workspace }) : Promise.resolve([]),
  repoSummary: (workspace: string) =>
    tauri
      ? invoke<RepoSummary>("workspace_repo_summary", { workspace })
      : Promise.resolve({
          isRepo: true,
          branch: "main",
          changedFiles: [
            { path: "src/App.tsx", status: "M" },
            { path: "src/styles.css", status: "M" },
          ],
          stagedCount: 0,
          unstagedCount: 2,
          untrackedCount: 0,
          ahead: 1,
          behind: 0,
          hasUpstream: true,
          hasRemote: true,
          prCommitCount: 1,
          prUrl: null,
        }),
  gitDiff: (workspace: string) =>
    tauri
      ? invoke<string>("workspace_git_diff", { workspace })
      : Promise.resolve("diff --git a/src/App.tsx b/src/App.tsx\n--- a/src/App.tsx\n+++ b/src/App.tsx\n@@\n-OpenCode shell\n+OpenCode shell with T3 workspace tools\n"),
  readWorkspaceFile: (workspace: string, path: string) =>
    tauri
      ? invoke<WorkspaceFile>("workspace_read_file", { workspace, path })
      : Promise.resolve({ path, content: "Native file previews are available in the desktop app.\n", truncated: false }),
  workspaceEditors: () =>
    tauri
      ? invoke<EditorTarget[]>("workspace_editors")
      : Promise.resolve([
          { id: "finder", label: "Finder", available: true },
          { id: "vscode", label: "VS Code", available: true },
          { id: "cursor", label: "Cursor", available: false },
        ]),
  openWorkspace: (workspace: string, target: string) =>
    tauri ? invoke<void>("workspace_open", { workspace, target }) : Promise.resolve(),
  commitWorkspace: (workspace: string, message: string | null) =>
    tauri
      ? invoke<GitActionResult>("workspace_commit", { workspace, message })
      : Promise.resolve({ message: message?.trim() || "Update workspace changes", url: null }),
  pushWorkspace: (workspace: string) =>
    tauri
      ? invoke<GitActionResult>("workspace_push", { workspace })
      : Promise.resolve({ message: "Pushed current branch", url: null }),
  createPullRequest: (workspace: string) =>
    tauri
      ? invoke<GitActionResult>("workspace_create_pr", { workspace })
      : Promise.resolve({ message: "Pull request ready", url: "https://github.com/z4mbo/zAI/pull/1" }),

  terminalOpen: (workspace: string, cols: number, rows: number) =>
    tauri
      ? invoke<TerminalSession>("terminal_open", { workspace, cols, rows })
      : Promise.resolve({ id: crypto.randomUUID(), cwd: workspace, shell: "demo" }),
  terminalWrite: (sessionId: string, data: string) => {
    if (tauri) return invoke<void>("terminal_write", { sessionId, data })
    mockTerminalListeners.forEach((listener) =>
      listener({ sessionId, kind: "data", data, exitCode: null }),
    )
    return Promise.resolve()
  },
  terminalResize: (sessionId: string, cols: number, rows: number) =>
    tauri ? invoke<void>("terminal_resize", { sessionId, cols, rows }) : Promise.resolve(),
  terminalClose: (sessionId: string) =>
    tauri ? invoke<void>("terminal_close", { sessionId }) : Promise.resolve(),
  listenTerminal: async (onEvent: (event: TerminalEvent) => void): Promise<UnlistenFn> => {
    if (!tauri) {
      mockTerminalListeners.add(onEvent)
      return () => mockTerminalListeners.delete(onEvent)
    }
    return listen<TerminalEvent>("zai://terminal", (event) => onEvent(event.payload))
  },

  setWindowTheme: (theme: Theme | null) =>
    tauri ? getCurrentWindow().setTheme(theme) : Promise.resolve(),

  openRouterStatus: () =>
    tauri ? invoke<OpenRouterStatus>("openrouter_status") : Promise.resolve({ connected: true }),
  saveOpenRouterKey: (key: string) =>
    tauri ? invoke<OpenRouterStatus>("openrouter_save_key", { key }) : Promise.resolve({ connected: true }),
  clearOpenRouterKey: () =>
    tauri ? invoke<OpenRouterStatus>("openrouter_clear_key") : Promise.resolve({ connected: false }),
  openRouterModels: () =>
    tauri
      ? invoke<OpenRouterModel[]>("openrouter_models")
      : Promise.resolve([
          { id: "anthropic/claude-sonnet-4.6", name: "Claude Sonnet 4.6", contextLength: 1_000_000, promptPrice: null, completionPrice: null },
          { id: "google/gemini-3.1-pro-preview", name: "Gemini 3.1 Pro Preview", contextLength: 1_000_000, promptPrice: null, completionPrice: null },
          { id: "openai/gpt-5.4", name: "OpenAI GPT-5.4", contextLength: 1_000_000, promptPrice: null, completionPrice: null },
        ]),
  respondApproval: (id: string, allow: boolean) =>
    tauri ? invoke<void>("respond_approval", { id, allow }) : Promise.resolve(),

  listen: async (
    onSession: (event: SessionEvent) => void,
    onApproval: (request: ApprovalRequest) => void,
  ): Promise<UnlistenFn> => {
    if (!tauri) {
      mockSessionListener = onSession
      mockApprovalListener = onApproval
      return () => {
        if (mockSessionListener === onSession) mockSessionListener = undefined
        if (mockApprovalListener === onApproval) mockApprovalListener = undefined
      }
    }
    const unlistenSession = await listen<SessionEvent>("zai://session", (event) => onSession(event.payload))
    try {
      const unlistenApproval = await listen<ApprovalRequest>("zai://approval", (event) => onApproval(event.payload))
      return () => {
        unlistenSession()
        unlistenApproval()
      }
    } catch (error) {
      unlistenSession()
      throw error
    }
  },
}
