import { app, BrowserWindow, screen, shell, ipcMain, Menu } from 'electron'
import { join } from 'path'
import { registerPtyIpc, killAllPty, killPtysForWindow, collectPtyIdsForWindow, killPtyById } from './pty/pty-ipc'
import { registerFsIpc, closeAllWatchers, closeWatchersForWindow } from './filesystem/fs-ipc'
import { registerProjectIpc } from './project/project-ipc'
import { detectEngines, getAvailableEngines } from './ai-engines/engine-registry'
import { getCommand, isInSessionCommand } from './ai-engines/command-dictionary'
import { registerConfigIpc } from './ai-engines/config-ipc'
import { startGuiServer, stopGuiServer } from './gui-control/gui-server'
import { registerGitIpc } from './git/git-ipc'
import { getProjectsDir } from './util/paths'
import { settingsStore } from './settings/settings-store'
import { registerOpenRouterIpc } from './openrouter/openrouter-ipc'
import { hydrateProcessExecutablePath } from './ai-engines/executable-resolver'
import {
  isRendererSettingKey,
  isValidRendererSettingValue
} from './settings/settings-policy'
import {
  isOwnedCanvasDocumentUrl,
  registerCanvasProtocol,
  registerCanvasScheme
} from './canvas/canvas-protocol'

registerCanvasScheme()

// Log uncaught exceptions to console instead of showing a dialog
process.on('uncaughtException', (error) => {
  console.error(`[UNCAUGHT] ${error.stack || error.message}`)
})

process.on('unhandledRejection', (reason) => {
  console.error(`[UNHANDLED REJECTION] ${reason instanceof Error ? reason.stack || reason.message : String(reason)}`)
})

let mainWindow: BrowserWindow | null = null

/** Store pre-focus-mode bounds so we can restore when exiting focus mode */
const preFocusBounds = new Map<BrowserWindow, Electron.Rectangle>()

function setupWindow(win: BrowserWindow, { maximize = true } = {}): void {
  // Keep developer shortcuts platform-native. Paste is intentionally not
  // intercepted here: form fields must retain normal clipboard behavior.
  win.webContents.on('before-input-event', (event, input) => {
    if (win.isDestroyed()) return
    if (input.type === 'keyDown') {
      // DevTools: F12 or Ctrl+Shift+I
      const devToolsShortcut =
        input.key === 'F12' ||
        (input.control && input.shift && input.key.toLowerCase() === 'i') ||
        (input.meta && input.alt && input.key.toLowerCase() === 'i')
      if (devToolsShortcut) {
        win.webContents.toggleDevTools()
        event.preventDefault()
        return
      }
    }
  })

  win.on('ready-to-show', () => {
    if (maximize) win.maximize()
    win.show()
  })

  // Debug: catch renderer crashes
  win.webContents.on('render-process-gone', (_event, details) => {
    console.error(`[window] render-process-gone: reason=${details.reason}, exitCode=${details.exitCode}`)
  })

  // Never let renderer or Canvas content create a child browsing context.
  // Trusted app links use same-frame anchors and are handled below.
  win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))

  win.webContents.on('will-frame-navigate', (event) => {
    if (event.isMainFrame) {
      event.preventDefault()
      try {
        const url = new URL(event.url)
        if (url.protocol === 'https:' || url.protocol === 'http:') {
          void shell.openExternal(url.toString())
        }
      } catch {
        // Invalid and non-web URLs remain blocked.
      }
      return
    }

    // A sandboxed Canvas may load/reload only a token issued to this renderer.
    if (!isOwnedCanvasDocumentUrl(event.url, win.webContents.id)) {
      event.preventDefault()
    }
  })

  // On Windows, ConPTY crashes (0xC000041D) if a PTY is alive during native
  // window destruction. For secondary windows, we intercept the close, kill
  // PTYs first, wait for ConPTY cleanup to finish, THEN destroy the window.
  let closingHandled = false

  win.on('close', (e) => {
    const remaining = BrowserWindow.getAllWindows().length
    const isSecondary = remaining > 1
    console.log(`[window] close — remaining: ${remaining}, secondary: ${isSecondary}, handled: ${closingHandled}`)

    // Clean up FS watchers owned by this window (always safe)
    closeWatchersForWindow(win)

    if (!isSecondary || closingHandled) {
      // Last window or already handled — just detach PTYs and let the close proceed.
      // Actual kill happens in window-all-closed / before-quit.
      console.log('[window] detaching PTYs and proceeding with close')
      killPtysForWindow(win, { detachOnly: true })
      return
    }

    // Secondary window: block close, kill PTYs first, then destroy after delay.
    e.preventDefault()
    closingHandled = true

    const ptyIds = collectPtyIdsForWindow(win)
    console.log(`[window] secondary — killing ${ptyIds.length} PTYs before destroying window`)

    // Kill PTYs while the window is still alive (avoids ConPTY crash)
    for (const id of ptyIds) {
      killPtyById(id)
    }
    // Remove from owner map so no more IPC goes to this window
    killPtysForWindow(win, { detachOnly: true })

    // Give ConPTY time to fully clean up, then destroy the window
    setTimeout(() => {
      console.log('[window] deferred destroy after PTY cleanup')
      if (!win.isDestroyed()) {
        win.destroy()
      }
    }, 300)
  })

  win.on('closed', () => {
    console.log('[window] closed event fired')
    if (win === mainWindow) mainWindow = null
    preFocusBounds.delete(win)
  })
}

