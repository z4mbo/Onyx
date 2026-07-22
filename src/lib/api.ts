import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { open } from "@tauri-apps/plugin-dialog"
import type {
  AgentSession,
  ApprovalRequest,
  OpenRouterModel,
  OpenRouterStatus,
  ProviderId,
  ProviderStatus,
  SessionEvent,
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

export const api = {
  isTauri: tauri,

  listProviders: () => (tauri ? invoke<ProviderStatus[]>("list_providers") : Promise.resolve(demoProviders)),
  listSessions: () => (tauri ? invoke<AgentSession[]>("list_sessions") : Promise.resolve(mockSessions)),

  createSession: (provider: ProviderId, model: string | null, workspace: string) =>
    tauri
      ? invoke<AgentSession>("create_session", { input: { provider, model, workspace } })
      : Promise.resolve({
          id: crypto.randomUUID(),
          title: "New session",
          provider,
          model,
          workspace,
          providerSessionId: null,
          status: "idle" as const,
          messages: [],
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        }),

  deleteSession: (sessionId: string) =>
    tauri ? invoke<void>("delete_session", { sessionId }) : Promise.resolve(),
  sendMessage: (sessionId: string, content: string) =>
    tauri
      ? invoke<AgentSession>("send_message", { input: { sessionId, content } })
      : Promise.reject(new Error("Run zAI with `npm run dev` to launch coding agents.")),
  cancelTurn: (sessionId: string) =>
    tauri ? invoke<void>("cancel_turn", { sessionId }) : Promise.resolve(),

  chooseWorkspace: async () => {
    if (!tauri) return "/Users/you/Developer/project"
    const value = await open({ directory: true, multiple: false, title: "Choose a workspace" })
    return typeof value === "string" ? value : null
  },
  workspaceEntries: (workspace: string) =>
    tauri ? invoke<WorkspaceEntry[]>("workspace_entries", { workspace }) : Promise.resolve([]),

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
    if (!tauri) return () => undefined
    const unlistenSession = await listen<SessionEvent>("zai://session", (event) => onSession(event.payload))
    const unlistenApproval = await listen<ApprovalRequest>("zai://approval", (event) => onApproval(event.payload))
    return () => {
      unlistenSession()
      unlistenApproval()
    }
  },
}
