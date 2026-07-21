import { useCallback, useState, useEffect, useRef } from 'react'
import { APP_NAME } from '@/lib/constants'
import * as api from '@/lib/api'
import { useProjectStore } from '@/stores/project-store'
import { useSplitViewStore } from '@/stores/split-view-store'
import { useSettingsStore } from '@/stores/settings-store'
import type { Project } from '@/lib/api'
import logoImg from '../../../../resources/logo.png'

export default function TitleBar() {
  const panels = useSplitViewStore((s) => s.panels)
  const activatePanel = useSplitViewStore((s) => s.activatePanel)
  const closePanel = useSplitViewStore((s) => s.closePanel)
  const clearProject = useProjectStore((s) => s.clearProject)
  const selectProject = useProjectStore((s) => s.selectProject)
  const defaultEngine = useSettingsStore((s) => s.defaultEngine)

  const createProject = useProjectStore((s) => s.createProject)

  const [showProjectDropdown, setShowProjectDropdown] = useState(false)
  const [showCloseConfirm, setShowCloseConfirm] = useState(false)
  const [showNewProject, setShowNewProject] = useState(false)
  const [newProjectName, setNewProjectName] = useState('')
  const [creatingProject, setCreatingProject] = useState(false)
  const [projects, setProjects] = useState<Project[]>([])
  const [platform, setPlatform] = useState<'win32' | 'darwin' | 'linux'>(() =>
    navigator.userAgent.includes('Mac') ? 'darwin' : navigator.userAgent.includes('Windows') ? 'win32' : 'linux'
  )
  const dropdownRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    api.getPlatform().then(setPlatform).catch(() => {})
  }, [])

  const handleBack = useCallback(() => {
    setShowCloseConfirm(true)
  }, [])

  const confirmClose = useCallback(() => {
    setShowCloseConfirm(false)
    if (panels.length > 1) {
      const activePanel = panels[panels.length - 1]
      closePanel(activePanel.panelId)
    } else {
      clearProject()
    }
  }, [panels, closePanel, clearProject])

  const handleMinimize = useCallback(() => api.windowMinimize(), [])
  const handleMaximize = useCallback(() => api.windowMaximize(), [])
  const handleClose = useCallback(() => api.windowClose(), [])

  // Fetch projects when dropdown opens
  useEffect(() => {
    if (showProjectDropdown) {
      api.listProjects().then(setProjects).catch(() => setProjects([]))
    }
  }, [showProjectDropdown])

  // Close dropdown when clicking outside
  useEffect(() => {
    if (!showProjectDropdown) return
    const handler = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setShowProjectDropdown(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [showProjectDropdown])

  const handleOpenProject = useCallback(
    (project: Project) => {
      setShowProjectDropdown(false)
      setShowNewProject(false)
      setNewProjectName('')
      selectProject(project, defaultEngine)
    },
    [selectProject, defaultEngine]
  )

  const handleCreateProject = useCallback(async () => {
    const trimmed = newProjectName.trim()
    if (!trimmed) return
    setCreatingProject(true)
    try {
      const project = await createProject(trimmed)
      setShowProjectDropdown(false)
      setShowNewProject(false)
      setNewProjectName('')
      selectProject(project, defaultEngine)
    } catch (err) {
      console.error('[TitleBar] Failed to create project:', err)
    }
    setCreatingProject(false)
  }, [newProjectName, createProject, selectProject, defaultEngine])

  // Get names of projects already open in panels
  const openProjectNames = new Set(panels.map((p) => p.project.name))

  return (
    <header
      className="flex h-10 shrink-0 items-center justify-between border-b border-win-border bg-win-surface pr-3 select-none"
      style={{
        WebkitAppRegion: 'drag',
        paddingLeft: platform === 'darwin' ? 76 : 12
      } as React.CSSProperties}
    >
      {/* Left: back + app name + breadcrumb */}
      <div className="flex items-center gap-2 text-sm" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
        {panels.length > 0 && (
          <button
            onClick={handleBack}
            className="flex items-center justify-center h-6 w-6 rounded-md text-win-text-secondary hover:text-win-text hover:bg-win-hover transition-colors"
            title={panels.length > 1 ? 'Close active project' : 'Back to projects'}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="15 18 9 12 15 6" />
            </svg>
          </button>
        )}
        {/* App icon */}
        <img src={logoImg} alt="" className="h-6 w-6 rounded-md object-contain" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties} />
        <span className="font-semibold text-win-text" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}>{APP_NAME}</span>

        {/* Breadcrumb: show all open projects */}
        {panels.map((panel, idx) => {
          const isActive = idx === panels.length - 1
          return (
            <span key={panel.panelId} className="flex items-center gap-2">
              <span className="text-win-text-tertiary">/</span>
              <button
                onClick={() => {
                  if (!isActive) activatePanel(panel.panelId)
                }}
                className={`transition-colors ${
                  isActive
                    ? 'font-semibold text-win-text cursor-default'
                    : 'text-win-text-secondary hover:text-win-text cursor-pointer'
                }`}
              >
                {panel.project.name}
              </button>
            </span>
          )
        })}

        {/* Project switcher dropdown */}
        {panels.length > 0 && (
          <div className="relative" ref={dropdownRef}>
            <button
              onClick={() => setShowProjectDropdown((v) => !v)}
              className="flex items-center justify-center h-6 w-6 rounded-md text-win-text-tertiary hover:text-win-text hover:bg-win-hover transition-colors"
              title="Open project"
            >
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <polyline points="6 9 12 15 18 9" />
              </svg>
            </button>

            {showProjectDropdown && (
              <div className="absolute left-0 top-full mt-1 z-50 w-56 rounded-lg border border-win-border bg-win-card shadow-lg py-1">
                <div className="px-3 py-2 text-[11px] font-medium text-win-text-tertiary uppercase tracking-wider">
                  Open project
                </div>
                {projects.length === 0 ? (
                  <div className="px-3 py-2 text-sm text-win-text-tertiary">No projects</div>
                ) : (
                  projects
                    .filter((p) => !openProjectNames.has(p.name))
                    .map((project) => (
                      <button
                        key={project.name}
                        onClick={() => handleOpenProject(project)}
                        className="flex w-full items-center gap-2 px-3 py-2 text-sm text-win-text-secondary hover:bg-win-hover hover:text-win-text transition-colors text-left"
                      >
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="shrink-0 text-win-text-tertiary">
                          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
                        </svg>
                        <span className="truncate">{project.name}</span>
                      </button>
                    ))
                )}
                {projects.filter((p) => !openProjectNames.has(p.name)).length === 0 && projects.length > 0 && (
                  <div className="px-3 py-2 text-sm text-win-text-tertiary">All projects are open</div>
                )}

                {/* Divider + New project */}
                <div className="my-1 border-t border-win-border" />
                {showNewProject ? (
                  <div className="px-2 py-1.5">
                    <div className="flex gap-1.5">
                      <input
                        type="text"
                        value={newProjectName}
                        onChange={(e) => setNewProjectName(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') handleCreateProject()
                          if (e.key === 'Escape') { setShowNewProject(false); setNewProjectName('') }
                        }}
                        placeholder="Project name..."
                        autoFocus
                        className="flex-1 min-w-0 rounded-md border border-win-border bg-win-bg px-2 py-1.5 text-sm text-win-text placeholder-win-text-tertiary outline-none focus:border-win-accent transition-colors"
                      />
                      <button
                        onClick={handleCreateProject}
                        disabled={!newProjectName.trim() || creatingProject}
                        className="rounded-md bg-win-accent px-2.5 py-1.5 text-xs font-medium text-white hover:bg-win-accent-dark disabled:opacity-40 transition-colors"
                      >
                        {creatingProject ? '...' : 'Create'}
                      </button>
                    </div>
                  </div>
                ) : (
                  <button
                    onClick={() => setShowNewProject(true)}
                    className="flex w-full items-center gap-2 px-3 py-2 text-sm text-win-accent hover:bg-win-hover transition-colors text-left"
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="shrink-0">
                      <line x1="12" y1="5" x2="12" y2="19" />
                      <line x1="5" y1="12" x2="19" y2="12" />
                    </svg>
                    <span>New project</span>
                  </button>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Right: window controls - Windows 11 style */}
      {platform !== 'darwin' && (
        <div
          className="flex items-center"
          style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
        >
        <button
          onClick={handleMinimize}
          className="flex h-8 w-11 items-center justify-center text-win-text-secondary hover:bg-black/5 transition-colors"
          aria-label="Minimize"
        >
          <svg width="10" height="1" viewBox="0 0 10 1" fill="currentColor">
            <rect width="10" height="1" />
          </svg>
        </button>
        <button
          onClick={handleMaximize}
          className="flex h-8 w-11 items-center justify-center text-win-text-secondary hover:bg-black/5 transition-colors"
          aria-label="Maximize"
        >
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1">
            <rect x="0.5" y="0.5" width="9" height="9" />
          </svg>
        </button>
        <button
          onClick={handleClose}
          className="flex h-8 w-11 items-center justify-center text-win-text-secondary hover:bg-[#C42B1C] hover:text-white transition-colors"
          aria-label="Close"
        >
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.2">
            <line x1="0" y1="0" x2="10" y2="10" />
            <line x1="10" y1="0" x2="0" y2="10" />
          </svg>
        </button>
        </div>
      )}

      {/* Close project confirmation dialog */}
      {showCloseConfirm && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm" onClick={() => setShowCloseConfirm(false)}>
          <div
            className="w-full max-w-md rounded-lg border border-win-border bg-win-card p-6"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center gap-3 mb-3">
              <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-amber-100 text-amber-600">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
                  <line x1="12" y1="9" x2="12" y2="13" />
                  <line x1="12" y1="17" x2="12.01" y2="17" />
                </svg>
              </div>
              <h2 className="text-base font-semibold text-win-text">Close project?</h2>
            </div>
            <p className="text-sm text-win-text-secondary mb-5">
              Are you sure you want to close this project? This will close your active project and any unsaved progress will be lost.
            </p>
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setShowCloseConfirm(false)}
                className="rounded-lg border border-win-border px-4 py-2 text-sm font-medium text-win-text-secondary hover:bg-win-hover transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={confirmClose}
                className="rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 transition-colors"
              >
                Close project
              </button>
            </div>
          </div>
        </div>
      )}
    </header>
  )
}
