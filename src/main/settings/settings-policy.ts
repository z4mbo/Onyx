const ENGINE_IDS = new Set(['claude', 'gemini', 'codex', 'kimi', 'openrouter'])

export const RENDERER_SETTING_KEYS = [
  'defaultEngine',
  'sidebarWidth',
  'sidebarCollapsed',
  'rightPanelWidth',
  'terminalTheme',
  'terminalThemeCustom'
] as const

export type RendererSettingKey = typeof RENDERER_SETTING_KEYS[number]

const RENDERER_SETTING_KEY_SET = new Set<string>(RENDERER_SETTING_KEYS)

export function isRendererSettingKey(value: unknown): value is RendererSettingKey {
  return typeof value === 'string' && RENDERER_SETTING_KEY_SET.has(value)
}

function isThemeObject(value: unknown): boolean {
  if (value === null) return true
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false
  const entries = Object.entries(value as Record<string, unknown>)
  return entries.length <= 64 && entries.every(([key, color]) =>
    key.length > 0 && key.length <= 64 && typeof color === 'string' && color.length <= 64
  )
}

export function isValidRendererSettingValue(key: RendererSettingKey, value: unknown): boolean {
  switch (key) {
    case 'defaultEngine':
      return typeof value === 'string' && ENGINE_IDS.has(value)
    case 'sidebarWidth':
      return Number.isInteger(value) && Number(value) >= 160 && Number(value) <= 1200
    case 'rightPanelWidth':
      return Number.isInteger(value) && Number(value) >= 240 && Number(value) <= 1600
    case 'sidebarCollapsed':
      return typeof value === 'boolean'
    case 'terminalTheme':
      return typeof value === 'string' && /^[a-zA-Z0-9_-]{1,64}$/.test(value)
    case 'terminalThemeCustom':
      return isThemeObject(value)
  }
}