function createBrowserWindow(): BrowserWindow {
  const isMac = process.platform === 'darwin'
  const iconFile = isMac ? 'logo.png' : 'icon.ico'

  return new BrowserWindow({
    width: 1200,
    height: 800,
    minWidth: 800,
    minHeight: 500,
    show: false,
    frame: isMac,
    ...(isMac
      ? {
          titleBarStyle: 'hiddenInset' as const,
          trafficLightPosition: { x: 14, y: 14 }
        }
      : {}),
    icon: app.isPackaged
      ? join(process.resourcesPath, iconFile)
      : join(__dirname, `../../resources/${iconFile}`),
    title: 'zAI',
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: false
    }
  })
}

function createWindow(): void {
  mainWindow = createBrowserWindow()
  setupWindow(mainWindow)

  // Load the renderer
  const isDev = !app.isPackaged
  if (isDev && process.env['ELECTRON_RENDERER_URL']) {
    mainWindow.loadURL(process.env['ELECTRON_RENDERER_URL'])
  } else {
    mainWindow.loadFile(join(__dirname, '../renderer/index.html'))
  }
}

/**
 * Registers IPC handlers that don't depend on the main window instance.
 */
function registerGlobalIpc(): void {
  // AI engine detection
  ipcMain.handle('engines:detect', async () => {
    return detectEngines()
  })

  ipcMain.handle('engines:available', async () => {
    return getAvailableEngines()
  })

  ipcMain.handle(
    'engines:command',
    async (_event, engineId: string, intent: string, params?: Record<string, string>) => {
      return getCommand(engineId, intent as Parameters<typeof getCommand>[1], params)
    }
  )

  ipcMain.handle('engines:is-in-session', async (_event, intent: string) => {
    return isInSessionCommand(intent as Parameters<typeof isInSessionCommand>[0])
  })

  // Shell
  ipcMain.handle('shell:open-path', async (_event, filePath: string) => {
    return shell.openPath(filePath)
  })

  // Terminal context menu (right-click)
  ipcMain.handle('context-menu:terminal', async (event, hasSelection: boolean) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (!win) return null

    return new Promise<string | null>((resolve) => {
      let resolved = false
      const items: Electron.MenuItemConstructorOptions[] = []

      if (hasSelection) {
        items.push({ label: 'Copy', click: () => { resolved = true; resolve('copy') } })
      }
      items.push({ label: 'Paste', click: () => { resolved = true; resolve('paste') } })

      const menu = Menu.buildFromTemplate(items)
      menu.popup({
        window: win,
        callback: () => { if (!resolved) resolve(null) }
      })
    })
  })

  // App info
  ipcMain.handle('app:version', () => app.getVersion())
  ipcMain.handle('app:get-platform', () => {
    if (process.platform === 'win32' || process.platform === 'darwin') return process.platform
    return 'linux'
  })

  // Settings
  ipcMain.handle('settings:get', async (_event, key: unknown) => {
    if (!isRendererSettingKey(key)) throw new Error('Unknown setting.')
    return settingsStore.get(key)
  })

  ipcMain.handle('settings:set', async (_event, key: unknown, value: unknown) => {
    if (!isRendererSettingKey(key) || !isValidRendererSettingValue(key, value)) {
      throw new Error('Invalid setting value.')
    }
    settingsStore.set(key, value)
  })

  // Pop out a project into its own window
  ipcMain.handle('window:pop-out-project', async (event, projectName: string, engineId: string) => {
    const senderWin = BrowserWindow.fromWebContents(event.sender)
    const newWin = createBrowserWindow()

    // Position the new window offset from the sender so both are visible
    if (senderWin && !senderWin.isDestroyed()) {
      const bounds = senderWin.getBounds()
      newWin.setBounds({
        x: bounds.x + 60,
        y: bounds.y + 60,
        width: bounds.width,
        height: bounds.height
      })
    }

    setupWindow(newWin, { maximize: false })

    const isDev = !app.isPackaged
    const query = `?popout=${encodeURIComponent(projectName)}&engine=${encodeURIComponent(engineId)}`
    if (isDev && process.env['ELECTRON_RENDERER_URL']) {
      newWin.loadURL(process.env['ELECTRON_RENDERER_URL'] + query)
    } else {
      newWin.loadFile(join(__dirname, '../renderer/index.html'), {
        search: query
      })
    }
  })

  // Window controls — use event.sender to target the correct window
  ipcMain.on('window:minimize', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.minimize()
  })

  ipcMain.on('window:maximize', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (win?.isMaximized()) {
      win.unmaximize()
    } else {
      win?.maximize()
    }
  })

  ipcMain.on('window:close', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.close()
  })

  // Focus mode — resize window to mobile-like dimensions or restore
  ipcMain.on('window:set-focus-mode', (event, enabled: boolean) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (!win) return

    if (enabled) {
      // Save current bounds before resizing
      preFocusBounds.set(win, win.getBounds())
      // If maximized, unmaximize first so setBounds works
      if (win.isMaximized()) win.unmaximize()
      // Mobile-like: 420px wide, 760px tall, centered on current display
      const workArea = screen.getDisplayMatching(win.getBounds()).workArea
      const focusW = 420
      const focusH = 760
      const x = workArea.x + Math.round((workArea.width - focusW) / 2)
      const y = workArea.y + Math.round((workArea.height - focusH) / 2)
      win.setMinimumSize(360, 500)
      win.setBounds({ x, y, width: focusW, height: focusH })
    } else {
      // Restore previous bounds
      const saved = preFocusBounds.get(win)
      win.setMinimumSize(800, 500)
      if (saved) {
        win.setBounds(saved)
        preFocusBounds.delete(win)
      }
    }
  })

}

