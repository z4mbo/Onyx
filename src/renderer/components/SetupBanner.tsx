import { useState, useEffect, useCallback, type ReactNode } from 'react'
import * as api from '@/lib/api'
import InstallEngineDialog from './InstallEngineDialog'

interface EngineInfo {
  id: string
  name: string
  isAvailable: boolean
}

type Platform = 'win32' | 'darwin' | 'linux'

const INSTALL_COMMANDS: Record<string, { win32: string; unix: string }> = {
  claude: {
    win32: 'irm https://claude.ai/install.ps1 | iex',
    unix: 'curl -fsSL https://claude.ai/install.sh | bash'
  },
  gemini: {
    win32: 'npm install -g @google/gemini-cli',
    unix: 'npm install -g @google/gemini-cli'
  },
  codex: {
    win32: 'npm install -g @openai/codex',
    unix: 'npm install -g @openai/codex'
  },
  kimi: {
    win32: 'irm https://code.kimi.com/kimi-code/install.ps1 | iex',
    unix: 'curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash'
  }
}

const ENGINE_ICONS: Record<string, ReactNode> = {
  claude: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 2L2 7l10 5 10-5-10-5z" />
      <path d="M2 17l10 5 10-5" />
      <path d="M2 12l10 5 10-5" />
    </svg>
  ),
  gemini: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
    </svg>
  ),
  codex: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="16 18 22 12 16 6" />
      <polyline points="8 6 2 12 8 18" />
    </svg>
  ),
  kimi: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M5 4v16" />
      <path d="M19 4 9 12l10 8" />
    </svg>
  )
}

export default function SetupBanner() {
  const [engines, setEngines] = useState<EngineInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [installing, setInstalling] = useState<{ id: string; name: string } | null>(null)
  const [showExplainer, setShowExplainer] = useState(false)
  const [platform, setPlatform] = useState<Platform>(() => navigator.userAgent.includes('Windows') ? 'win32' : navigator.userAgent.includes('Mac') ? 'darwin' : 'linux')

  const detect = useCallback(async () => {
    setLoading(true)
    try {
      const [result, currentPlatform] = await Promise.all([
        api.detectEngines(),
        api.getPlatform().catch((): Platform => navigator.userAgent.includes('Windows') ? 'win32' : navigator.userAgent.includes('Mac') ? 'darwin' : 'linux')
      ])
      setEngines(result as EngineInfo[])
      setPlatform(currentPlatform)
    } catch (err) {
      console.error('[SetupBanner] Failed to detect engines:', err)
    }
    setLoading(false)
  }, [])

  useEffect(() => {
    detect()
  }, [detect])

  // OpenRouter is a provider used through Kimi Code, not a standalone CLI.
  const cliEngines = engines.filter((e) => e.id !== 'openrouter')
  const missingEngines = cliEngines.filter((e) => !e.isAvailable)
  const noneFound = missingEngines.length === cliEngines.length && cliEngines.length > 0

  // Keep every installable assistant discoverable on first run. OpenRouter is
  // intentionally excluded above because it is configured as a provider.
  const displayedEngines = missingEngines

  if (loading || missingEngines.length === 0) return null

  return (
    <>
      <div className="mb-6 rounded-lg border border-win-border bg-win-card px-4 py-3">
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2 min-w-0">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="shrink-0 text-win-text-tertiary">
              <circle cx="12" cy="12" r="10" />
              <line x1="12" y1="8" x2="12" y2="12" />
              <line x1="12" y1="16" x2="12.01" y2="16" />
            </svg>
            <span className="text-xs text-win-text-secondary">
              {noneFound
                ? 'No AI engine found'
                : `${missingEngines.map((e) => e.name).join(' and ')} not found`}
            </span>
            {!noneFound && (
              <button
                onClick={() => setShowExplainer((v) => !v)}
                className="text-[11px] text-win-text-tertiary hover:text-win-text-secondary transition-colors"
              >
                {showExplainer ? 'Hide' : 'Why am I seeing this?'}
              </button>
            )}
          </div>
          <div className="flex items-center gap-2 shrink-0">
            {displayedEngines.map((engine) => (
              <button
                key={engine.id}
                onClick={() => setInstalling({ id: engine.id, name: engine.name })}
                className="text-xs text-win-text-secondary hover:text-win-text underline underline-offset-2 transition-colors"
              >
                Install {engine.name}
              </button>
            ))}
          </div>
        </div>

        {(showExplainer || noneFound) && (
          <div className="mt-3 border-t border-win-border pt-3 text-xs text-win-text-secondary leading-relaxed space-y-2">
            <p>
              zAI needs at least one AI coding assistant installed on your system. These command-line tools run locally on your machine.
            </p>
            <p>
              Choose from <strong className="text-win-text">Claude Code</strong>, <strong className="text-win-text">Gemini CLI</strong>, <strong className="text-win-text">Codex CLI</strong>, and <strong className="text-win-text">Kimi Code</strong>. OpenRouter is configured separately in Settings and runs through Kimi Code.
            </p>
            <p>
              Click "Install" above for step-by-step instructions. After installing, restart the app and this message will go away.
            </p>
          </div>
        )}
      </div>

      {installing && (
        <InstallEngineDialog
          engineId={installing.id}
          engineName={installing.name}
          installCommand={platform === 'win32'
            ? INSTALL_COMMANDS[installing.id].win32
            : INSTALL_COMMANDS[installing.id].unix}
          onClose={() => {
            setInstalling(null)
            detect()
          }}
        />
      )}
    </>
  )
}
