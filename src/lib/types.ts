export type ProviderId = "claude" | "codex" | "gemini" | "kimi" | "openrouter"
export type ProviderBrand = "openai" | "anthropic" | "google" | "xai" | "moonshot" | "openrouter"
export type ReasoningEffort = "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultracode"
export type SpeedMode = "standard" | "fast"
export type InteractionMode = "build" | "plan"
export type AccessMode = "approval_required" | "auto_accept_edits" | "full_access"
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

export interface ProviderModelOption {
  id: string
  name: string
  description: string | null
  isDefault: boolean
  reasoning: ReasoningEffort[]
  defaultReasoning: ReasoningEffort | null
  speeds: SpeedMode[]
  defaultSpeed: SpeedMode
  contextLength: number | null
}

export interface Message {
  id: string
  role: MessageRole
  kind: MessageKind
  content: string
  createdAt: string
}

export interface ContextUsage {
  usedTokens: number
  maxTokens: number | null
  inputTokens: number | null
  cachedInputTokens: number | null
  outputTokens: number | null
  reasoningOutputTokens: number | null
}

export interface UsageWindow {
  label: string
  usedPercent: number
  windowMinutes: number | null
  resetsAt: number | null
}

export interface ProviderUsage {
  provider: ProviderId
  plan: string | null
  windows: UsageWindow[]
  updatedAt: string
}

export interface AgentSession {
  id: string
  title: string
  provider: ProviderId
  providerBrand: ProviderBrand
  model: string | null
  reasoning: ReasoningEffort | null
  speedMode: SpeedMode
  interactionMode: InteractionMode
  accessMode: AccessMode
  workspace: string
  providerSessionId: string | null
  status: SessionStatus
  messages: Message[]
  contextUsage: ContextUsage | null
  createdAt: string
  updatedAt: string
}

export type SessionEvent =
  | { type: "snapshot"; session: AgentSession }
  | { type: "delta"; sessionId: string; messageId: string; delta: string }
  | { type: "activity"; sessionId: string; message: Message }
  | { type: "context_usage"; sessionId: string; usage: ContextUsage }
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
  description?: string | null
  contextLength: number | null
  promptPrice: string | null
  completionPrice: string | null
  inputModalities?: string[]
  outputModalities?: string[]
}

export interface OpenRouterStatus {
  connected: boolean
}

export interface OpenAiStatus {
  connected: boolean
}

export type OverlayPosition =
  | "top_left"
  | "top_center"
  | "top_right"
  | "center"
  | "bottom_left"
  | "bottom_center"
  | "bottom_right"

export interface VoiceSettings {
  dictationShortcut: string
  agentShortcut: string
  overlayPosition: OverlayPosition
  overlayMargin: number
  transcriptionProvider: "openrouter" | "openai"
  transcriptionModel: string
  agentProvider: ProviderId
  agentModel: string
  webProvider: ProviderId
  webModel: string
  filesProvider: ProviderId
  filesModel: string
  imageProvider: ProviderId
  imageModel: string
  reasoning: ReasoningEffort
  language: string | null
  speakResponses: boolean
  voiceProvider: "system" | "openrouter" | "openai"
  voiceId: string
  voiceModel: string
  voiceRate: number
}

export interface VoiceHistoryItem {
  id: string
  createdAt: string
  kind: "dictation" | "agent"
  text: string
  answer?: string | null
  appName?: string | null
  model?: string | null
}

export interface TranscriptionReply {
  text: string
  model: string
}

export interface NativeVoicePermissions {
  inputMonitoring: boolean
  accessibility: boolean
}

export interface ActiveAppContext {
  name: string
  process: string
  accent: string
  symbol: string
}

export interface HoldPayload {
  mode: "dictation" | "agent"
  phase: "pressed" | "released"
}

export interface ChatMessage {
  id: string
  role: "user" | "assistant"
  content: string
  media: ChatMedia[]
  createdAt: string
}

export interface ChatRequest {
  provider: ProviderId
  model: string
  messages: Array<{ role: "user" | "assistant"; content: string }>
  webSearch?: boolean
}

export interface ChatMedia {
  kind: "image" | "video"
  url: string
  mimeType: string | null
}

export interface ChatThread {
  id: string
  title: string
  provider: ProviderId
  model: string
  mode: "chat" | "image" | "video"
  messages: ChatMessage[]
  createdAt: string
  updatedAt: string
}

export interface ChatReply {
  content: string
  model: string
  media: ChatMedia[]
}

export interface VideoJob {
  id: string
  status: "pending" | "queued" | "processing" | "completed" | "failed"
  pollingUrl: string
  contentUrl: string | null
  error: string | null
}

export interface AccountProfile {
  id: string
  name: string
  email: string
  imageUrl: string | null
}

export interface CloudStatus {
  configured: boolean
  authenticated: boolean
  syncing: boolean
  lastSyncedAt: string | null
  error: string | null
}

export type WslMode = "off" | "default" | "distribution"

export interface DesktopPreferences {
  wslMode: WslMode
  wslDistribution: string
}
