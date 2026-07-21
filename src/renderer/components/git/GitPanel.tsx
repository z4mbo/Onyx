import { useEffect, useRef, useState, useCallback } from 'react'
import { useGitStore } from '@/stores/git-store'
import { useProjectStore } from '@/stores/project-store'
import { onFsChanged } from '@/lib/api'

export default function GitPanel() {
  const activeProject = useProjectStore((s) => s.activeProject)
  const {
    gitAvailable, status, changedFiles, loading, error,
    checkGitAvailable, refreshAll, stageFiles, commit, push, pull, initRepo
  } = useGitStore()

  const [commitMsg, setCommitMsg] = useState('')
  const [showCommitInput, setShowCommitInput] = useState(false)
  const commitInputRef = useRef<HTMLInputElement>(null)
  const debounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  const cwd = activeProject?.path ?? ''

  // Initial load.
  useEffect(() => {
    checkGitAvailable()
  }, [checkGitAvailable])

  useEffect(() => {
    if (!cwd || gitAvailable === false) return
    refreshAll(cwd)
  }, [cwd, gitAvailable, refreshAll])

  // Auto-refresh on fs changes (debounced)
  useEffect(() => {
    if (!cwd) return
    const unsub = onFsChanged((rootPath, changedDir) => {
      if (!rootPath.startsWith(cwd) && !cwd.startsWith(rootPath)) return
      // Skip .git internal changes that are too noisy
      const relative = changedDir.replace(/\\/g, '/')
      if (relative.includes('/.git/objects') || relative.includes('/.git/logs')) return

      if (debounceRef.current) clearTimeout(debounceRef.current)
      debounceRef.current = setTimeout(() => refreshAll(cwd), 500)
    })
    return () => {
      unsub()
      if (debounceRef.current) clearTimeout(debounceRef.current)
    }
  }, [cwd, refreshAll])

  // Focus commit input when shown
  useEffect(() => {
    if (showCommitInput) commitInputRef.current?.focus()
  }, [showCommitInput])

  const handleStageAll = useCallback(() => {
    if (!cwd) return
    stageFiles(cwd, ['.'])
  }, [cwd, stageFiles])

  const handleStageFile = useCallback((path: string) => {
    if (!cwd) return
    stageFiles(cwd, [path])
  }, [cwd, stageFiles])

  const handleCommit = useCallback(async () => {
    if (!cwd || !commitMsg.trim()) return
    await commit(cwd, commitMsg.trim())
    setCommitMsg('')
    setShowCommitInput(false)
  }, [cwd, commitMsg, commit])

  const handlePush = useCallback(() => { if (cwd) push(cwd) }, [cwd, push])
  const handlePull = useCallback(() => { if (cwd) pull(cwd) }, [cwd, pull])
  const handleInit = useCallback(() => { if (cwd) initRepo(cwd) }, [cwd, initRepo])

  // -- Render states --

  if (!activeProject) {
    return (
      <div className="flex flex-1 items-center justify-center p-4 text-center text-sm text-win-text-tertiary">
        Open a project to see git status
      </div>
    )
  }

  if (gitAvailable === false) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 p-4 text-center">
        <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-win-text-tertiary">
          <circle cx="12" cy="12" r="10" />
          <line x1="15" y1="9" x2="9" y2="15" />
          <line x1="9" y1="9" x2="15" y2="15" />
        </svg>
        <p className="text-sm text-win-text-secondary">Git is not installed</p>
        <a
          href="https://git-scm.com/downloads"
          rel="noopener noreferrer"
          className="text-xs text-win-accent hover:underline"
        >
          Download Git
        </a>
      </div>
    )
  }

  if (gitAvailable === null) {
    return (
      <div className="flex flex-1 items-center justify-center p-4 text-sm text-win-text-tertiary">
        Checking git...
      </div>
    )
  }

  if (status && !status.isRepo) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 p-4 text-center">
        <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-win-text-tertiary">
          <path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4" />
          <path d="M9 18c-4.51 2-5-2-7-2" />
        </svg>
        <p className="text-sm text-win-text-secondary">Not a git repository</p>
        <button
          onClick={handleInit}
          disabled={loading}
          className="rounded-md bg-win-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-win-accent/90 disabled:opacity-50"
        >
          Initialize Git
        </button>
      </div>
    )
  }

  if (!status) {
    return (
      <div className="flex flex-1 items-center justify-center p-4 text-sm text-win-text-tertiary">
        Loading...
      </div>
    )
  }

  // Separate staged and unstaged files
  const stagedFiles = changedFiles.filter((f) => f.staged)
  const unstagedFiles = changedFiles.filter((f) => !f.staged)

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {/* Branch header */}
      <div className="flex items-center gap-2 border-b border-win-border px-3 py-2.5 shrink-0">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-win-accent shrink-0">
          <line x1="6" y1="3" x2="6" y2="15" />
          <circle cx="18" cy="6" r="3" />
          <circle cx="6" cy="18" r="3" />
          <path d="M18 9a9 9 0 0 1-9 9" />
        </svg>
        <span className="text-sm font-semibold text-win-text truncate">{status.branch}</span>
        {status.hasRemote && (status.ahead > 0 || status.behind > 0) && (
          <span className="text-[10px] text-win-text-tertiary shrink-0">
            {status.ahead > 0 && `+${status.ahead}`}
            {status.ahead > 0 && status.behind > 0 && ' '}
            {status.behind > 0 && `-${status.behind}`}
          </span>
        )}
      </div>

      {/* Error banner */}
      {error && (
        <div className="mx-3 mt-2 rounded border border-red-500/30 bg-red-500/10 px-2 py-1.5 text-xs text-red-400 shrink-0">
          {error}
        </div>
      )}

      {/* File list */}
      <div className="flex-1 overflow-y-auto">
        {/* Staged files */}
        {stagedFiles.length > 0 && (
          <div className="px-3 pt-3">
            <p className="text-[10px] font-semibold uppercase tracking-wider text-win-text-tertiary mb-1">
              Staged ({stagedFiles.length})
            </p>
            {stagedFiles.map((f) => (
              <FileRow key={`s-${f.path}`} file={f} />
            ))}
          </div>
        )}

        {/* Unstaged / untracked files */}
        {unstagedFiles.length > 0 && (
          <div className="px-3 pt-3">
            <p className="text-[10px] font-semibold uppercase tracking-wider text-win-text-tertiary mb-1">
              Unsaved Changes ({unstagedFiles.length})
            </p>
            {unstagedFiles.map((f) => (
              <FileRow
                key={`u-${f.path}`}
                file={f}
                onStage={() => handleStageFile(f.path)}
              />
            ))}
          </div>
        )}

        {stagedFiles.length === 0 && unstagedFiles.length === 0 && (
          <div className="flex items-center justify-center py-8 text-sm text-win-text-tertiary">
            Working tree clean
          </div>
        )}

      </div>

      {/* Action buttons */}
      <div className="shrink-0 border-t border-win-border p-3 space-y-2">
        {/* Commit input */}
        {showCommitInput ? (
          <div className="flex gap-1.5">
            <input
              ref={commitInputRef}
              type="text"
              value={commitMsg}
              onChange={(e) => setCommitMsg(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleCommit()
                if (e.key === 'Escape') { setShowCommitInput(false); setCommitMsg('') }
              }}
              placeholder="Commit message..."
              className="flex-1 rounded border border-win-border bg-win-surface px-2 py-1 text-xs text-win-text placeholder:text-win-text-tertiary outline-none focus:border-win-accent"
            />
            <button
              onClick={handleCommit}
              disabled={!commitMsg.trim() || loading}
              className="rounded bg-win-accent px-2 py-1 text-xs font-medium text-white hover:bg-win-accent/90 disabled:opacity-50"
            >
              OK
            </button>
          </div>
        ) : (
          <div className="flex gap-1.5">
            {unstagedFiles.length > 0 && (
              <ActionButton onClick={handleStageAll} disabled={loading} label="Save All" />
            )}
            <ActionButton
              onClick={() => setShowCommitInput(true)}
              disabled={loading || stagedFiles.length === 0}
              label="Commit the save files"
              primary
            />
          </div>
        )}

        {status.hasRemote && (
          <div className="flex gap-1.5">
            <ActionButton onClick={handlePush} disabled={loading} label={status.ahead > 0 ? `Push (${status.ahead})` : 'Push'} />
            <ActionButton onClick={handlePull} disabled={loading} label={status.behind > 0 ? `Pull (${status.behind})` : 'Pull'} />
          </div>
        )}

        {/* Friendly help guide */}
        <GitHelpGuide hasRemote={status.hasRemote} />
      </div>

      {/* Loading overlay */}
      {loading && (
        <div className="absolute inset-0 flex items-center justify-center bg-win-bg/50">
          <div className="h-4 w-4 animate-spin rounded-full border-2 border-win-accent border-t-transparent" />
        </div>
      )}
    </div>
  )
}

