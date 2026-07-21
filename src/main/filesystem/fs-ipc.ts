import { ipcMain, BrowserWindow } from 'electron'
import { stat, readFile, writeFile, mkdir } from 'fs/promises'
import { watch, type FSWatcher } from 'fs'
import { dirname, join, resolve } from 'path'
import { listDisks } from './disk-service'
import { readDirectory } from './tree-service'
import { resolveExistingPathWithinRoot } from './path-containment'

interface WatchSubscriber {
  window: BrowserWindow
  rootPath: string
  count: number
}

interface WatchRecord {
  watcher: FSWatcher
  directory: string
  subscribers: Map<number, WatchSubscriber>
  debounceTimer: ReturnType<typeof setTimeout> | null
}

const watchRecords = new Map<string, WatchRecord>()

function watchKey(dirPath: string): string {
  const absolutePath = resolve(dirPath)
  return process.platform === 'win32' ? absolutePath.toLowerCase() : absolutePath
}

function closeWatchRecord(key: string, record: WatchRecord): void {
  if (record.debounceTimer) clearTimeout(record.debounceTimer)
  record.watcher.close()
  watchRecords.delete(key)
}

/**
 * Registers all filesystem-related IPC handlers.
 * Call once. Uses event.sender to route watch events to the correct window.
 */
export function registerFsIpc(_mainWindow: BrowserWindow): void {
  // Guard against double-registration (second window)
  if ((registerFsIpc as { _registered?: boolean })._registered) return
  ;(registerFsIpc as { _registered?: boolean })._registered = true

  ipcMain.handle('fs:list-disks', async () => {
    console.log('[fs:list-disks] listing drives')
    return listDisks()
  })

  ipcMain.handle('fs:read-dir', async (_event, dirPath: string) => {
    return readDirectory(dirPath)
  })

  ipcMain.handle('fs:stat', async (_event, filePath: string) => {
    try {
      const info = await stat(filePath)
      return {
        exists: true,
        isDirectory: info.isDirectory(),
        isFile: info.isFile(),
        size: info.size,
        modified: info.mtime.toISOString(),
        created: info.birthtime.toISOString()
      }
    } catch (err) {
      const code = (err as NodeJS.ErrnoException).code
      if (code === 'ENOENT') {
        return { exists: false }
      }
      console.error(`[fs:stat] FAILED "${filePath}": ${(err as Error).message}`)
      throw err
    }
  })

  ipcMain.handle('fs:read-file', async (_event, filePath: string) => {
    try {
      return await readFile(filePath, 'utf-8')
    } catch (err) {
      const code = (err as NodeJS.ErrnoException).code
      if (code === 'ENOENT') return null
      console.error(`[fs:read-file] FAILED "${filePath}": ${(err as Error).message}`)
      throw err
    }
  })

  // Canvas HTML is project-controlled script. Its bridge may only read existing
  // files beneath the active project, including after symlinks are resolved.
  ipcMain.handle(
    'fs:canvas-read-file',
    async (_event, projectRoot: string, relativePath: string) => {
      const filePath = await resolveExistingPathWithinRoot(projectRoot, relativePath)
      const info = await stat(filePath)
      if (!info.isFile()) throw new Error('Canvas requested a path that is not a file.')
      if (info.size > 2 * 1024 * 1024) throw new Error('Canvas files are limited to 2 MB.')
      return readFile(filePath, 'utf-8')
    }
  )

  ipcMain.handle(
    'fs:canvas-read-dir',
    async (_event, projectRoot: string, relativePath: string) => {
      const dirPath = await resolveExistingPathWithinRoot(projectRoot, relativePath)
      const info = await stat(dirPath)
      if (!info.isDirectory()) throw new Error('Canvas requested a path that is not a directory.')
      return (await readDirectory(dirPath)).slice(0, 2_000)
    }
  )

  ipcMain.handle('fs:write-file', async (_event, filePath: string, content: string) => {
    try {
      await mkdir(dirname(filePath), { recursive: true })
      await writeFile(filePath, content, 'utf-8')
    } catch (err) {
      console.error(`[fs:write-file] FAILED "${filePath}": ${(err as Error).message}`)
      throw err
    }
  })

  // ---- Filesystem watcher ---------------------------------------------------

  ipcMain.handle('fs:watch', async (event, dirPath: string) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender)
    if (!senderWindow || senderWindow.isDestroyed()) return
    const directory = resolve(dirPath)
    const key = watchKey(directory)
    const existingRecord = watchRecords.get(key)
    if (existingRecord) {
      const existingSubscriber = existingRecord.subscribers.get(senderWindow.id)
      if (existingSubscriber) existingSubscriber.count += 1
      else {
        existingRecord.subscribers.set(senderWindow.id, {
          window: senderWindow,
          rootPath: dirPath,
          count: 1
        })
      }
      return
    }

    try {
      const subscribers = new Map<number, WatchSubscriber>([
        [senderWindow.id, { window: senderWindow, rootPath: dirPath, count: 1 }]
      ])
      let record: WatchRecord
      const watcher = watch(directory, { recursive: true }, (_eventType, filename) => {
        // Debounce: batch rapid changes into a single notification
        if (record.debounceTimer) clearTimeout(record.debounceTimer)
        record.debounceTimer = setTimeout(() => {
          const changedDir = filename
            ? dirname(join(record.directory, filename.toString()))
            : record.directory
          for (const [windowId, subscriber] of record.subscribers) {
            if (subscriber.window.isDestroyed()) {
              record.subscribers.delete(windowId)
              continue
            }
            try {
              subscriber.window.webContents.send(
                'fs:changed',
                subscriber.rootPath,
                changedDir
              )
            } catch {
              record.subscribers.delete(windowId)
            }
          }
        }, 300)
      })

      record = { watcher, directory, subscribers, debounceTimer: null }

      watcher.on('error', (err) => {
        console.warn(`[fs:watch] watcher error for "${directory}": ${err.message}`)
        if (watchRecords.get(key) === record) closeWatchRecord(key, record)
      })

      watchRecords.set(key, record)
      console.log(`[fs:watch] watching "${directory}"`)
    } catch (err) {
      console.error(`[fs:watch] FAILED to watch "${directory}": ${(err as Error).message}`)
    }
  })

  ipcMain.handle('fs:unwatch', async (event, dirPath: string) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender)
    if (!senderWindow) return
    const key = watchKey(dirPath)
    const record = watchRecords.get(key)
    if (!record) return
    const subscriber = record.subscribers.get(senderWindow.id)
    if (!subscriber) return
    subscriber.count -= 1
    if (subscriber.count <= 0) record.subscribers.delete(senderWindow.id)
    if (record.subscribers.size === 0) {
      closeWatchRecord(key, record)
      console.log(`[fs:unwatch] stopped watching "${record.directory}"`)
    }
  })
}

/**
 * Close all watchers. Call during app shutdown.
 */
export function closeAllWatchers(): void {
  for (const [key, record] of watchRecords) {
    closeWatchRecord(key, record)
  }
}

/**
 * Close watchers owned by a specific window. Call when a window closes.
 */
export function closeWatchersForWindow(win: BrowserWindow): void {
  for (const [key, record] of watchRecords) {
    if (!record.subscribers.delete(win.id)) continue
    if (record.subscribers.size === 0) {
      closeWatchRecord(key, record)
      console.log(`[fs-ipc] closed watcher for "${record.directory}" (last window closed)`)
    }
  }
}
