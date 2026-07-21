import { ipcMain } from 'electron'
import {
  clearOpenRouterApiKey,
  getOpenRouterStatus,
  getSelectedOpenRouterModel,
  listOpenRouterModels,
  saveOpenRouterApiKey,
  setSelectedOpenRouterModel
} from './openrouter-service'
import { killPtysForEngine } from '../pty/pty-ipc'

/** Registers OpenRouter credential, model discovery, and selection IPC. */
export function registerOpenRouterIpc(): void {
  ipcMain.handle('openrouter:save-api-key', async (_event, apiKey: string) => {
    const result = await saveOpenRouterApiKey(apiKey)
    if (result.success) killPtysForEngine('openrouter')
    return result
  })

  ipcMain.handle('openrouter:clear-api-key', async () => {
    const result = clearOpenRouterApiKey()
    if (result.success) killPtysForEngine('openrouter')
    return result
  })

  ipcMain.handle('openrouter:status', async () => {
    return getOpenRouterStatus()
  })

  ipcMain.handle('openrouter:list-models', async (_event, forceRefresh?: boolean) => {
    return listOpenRouterModels(forceRefresh === true)
  })

  ipcMain.handle('openrouter:get-selected-model', async () => {
    return getSelectedOpenRouterModel()
  })

  ipcMain.handle('openrouter:set-selected-model', async (_event, modelId: string) => {
    return setSelectedOpenRouterModel(modelId)
  })
}
