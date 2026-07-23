import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { getCurrentWindow, type Theme } from "@tauri-apps/api/window"
import { open } from "@tauri-apps/plugin-dialog"
import type {
  AgentSession,
  AccessMode,
  ActiveAppContext,
  ApprovalRequest,
  ChatReply,
  ChatRequest,
  EditorTarget,
  GitActionResult,
  OpenRouterModel,
  OpenRouterStatus,
  ProviderBrand,
  ProviderId,
  ProviderModelOption,
  ProviderStatus,
  ProviderUsage,
  ReasoningEffort,
  RepoSummary,
  SessionEvent,
  TerminalEvent,
  TerminalSession,
  TranscriptionReply,
  VideoJob,
  VoiceSettings,
  WorkspaceFile,
  WorkspaceEntry,
} from "./types"
import { brandForRuntime } from "./providers"

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
  installUrl: "https://github.com/z4mbo/Onyx#providers",
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

interface CreateSessionOptions {
  provider: ProviderId
  providerBrand: ProviderBrand
  model: string | null
  reasoning: ReasoningEffort | null
  speedMode: "standard" | "fast"
  interactionMode: "build" | "plan"
  accessMode: AccessMode
  workspace: string
}

const createMockSession = (input: CreateSessionOptions) => {
  const now = new Date().toISOString()
  const session: AgentSession = {
    id: crypto.randomUUID(),
    title: "New session",
    provider: input.provider,
    providerBrand: input.providerBrand ?? brandForRuntime(input.provider),
    model: input.model,
    reasoning: input.reasoning,
    speedMode: input.speedMode,
    interactionMode: input.interactionMode,
    accessMode: input.accessMode,
    workspace: input.workspace,
    providerSessionId: null,
    status: "idle",
    messages: [],
    contextUsage: null,
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
  const response = "This is Onyx's browser-preview transport. The native app streams this turn from your selected CLI or OpenRouter model."
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

  providerModels: (provider: ProviderId) =>
    tauri ? invoke<ProviderModelOption[]>("list_provider_models", { provider }) : Promise.resolve([]),
  providerUsage: (provider: ProviderId) =>
    tauri ? invoke<ProviderUsage | null>("provider_usage", { provider }) : Promise.resolve(null),

  createSession: (input: CreateSessionOptions) =>
    tauri
      ? invoke<AgentSession>("create_session", { input })
      : Promise.resolve(createMockSession(input)),
  updateSessionOptions: (input: Pick<AgentSession, "id" | "provider" | "providerBrand" | "model" | "reasoning" | "speedMode" | "interactionMode" | "accessMode">) => {
    const payload = { ...input, sessionId: input.id }
    if (tauri) return invoke<AgentSession>("update_session_options", { input: payload })
    const session = mockSessions.find((item) => item.id === input.id)
    if (!session) return Promise.reject(new Error("Demo session not found"))
    Object.assign(session, input, { updatedAt: new Date().toISOString() })
    putMockSession(session)
    return Promise.resolve(cloneSession(session))
  },

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
  chooseFiles: async () => {
    if (!tauri) return ["/Users/you/Developer/project/README.md"]
    const value = await open({ directory: false, multiple: true, title: "Attach files" })
    if (Array.isArray(value)) return value
    return typeof value === "string" ? [value] : []
  },
  workspaceEntries: (workspace: string) =>
    tauri ? invoke<WorkspaceEntry[]>("workspace_entries", { workspace }) : Promise.resolve([]),
  repoSummary: (workspace: string) =>
    tauri
      ? invoke<RepoSummary>("workspace_repo_summary", { workspace })
      : Promise.resolve<RepoSummary>({
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
  initGit: (workspace: string) => tauri
    ? invoke<GitActionResult>("workspace_git_init", { workspace })
    : Promise.resolve({ message: "Initialized Git repository on branch main", url: null }),
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
      : Promise.resolve({ message: "Pull request ready", url: "https://github.com/z4mbo/Onyx/pull/1" }),

  terminalOpen: (workspace: string, cols: number, rows: number, wslDistribution: string | null = null) =>
    tauri
      ? invoke<TerminalSession>("terminal_open", { workspace, cols, rows, wslDistribution })
      : Promise.resolve({ id: crypto.randomUUID(), cwd: workspace, shell: "demo" }),
  listWslDistributions: () => tauri ? invoke<string[]>("list_wsl_distributions") : Promise.resolve([]),
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
    return listen<TerminalEvent>("onyx://terminal", (event) => onEvent(event.payload))
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
          { id: "x-ai/grok-4", name: "Grok 4", contextLength: 256_000, promptPrice: null, completionPrice: null },
        ]),
  getVoiceSettings: () =>
    tauri
      ? invoke<VoiceSettings>("get_voice_settings")
      : Promise.resolve<VoiceSettings>({
          dictationShortcut: "Ctrl+Shift (hold)", agentShortcut: "Ctrl+Alt (hold)", overlayPosition: "bottom_center",
          overlayMargin: 18, transcriptionProvider: "openrouter", transcriptionModel: "openai/whisper-large-v3",
          agentProvider: "openrouter", agentModel: "openrouter/auto",
          webProvider: "openrouter", webModel: "openrouter/auto",
          filesProvider: "codex", filesModel: "default",
          imageProvider: "openrouter", imageModel: "",
          reasoning: "medium", language: null,
          speakResponses: true, voiceProvider: "openrouter", voiceId: "alloy", voiceModel: "openai/gpt-4o-mini-tts-2025-12-15", voiceRate: 1,
        }),
  applyVoiceSettings: (settings: VoiceSettings) =>
    tauri ? invoke<VoiceSettings>("apply_voice_settings", { settings }) : Promise.resolve(settings),
  transcribeAudio: (audioBase64: string, format: string) =>
    tauri
      ? invoke<TranscriptionReply>("transcribe_audio", { request: { audioBase64, format } })
      : Promise.resolve({ text: "Browser preview transcription", model: "demo" }),
  speakText: (text: string) => tauri
    ? invoke<string>("speak_text", { text })
    : Promise.resolve(""),
  injectText: (text: string) => tauri ? invoke<void>("inject_text", { text }) : Promise.resolve(),
  activeAppContext: () => tauri
    ? invoke<ActiveAppContext>("active_app_context")
    : Promise.resolve({ name: "Browser preview", process: "browser", accent: "#7c6ff2", symbol: "O" }),
  setAgentExpanded: (expanded: boolean) => tauri ? invoke<void>("set_agent_expanded", { expanded }) : Promise.resolve(),
  hideWindow: (label: string) => tauri ? invoke<void>("hide_window", { label }) : Promise.resolve(),
  showMainWindow: () => tauri ? invoke<void>("show_main_window") : Promise.resolve(),
  platform: () => tauri ? invoke<string>("platform") : Promise.resolve("browser"),
  chatSend: (request: ChatRequest) => tauri
    ? invoke<ChatReply>("chat_send", { request })
    : Promise.resolve({ content: "Onyx Chat browser preview is ready. Connect a CLI or OpenRouter in the desktop build for live responses.", model: request.model, media: [] }),
  generateImage: (model: string, prompt: string, aspectRatio: string | null) => tauri
    ? invoke<ChatReply>("generate_image", { request: { model, prompt, aspectRatio } })
    : Promise.resolve({ content: "Image generation is available in the native app.", model, media: [] }),
  startVideo: (model: string, prompt: string, aspectRatio: string | null) => tauri
    ? invoke<VideoJob>("start_video", { request: { model, prompt, aspectRatio } })
    : Promise.resolve({ id: crypto.randomUUID(), status: "completed", pollingUrl: "", contentUrl: null, error: null }),
  pollVideo: (id: string) => tauri ? invoke<VideoJob>("poll_video", { id }) : Promise.resolve({ id, status: "completed" as const, pollingUrl: "", contentUrl: null, error: null }),
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
    const unlistenSession = await listen<SessionEvent>("onyx://session", (event) => onSession(event.payload))
    try {
      const unlistenApproval = await listen<ApprovalRequest>("onyx://approval", (event) => onApproval(event.payload))
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
