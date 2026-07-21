import { execFile } from 'child_process'
import { promisify } from 'util'
import { resolveExecutable } from './executable-resolver'
import {
  getKimiVersionInvocation,
  isVersionAtLeast,
  parseSemanticVersion
} from './kimi-version'

export { isVersionAtLeast, parseSemanticVersion } from './kimi-version'

const execFileAsync = promisify(execFile)
const MINIMUM_OPENROUTER_KIMI_VERSION = [0, 6, 0] as const

interface KimiCapability {
  displayVersion?: string
  supportsOpenRouter: boolean
}

export type AIEngineId = 'claude' | 'gemini' | 'codex' | 'kimi' | 'openrouter'

export interface AIEngine {
  id: AIEngineId
  name: string
  command: string
  detectCommand: string
  isAvailable: boolean
  version?: string
  availabilityReason?: string
}

const ENGINE_DEFINITIONS: Omit<AIEngine, 'isAvailable'>[] = [
  {
    id: 'claude',
    name: 'Claude Code',
    command: 'claude',
    detectCommand: 'claude'
  },
  {
    id: 'gemini',
    name: 'Gemini CLI',
    command: 'gemini',
    detectCommand: 'gemini'
  },
  {
    id: 'codex',
    name: 'Codex CLI',
    command: 'codex',
    detectCommand: 'codex'
  },
  {
    id: 'kimi',
    name: 'Kimi Code',
    command: 'kimi',
    detectCommand: 'kimi'
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    command: 'kimi',
    detectCommand: 'kimi'
  }
]

/**
 * Checks whether a command is available in the effective GUI/login PATH.
 */
async function isCommandAvailable(command: string): Promise<boolean> {
  return (await resolveExecutable(command)) !== null
}

async function detectKimiVersion(executable: string): Promise<KimiCapability> {
  try {
    const invocation = getKimiVersionInvocation(executable)
    const { stdout, stderr } = await execFileAsync(invocation.executable, invocation.args, {
      timeout: 5000,
      windowsHide: true,
      maxBuffer: 1024 * 1024
    })
    const version = parseSemanticVersion(`${stdout}\n${stderr}`)
    if (!version) return { supportsOpenRouter: false }
    return {
      displayVersion: version.join('.'),
      supportsOpenRouter: isVersionAtLeast(version, MINIMUM_OPENROUTER_KIMI_VERSION)
    }
  } catch {
    return { supportsOpenRouter: false }
  }
}

/**
 * Detects which AI engines are installed and available on the system.
 */
export async function detectEngines(): Promise<AIEngine[]> {
  const kimiExecutable = await resolveExecutable('kimi')
  const [otherAvailability, kimiCapability] = await Promise.all([
    Promise.all(
      ENGINE_DEFINITIONS
        .filter((definition) => definition.id !== 'kimi' && definition.id !== 'openrouter')
        .map(async (definition) => [definition.id, await isCommandAvailable(definition.detectCommand)] as const)
    ),
    kimiExecutable
      ? detectKimiVersion(kimiExecutable)
      : Promise.resolve<KimiCapability>({ supportsOpenRouter: false })
  ])
  const availableById = new Map<AIEngineId, boolean>(otherAvailability)

  return ENGINE_DEFINITIONS.map((definition) => {
    if (definition.id === 'kimi') {
      return {
        ...definition,
        isAvailable: kimiExecutable !== null,
        version: kimiCapability.displayVersion
      }
    }
    if (definition.id === 'openrouter') {
      return {
        ...definition,
        isAvailable: kimiExecutable !== null && kimiCapability.supportsOpenRouter,
        version: kimiCapability.displayVersion,
        availabilityReason: kimiExecutable === null
          ? 'Install Kimi Code first.'
          : kimiCapability.supportsOpenRouter
            ? undefined
            : 'OpenRouter requires Kimi Code 0.6.0 or newer.'
      }
    }
    return { ...definition, isAvailable: availableById.get(definition.id) ?? false }
  })
}

/**
 * Returns only the engines that are currently available.
 */
export async function getAvailableEngines(): Promise<AIEngine[]> {
  const engines = await detectEngines()
  return engines.filter((e) => e.isAvailable)
}
