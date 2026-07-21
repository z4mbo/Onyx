import { useState, useEffect, useCallback } from 'react'
import { useProjectStore } from '@/stores/project-store'
import { useSettingsStore } from '@/stores/settings-store'
import { APP_NAME } from '@/lib/constants'
import { showOpenDirectory, getProjectsDir, shellOpenPath, type Project } from '@/lib/api'
import DeleteProjectDialog from '@/components/project/DeleteProjectDialog'
import SetupBanner from '@/components/SetupBanner'
import HelpDialog from '@/components/HelpDialog'
import logoImg from '../../../resources/logo.png'

export default function WelcomeScreen() {
  const projects = useProjectStore((s) => s.projects)
  const loading = useProjectStore((s) => s.loading)
  const loadProjects = useProjectStore((s) => s.loadProjects)
  const selectProject = useProjectStore((s) => s.selectProject)
  const createProject = useProjectStore((s) => s.createProject)
  const importProject = useProjectStore((s) => s.importProject)
  const defaultEngine = useSettingsStore((s) => s.defaultEngine)

  const [newName, setNewName] = useState('')
  const [creating, setCreating] = useState(false)
  const [importing, setImporting] = useState(false)
  const [showCreate, setShowCreate] = useState(false)
  const [deleteTarget, setDeleteTarget] = useState<typeof projects[number] | null>(null)
  const [showHelp, setShowHelp] = useState(false)

  useEffect(() => {
    loadProjects()
  }, [loadProjects])

  const handleCreate = async () => {
    const trimmed = newName.trim()
    if (!trimmed) return
    setCreating(true)
    try {
      const project = await createProject(trimmed)
      selectProject(project, defaultEngine)
    } catch (err) {
      console.error('[WelcomeScreen] Failed to create project:', err)
    }
    setCreating(false)
    setNewName('')
    setShowCreate(false)
  }

  const handleImport = async () => {
    const folderPath = await showOpenDirectory()
    if (!folderPath) return
    setImporting(true)
    try {
      const project = await importProject(folderPath)
      selectProject(project, defaultEngine)
    } catch (err) {
      console.error('[WelcomeScreen] Failed to import project:', err)
    }
    setImporting(false)
  }

  const handleSelectProject = useCallback(
    (project: Project) => {
      selectProject(project, defaultEngine)
    },
    [selectProject, defaultEngine]
  )

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') handleCreate()
    if (e.key === 'Escape') {
      setShowCreate(false)
      setNewName('')
    }
  }

  return (
    <div className="flex h-full w-full flex-col items-center justify-center bg-win-bg text-win-text">
      <div className="w-full max-w-2xl px-8">
        {/* Header */}
        <div className="mb-12 text-center">
          <img src={logoImg} alt="App logo" className="mx-auto mb-6 h-24 w-24 rounded-2xl object-contain" />
          <h1 className="text-3xl font-semibold text-win-text">{APP_NAME}</h1>
          <p className="mt-3 text-base text-win-text-secondary">
            {projects.length > 0 ? 'Pick a project to start chatting with your AI assistant' : 'Create your first project to get started'}
          </p>
        </div>

        {/* Help button */}
        <button
          onClick={() => setShowHelp(true)}
          className="mb-6 flex w-full items-center gap-3 rounded-xl border border-win-accent/30 bg-win-accent-subtle px-5 py-4 text-left text-sm font-medium text-win-accent hover:bg-win-accent hover:text-white transition-all"
        >
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="shrink-0">
            <circle cx="12" cy="12" r="10" />
            <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
            <line x1="12" y1="17" x2="12.01" y2="17" />
          </svg>
          How to use AI in the terminal — understand context, memory and organization
        </button>

        {/* Engine setup banner */}
        <SetupBanner />

        {/* Project list */}
        <div className="mb-6">
          {loading ? (
            <div className="flex flex-col items-center py-14 gap-4">
              <div className="h-8 w-8 animate-spin rounded-full border-2 border-win-border border-t-win-accent" />
              <span className="text-base text-win-text-secondary">Loading your projects...</span>
            </div>
          ) : projects.length === 0 ? (
            <div className="flex flex-col items-center py-14 gap-3">
              <svg width="56" height="56" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" strokeLinecap="round" strokeLinejoin="round" className="text-win-text-tertiary">
                <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
                <line x1="12" y1="11" x2="12" y2="17" />
                <line x1="9" y1="14" x2="15" y2="14" />
              </svg>
              <p className="text-lg text-win-text-secondary">No projects yet</p>
              <p className="text-sm text-win-text-tertiary">Create one below to start working with AI</p>
            </div>
          ) : (
            <div className="space-y-2">
              {projects.map((project) => (
                <div
                  key={project.name}
                  className="group flex items-center rounded-xl border border-win-border bg-win-card transition-all hover:bg-win-hover"
                >
                  <button
                    onClick={() => handleSelectProject(project)}
                    className="flex flex-1 items-center gap-4 px-5 py-5 text-left min-w-0"
                  >
                    <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-win-accent-subtle text-win-accent group-hover:bg-win-accent group-hover:text-white transition-colors">
                      <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                        <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
                      </svg>
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="text-base font-medium text-win-text truncate">
                        {project.name}
                      </div>
                      <div className="text-sm text-win-text-tertiary truncate mt-0.5">{project.path}</div>
                    </div>
                  </button>

                  {/* Delete button */}
                  <button
                    onClick={(e) => {
                      e.stopPropagation()
                      setDeleteTarget(project)
                    }}
                    className="shrink-0 mr-3 flex h-9 w-9 items-center justify-center rounded-lg text-win-text-tertiary opacity-0 group-hover:opacity-100 hover:bg-red-50 hover:text-red-600 transition-all"
                    title={`Delete ${project.name}`}
                  >
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <polyline points="3 6 5 6 21 6" />
                      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                    </svg>
                  </button>

                  {/* Arrow */}
                  <svg
                    width="18"
                    height="18"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    className="shrink-0 mr-5 text-win-text-tertiary group-hover:text-win-accent transition-colors"
                  >
                    <polyline points="9 18 15 12 9 6" />
                  </svg>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Create / Import project */}
        {showCreate ? (
          <div className="flex gap-3">
            <input
              type="text"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Enter project name..."
              autoFocus
              className="flex-1 rounded-xl border border-win-border bg-win-card px-5 py-3.5 text-base text-win-text placeholder-win-text-tertiary outline-none focus:border-win-accent focus:ring-2 focus:ring-win-accent/20 transition-all"
            />
            <button
              onClick={handleCreate}
              disabled={!newName.trim() || creating}
              className="rounded-xl bg-win-accent px-6 py-3.5 text-base font-medium text-white hover:bg-win-accent-dark disabled:opacity-40 transition-colors"
            >
              {creating ? '...' : 'Create'}
            </button>
            <button
              onClick={() => { setShowCreate(false); setNewName('') }}
              className="rounded-xl border border-win-border px-5 py-3.5 text-base text-win-text-secondary hover:bg-win-hover transition-colors"
            >
              Cancel
            </button>
          </div>
        ) : (
          <div className="flex gap-3">
            <button
              onClick={() => setShowCreate(true)}
              className="flex flex-1 items-center justify-center gap-2.5 rounded-xl border-2 border-dashed border-win-border-strong px-5 py-5 text-base font-medium text-win-text-secondary hover:border-win-accent hover:text-win-accent hover:bg-win-accent-subtle transition-all"
            >
              <svg width="18" height="18" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
                <line x1="8" y1="3" x2="8" y2="13" />
                <line x1="3" y1="8" x2="13" y2="8" />
              </svg>
              New Project
            </button>
            <button
              onClick={handleImport}
              disabled={importing}
              className="flex flex-1 items-center justify-center gap-2.5 rounded-xl border-2 border-dashed border-win-border-strong px-5 py-5 text-base font-medium text-win-text-secondary hover:border-win-accent hover:text-win-accent hover:bg-win-accent-subtle transition-all disabled:opacity-40"
            >
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
                <polyline points="12 13 12 17" />
                <polyline points="9 10 12 7 15 10" />
              </svg>
              {importing ? 'Importing...' : 'Import Folder'}
            </button>
            <button
              onClick={async () => { const dir = await getProjectsDir(); shellOpenPath(dir) }}
              title="Open projects folder"
              className="flex items-center justify-center gap-2.5 rounded-xl border-2 border-dashed border-win-border-strong px-5 py-5 text-base font-medium text-win-text-secondary hover:border-win-accent hover:text-win-accent hover:bg-win-accent-subtle transition-all"
            >
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                <polyline points="15 3 21 3 21 9" />
                <line x1="10" y1="14" x2="21" y2="3" />
              </svg>
            </button>
          </div>
        )}
      </div>

      {/* Delete confirmation dialog */}
      {deleteTarget && (
        <DeleteProjectDialog
          projectName={deleteTarget.name}
          imported={deleteTarget.imported}
          onClose={() => setDeleteTarget(null)}
        />
      )}

      {/* Help dialog */}
      {showHelp && <HelpDialog onClose={() => setShowHelp(false)} />}
    </div>
  )
}