function configureApplicationMenu(): void {
  if (process.platform !== 'darwin') {
    Menu.setApplicationMenu(null)
    return
  }

  const template: Electron.MenuItemConstructorOptions[] = [
    {
      role: 'appMenu',
      submenu: [
        { role: 'about' },
        { type: 'separator' },
        { role: 'services' },
        { type: 'separator' },
        { role: 'hide' },
        { role: 'hideOthers' },
        { role: 'unhide' },
        { type: 'separator' },
        { role: 'quit' }
      ]
    },
    {
      label: 'Edit',
      submenu: [
        { role: 'undo' },
        { role: 'redo' },
        { type: 'separator' },
        { role: 'cut' },
        { role: 'copy' },
        { role: 'paste' },
        { role: 'selectAll' }
      ]
    }
  ]
  Menu.setApplicationMenu(Menu.buildFromTemplate(template))
}

// --- App lifecycle ---

app.whenReady().then(async () => {
  // Finder/Dock launches omit the login-shell PATH. Hydrate before any terminals start.
  await hydrateProcessExecutablePath()

  // Keep normal macOS application behavior; other platforms use custom window chrome.
  configureApplicationMenu()

  // Canvas documents use their own sandboxed origin and restrictive CSP.
  registerCanvasProtocol()

  // Register IPC handlers that don't need the window
  registerGlobalIpc()
  registerProjectIpc()
  registerConfigIpc()
  registerGitIpc()
  registerOpenRouterIpc()

  // Create the main window
  createWindow()

  // Register IPC handlers that need the window reference
  if (mainWindow) {
    registerFsIpc(mainWindow)
    registerPtyIpc(mainWindow)
    startGuiServer(mainWindow).catch((err) => {
      console.error('[main] Failed to start GUI control server:', err)
    })
  }

  // macOS: re-create window when dock icon is clicked and no windows exist
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow()
      if (mainWindow) {
        registerPtyIpc(mainWindow)
        startGuiServer(mainWindow).catch((err) => {
          console.error('[main] Failed to restart GUI control server:', err)
        })
      }
    }
  })
})

app.on('window-all-closed', () => {
  // Kill all PTY processes and close watchers. macOS keeps the local GUI
  // server alive while the app remains resident so Dock reactivation works.
  killAllPty()
  closeAllWatchers()

  // On macOS, apps typically stay active until Cmd+Q
  if (process.platform !== 'darwin') {
    stopGuiServer()
    app.quit()
  }
})

app.on('before-quit', () => {
  killAllPty()
  stopGuiServer()
})