// -- Sub-components --

function FileRow({ file, onStage }: { file: { path: string; status: string; staged: boolean }; onStage?: () => void }) {
  const statusColors: Record<string, string> = {
    modified: 'text-yellow-400',
    added: 'text-green-400',
    deleted: 'text-red-400',
    untracked: 'text-blue-400',
    renamed: 'text-purple-400'
  }
  const statusLetters: Record<string, string> = {
    modified: 'M', added: 'A', deleted: 'D', untracked: 'U', renamed: 'R'
  }

  return (
    <div className="group flex items-center gap-1.5 rounded px-1 py-0.5 hover:bg-win-hover text-xs">
      <span className={`w-3 shrink-0 font-mono font-bold ${statusColors[file.status] ?? 'text-win-text-tertiary'}`}>
        {statusLetters[file.status] ?? '?'}
      </span>
      <span className="flex-1 truncate text-win-text-secondary" title={file.path}>
        {file.path}
      </span>
      {onStage && !file.staged && (
        <button
          onClick={onStage}
          className="invisible group-hover:visible shrink-0 rounded px-1 text-[10px] text-win-accent hover:bg-win-accent/10"
          title="Stage file"
        >
          +
        </button>
      )}
    </div>
  )
}

function ActionButton({ onClick, disabled, label, primary }: { onClick: () => void; disabled: boolean; label: string; primary?: boolean }) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`flex-1 rounded px-2 py-1.5 text-xs font-medium transition-colors disabled:opacity-50 ${
        primary
          ? 'bg-win-accent text-white hover:bg-win-accent/90'
          : 'border border-win-border bg-win-surface text-win-text-secondary hover:bg-win-hover hover:text-win-text'
      }`}
    >
      {label}
    </button>
  )
}

