export type CommandIntent =
  | 'start-session'
  | 'continue-session'
  | 'add-dir'
  | 'add-file'
  | 'add-mcp'
  | 'list-mcp'
  | 'remove-mcp'
  | 'show-help'

export interface CommandParams {
  /** Working directory for session commands */
  cwd?: string
  /** Path to add as allowed directory */
  dirPath?: string
  /** Path to a file to add as context */
  filePath?: string
  /** MCP server name */
  mcpName?: string
  /** MCP server command */
  mcpCommand?: string
  /** Additional arguments */
  args?: string[]
}

interface CommandTemplate {
  /** The command template string. Use {param} for interpolation. */
  template: string
  /** Whether this is an in-session slash command (typed inside the REPL). */
  isInSession: boolean
}

/**
 * Command mappings per engine. Keys are engine IDs, values map intents to templates.
 */
const COMMAND_MAP: Record<string, Record<CommandIntent, CommandTemplate>> = {
  claude: {
    'start-session': {
      template: 'claude',
      isInSession: false
    },
    'continue-session': {
      template: 'claude --continue',
      isInSession: false
    },
    'add-dir': {
      template: '/add-dir {dirPath}',
      isInSession: true
    },
    'add-file': {
      template: '@{filePath}',
      isInSession: true
    },
    'add-mcp': {
      template: '/mcp add {mcpName} {mcpCommand}',
      isInSession: true
    },
    'list-mcp': {
      template: '/mcp',
      isInSession: true
    },
    'remove-mcp': {
      template: '/mcp remove {mcpName}',
      isInSession: true
    },
    'show-help': {
      template: '/help',
      isInSession: true
    }
  },
  gemini: {
    'start-session': {
      template: 'gemini',
      isInSession: false
    },
    'continue-session': {
      template: 'gemini',
      isInSession: false
    },
    'add-dir': {
      template: '/add-dir {dirPath}',
      isInSession: true
    },
    'add-file': {
      template: '@{filePath}',
      isInSession: true
    },
    'add-mcp': {
      template: '/mcp add {mcpName}',
      isInSession: true
    },
    'list-mcp': {
      template: '/mcp list',
      isInSession: true
    },
    'remove-mcp': {
      template: '/mcp remove {mcpName}',
      isInSession: true
    },
    'show-help': {
      template: '/help',
      isInSession: true
    }
  },
  codex: {
    'start-session': {
      template: 'codex',
      isInSession: false
    },
    'continue-session': {
      template: 'codex --resume',
      isInSession: false
    },
    'add-dir': {
      template: '/mention {dirPath}',
      isInSession: true
    },
    'add-file': {
      template: '@{filePath}',
      isInSession: true
    },
    'add-mcp': {
      template: '/mcp',
      isInSession: true
    },
    'list-mcp': {
      template: '/mcp',
      isInSession: true
    },
    'remove-mcp': {
      template: '/mcp',
      isInSession: true
    },
    'show-help': {
      template: '/help',
      isInSession: true
    }
  },
  kimi: {
    'start-session': {
      template: 'kimi',
      isInSession: false
    },
    'continue-session': {
      template: 'kimi --continue',
      isInSession: false
    },
    'add-dir': {
      template: '/add-dir {dirPath}',
      isInSession: true
    },
    'add-file': {
      template: '@{filePath}',
      isInSession: true
    },
    'add-mcp': {
      template: '/mcp-config',
      isInSession: true
    },
    'list-mcp': {
      template: '/mcp',
      isInSession: true
    },
    'remove-mcp': {
      template: '/mcp-config',
      isInSession: true
    },
    'show-help': {
      template: '/help',
      isInSession: true
    }
  },
  openrouter: {
    'start-session': {
      // Exit the provider-bearing shell as soon as Kimi exits so the API key
      // cannot remain available in a reusable general-purpose terminal.
      template: 'kimi; exit',
      isInSession: false
    },
    'continue-session': {
      template: 'kimi --continue; exit',
      isInSession: false
    },
    'add-dir': {
      template: '/add-dir {dirPath}',
      isInSession: true
    },
    'add-file': {
      template: '@{filePath}',
      isInSession: true
    },
    'add-mcp': {
      template: '/mcp-config',
      isInSession: true
    },
    'list-mcp': {
      template: '/mcp',
      isInSession: true
    },
    'remove-mcp': {
      template: '/mcp-config',
      isInSession: true
    },
    'show-help': {
      template: '/help',
      isInSession: true
    }
  }
}

/**
 * Interpolates a template string with the given params.
 * Replaces {key} placeholders with values from params.
 */
function interpolate(template: string, params?: CommandParams): string {
  if (!params) return template

  let result = template
  if (params.cwd) result = result.replace('{cwd}', params.cwd)
  if (params.dirPath) result = result.replace('{dirPath}', params.dirPath)
  if (params.filePath) result = result.replace('{filePath}', params.filePath)
  if (params.mcpName) result = result.replace('{mcpName}', params.mcpName)
  if (params.mcpCommand) result = result.replace('{mcpCommand}', params.mcpCommand)

  // Remove any unresolved placeholders
  result = result.replace(/\s*\{[^}]+\}/g, '')

  return result.trim()
}

/**
 * Returns the command string for a given engine and intent,
 * with optional parameter interpolation.
 */
export function getCommand(
  engineId: string,
  intent: CommandIntent,
  params?: CommandParams
): string {
  const engineMap = COMMAND_MAP[engineId]
  if (!engineMap) {
    throw new Error(`Unknown engine: ${engineId}`)
  }

  const entry = engineMap[intent]
  if (!entry) {
    throw new Error(`Unknown intent "${intent}" for engine "${engineId}"`)
  }

  return interpolate(entry.template, params)
}

/**
 * Returns true if the given intent is an in-session slash command
 * (should be typed into an existing REPL rather than spawned as a new process).
 */
export function isInSessionCommand(intent: CommandIntent): boolean {
  // All engines agree on which intents are in-session, so just check claude
  const entry = COMMAND_MAP['claude']?.[intent]
  return entry?.isInSession ?? false
}
