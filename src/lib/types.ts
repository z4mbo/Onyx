export type ProviderId = "claude" | "codex" | "gemini" | "kimi" | "openrouter"
export type SessionStatus = "idle" | "running" | "waiting_approval" | "failed"
export type MessageRole = "user" | "assistant" | "system" | "tool"
export type MessageKind = "text" | "reasoning" | "tool" | "error"

export interface ProviderStatus {
  id: ProviderId
  name: string
  available: boolean
  executablePath: string | null
  version: string | null
  installUrl: string
  transport: string
}

export interface Message {
  id: string
  role: MessageRole
  kind: MessageKind
  content: string
  createdAt: string
}

export interface AgentSession {
  id: string
  title: string
  provider: ProviderId
  model: string | null
  workspace: string
  providerSessionId: string | null
  status: SessionStatus
  messages: Message[]
  createdAt: string
  updatedAt: string
}

export type SessionEvent =
  | { type: "snapshot"; session: AgentSession }
  | { type: "delta"; sessionId: string; messageId: string; delta: string }
  | { type: "activity"; sessionId: string; message: Message }
  | { type: "removed"; sessionId: string }

export interface ApprovalRequest {
  id: string
  sessionId: string
  title: string
  detail: string
  risk: string
  createdAt: string
}

export interface WorkspaceEntry {
  name: string
  path: string
  isDirectory: boolean
  depth: number
}

export interface OpenRouterModel {
  id: string
  name: string
  contextLength: number | null
  promptPrice: string | null
  completionPrice: string | null
}

export interface OpenRouterStatus {
  connected: boolean
}
