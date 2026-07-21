import { useEffect } from 'react'
import { useSettingsStore } from '@/stores/settings-store'
import { useProjectStore } from '@/stores/project-store'
import * as api from '@/lib/api'

type RightPanelTab = 'tips' | 'agents' | 'skills' | 'mcps' | 'canvas'
type CanvasMode = 'panel' | 'full' | 'bottom'

interface GuiActionPayload {
  action: 'switch_tab' | 'open_panel' | 'close_panel' | 'add_connection' | 'set_canvas_mode'
  tab?: RightPanelTab
  mode?: CanvasMode
  name?: string
  type?: 'sse' | 'stdio'
  url?: string
  headers?: Record<string, string>
}

/**
 * Listens for GUI control actions from the main process (forwarded from the MCP server)
 * and dispatches them to the settings store.
 */
export function useGuiActions(): void {
  useEffect(() => {
    const unsub = api.onGuiAction(async (rawPayload) => {
      const payload = rawPayload as GuiActionPayload
      const store = useSettingsStore.getState()

      switch (payload.action) {
        case 'switch_tab':
          if (payload.tab) {
            store.setRightPanelActiveTab(payload.tab)
          }
          break

        case 'open_panel':
        case 'close_panel':
          // Right panel is always active — no-op
          break

        case 'set_canvas_mode':
          if (payload.mode) {
            store.setCanvasMode(payload.mode)
            if (payload.mode === 'panel') {
              store.setRightPanelActiveTab('canvas')
            }
          }
          break

        case 'add_connection': {
          const project = useProjectStore.getState().activeProject
          if (!project) {
            console.warn('[useGuiActions] No active project, cannot add connection')
            break
          }
          if (!payload.name || !payload.type || !payload.url) break

          try {
            const env: Record<string, string> = {}
            if (payload.headers) {
              for (const [key, value] of Object.entries(payload.headers)) {
                if (key.trim()) {
                  env[`MCP_HEADER_${key.toUpperCase().replace(/[^A-Z0-9_]/g, '_')}`] = value
                }
              }
            }

            const server: api.McpServer =
              payload.type === 'sse'
                ? {
                    command: 'npx',
                    args: ['-y', 'mcp-remote', payload.url],
                    env: Object.keys(env).length > 0 ? env : undefined
                  }
                : {
                    command: payload.url,
                    args: [],
                    env: Object.keys(env).length > 0 ? env : undefined
                  }

            await api.addMcpServer(project.name, payload.name, server)

            // Switch to Connections tab and notify MCPPanel to refresh
            store.setRightPanelActiveTab('mcps')
            window.dispatchEvent(new CustomEvent('mcp-connections-changed'))
          } catch (err) {
            console.error('[useGuiActions] Failed to add connection:', err)
          }
          break
        }
      }
    })

    return unsub
  }, [])
}
