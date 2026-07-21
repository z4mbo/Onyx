import { useEffect, useState, useRef, useCallback } from 'react'
import { useSettingsStore } from '@/stores/settings-store'
import { ENGINE_NAMES, type EngineId } from '@/lib/constants'
import * as api from '@/lib/api'
import TerminalThemeSection from './TerminalThemeSection'
import OpenRouterProviderSection from './OpenRouterProviderSection'

export default function SettingsDialog() {
  const show = useSettingsStore((s) => s.showSettingsDialog)
  const setShow = useSettingsStore((s) => s.setShowSettingsDialog)
  const defaultEngine = useSettingsStore((s) => s.defaultEngine)
  const updateSetting = useSettingsStore((s) => s.updateSetting)

  const [activeSection, setActiveSection] = useState<'general' | 'providers' | 'terminal' | 'git' | 'about'>('general')
  const [gitName, setGitName] = useState('')
  const [gitEmail, setGitEmail] = useState('')
  const [gitAvailable, setGitAvailable] = useState<boolean | null>(null)
  const [appVersion, setAppVersion] = useState('')

  const nameTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const emailTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  // Load git config + app version on open
  useEffect(() => {
    if (!show) return
    api.gitAvailable().then(setGitAvailable).catch(() => setGitAvailable(false))
    api.gitConfigGet('user.name').then((v) => setGitName(v ?? '')).catch(() => {})
    api.gitConfigGet('user.email').then((v) => setGitEmail(v ?? '')).catch(() => {})
    api.getAppVersion().then(setAppVersion).catch(() => setAppVersion('unknown'))
  }, [show])

  // Close on Escape
  useEffect(() => {
    if (!show) return
    const handler = (e: KeyboardEvent) => { if (e.key === 'Escape') setShow(false) }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [show, setShow])

  // Debounced git config saves
  const handleNameChange = useCallback((value: string) => {
    setGitName(value)
    if (nameTimer.current) clearTimeout(nameTimer.current)
    nameTimer.current = setTimeout(() => {
      if (value.trim()) api.gitConfigSet('user.name', value.trim()).catch(() => {})
    }, 500)
  }, [])

  const handleEmailChange = useCallback((value: string) => {
    setGitEmail(value)
    if (emailTimer.current) clearTimeout(emailTimer.current)
    emailTimer.current = setTimeout(() => {
      if (value.trim()) api.gitConfigSet('user.email', value.trim()).catch(() => {})
    }, 500)
  }, [])

  const handleDefaultEngineChange = useCallback(async (engine: EngineId) => {
    if (engine === 'openrouter') {
      try {
        const [status, detectedEngines] = await Promise.all([
          api.openRouterGetStatus(),
          api.detectEngines()
        ])
        const openRouterAvailable = detectedEngines
          .some((item) => item.id === 'openrouter' && item.isAvailable)
        if (!status.hasApiKey || !status.selectedModelId || !openRouterAvailable) {
          setActiveSection('providers')
          return
        }
      } catch {
        setActiveSection('providers')
        return
      }
    }
    await updateSetting('defaultEngine', engine)
  }, [updateSetting])

  if (!show) return null

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm"
      onClick={(e) => { if (e.target === e.currentTarget) setShow(false) }}
    >
      <div className="w-full max-w-2xl rounded-lg border border-win-border bg-win-card shadow-xl">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-win-border px-5 py-3.5">
          <h2 className="text-sm font-semibold text-win-text">Settings</h2>
          <button
            onClick={() => setShow(false)}
            className="flex h-6 w-6 items-center justify-center rounded-md text-win-text-tertiary hover:bg-win-hover hover:text-win-text transition-colors"
          >
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
              <line x1="1" y1="1" x2="11" y2="11" />
              <line x1="11" y1="1" x2="1" y2="11" />
            </svg>
          </button>
        </div>

        {/* Section tabs */}
        <div className="flex border-b border-win-border bg-win-surface">
          {(['general', 'providers', 'terminal', 'git', 'about'] as const).map((section) => (
            <button
              key={section}
              onClick={() => setActiveSection(section)}
              className={`relative px-4 py-2.5 text-xs font-medium capitalize transition-colors ${
                activeSection === section
                  ? 'text-win-accent'
                  : 'text-win-text-secondary hover:text-win-text'
              }`}
            >
              {section}
              {activeSection === section && (
                <div className="absolute bottom-0 left-3 right-3 h-[2px] rounded-full bg-win-accent" />
              )}
            </button>
          ))}
        </div>

        {/* Content */}
        <div className="p-5 min-h-[200px] max-h-[60vh] overflow-y-auto">
          {activeSection === 'general' && (
            <div className="space-y-4">
              <div>
                <label className="block text-xs font-medium text-win-text-secondary mb-1.5">
                  Default AI Engine
                </label>
                <select
                  value={defaultEngine}
                  onChange={(e) => void handleDefaultEngineChange(e.target.value as EngineId)}
                  className="w-full rounded border border-win-border bg-win-surface px-3 py-2 text-sm text-win-text outline-none focus:border-win-accent"
                >
                  {Object.entries(ENGINE_NAMES).map(([id, name]) => (
                    <option key={id} value={id}>{name}</option>
                  ))}
                </select>
                <p className="mt-1 text-[10px] text-win-text-tertiary">
                  New sessions will use this engine by default. OpenRouter requires Kimi Code plus a connected provider and selected model.
                </p>
              </div>
            </div>
          )}

          {activeSection === 'terminal' && (
            <TerminalThemeSection />
          )}

          {activeSection === 'providers' && (
            <OpenRouterProviderSection />
          )}

          {activeSection === 'git' && (
            <div className="space-y-4">
              {/* Git status */}
              <div className="flex items-center gap-2">
                {gitAvailable === true ? (
                  <>
                    <div className="h-2 w-2 rounded-full bg-green-400" />
                    <span className="text-xs text-win-text-secondary">Git detected</span>
                  </>
                ) : gitAvailable === false ? (
                  <>
                    <div className="h-2 w-2 rounded-full bg-red-400" />
                    <span className="text-xs text-win-text-secondary">Git not found</span>
                    <a
                      href="https://git-scm.com/downloads"
                      rel="noopener noreferrer"
                      className="text-xs text-win-accent hover:underline ml-1"
                    >
                      Install
                    </a>
                  </>
                ) : (
                  <span className="text-xs text-win-text-tertiary">Checking...</span>
                )}
              </div>

              {/* Git user config */}
              <div>
                <label className="block text-xs font-medium text-win-text-secondary mb-1.5">
                  user.name
                </label>
                <input
                  type="text"
                  value={gitName}
                  onChange={(e) => handleNameChange(e.target.value)}
                  placeholder="Your Name"
                  className="w-full rounded border border-win-border bg-win-surface px-3 py-2 text-sm text-win-text placeholder:text-win-text-tertiary outline-none focus:border-win-accent"
                />
              </div>

              <div>
                <label className="block text-xs font-medium text-win-text-secondary mb-1.5">
                  user.email
                </label>
                <input
                  type="text"
                  value={gitEmail}
                  onChange={(e) => handleEmailChange(e.target.value)}
                  placeholder="you@example.com"
                  className="w-full rounded border border-win-border bg-win-surface px-3 py-2 text-sm text-win-text placeholder:text-win-text-tertiary outline-none focus:border-win-accent"
                />
              </div>

              <p className="text-[10px] text-win-text-tertiary">
                These are saved to your global git config (~/.gitconfig)
              </p>
            </div>
          )}

          {activeSection === 'about' && (
            <div className="space-y-4">
              <div>
                <p className="text-xs text-win-text-tertiary">Version</p>
                <p className="text-sm font-medium text-win-text">{appVersion || '...'}</p>
              </div>

              <div className="rounded-md border border-win-border bg-win-surface p-3">
                <p className="text-xs font-medium text-win-text-secondary">Updates</p>
                <p className="mt-1 text-[10px] leading-relaxed text-win-text-tertiary">
                  Updates are installed manually so private GitHub credentials never need to be embedded in the app.
                  Download the build for your platform from{' '}
                  <a
                    href="https://github.com/z4mbo/zAI/releases/latest"
                    rel="noopener noreferrer"
                    className="font-medium text-win-accent hover:underline"
                  >
                    GitHub Releases
                  </a>
                  . Repository access is required while zAI is private.
                </p>
              </div>

              <div>
                <p className="text-xs text-win-text-tertiary">Repositories</p>
                <div className="mt-1 flex flex-col items-start gap-1">
                  <a
                    href="https://github.com/z4mbo/zAI"
                    rel="noopener noreferrer"
                    className="text-sm text-win-accent hover:underline"
                  >
                    zAI on GitHub
                  </a>
                  <a
                    href="https://github.com/BrunoPigat/friendly-terminal"
                    rel="noopener noreferrer"
                    className="text-xs text-win-text-secondary hover:text-win-accent hover:underline"
                  >
                    Based on Friendly Terminal by BrunoPigat
                  </a>
                </div>
              </div>
              <div className="rounded-md border border-win-border bg-win-surface p-3">
                <p className="text-xs font-medium text-win-text-secondary">GPL-3.0 License</p>
                <p className="mt-1 text-[10px] leading-relaxed text-win-text-tertiary">
                  zAI is free software distributed under GPL-3.0, without any warranty; without even the implied warranty of merchantability or fitness for a particular purpose.
                </p>
              </div>
              <div className="pt-2 border-t border-win-border">
                <p className="text-[10px] text-win-text-tertiary">
                  zAI — AI coding assistant interface
                </p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
