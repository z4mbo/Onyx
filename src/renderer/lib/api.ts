export interface Project {
  name: string
  path: string
  createdAt: string
  imported: boolean
}

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

export interface McpServer {
  command: string
  args?: string[]
  env?: Record<string, string>
}

/** Legacy editor shape retained for the project MCP form. */
export interface MCPServer extends McpServer {
  id: string
  name: string
  enabled: boolean
}

export interface AIEngineInfo {
  id: string
  name: string
  command: string
  isAvailable: boolean
  version?: string
  availabilityReason?: string
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

interface IElectronAPI {
  ptySpawn: (id: string, options?: PtySpawnOptions) => Promise<PtySpawnResult>
  ptyWrite: (id: string, data: string) => void
  ptyResize: (id: string, cols: number, rows: number) => void
  ptyKill: (id: string) => Promise<void>
  onPtyData: (callback: (id: string, data: string) => void) => () => void
  onPtyExit: (callback: (id: string, code: number) => void) => () => void

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

  listProjects: () => Promise<Project[]>
  createProject: (name: string) => Promise<Project>
  importProject: (folderPath: string) => Promise<Project>
  deleteProject: (name: string) => Promise<void>
  getProjectsDir: () => Promise<string>

  showOpenDirectory: () => Promise<string | null>

  listMcpServers: (projectName: string) => Promise<Record<string, McpServer>>
  addMcpServer: (projectName: string, name: string, server: McpServer) => Promise<void>
  updateMcpServer: (projectName: string, name: string, server: McpServer) => Promise<void>
  removeMcpServer: (projectName: string, name: string) => Promise<void>

  listEngines: () => Promise<AIEngineInfo[]>
  detectEngines: () => Promise<AIEngineInfo[]>
  getCommand: (engineId: string, intent: string, params?: Record<string, string>) => Promise<string>
  listAgents: (engineId: string, projectPath: string) => Promise<AgentEntry[]>
  listSkills: (engineId: string, projectPath: string) => Promise<SkillEntry[]>

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

  shellOpenPath: (filePath: string) => Promise<string>

  getAppVersion: () => Promise<string>
  getPlatform: () => Promise<'win32' | 'darwin' | 'linux'>

  openRouterSaveApiKey: (apiKey: string) => Promise<OpenRouterOperationResult>
  openRouterClearApiKey: () => Promise<OpenRouterOperationResult>
  openRouterGetStatus: () => Promise<OpenRouterStatus>
  openRouterListModels: (forceRefresh?: boolean) => Promise<OpenRouterModelsResult>
  openRouterGetSelectedModel: () => Promise<string | null>
  openRouterSetSelectedModel: (modelId: string) => Promise<OpenRouterOperationResult>

  getSetting: (key: string) => Promise<unknown>
  setSetting: (key: string, value: unknown) => Promise<void>

  onGuiAction: (callback: (payload: unknown) => void) => () => void

  clipboardReadText: () => string
  showTerminalContextMenu: (hasSelection: boolean) => Promise<string | null>


