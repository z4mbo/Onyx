import { contextBridge, ipcRenderer, clipboard } from 'electron'
import type { IElectronAPI } from './types'

const api: IElectronAPI = {
  // PTY
  ptySpawn: (id, options) => ipcRenderer.invoke('pty:spawn', id, options),
  ptyWrite: (id, data) => ipcRenderer.send('pty:write', id, data),
  ptyResize: (id, cols, rows) => ipcRenderer.send('pty:resize', id, cols, rows),
  ptyKill: (id) => ipcRenderer.invoke('pty:kill', id),
  onPtyData: (callback) => {
    const listener = (_event: Electron.IpcRendererEvent, id: string, data: string) => callback(id, data)
    ipcRenderer.on('pty:data', listener)
    return () => ipcRenderer.removeListener('pty:data', listener)
  },
  onPtyExit: (callback) => {
    const listener = (_event: Electron.IpcRendererEvent, id: string, code: number) => callback(id, code)
    ipcRenderer.on('pty:exit', listener)
    return () => ipcRenderer.removeListener('pty:exit', listener)
  },

  // Filesystem
  listDisks: () => ipcRenderer.invoke('fs:list-disks'),
  readDir: (dirPath) => ipcRenderer.invoke('fs:read-dir', dirPath),
  readFile: (filePath) => ipcRenderer.invoke('fs:read-file', filePath),
  canvasReadFile: (projectRoot, relativePath) => ipcRenderer.invoke('fs:canvas-read-file', projectRoot, relativePath),
  canvasReadDir: (projectRoot, relativePath) => ipcRenderer.invoke('fs:canvas-read-dir', projectRoot, relativePath),
  canvasCreateDocument: (content) => ipcRenderer.invoke('canvas:create-document', content),
  canvasDisposeDocument: (token) => ipcRenderer.invoke('canvas:dispose-document', token),
  writeFile: (filePath, content) => ipcRenderer.invoke('fs:write-file', filePath, content),
  stat: (filePath) => ipcRenderer.invoke('fs:stat', filePath),
  fsWatch: (dirPath) => ipcRenderer.invoke('fs:watch', dirPath),
  fsUnwatch: (dirPath) => ipcRenderer.invoke('fs:unwatch', dirPath),
  onFsChanged: (callback) => {
    const listener = (_event: Electron.IpcRendererEvent, rootPath: string, changedDir: string) => callback(rootPath, changedDir)
    ipcRenderer.on('fs:changed', listener)
    return () => ipcRenderer.removeListener('fs:changed', listener)
  },

  // Projects
  listProjects: () => ipcRenderer.invoke('project:list'),
  createProject: (name) => ipcRenderer.invoke('project:create', name),
  importProject: (folderPath) => ipcRenderer.invoke('project:import', folderPath),
  deleteProject: (name) => ipcRenderer.invoke('project:delete', name),
  getProjectsDir: () => ipcRenderer.invoke('project:get-projects-dir'),

  // Dialogs
  showOpenDirectory: () => ipcRenderer.invoke('dialog:open-directory'),

  // MCP
  listMcpServers: (projectName) => ipcRenderer.invoke('mcp:list', projectName),
  addMcpServer: (projectName, name, server) => ipcRenderer.invoke('mcp:add', projectName, name, server),
  updateMcpServer: (projectName, name, server) => ipcRenderer.invoke('mcp:update', projectName, name, server),
  removeMcpServer: (projectName, name) => ipcRenderer.invoke('mcp:remove', projectName, name),

  // AI Engines
  listEngines: () => ipcRenderer.invoke('engines:available'),
  detectEngines: () => ipcRenderer.invoke('engines:detect'),
  getCommand: (engineId, intent, params) => ipcRenderer.invoke('engines:command', engineId, intent, params),
  listAgents: (engineId, projectPath) => ipcRenderer.invoke('engines:list-agents', engineId, projectPath),
  listSkills: (engineId, projectPath) => ipcRenderer.invoke('engines:list-skills', engineId, projectPath),

  // Git
  gitAvailable: () => ipcRenderer.invoke('git:available'),
  gitStatus: (cwd) => ipcRenderer.invoke('git:status', cwd),
  gitChangedFiles: (cwd) => ipcRenderer.invoke('git:changed-files', cwd),
  gitAdd: (cwd, files) => ipcRenderer.invoke('git:add', cwd, files),
  gitCommit: (cwd, message) => ipcRenderer.invoke('git:commit', cwd, message),
  gitPush: (cwd, remote, branch) => ipcRenderer.invoke('git:push', cwd, remote, branch),
  gitPull: (cwd) => ipcRenderer.invoke('git:pull', cwd),
  gitInit: (cwd) => ipcRenderer.invoke('git:init', cwd),
  gitConfigGet: (key) => ipcRenderer.invoke('git:config-get', key),
  gitConfigSet: (key, value) => ipcRenderer.invoke('git:config-set', key, value),

  // Shell
  shellOpenPath: (filePath) => ipcRenderer.invoke('shell:open-path', filePath),

  // App
  getAppVersion: () => ipcRenderer.invoke('app:version'),
  getPlatform: () => ipcRenderer.invoke('app:get-platform'),

  // OpenRouter
  openRouterSaveApiKey: (apiKey) => ipcRenderer.invoke('openrouter:save-api-key', apiKey),
  openRouterClearApiKey: () => ipcRenderer.invoke('openrouter:clear-api-key'),
  openRouterGetStatus: () => ipcRenderer.invoke('openrouter:status'),
  openRouterListModels: (forceRefresh) => ipcRenderer.invoke('openrouter:list-models', forceRefresh),
  openRouterGetSelectedModel: () => ipcRenderer.invoke('openrouter:get-selected-model'),
  openRouterSetSelectedModel: (modelId) => ipcRenderer.invoke('openrouter:set-selected-model', modelId),

  // Settings
  getSetting: (key) => ipcRenderer.invoke('settings:get', key),
  setSetting: (key, value) => ipcRenderer.invoke('settings:set', key, value),

  // GUI control
  onGuiAction: (callback) => {
    const listener = (_event: Electron.IpcRendererEvent, payload: unknown) => callback(payload)
    ipcRenderer.on('gui:action', listener)
    return () => ipcRenderer.removeListener('gui:action', listener)
  },

  // Clipboard
  clipboardReadText: () => clipboard.readText(),
  showTerminalContextMenu: (hasSelection) => ipcRenderer.invoke('context-menu:terminal', hasSelection),

  // Window controls
  windowMinimize: () => ipcRenderer.send('window:minimize'),
  windowMaximize: () => ipcRenderer.send('window:maximize'),
  windowClose: () => ipcRenderer.send('window:close'),
  windowSetFocusMode: (enabled) => ipcRenderer.send('window:set-focus-mode', enabled),
  windowPopOutProject: (projectName, engineId) => ipcRenderer.invoke('window:pop-out-project', projectName, engineId),
}

contextBridge.exposeInMainWorld('api', api)
