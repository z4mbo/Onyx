import { useEffect } from 'react'

interface Props {
  onClose: () => void
}

export default function HelpDialog({ onClose }: Props) {
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handleKey)
    return () => document.removeEventListener('keydown', handleKey)
  }, [onClose])

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm" onClick={onClose}>
      <div
        className="w-full max-w-2xl max-h-[80vh] overflow-y-auto rounded-lg border border-win-border bg-win-card p-6"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-lg font-semibold text-win-text">How to use AI in the terminal</h2>
          <button
            onClick={onClose}
            className="flex h-8 w-8 items-center justify-center rounded-lg text-win-text-tertiary hover:bg-win-hover hover:text-win-text transition-colors"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>

        <div className="space-y-5 text-sm text-win-text-secondary leading-relaxed">
          {/* Context */}
          <section>
            <h3 className="text-base font-medium text-win-text mb-2">Context = Information</h3>
            <p>
              When you open a session with Claude, Gemini, Codex, Kimi Code, or OpenRouter through Kimi, the <strong className="text-win-text">context is empty</strong> — no messages have been sent yet.
            </p>
            <p className="mt-2">
              Every message you send becomes context. Every response the AI gives you also becomes context. In other words, the more you chat, the more information the AI has available in that session.
            </p>
          </section>

          {/* Information windows */}
          <section>
            <h3 className="text-base font-medium text-win-text mb-2">Information windows</h3>
            <p>
              The idea is to always work with <strong className="text-win-text">information windows</strong>. Each task is a window.
            </p>
            <div className="mt-3 rounded-lg border border-win-border bg-win-bg px-4 py-3 space-y-2">
              <p><strong className="text-win-text">Example:</strong></p>
              <p>You want to analyze spreadsheet Y. The window is: <em>spreadsheet Y</em>.</p>
              <p>When you finish the analysis and everything you need with spreadsheet Y, close the window with the <code className="rounded bg-win-hover px-1.5 py-0.5 text-win-text font-mono text-xs">/clear</code> command, which clears all context — meaning it wipes all the information that was in the session's memory.</p>
            </div>
          </section>

          {/* Saving to memory */}
          <section>
            <h3 className="text-base font-medium text-win-text mb-2">Saving to project memory</h3>
            <p>
              If you did a large analysis and want to save the conclusions for later use, just ask the AI to <strong className="text-win-text">save the relevant information to the project memory</strong>.
            </p>
            <p className="mt-2">
              Project memory is stored in an engine-specific Markdown file, such as <code className="rounded bg-win-hover px-1.5 py-0.5 text-win-text font-mono text-xs">CLAUDE.md</code> or <code className="rounded bg-win-hover px-1.5 py-0.5 text-win-text font-mono text-xs">AGENTS.md</code>. The AI writes the information you ask it to save into this file.
            </p>
          </section>

          {/* Organization */}
          <section>
            <h3 className="text-base font-medium text-win-text mb-2">Project organization</h3>
            <p>
              Always organize your project folders — <strong className="text-win-text">each project has its own folder</strong>. Always start the terminal in the project folder.
            </p>
          </section>

          {/* Summary */}
          <section className="rounded-lg border border-win-accent/20 bg-win-accent-subtle px-4 py-3">
            <h3 className="text-sm font-medium text-win-accent mb-2">Quick summary</h3>
            <ul className="space-y-1.5 text-xs text-win-text-secondary">
              <li className="flex gap-2">
                <span className="shrink-0 text-win-accent">1.</span>
                <span>Open a session and chat — everything becomes context</span>
              </li>
              <li className="flex gap-2">
                <span className="shrink-0 text-win-accent">2.</span>
                <span>Done with the task? Use <code className="rounded bg-win-hover px-1 py-0.5 font-mono">/clear</code> to wipe the context</span>
              </li>
              <li className="flex gap-2">
                <span className="shrink-0 text-win-accent">3.</span>
                <span>Want to keep something? Ask the AI to save it to project memory</span>
              </li>
              <li className="flex gap-2">
                <span className="shrink-0 text-win-accent">4.</span>
                <span>Each project = a separate folder</span>
              </li>
            </ul>
          </section>
        </div>

        <div className="mt-5 flex justify-end">
          <button
            onClick={onClose}
            className="rounded-lg bg-win-accent px-5 py-2.5 text-sm font-medium text-white hover:bg-win-accent-dark transition-colors"
          >
            Got it
          </button>
        </div>
      </div>
    </div>
  )
}
