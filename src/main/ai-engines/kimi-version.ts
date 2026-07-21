import { win32 } from 'path'

interface ExecutableInvocation {
  executable: string
  args: string[]
}

/**
 * npm-installed CLIs resolve to .cmd shims on Windows, which execFile cannot
 * launch directly. The shim path is escaped as a single-quoted PowerShell
 * literal and then UTF-16LE/base64 encoded, so spaces and metacharacters in a
 * user profile cannot become command source or alter argv parsing.
 */
export function getKimiVersionInvocation(
  executable: string,
  platform: NodeJS.Platform = process.platform
): ExecutableInvocation {
  if (platform === 'win32' && /\.(?:cmd|bat)$/i.test(executable)) {
    const escapedExecutable = executable.replace(/'/g, "''")
    const script = `& '${escapedExecutable}' --version; exit $LASTEXITCODE`
    const windowsRoot = process.env.SystemRoot || process.env.WINDIR || 'C:\\Windows'
    return {
      executable: win32.join(windowsRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe'),
      args: [
        '-NoLogo',
        '-NoProfile',
        '-NonInteractive',
        '-EncodedCommand',
        Buffer.from(script, 'utf16le').toString('base64')
      ]
    }
  }
  return { executable, args: ['--version'] }
}

export function parseSemanticVersion(output: string): [number, number, number] | null {
  const match = output.match(/(?:^|[^0-9])(\d+)\.(\d+)\.(\d+)(?:[^0-9]|$)/)
  if (!match) return null
  return [Number(match[1]), Number(match[2]), Number(match[3])]
}

export function isVersionAtLeast(
  version: readonly number[],
  minimum: readonly number[]
): boolean {
  for (let index = 0; index < Math.max(version.length, minimum.length); index += 1) {
    const current = version[index] ?? 0
    const required = minimum[index] ?? 0
    if (current !== required) return current > required
  }
  return true
}
