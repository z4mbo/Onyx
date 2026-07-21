import { useState, useRef, useCallback, useEffect } from 'react'
import type { ITheme } from '@xterm/xterm'
import { useSettingsStore } from '@/stores/settings-store'
import { TERMINAL_THEMES, DEFAULT_THEME_ID, resolveTerminalTheme } from '@/lib/terminal-themes'

/** ANSI color keys that appear in the custom editor grid */
const ANSI_KEYS: Array<{ key: keyof ITheme; label: string }> = [
  { key: 'black', label: 'Black' },
  { key: 'red', label: 'Red' },
  { key: 'green', label: 'Green' },
  { key: 'yellow', label: 'Yellow' },
  { key: 'blue', label: 'Blue' },
  { key: 'magenta', label: 'Magenta' },
  { key: 'cyan', label: 'Cyan' },
  { key: 'white', label: 'White' },
  { key: 'brightBlack', label: 'Bright Black' },
  { key: 'brightRed', label: 'Bright Red' },
  { key: 'brightGreen', label: 'Bright Green' },
  { key: 'brightYellow', label: 'Bright Yellow' },
  { key: 'brightBlue', label: 'Bright Blue' },
  { key: 'brightMagenta', label: 'Bright Magenta' },
  { key: 'brightCyan', label: 'Bright Cyan' },
  { key: 'brightWhite', label: 'Bright White' }
]

const CORE_KEYS: Array<{ key: keyof ITheme; label: string }> = [
  { key: 'background', label: 'Background' },
  { key: 'foreground', label: 'Foreground' },
  { key: 'cursor', label: 'Cursor' },
  { key: 'selectionBackground', label: 'Selection' }
]

/** Strips alpha channel from 8-char hex (e.g. #264F7840 -> #264F78) so <input type="color"> works */
function toHex6(color: string | undefined): string {
  if (!color) return '#000000'
  const c = color.replace('#', '')
  return '#' + c.slice(0, 6)
}

