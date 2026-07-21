import type { ITerminalOptions } from '@xterm/xterm'

export const APP_NAME = 'zAI'

/**
 * Supported AI coding assistants.
 */
export const ENGINE_NAMES = {
  claude: 'Claude',
  gemini: 'Gemini',
  codex: 'Codex',
  kimi: 'Kimi Code',
  openrouter: 'OpenRouter'
} as const

export type EngineId = keyof typeof ENGINE_NAMES

/** Dot-color class for engine badges / indicators. */
export const ENGINE_COLORS: Record<EngineId, string> = {
  claude: 'bg-orange-400',
  gemini: 'bg-blue-400',
  codex: 'bg-green-400',
  kimi: 'bg-violet-400',
  openrouter: 'bg-fuchsia-400'
}

/** Text color class for engine-tinted icons. */
export const ENGINE_TEXT_COLORS: Record<EngineId, string> = {
  claude: 'text-orange-500',
  gemini: 'text-blue-500',
  codex: 'text-green-500',
  kimi: 'text-violet-500',
  openrouter: 'text-fuchsia-500'
}

/** Engine instruction / memory file names. */
export const ENGINE_MD_FILES: Record<EngineId, string> = {
  claude: 'CLAUDE.md',
  gemini: 'GEMINI.md',
  codex: 'AGENTS.md',
  kimi: 'AGENTS.md',
  openrouter: 'AGENTS.md'
}

/** Compact / compress slash-command per engine. */
export const ENGINE_COMPACT_CMD: Record<EngineId, string> = {
  claude: '/compact',
  gemini: '/compress',
  codex: '/compact',
  kimi: '/compact',
  openrouter: '/compact'
}

/** Engine config directory name (relative to project root). */
export const ENGINE_DIRS: Record<EngineId, string> = {
  claude: '.claude',
  gemini: '.gemini',
  codex: '.agents',
  kimi: '.kimi-code',
  openrouter: '.kimi-code'
}

/**
 * Default xterm.js terminal options.
 * Dark terminal for contrast against the light UI shell.
 */
export const DEFAULT_TERMINAL_OPTIONS: ITerminalOptions = {
  fontFamily: "'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace",
  fontSize: 14,
  fontWeight: '400',
  lineHeight: 1.3,
  cursorBlink: true,
  cursorStyle: 'bar',
  scrollback: 10_000,
  allowProposedApi: true,
  theme: {
    background: '#1E1E1E',
    foreground: '#D4D4D4',
    cursor: '#D4D4D4',
    cursorAccent: '#1E1E1E',
    selectionBackground: '#264F7840',
    selectionForeground: '#D4D4D4',
    black: '#1E1E1E',
    red: '#F44747',
    green: '#6A9955',
    yellow: '#D7BA7D',
    blue: '#569CD6',
    magenta: '#C586C0',
    cyan: '#4EC9B0',
    white: '#D4D4D4',
    brightBlack: '#808080',
    brightRed: '#F44747',
    brightGreen: '#6A9955',
    brightYellow: '#D7BA7D',
    brightBlue: '#569CD6',
    brightMagenta: '#C586C0',
    brightCyan: '#4EC9B0',
    brightWhite: '#FFFFFF'
  }
}

/**
 * Default sidebar width in pixels.
 */
export const DEFAULT_SIDEBAR_WIDTH = 280

/**
 * Min / max sidebar resize bounds.
 */
export const SIDEBAR_MIN_WIDTH = 200
export const SIDEBAR_MAX_WIDTH = 500
