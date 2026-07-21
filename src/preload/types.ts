export interface DirEntry {
  name: string
  path: string
  isDirectory: boolean
  size: number
  modified: number
}

export interface DiskInfo {
  name: string
  mount: string
  free: number
  size: number
}

export interface Project {
  name: string
  path: string
  createdAt: string
  imported: boolean
}

export interface McpServer {
  command: string
  args?: string[]
  env?: Record<string, string>
}

export interface McpConfig {
  mcpServers: Record<string, McpServer>
}

export interface AIEngineInfo {
  id: string
  name: string
  command: string
  isAvailable: boolean
  version?: string
  availabilityReason?: string
}

export interface AgentEntry {
  name: string
  description: string
  model?: string
  tools?: string
  filePath: string
}

export interface SkillEntry {
  name: string
  description: string
  userInvocable?: boolean
  filePath: string
}

export interface GitStatus {
  isRepo: boolean
  branch: string
  ahead: number
  behind: number
  staged: number
  modified: number
  untracked: number
  hasRemote: boolean
}

export interface GitFileChange {
  path: string
  status: 'modified' | 'added' | 'deleted' | 'untracked' | 'renamed'
  staged: boolean
}

export interface PtySpawnOptions {
  engineId?: 'claude' | 'gemini' | 'codex' | 'kimi' | 'openrouter'
  shell?: string
  cwd?: string
  env?: Record<string, string>
  cols?: number
  rows?: number
}

export interface PtySpawnResult {
  success: boolean
  error?: string
}

export interface OpenRouterModelInfo {
  id: string
  name: string
  description: string
  contextLength: number
  inputModalities: string[]
  supportsThinking: boolean
  supportsImages: boolean
}

export interface OpenRouterOperationResult {
  success: boolean
  error?: string
}

export interface OpenRouterModelsResult extends OpenRouterOperationResult {
  models: OpenRouterModelInfo[]
}

export interface OpenRouterStatus {
  hasApiKey: boolean
  selectedModelId: string | null
}

export interface CanvasDocumentInfo {
  token: string
  url: string
}

export interface IElectronAPI {
  // PTY
  ptySpawn: (id: string, options?: PtySpawnOptions) => Promise<PtySpawnResult>
  ptyWrite: (id: string, data: string) => void
  ptyResize: (id: string, cols: number, rows: number) => void
  ptyKill: (id: string) => Promise<void>
  onPtyData: (callback: (id: string, data: string) => void) => () => void
  onPtyExit: (callback: (id: string, code: number) => void) => () => void

  // Filesystem
  listDisks: () => Promise<DiskInfo[]>
  readDir: (dirPath: string) => Promise<DirEntry[]>
  readFile: (filePath: string) => Promise<string | null>
  canvasReadFile: (projectRoot: string, relativePath: string) => Promise<string>
  canvasReadDir: (projectRoot: string, relativePath: string) => Promise<DirEntry[]>
  canvasCreateDocument: (content: string) => Promise<CanvasDocumentInfo>
  canvasDisposeDocument: (token: string) => Promise<void>
  writeFile: (filePath: string, content: string) => Promise<void>
  stat: (filePath: string) => Promise<DirEntry>
  fsWatch: (dirPath: string) => Promise<void>
  fsUnwatch: (dirPath: string) => Promise<void>
  onFsChanged: (callback: (rootPath: string, changedDir: string) => void) => () => void

  // Projects
  listProjects: () => Promise<Project[]>
  createProject: (name: string) => Promise<Project>
  importProject: (folderPath: string) => Promise<Project>
  deleteProject: (name: string) => Promise<void>
  getProjectsDir: () => Promise<string>

  // Dialogs
  showOpenDirectory: () => Promise<string | null>

  // MCP
  listMcpServers: (projectName: string) => Promise<Record<string, McpServer>>
  addMcpServer: (projectName: string, name: string, server: McpServer) => Promise<void>
  updateMcpServer: (projectName: string, name: string, server: McpServer) => Promise<void>
  removeMcpServer: (projectName: string, name: string) => Promise<void>

  // AI Engines
  listEngines: () => Promise<AIEngineInfo[]>
  detectEngines: () => Promise<AIEngineInfo[]>
  getCommand: (engineId: string, intent: string, params?: Record<string, string>) => Promise<string>
  listAgents: (engineId: string, projectPath: string) => Promise<AgentEntry[]>
  listSkills: (engineId: string, projectPath: string) => Promise<SkillEntry[]>

  // Git
  gitAvailable: () => Promise<boolean>
  gitStatus: (cwd: string) => Promise<GitStatus>
  gitChangedFiles: (cwd: string) => Promise<GitFileChange[]>
  gitAdd: (cwd: string, files: string[]) => Promise<void>
  gitCommit: (cwd: string, message: string) => Promise<string>
  gitPush: (cwd: string, remote?: string, branch?: string) => Promise<string>
  gitPull: (cwd: string) => Promise<string>
  gitInit: (cwd: string) => Promise<string>
  gitConfigGet: (key: string) => Promise<string | null>
  gitConfigSet: (key: string, value: string) => Promise<void>

  // Shell
  shellOpenPath: (filePath: string) => Promise<string>

  // App
  getAppVersion: () => Promise<string>
  getPlatform: () => Promise<'win32' | 'darwin' | 'linux'>

  // OpenRouter
  openRouterSaveApiKey: (apiKey: string) => Promise<OpenRouterOperationResult>
  openRouterClearApiKey: () => Promise<OpenRouterOperationResult>
  openRouterGetStatus: () => Promise<OpenRouterStatus>
  openRouterListModels: (forceRefresh?: boolean) => Promise<OpenRouterModelsResult>
  openRouterGetSelectedModel: () => Promise<string | null>
  openRouterSetSelectedModel: (modelId: string) => Promise<OpenRouterOperationResult>

  // Settings
  getSetting: (key: string) => Promise<unknown>
  setSetting: (key: string, value: unknown) => Promise<void>

  // GUI control
  onGuiAction: (callback: (payload: unknown) => void) => () => void

  // Clipboard
  clipboardReadText: () => string
  showTerminalContextMenu: (hasSelection: boolean) => Promise<string | null>

  // Window controls
  windowMinimize: () => void
  windowMaximize: () => void
  windowClose: () => void
  windowSetFocusMode: (enabled: boolean) => void
  windowPopOutProject: (projectName: string, engineId: string) => Promise<void>
}
