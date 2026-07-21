import { WebSocketServer, WebSocket } from 'ws'
import { writeFile, unlink } from 'fs/promises'
import { join } from 'path'
import { app, BrowserWindow } from 'electron'

let wss: WebSocketServer | null = null
let serverPort = 0
let portFilePath: string | null = null
let fallbackWindow: BrowserWindow | null = null

const VALID_TABS = ['tips', 'agents', 'skills', 'mcps', 'canvas'] as const
type Tab = (typeof VALID_TABS)[number]

const VALID_CONNECTION_TYPES = ['sse', 'stdio'] as const

const VALID_CANVAS_MODES = ['panel', 'full', 'bottom'] as const

interface GuiAction {
  action: 'switch_tab' | 'open_panel' | 'close_panel' | 'add_connection' | 'set_canvas_mode'
  tab?: string
  mode?: string
  name?: string
  type?: string
  url?: string
  headers?: Record<string, string>
}

function getPortFilePath(): string {
  const userDataDir = app.getPath('userData')
  return join(userDataDir, '.gui-port')
}

/**
 * Starts the WebSocket GUI control server on a random port.
 * Writes the port number to a .gui-port file in userData.
 */
export async function startGuiServer(mainWindow: BrowserWindow): Promise<void> {
  // macOS keeps the local server alive when the last window closes. Refresh
  // the fallback whenever Dock activation creates a replacement window.
  fallbackWindow = mainWindow
  if (wss) return

  return new Promise((resolve, reject) => {
    wss = new WebSocketServer({ host: '127.0.0.1', port: 0, maxPayload: 64 * 1024 }, async () => {
      const addr = wss!.address()
      if (typeof addr === 'object' && addr) {
        serverPort = addr.port
        portFilePath = getPortFilePath()

        try {
          await writeFile(portFilePath, String(serverPort), 'utf-8')
          console.log(`[gui-server] WebSocket server started on 127.0.0.1:${serverPort}`)
          console.log(`[gui-server] Port file written to ${portFilePath}`)
        } catch (err) {
          console.error('[gui-server] Failed to write port file:', err)
        }
      }
      resolve()
    })

    wss.on('error', (err) => {
      console.error('[gui-server] WebSocket server error:', err)
      reject(err)
    })

    wss.on('connection', (ws: WebSocket, request) => {
      // Browser WebSocket handshakes always carry Origin. The bundled local
      // Node client does not, so reject web pages scanning localhost ports.
      if (request.headers.origin) {
        ws.close(1008, 'Browser origins are not allowed')
        return
      }
      console.log('[gui-server] Client connected')

      ws.on('message', (rawData) => {
        try {
          const data = JSON.parse(rawData.toString()) as GuiAction
          // Use the focused window or the most recently registered fallback.
          const targetWin = BrowserWindow.getFocusedWindow() || fallbackWindow
          if (!targetWin || targetWin.isDestroyed()) {
            ws.send(JSON.stringify({ success: false, error: 'No active window' }))
            return
          }
          const result = handleAction(data, targetWin)
          ws.send(JSON.stringify(result))
        } catch (err) {
          const errorMsg = err instanceof Error ? err.message : String(err)
          ws.send(JSON.stringify({ success: false, error: errorMsg }))
        }
      })

      ws.on('close', () => {
        console.log('[gui-server] Client disconnected')
      })
    })
  })
}

/**
 * Handles an incoming GUI action and forwards it to the renderer.
 */
function handleAction(
  data: GuiAction,
  mainWindow: BrowserWindow
): { success: boolean; error?: string } {
  switch (data.action) {
    case 'switch_tab': {
      if (!data.tab || !VALID_TABS.includes(data.tab as Tab)) {
        return { success: false, error: `Invalid tab "${data.tab}". Must be one of: ${VALID_TABS.join(', ')}` }
      }
      mainWindow.webContents.send('gui:action', { action: 'switch_tab', tab: data.tab })
      return { success: true }
    }

    case 'open_panel': {
      mainWindow.webContents.send('gui:action', { action: 'open_panel' })
      return { success: true }
    }

    case 'close_panel': {
      mainWindow.webContents.send('gui:action', { action: 'close_panel' })
      return { success: true }
    }

    case 'set_canvas_mode': {
      if (!data.mode || !VALID_CANVAS_MODES.includes(data.mode as (typeof VALID_CANVAS_MODES)[number])) {
        return { success: false, error: `Invalid canvas mode "${data.mode}". Must be one of: ${VALID_CANVAS_MODES.join(', ')}` }
      }
      mainWindow.webContents.send('gui:action', { action: 'set_canvas_mode', mode: data.mode })
      return { success: true }
    }

    case 'add_connection': {
      if (!data.name || typeof data.name !== 'string' || !data.name.trim()) {
        return { success: false, error: 'Connection name is required' }
      }
      if (!data.type || !VALID_CONNECTION_TYPES.includes(data.type as (typeof VALID_CONNECTION_TYPES)[number])) {
        return { success: false, error: `Invalid type "${data.type}". Must be one of: ${VALID_CONNECTION_TYPES.join(', ')}` }
      }
      if (!data.url || typeof data.url !== 'string' || !data.url.trim()) {
        return { success: false, error: 'URL/command is required' }
      }
      mainWindow.webContents.send('gui:action', {
        action: 'add_connection',
        name: data.name.trim(),
        type: data.type,
        url: data.url.trim(),
        headers: data.headers || {}
      })
      return { success: true }
    }

    default:
      return { success: false, error: `Unknown action "${data.action}"` }
  }
}

/**
 * Returns the port the GUI server is listening on.
 */
export function getGuiServerPort(): number {
  return serverPort
}

/**
 * Returns the path to the .gui-port file.
 */
export function getGuiPortFilePath(): string {
  return getPortFilePath()
}

/**
 * Stops the WebSocket server and cleans up the port file.
 */
export async function stopGuiServer(): Promise<void> {
  fallbackWindow = null
  if (wss) {
    // Close all connected clients
    for (const client of wss.clients) {
      client.close()
    }
    wss.close()
    wss = null
    serverPort = 0
    console.log('[gui-server] WebSocket server stopped')
  }

  if (portFilePath) {
    try {
      await unlink(portFilePath)
    } catch {
      // File may already be gone
    }
    portFilePath = null
  }
}
