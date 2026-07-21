import { execFile } from 'child_process'
import { homedir } from 'os'
import { basename, join } from 'path'
import { readdir, statfs } from 'fs/promises'
import { promisify } from 'util'

const execFileAsync = promisify(execFile)

export interface DiskInfo {
  name: string
  mount: string
  free: number
  size: number
}

/**
 * Lists useful filesystem roots on the current platform.
 * Windows returns mounted drive letters. macOS returns Home, filesystem root,
 * and mounted volumes. Other Unix platforms return Home and root.
 */
export async function listDisks(): Promise<DiskInfo[]> {
  if (process.platform === 'win32') return listWindowsDisks()
  if (process.platform === 'darwin') return listMacDisks()

  return Promise.all([
    diskInfo('Home', homedir()),
    diskInfo('Root', '/')
  ])
}

async function listWindowsDisks(): Promise<DiskInfo[]> {
  const psCommand = [
    'Get-PSDrive -PSProvider FileSystem',
    '| Where-Object { $_.Used -ne $null }',
    '| Select-Object Name, @{N="Label";E={(Get-Volume -DriveLetter $_.Name -ErrorAction SilentlyContinue).FileSystemLabel}}, Free, Used',
    '| ConvertTo-Json -Compress'
  ].join(' ')

  try {
    const { stdout } = await execFileAsync('powershell.exe', [
      '-NoProfile',
      '-NonInteractive',
      '-Command',
      psCommand
    ], { timeout: 10000 })

    const trimmed = stdout.trim()
    if (!trimmed) return []

    const parsed = JSON.parse(trimmed)
    // PowerShell returns a single object when there is one drive, array otherwise.
    const drives: Array<{
      Name: string
      Label: string | null
      Free: number
      Used: number
    }> = Array.isArray(parsed) ? parsed : [parsed]

    return drives.map((drive) => ({
      name: drive.Label || `Local Disk (${drive.Name}:)`,
      mount: `${drive.Name}:\\`,
      free: drive.Free ?? 0,
      size: (drive.Free ?? 0) + (drive.Used ?? 0)
    }))
  } catch (error) {
    console.error('Failed to list disks via PowerShell:', error)
    return [{ name: 'Local Disk (C:)', mount: 'C:\\', free: 0, size: 0 }]
  }
}

async function listMacDisks(): Promise<DiskInfo[]> {
  const roots: Array<{ name: string; mount: string }> = [
    { name: 'Home', mount: homedir() },
    { name: 'Macintosh HD', mount: '/' }
  ]

  try {
    const volumes = await readdir('/Volumes', { withFileTypes: true })
    for (const volume of volumes) {
      if (!volume.isDirectory() && !volume.isSymbolicLink()) continue
      const mount = join('/Volumes', volume.name)
      // /Volumes/Macintosh HD commonly points back to /. Keep the dedicated
      // root shortcut and omit that duplicate, while retaining external mounts.
      if (volume.name === 'Macintosh HD') continue
      roots.push({ name: basename(mount), mount })
    }
  } catch (error) {
    console.warn('Failed to list mounted macOS volumes:', error)
  }

  const disks = await Promise.all(
    roots.map(async ({ name, mount }) => {
      try {
        return await diskInfo(name, mount)
      } catch {
        return null
      }
    })
  )
  return disks.filter((disk): disk is DiskInfo => disk !== null)
}

async function diskInfo(name: string, mount: string): Promise<DiskInfo> {
  try {
    const stats = await statfs(mount)
    return {
      name,
      mount,
      free: stats.bavail * stats.bsize,
      size: stats.blocks * stats.bsize
    }
  } catch {
    return { name, mount, free: 0, size: 0 }
  }
}