export default function TerminalThemeSection() {
  const terminalTheme = useSettingsStore((s) => s.terminalTheme)
  const terminalThemeCustom = useSettingsStore((s) => s.terminalThemeCustom)
  const updateSetting = useSettingsStore((s) => s.updateSetting)

  // Local state for custom colors (debounced save)
  const [customColors, setCustomColors] = useState<ITheme>(() => {
    if (terminalThemeCustom) return terminalThemeCustom
    return resolveTerminalTheme(terminalTheme)
  })

  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  // Sync local state when the custom theme is loaded from persisted settings
  useEffect(() => {
    if (terminalThemeCustom) {
      setCustomColors(terminalThemeCustom)
    }
  }, [terminalThemeCustom])

  const handlePresetChange = useCallback((id: string) => {
    updateSetting('terminalTheme', id)
    if (id === 'custom') {
      // Pre-populate custom colors from the previously active preset
      const base = resolveTerminalTheme(terminalTheme === 'custom' ? DEFAULT_THEME_ID : terminalTheme)
      const colors = terminalThemeCustom ?? base
      setCustomColors(colors)
      updateSetting('terminalThemeCustom', colors)
    }
  }, [terminalTheme, terminalThemeCustom, updateSetting])

  const handleColorChange = useCallback((key: keyof ITheme, value: string) => {
    setCustomColors((prev) => {
      const next = { ...prev, [key]: value }
      // Debounce the persistence
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current)
      saveTimerRef.current = setTimeout(() => {
        updateSetting('terminalThemeCustom', next)
      }, 500)
      // Apply immediately to store for live preview
      useSettingsStore.setState({ terminalThemeCustom: next })
      return next
    })
  }, [updateSetting])

  const handleReset = useCallback(() => {
    updateSetting('terminalTheme', DEFAULT_THEME_ID)
    updateSetting('terminalThemeCustom', null)
    setCustomColors(resolveTerminalTheme(DEFAULT_THEME_ID))
  }, [updateSetting])

  const isCustom = terminalTheme === 'custom'
  const previewTheme = isCustom ? customColors : resolveTerminalTheme(terminalTheme)

  return (
    <div className="space-y-4">
      {/* Preset selector */}
      <div>
        <label className="block text-xs font-medium text-win-text-secondary mb-1.5">
          Terminal Theme
        </label>
        <select
          value={terminalTheme}
          onChange={(e) => handlePresetChange(e.target.value)}
          className="w-full rounded border border-win-border bg-win-surface px-3 py-2 text-sm text-win-text outline-none focus:border-win-accent"
        >
          <optgroup label="Dark">
            {TERMINAL_THEMES.filter((t) => !t.id.includes('light')).map((t) => (
              <option key={t.id} value={t.id}>{t.name}</option>
            ))}
          </optgroup>
          <optgroup label="Light">
            {TERMINAL_THEMES.filter((t) => t.id.includes('light')).map((t) => (
              <option key={t.id} value={t.id}>{t.name}</option>
            ))}
          </optgroup>
          <option value="custom">Custom</option>
        </select>
      </div>

      {/* Preview box */}
      <div
        className="rounded-md border border-win-border p-3 font-mono text-xs leading-relaxed overflow-hidden"
        style={{ backgroundColor: previewTheme.background, minHeight: 100 }}
      >
        <div style={{ color: previewTheme.foreground }}>
          <span style={{ color: previewTheme.green }}>user@machine</span>
          <span style={{ color: previewTheme.foreground }}>:</span>
          <span style={{ color: previewTheme.blue }}>~/project</span>
          <span style={{ color: previewTheme.foreground }}>$ </span>
          <span style={{ color: previewTheme.foreground }}>npm run build</span>
        </div>
        <div style={{ color: previewTheme.cyan }}>Building project...</div>
        <div>
          <span style={{ color: previewTheme.green }}>✓ </span>
          <span style={{ color: previewTheme.foreground }}>Compiled successfully</span>
        </div>
        <div>
          <span style={{ color: previewTheme.yellow }}>⚠ </span>
          <span style={{ color: previewTheme.yellow }}>2 warnings</span>
        </div>
        <div>
          <span style={{ color: previewTheme.red }}>✗ </span>
          <span style={{ color: previewTheme.red }}>1 error found</span>
        </div>
        <div style={{ color: previewTheme.brightBlack }}>
          <span style={{ color: previewTheme.magenta }}>const</span>
          {' x = '}
          <span style={{ color: previewTheme.yellow }}>42</span>
        </div>
      </div>

      {/* Custom color editor */}
      {isCustom && (
        <div className="space-y-3">
          <p className="text-[10px] text-win-text-tertiary">
            Click a color swatch or type a hex value to customize
          </p>

          {/* Core colors */}
          <div>
            <p className="text-[10px] font-medium text-win-text-secondary mb-1.5 uppercase tracking-wide">Core</p>
            <div className="grid grid-cols-2 gap-2">
              {CORE_KEYS.map(({ key, label }) => (
                <ColorField
                  key={key}
                  label={label}
                  value={toHex6(customColors[key] as string)}
                  onChange={(v) => handleColorChange(key, v)}
                />
              ))}
            </div>
          </div>

          {/* ANSI colors */}
          <div>
            <p className="text-[10px] font-medium text-win-text-secondary mb-1.5 uppercase tracking-wide">ANSI Colors</p>
            <div className="grid grid-cols-2 gap-2">
              {ANSI_KEYS.map(({ key, label }) => (
                <ColorField
                  key={key}
                  label={label}
                  value={toHex6(customColors[key] as string)}
                  onChange={(v) => handleColorChange(key, v)}
                />
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Reset button */}
      {terminalTheme !== DEFAULT_THEME_ID && (
        <button
          onClick={handleReset}
          className="text-xs text-win-text-secondary hover:text-win-text transition-colors"
        >
          Reset to default (VS Code Dark)
        </button>
      )}
    </div>
  )
}

/** Single color picker row: swatch + hex text input */
function ColorField({ label, value, onChange }: { label: string; value: string; onChange: (v: string) => void }) {
  return (
    <div className="flex items-center gap-2">
      <input
        type="color"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="h-6 w-6 shrink-0 cursor-pointer rounded border border-win-border bg-transparent p-0"
        title={label}
      />
      <input
        type="text"
        value={value}
        onChange={(e) => {
          const v = e.target.value
          if (/^#[0-9a-fA-F]{0,6}$/.test(v)) onChange(v)
        }}
        onBlur={(e) => {
          // Pad short hex inputs
          let v = e.target.value
          if (v.length === 4) {
            v = '#' + v[1] + v[1] + v[2] + v[2] + v[3] + v[3]
            onChange(v)
          }
        }}
        className="w-[72px] rounded border border-win-border bg-win-surface px-1.5 py-0.5 text-[10px] font-mono text-win-text outline-none focus:border-win-accent"
        spellCheck={false}
      />
      <span className="text-[10px] text-win-text-tertiary truncate">{label}</span>
    </div>
  )
}