function GitHelpGuide({ hasRemote }: { hasRemote: boolean }) {
  const [expanded, setExpanded] = useState(false)

  return (
    <div className="mx-3 my-3">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-1.5 text-[11px] text-win-text-tertiary hover:text-win-text-secondary transition-colors"
      >
        <svg
          width="10"
          height="10"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          className={`transition-transform ${expanded ? 'rotate-90' : ''}`}
        >
          <polyline points="9 18 15 12 9 6" />
        </svg>
        What do these buttons do?
      </button>

      {expanded && (
        <div className="mt-2 space-y-2.5 text-[11px] leading-relaxed text-win-text-secondary">
          <HelpEntry
            letter="+"
            letterClass="text-green-400"
            title="Stage (the + button on each file)"
            desc="Marks a file to be included in your next save point. Think of it as putting files in a box before sealing it."
          />
          <HelpEntry
            letter="S"
            letterClass="text-blue-400"
            title="Save All"
            desc="Stages every changed file at once — puts everything in the box in one go."
          />
          <HelpEntry
            letter="C"
            letterClass="text-win-accent"
            title="Commit the save files"
            desc="Creates a save point with a short message describing what you changed. Like taking a snapshot of your project that you can go back to later."
          />
          {hasRemote && (
            <>
              <HelpEntry
                letter="↑"
                letterClass="text-emerald-400"
                title="Push"
                desc="Sends your saved snapshots to the cloud (GitHub, GitLab, etc.) so others can see your changes and they're backed up online."
              />
              <HelpEntry
                letter="↓"
                letterClass="text-amber-400"
                title="Pull"
                desc="Downloads the latest changes that others pushed to the cloud, so your local copy stays up to date."
              />
            </>
          )}
          <div className="border-t border-win-border/50 pt-2 text-[10px] text-win-text-tertiary leading-relaxed">
            <span className="font-medium">Typical flow:</span> make changes → Save All → write a message → Commit{hasRemote ? ' → Push' : ''}.
          </div>
        </div>
      )}
    </div>
  )
}

function HelpEntry({ letter, letterClass, title, desc }: { letter: string; letterClass: string; title: string; desc: string }) {
  return (
    <div className="flex gap-2">
      <span className={`w-3.5 shrink-0 text-center font-mono font-bold ${letterClass}`}>{letter}</span>
      <div>
        <p className="font-medium text-win-text">{title}</p>
        <p className="text-win-text-tertiary">{desc}</p>
      </div>
    </div>
  )
}
