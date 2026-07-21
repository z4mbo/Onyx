import { execFile } from 'child_process'
import { constants as fsConstants } from 'fs'
import { access, stat } from 'fs/promises'
import { homedir } from 'os'
import { basename, delimiter, isAbsolute, join } from 'path'
import { promisify } from 'util'

const execFileAsync = promisify(execFile)
const LOGIN_PATH_START = '__ZAI_LOGIN_PATH__'
const LOGIN_PATH_END = '__ZAI_LOGIN_PATH_END__'

let cachedLoginShellPath: string | null | undefined

function uniquePaths(paths: Array<string | undefined>): string[] {
  const seen = new Set<string>()
  const result: string[] = []

  for (const pathEntry of paths) {
    if (!pathEntry) continue
    const normalized = pathEntry.trim()
    if (!normalized) continue
    const key = process.platform === 'win32' ? normalized.toLowerCase() : normalized
    if (seen.has(key)) continue
    seen.add(key)
    result.push(normalized)
  }

  return result
}

function commonExecutableDirectories(): string[] {
  const home = homedir()

  if (process.platform === 'win32') {
    const userProfile = process.env.USERPROFILE || home
    const appData = process.env.APPDATA || join(userProfile, 'AppData', 'Roaming')
    const localAppData = process.env.LOCALAPPDATA || join(userProfile, 'AppData', 'Local')
    const programData = process.env.ProgramData || 'C:\\ProgramData'

    return uniquePaths([
      join(appData, 'npm'),
      join(localAppData, 'Microsoft', 'WindowsApps'),
      join(localAppData, 'Programs', 'Python', 'Scripts'),
      join(localAppData, 'pnpm'),
      join(userProfile, '.local', 'bin'),
      join(userProfile, 'scoop', 'shims'),
      join(userProfile, '.bun', 'bin'),
      join(userProfile, '.volta', 'bin'),
      join(programData, 'chocolatey', 'bin')
    ])
  }

  return uniquePaths([
    join(home, '.local', 'bin'),
    join(home, '.cargo', 'bin'),
    join(home, '.bun', 'bin'),
    join(home, '.volta', 'bin'),
    join(home, '.npm-global', 'bin'),
    join(home, 'Library', 'pnpm'),
    join(home, 'Library', 'Application Support', 'pnpm'),
    '/opt/homebrew/bin',
    '/opt/homebrew/sbin',
    '/usr/local/bin',
    '/usr/local/sbin',
    '/opt/local/bin',
    '/usr/bin',
    '/bin',
    '/usr/sbin',
    '/sbin'
  ])
}

async function readLoginShellPath(): Promise<string | null> {
  if (process.platform === 'win32') return null
  if (cachedLoginShellPath !== undefined) return cachedLoginShellPath

  const shell = process.env.SHELL || '/bin/zsh'
  const shellName = basename(shell)
  const supportedShells = new Set(['bash', 'zsh', 'sh', 'dash', 'ksh', 'fish'])
  if (!isAbsolute(shell) || !supportedShells.has(shellName)) {
    cachedLoginShellPath = null
    return null
  }

  try {
    // zsh/bash interactive startup files commonly initialize nvm, fnm, or asdf.
    // Keep the probe bounded, and pass only a fixed marker script to a validated
    // absolute shell path. Simpler POSIX shells stay login-only.
    const markerScript = shellName === 'fish'
      ? `printf '\\n${LOGIN_PATH_START}%s${LOGIN_PATH_END}\\n' (string join : $PATH)`
      : `printf '\\n${LOGIN_PATH_START}%s${LOGIN_PATH_END}\\n' "$PATH"`
    const shellArgs = shellName === 'fish'
      ? ['-l', '-i', '-c', markerScript]
      : ['bash', 'zsh'].includes(shellName)
        ? ['-ilc', markerScript]
        : ['-lc', markerScript]
    const { stdout } = await execFileAsync(
      shell,
      shellArgs,
      {
        timeout: 5000,
        windowsHide: true,
        maxBuffer: 1024 * 1024
      }
    )
    const output = String(stdout)
    const start = output.lastIndexOf(LOGIN_PATH_START)
    const end = output.indexOf(LOGIN_PATH_END, start + LOGIN_PATH_START.length)
    cachedLoginShellPath = start >= 0 && end > start
      ? output.slice(start + LOGIN_PATH_START.length, end).trim()
      : null
  } catch {
    cachedLoginShellPath = null
  }

  return cachedLoginShellPath
}

/**
 * Returns a PATH suitable for an app launched from Finder, the Dock, or Explorer.
 * macOS GUI apps do not inherit the user's login-shell PATH, so it is merged with
 * known package-manager locations. Windows user-level CLI install locations are
 * included as well.
 */
export async function getAugmentedExecutablePath(): Promise<string> {
  const loginPath = await readLoginShellPath()
  const entries = uniquePaths([
    ...(loginPath?.split(delimiter) ?? []),
    ...(process.env.PATH?.split(delimiter) ?? []),
    ...commonExecutableDirectories()
  ])
  return entries.join(delimiter)
}

/** Synchronous PATH variant used by PTY creation after startup hydration. */
export function getAugmentedExecutablePathSync(): string {
  const entries = uniquePaths([
    ...(cachedLoginShellPath?.split(delimiter) ?? []),
    ...(process.env.PATH?.split(delimiter) ?? []),
    ...commonExecutableDirectories()
  ])
  return entries.join(delimiter)
}

/** Hydrates process.env.PATH once so child shells can find GUI-installed CLIs. */
export async function hydrateProcessExecutablePath(): Promise<void> {
  process.env.PATH = await getAugmentedExecutablePath()
}

async function isExecutableFile(filePath: string): Promise<boolean> {
  try {
    const fileStat = await stat(filePath)
    if (!fileStat.isFile()) return false
    await access(
      filePath,
      process.platform === 'win32' ? fsConstants.F_OK : fsConstants.X_OK
    )
    return true
  } catch {
    return false
  }
}

function commandCandidates(command: string): string[] {
  if (process.platform !== 'win32') return [command]
  if (/\.[a-z0-9]+$/i.test(command)) return [command]

  const pathExt = process.env.PATHEXT?.split(';').filter(Boolean) ?? [
    '.COM',
    '.EXE',
    '.BAT',
    '.CMD'
  ]
  return [command, ...pathExt.map((extension) => `${command}${extension.toLowerCase()}`)]
}

/**
 * Resolves a real executable without interpolating the command into shell text.
 */
export async function resolveExecutable(command: string): Promise<string | null> {
  if (!/^[a-zA-Z0-9._+-]+$/.test(command)) return null

  if (isAbsolute(command)) {
    return (await isExecutableFile(command)) ? command : null
  }

  const searchPath = await getAugmentedExecutablePath()
  for (const directory of searchPath.split(delimiter)) {
    if (!directory) continue
    for (const candidate of commandCandidates(command)) {
      const filePath = join(directory, candidate)
      if (await isExecutableFile(filePath)) return filePath
    }
  }

  // `where.exe` also understands Windows App Execution Aliases. Arguments are
  // passed as an argv array, never through cmd.exe or a shell string.
  if (process.platform === 'win32') {
    try {
      const { stdout } = await execFileAsync('where.exe', [command], {
        timeout: 5000,
        windowsHide: true,
        env: { ...process.env, PATH: searchPath }
      })
      const firstMatch = String(stdout).split(/\r?\n/).find(Boolean)
      return firstMatch?.trim() || null
    } catch {
      return null
    }
  }

  return null
}
