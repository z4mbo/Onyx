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

export interface RepoFileChange {
  path: string
  status: string
}

export interface RepoSummary {
  isRepo: boolean
  branch: string | null
  changedFiles: RepoFileChange[]
  stagedCount: number
  unstagedCount: number
  untrackedCount: number
  ahead: number
  behind: number
  hasUpstream: boolean
  hasRemote: boolean
  /** Commits between the remote default branch and HEAD, when Git can resolve that base. */
  prCommitCount: number | null
  prUrl: string | null
}

export interface WorkspaceFile {
  path: string
  content: string
  truncated: boolean
}

export interface EditorTarget {
  id: string
  label: string
  available: boolean
}

export interface GitActionResult {
  message: string
  url: string | null
}

export interface TerminalSession {
  id: string
  cwd: string
  shell: string
}

export interface TerminalEvent {
  sessionId: string
  kind: "data" | "exit" | "error"
  data: string | null
  exitCode: number | null
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
