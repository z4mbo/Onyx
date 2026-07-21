import type { EngineId } from '@/lib/constants'

export interface TerminalEntry {
  id: string
  /** PTY id returned from ptyCreate */
  ptyId: string | null
  name: string
  engine: EngineId
  isActive: boolean
  cwd: string
  /** Whether the terminal is still loading (engine starting) */
  isLoading: boolean
  /** Command intent used for the first CLI process launched in this PTY. */
  launchIntent?: 'start-session' | 'continue-session'
}

let nextId = 1
const windowUid = Math.random().toString(36).slice(2, 8)

export function generateTerminalId(): string {
  return `term-${windowUid}-${nextId++}`
}
