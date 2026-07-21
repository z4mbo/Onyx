import { useState, useEffect, useCallback } from 'react'
import { useProjectStore } from '@/stores/project-store'
import { useSettingsStore } from '@/stores/settings-store'
import { ENGINE_DIRS } from '@/lib/constants'
import * as api from '@/lib/api'
import type { SkillEntry } from '@/lib/api'

export default function SkillList() {
  const activeProject = useProjectStore((s) => s.activeProject)
  const defaultEngine = useSettingsStore((s) => s.defaultEngine)
  const [skills, setSkills] = useState<SkillEntry[]>([])
  const [loading, setLoading] = useState(false)
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set())

  const toggleExpanded = useCallback((filePath: string) => {
    setExpandedPaths((prev) => {
      const next = new Set(prev)
      if (next.has(filePath)) {
        next.delete(filePath)
      } else {
        next.add(filePath)
      }
      return next
    })
  }, [])

  const loadSkills = useCallback(
    (projectPath: string) => {
      api.listSkills(defaultEngine, projectPath)
        .then((result) => {
          setSkills(result)
          setLoading(false)
        })
        .catch((err) => {
          console.warn('[SkillList] Failed to load skills:', err)
          setSkills([])
          setLoading(false)
        })
    },
    [defaultEngine]
  )

  useEffect(() => {
    if (!activeProject) {
      setSkills([])
      return
    }

    setLoading(true)
    loadSkills(activeProject.path)

    const engineDirs = defaultEngine === 'kimi' || defaultEngine === 'openrouter'
      ? ['.agents', '.kimi-code']
      : [ENGINE_DIRS[defaultEngine]]

    const unsub = api.onFsChanged((rootPath, changedDir) => {
      if (rootPath !== activeProject.path) return
      const normalized = changedDir.replace(/\\/g, '/')
      if (engineDirs.some((engineDir) =>
        normalized.includes(`/${engineDir}/skills`) ||
        normalized.includes(`/${engineDir}/commands`)
      )) {
        loadSkills(activeProject.path)
      }
    })

    return () => { unsub() }
  }, [activeProject, defaultEngine, loadSkills])

  if (!activeProject) {
    return (
      <div className="p-4 text-sm text-win-text-tertiary">
        Select a project to view skills.
      </div>
    )
  }

  if (loading) {
    return (
      <div className="p-4 text-sm text-win-text-tertiary">Loading skills...</div>
    )
  }

  if (skills.length === 0) {
    return (
      <div className="flex flex-col items-center gap-2 p-6 text-center">
        <div className="text-win-text-tertiary">
          <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" />
          </svg>
        </div>
        <p className="text-sm text-win-text-secondary">No skills yet</p>
        <p className="text-xs text-win-text-tertiary">
          Ask your AI assistant on the chat to create a new skill — just describe what you want it to do.
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-2 p-4">
      <p className="text-[11px] leading-relaxed text-win-text-tertiary px-1 pb-1">
        Ask your AI assistant on the chat to create a new skill for you — just describe what you want it to do.
      </p>
      {skills.map((skill) => {
        const isExpanded = expandedPaths.has(skill.filePath)
        return (
          <button
            key={skill.filePath}
            onClick={() => toggleExpanded(skill.filePath)}
            className="rounded-lg border border-win-border bg-win-card px-5 py-4 hover:bg-win-hover transition-colors text-left w-full cursor-pointer"
          >
            <div className="flex items-center gap-2.5">
              <svg
                width="12"
                height="12"
                viewBox="0 0 12 12"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.5"
                className={`shrink-0 text-win-text-tertiary transition-transform ${isExpanded ? 'rotate-90' : ''}`}
              >
                <polyline points="4,2 8,6 4,10" />
              </svg>
              <span className={`text-sm font-medium text-win-text ${isExpanded ? '' : 'truncate'}`}>{skill.name}</span>
              {skill.userInvocable && (
                <span className="shrink-0 rounded-md bg-green-50 px-2.5 py-1 text-xs text-green-700 font-medium border border-green-200">
                  invocable
                </span>
              )}
            </div>
            {skill.description && (
              <p className={`mt-1.5 pl-[22px] text-sm leading-relaxed text-win-text-secondary ${isExpanded ? '' : 'line-clamp-2'}`}>
                {skill.description}
              </p>
            )}
            {isExpanded && (
              <p className="mt-2 pl-[22px] text-xs text-win-text-tertiary break-all">
                {skill.filePath}
              </p>
            )}
          </button>
        )
      })}
    </div>
  )
}