  windowMinimize: () => void
  windowMaximize: () => void
  windowClose: () => void
  windowSetFocusMode: (enabled: boolean) => void
  windowPopOutProject: (projectName: string, engineId: string) => Promise<void>
}

function getApi(): IElectronAPI {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (window as any).api as IElectronAPI
}

// PTY
export const ptySpawn = (id: string, opts?: PtySpawnOptions) => getApi().ptySpawn(id, opts)
export const ptyWrite = (id: string, data: string) => getApi().ptyWrite(id, data)
export const ptyResize = (id: string, cols: number, rows: number) => getApi().ptyResize(id, cols, rows)
export const ptyKill = (id: string) => getApi().ptyKill(id)
export const onPtyData = (cb: (id: string, data: string) => void) => getApi().onPtyData(cb)
export const onPtyExit = (cb: (id: string, code: number) => void) => getApi().onPtyExit(cb)

// Filesystem
export const listDisks = () => getApi().listDisks()
export const readDir = (p: string) => getApi().readDir(p)
export const readFile = (p: string) => getApi().readFile(p)
export const canvasReadFile = (projectRoot: string, relativePath: string) =>
  getApi().canvasReadFile(projectRoot, relativePath)
export const canvasReadDir = (projectRoot: string, relativePath: string) =>
  getApi().canvasReadDir(projectRoot, relativePath)
export const canvasCreateDocument = (content: string) => getApi().canvasCreateDocument(content)
export const canvasDisposeDocument = (token: string) => getApi().canvasDisposeDocument(token)
export const writeFile = (p: string, content: string) => getApi().writeFile(p, content)
export const fsWatch = (p: string) => getApi().fsWatch(p)
export const fsUnwatch = (p: string) => getApi().fsUnwatch(p)
export const onFsChanged = (cb: (rootPath: string, changedDir: string) => void) => getApi().onFsChanged(cb)

// Projects
export const listProjects = () => getApi().listProjects()
export const createProject = (name: string) => getApi().createProject(name)
export const importProject = (folderPath: string) => getApi().importProject(folderPath)
export const deleteProject = (name: string) => getApi().deleteProject(name)
export const getProjectsDir = () => getApi().getProjectsDir()

// Dialogs
export const showOpenDirectory = () => getApi().showOpenDirectory()

// MCP
export const listMcpServers = (projectName: string) => getApi().listMcpServers(projectName)
export const addMcpServer = (projectName: string, name: string, server: McpServer) =>
  getApi().addMcpServer(projectName, name, server)
export const updateMcpServer = (projectName: string, name: string, server: McpServer) =>
  getApi().updateMcpServer(projectName, name, server)
export const removeMcpServer = (projectName: string, name: string) =>
  getApi().removeMcpServer(projectName, name)

// AI Engines
export const listEngines = () => getApi().listEngines()
export const detectEngines = () => getApi().detectEngines()
export const getCommand = (engineId: string, intent: string, params?: Record<string, string>) =>
  getApi().getCommand(engineId, intent, params)
export const listAgents = async (engineId: string, projectPath: string) => {
  const fn = getApi().listAgents
  if (typeof fn !== 'function') return []
  return fn(engineId, projectPath)
}
export const listSkills = async (engineId: string, projectPath: string) => {
  const fn = getApi().listSkills
  if (typeof fn !== 'function') return []
  return fn(engineId, projectPath)
}

// Git
export const gitAvailable = () => getApi().gitAvailable()
export const gitStatus = (cwd: string) => getApi().gitStatus(cwd)
export const gitChangedFiles = (cwd: string) => getApi().gitChangedFiles(cwd)
export const gitAdd = (cwd: string, files: string[]) => getApi().gitAdd(cwd, files)
export const gitCommit = (cwd: string, message: string) => getApi().gitCommit(cwd, message)
export const gitPush = (cwd: string, remote?: string, branch?: string) => getApi().gitPush(cwd, remote, branch)
export const gitPull = (cwd: string) => getApi().gitPull(cwd)
export const gitInit = (cwd: string) => getApi().gitInit(cwd)
export const gitConfigGet = (key: string) => getApi().gitConfigGet(key)
export const gitConfigSet = (key: string, value: string) => getApi().gitConfigSet(key, value)

// Shell
export const shellOpenPath = (filePath: string) => getApi().shellOpenPath(filePath)

// App
export const getAppVersion = () => getApi().getAppVersion()
export const getPlatform = () => getApi().getPlatform()

// OpenRouter
export const openRouterSaveApiKey = (apiKey: string) => getApi().openRouterSaveApiKey(apiKey)
export const openRouterClearApiKey = () => getApi().openRouterClearApiKey()
export const openRouterGetStatus = () => getApi().openRouterGetStatus()
export const openRouterListModels = (forceRefresh?: boolean) =>
  getApi().openRouterListModels(forceRefresh)
export const openRouterGetSelectedModel = () => getApi().openRouterGetSelectedModel()
export const openRouterSetSelectedModel = (modelId: string) =>
  getApi().openRouterSetSelectedModel(modelId)

// Settings
export const getSetting = (key: string) => getApi().getSetting(key)
export const setSetting = (key: string, value: unknown) => getApi().setSetting(key, value)

// Clipboard
export const clipboardReadText = () => getApi().clipboardReadText()
export const showTerminalContextMenu = (hasSelection: boolean) => getApi().showTerminalContextMenu(hasSelection)

// GUI control
export const onGuiAction = (cb: (payload: unknown) => void) => getApi().onGuiAction(cb)

// Window
export const windowMinimize = () => getApi().windowMinimize()
export const windowMaximize = () => getApi().windowMaximize()
export const windowClose = () => getApi().windowClose()
export const windowSetFocusMode = (enabled: boolean) => getApi().windowSetFocusMode(enabled)
export const windowPopOutProject = (projectName: string, engineId: string) => getApi().windowPopOutProject(projectName, engineId)
