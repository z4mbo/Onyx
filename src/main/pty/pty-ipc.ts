import { ipcMain, BrowserWindow } from 'electron'
import { PtyManager, PtySpawnOptions } from './pty-manager'
import {
  getOpenRouterPtyEnvironment,
  sanitizeOpenRouterError
} from '../openrouter/openrouter-service'
import { detectEngines } from '../ai-engines/engine-registry'

const ptyManager = new PtyManager()

/** Map pty id → the BrowserWindow that owns it, so events route to the right window */
const ptyOwners = new Map<string, BrowserWindow>()
const ptyEngines = new Map<string, PtySpawnOptions['engineId']>()
const pendingPtySpawns = new Map<
  string,
  { token: symbol; owner: BrowserWindow; engineId: PtySpawnOptions['engineId'] }
>()

/**
 * Registers all PTY-related IPC handlers.
 * Call once. Uses event.sender to route data back to the originating window.
 */
export function registerPtyIpc(_mainWindow: BrowserWindow): void {
  // Guard against double-registration (second window)
  if ((registerPtyIpc as { _registered?: boolean })._registered) return
  ;(registerPtyIpc as { _registered?: boolean })._registered = true

  ipcMain.handle(
    'pty:spawn',
    async (
      event,
      id: string,
      options?: PtySpawnOptions
    ): Promise<{ success: boolean; error?: string }> => {
      const spawnOptions: PtySpawnOptions = options ?? {}
      const initialOwner = BrowserWindow.fromWebContents(event.sender)
      if (!initialOwner || initialOwner.isDestroyed()) {
        return { success: false, error: 'The terminal window is no longer available.' }
      }
      const spawnToken = Symbol(id)
      pendingPtySpawns.set(id, {
        token: spawnToken,
        owner: initialOwner,
        engineId: spawnOptions.engineId
      })
      try {
        if (spawnOptions.engineId === 'openrouter') {
          const engine = (await detectEngines()).find(({ id }) => id === 'openrouter')
          if (!engine?.isAvailable) {
            return {
              success: false,
              error: engine?.availabilityReason || 'OpenRouter requires a supported Kimi Code installation.'
            }
          }
          const openRouterEnvironment = await getOpenRouterPtyEnvironment()
          // Main-process values win so a renderer can never override or read the secret.
          spawnOptions.env = {
            ...(spawnOptions.env ?? {}),
            ...openRouterEnvironment
          }
        }

        // Provider setup may await network/model checks. Re-resolve ownership
        // immediately before spawn so a closed window cannot leave a headless
        // credential-bearing terminal behind.
        const senderWindow = BrowserWindow.fromWebContents(event.sender)
        const pendingSpawn = pendingPtySpawns.get(id)
        if (pendingSpawn?.token !== spawnToken) {
          return { success: false, error: 'Terminal launch was cancelled.' }
        }
        if (!senderWindow || senderWindow.isDestroyed() || senderWindow !== pendingSpawn.owner) {
          return { success: false, error: 'The terminal window was closed before launch.' }
        }
        ptyOwners.set(id, senderWindow)
        ptyEngines.set(id, spawnOptions.engineId)

        ptyManager.spawn(
          id,
          spawnOptions,
          (data: string) => {
            try {
              const win = ptyOwners.get(id)
              if (win && !win.isDestroyed()) {
                win.webContents.send('pty:data', id, data)
              }
            } catch {
              // Window may have been destroyed between the check and the send
            }
          },
          (exitCode: number, signal?: number) => {
            console.log(`[pty:exit] id="${id}" exitCode=${exitCode} signal=${signal}`)
            try {
              const win = ptyOwners.get(id)
              if (win && !win.isDestroyed()) {
                win.webContents.send('pty:exit', id, exitCode, signal)
              }
            } catch {
              // Window may have been destroyed between the check and the send
            }
            ptyOwners.delete(id)
            ptyEngines.delete(id)
          }
        )
        return { success: true }
      } catch (err) {
        const message = spawnOptions.engineId === 'openrouter'
          ? sanitizeOpenRouterError(err)
          : 'The terminal process could not be started.'
        console.error(`[pty:spawn] Failed to start PTY "${id}".`)
        ptyOwners.delete(id)
        ptyEngines.delete(id)
        return { success: false, error: message }
      } finally {
        if (pendingPtySpawns.get(id)?.token === spawnToken) {
          pendingPtySpawns.delete(id)
        }
      }
    }
  )

  ipcMain.on('pty:write', (_event, id: string, data: string) => {
    ptyManager.write(id, data)
  })

  ipcMain.on('pty:resize', (_event, id: string, cols: number, rows: number) => {
    ptyManager.resize(id, cols, rows)
  })

  ipcMain.handle(
    'pty:kill',
    async (_event, id: string): Promise<void> => {
      console.log(`[pty:kill] id="${id}"`)
      pendingPtySpawns.delete(id)
      ptyManager.kill(id)
      ptyOwners.delete(id)
      ptyEngines.delete(id)
    }
  )
}

/**
 * Kills all PTY processes. Call during app shutdown.
 */
export function killAllPty(): void {
  pendingPtySpawns.clear()
  ptyManager.killAll()
  ptyOwners.clear()
  ptyEngines.clear()
}

/** Terminates all PTYs launched for an engine and tells their renderers. */
export function killPtysForEngine(engineId: PtySpawnOptions['engineId']): void {
  for (const [id, pendingSpawn] of pendingPtySpawns) {
    if (pendingSpawn.engineId === engineId) pendingPtySpawns.delete(id)
  }
  const ids = [...ptyEngines.entries()]
    .filter(([, currentEngineId]) => currentEngineId === engineId)
    .map(([id]) => id)

  for (const id of ids) {
    const owner = ptyOwners.get(id)
    ptyManager.kill(id)
    ptyOwners.delete(id)
    ptyEngines.delete(id)
    if (owner && !owner.isDestroyed()) {
      owner.webContents.send('pty:exit', id, -1)
    }
  }
}

/**
 * Returns PTY IDs owned by a specific window (without modifying anything).
 */
export function collectPtyIdsForWindow(win: BrowserWindow): string[] {
  const ids: string[] = []
  for (const [id, owner] of ptyOwners) {
    if (owner === win) ids.push(id)
  }
  return ids
}

/**
 * Detaches or kills PTY processes owned by a specific window.
 * When detachOnly is true, callbacks are nulled but PTY processes stay alive.
 */
export function killPtysForWindow(win: BrowserWindow, { detachOnly = false } = {}): void {
  for (const [id, pendingSpawn] of pendingPtySpawns) {
    if (pendingSpawn.owner === win) pendingPtySpawns.delete(id)
  }
  const idsToProcess = collectPtyIdsForWindow(win)
  if (idsToProcess.length === 0) return

  console.log(`[pty-ipc] killPtysForWindow: ${detachOnly ? 'detaching' : 'killing'} ${idsToProcess.length} PTYs: ${idsToProcess.join(', ')}`)

  for (const id of idsToProcess) {
    ptyOwners.delete(id)
    ptyEngines.delete(id)
    if (detachOnly) {
      ptyManager.detach(id)
    } else {
      ptyManager.kill(id)
    }
  }
}

/**
 * Kill a single PTY by id. Used for deferred kills after window destruction.
 */
export function killPtyById(id: string): void {
  pendingPtySpawns.delete(id)
  ptyManager.kill(id)
  ptyOwners.delete(id)
  ptyEngines.delete(id)
}
