import Store from 'electron-store'

export type SettingsSchema = {
  [key: string]: unknown
  defaultEngine: string
  sidebarWidth: number
  sidebarCollapsed: boolean
  rightPanelWidth: number
  terminalTheme: string
  terminalThemeCustom: unknown
  /** Base64-encoded safeStorage ciphertext. Never contains the plaintext key. */
  openRouterApiKeyEncrypted: string
  openRouterSelectedModel: string
}

/** Shared main-process settings store. */
export const settingsStore = new Store<SettingsSchema>({
  name: 'settings',
  defaults: {
    defaultEngine: 'claude',
    sidebarWidth: 280,
    sidebarCollapsed: false,
    rightPanelWidth: 400,
    terminalTheme: 'vscode-dark',
    terminalThemeCustom: null,
    openRouterApiKeyEncrypted: '',
    openRouterSelectedModel: ''
  }
})
