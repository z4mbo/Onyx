import { useState, useCallback, useEffect, useRef } from 'react'
import { useProjectStore } from '@/stores/project-store'

interface DeleteProjectDialogProps {
  projectName: string
  imported: boolean
  onClose: () => void
}

export default function DeleteProjectDialog({ projectName, imported, onClose }: DeleteProjectDialogProps) {
  const deleteProject = useProjectStore((s) => s.deleteProject)

  const [confirmation, setConfirmation] = useState('')
  const [deleting, setDeleting] = useState(false)

  const overlayRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  const isMatch = confirmation === projectName

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [onClose])

  const handleOverlayClick = useCallback(
    (e: React.MouseEvent) => {
      if (e.target === overlayRef.current) onClose()
    },
    [onClose]
  )

  const handleDelete = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault()
      if (!isMatch) return
      setDeleting(true)
      try {
        await deleteProject(projectName)
        onClose()
      } catch (err) {
        console.error('Failed to delete project:', err)
      } finally {
        setDeleting(false)
      }
    },
    [isMatch, projectName, deleteProject, onClose]
  )

  return (
    <div
      ref={overlayRef}
      onClick={handleOverlayClick}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm"
    >
      <div className="w-full max-w-md rounded-lg border border-win-border bg-win-card p-6">
        {/* Warning icon + header */}
        <div className="flex items-start gap-3 mb-4">
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-red-50">
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              className="text-red-500"
            >
              <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
              <line x1="12" y1="9" x2="12" y2="13" />
              <line x1="12" y1="17" x2="12.01" y2="17" />
            </svg>
          </div>
          <div>
            <h2 className="text-base font-semibold text-win-text">
              {imported ? 'Remove Imported Project' : 'Delete Project'}
            </h2>
            <p className="mt-1 text-sm text-win-text-secondary">
              {imported
                ? 'This will remove the project from zAI. Your original folder will not be deleted.'
                : 'This will permanently delete your project and all its files.'}
            </p>
          </div>
        </div>

        {/* Warning message */}
        <div className={`mb-5 rounded-lg px-3.5 py-3 ${imported ? 'border border-amber-200 bg-amber-50' : 'border border-red-200 bg-red-50'}`}>
          {imported ? (
            <>
              <p className="text-xs leading-relaxed text-win-text-secondary">
                <strong className="text-win-text">{projectName}</strong> is an imported project.
                Only the link inside zAI will be removed.
              </p>
              <ul className="mt-2 space-y-1 text-xs text-win-text-secondary">
                <li className="flex items-center gap-2">
                  <span className="text-amber-400">&#x2022;</span>
                  Your original folder on disk stays untouched
                </li>
                <li className="flex items-center gap-2">
                  <span className="text-amber-400">&#x2022;</span>
                  All files, agents, and configs inside it are preserved
                </li>
                <li className="flex items-center gap-2">
                  <span className="text-amber-400">&#x2022;</span>
                  You can re-import the folder at any time
                </li>
              </ul>
            </>
          ) : (
            <>
              <p className="text-xs leading-relaxed text-win-text-secondary">
                Deleting <strong className="text-win-text">{projectName}</strong> will
                permanently remove <strong className="text-red-600">everything</strong> inside
                it, including:
              </p>
              <ul className="mt-2 space-y-1 text-xs text-win-text-secondary">
                <li className="flex items-center gap-2">
                  <span className="text-red-400">&#x2022;</span>
                  All agents and their configurations
                </li>
                <li className="flex items-center gap-2">
                  <span className="text-red-400">&#x2022;</span>
                  Memory files and conversation history
                </li>
                <li className="flex items-center gap-2">
                  <span className="text-red-400">&#x2022;</span>
                  MCP server configurations
                </li>
                <li className="flex items-center gap-2">
                  <span className="text-red-400">&#x2022;</span>
                  Skills, tips, and all project files
                </li>
              </ul>
            </>
          )}
        </div>

        {/* Confirmation form */}
        <form onSubmit={handleDelete} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <label htmlFor="delete-confirm" className="text-xs text-win-text-secondary">
              Type <strong className="text-win-text">{projectName}</strong> to confirm
            </label>
            <input
              ref={inputRef}
              id="delete-confirm"
              type="text"
              value={confirmation}
              onChange={(e) => setConfirmation(e.target.value)}
              placeholder={projectName}
              autoComplete="off"
              spellCheck={false}
              className="rounded-md border border-win-border bg-win-card px-3 py-2 text-sm text-win-text placeholder-win-text-tertiary outline-none focus:border-win-accent focus:ring-2 focus:ring-win-accent/20 transition-all"
            />
          </div>

          {/* Actions */}
          <div className="flex items-center justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md px-4 py-2 text-sm text-win-text-secondary hover:bg-win-hover transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!isMatch || deleting}
              className="rounded-md bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-500 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            >
              {deleting ? (imported ? 'Removing...' : 'Deleting...') : (imported ? 'Remove Project' : 'Delete Project')}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
