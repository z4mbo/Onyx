import { useState, useEffect, useRef, useMemo, useCallback } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { useProjectStore } from '@/stores/project-store'
import { useTerminalStore } from '@/stores/terminal-store'
import { useSettingsStore } from '@/stores/settings-store'
import * as api from '@/lib/api'

/**
 * Strips markdown formatting to produce plain text suitable for terminal input.
 */
function stripMarkdown(md: string): string {
  return md
    .replace(/\*\*(.+?)\*\*/g, '$1')
    .replace(/\*(.+?)\*/g, '$1')
    .replace(/_(.+?)_/g, '$1')
    .replace(/`(.+?)`/g, '$1')
    .replace(/\[(.+?)\]\(.+?\)/g, '$1')
    .replace(/^#+\s+/gm, '')
    .replace(/^[-*]\s+/gm, '')
    .replace(/^\d+\.\s+/gm, '')
    .trim()
}

function ChatBubble({
  content,
  index,
  activeTerminalId
}: {
  content: string
  index: number
  activeTerminalId: string | null
}) {
  const handleSend = useCallback(() => {
    if (!activeTerminalId) return
    const plainText = stripMarkdown(content)
      .replace(/\r?\n/g, ' ')
      .replace(/\s{2,}/g, ' ')
      .trim()
    api.ptyWrite(activeTerminalId, plainText + '\r')
  }, [activeTerminalId, content])

  return (
    <div className="flex flex-col items-start gap-1" style={{ animationDelay: `${index * 80}ms` }}>
      {/* SMS Bubble */}
      <div className="relative max-w-[90%] rounded-2xl rounded-bl-sm px-4 py-2.5 border border-win-border">
        {/* Tail */}
        <div className="absolute -bottom-0 -left-1.5 w-3 h-3 overflow-hidden">
          <div className="absolute -right-2 top-0 w-4 h-4 rounded-br-xl" />
        </div>
        <div className="prose prose-sm max-w-none
          prose-p:text-sm prose-p:leading-relaxed prose-p:text-win-text prose-p:m-0
          prose-a:text-win-accent prose-a:no-underline hover:prose-a:underline
          prose-code:text-xs prose-code:bg-white/60 prose-code:px-1.5 prose-code:py-0.5 prose-code:rounded prose-code:text-win-accent-dark prose-code:font-mono prose-code:border prose-code:border-win-border/50
          prose-pre:bg-white/50 prose-pre:border prose-pre:border-win-border/50 prose-pre:rounded prose-pre:text-xs
          prose-ul:text-sm prose-ul:text-win-text prose-ul:my-1 prose-ol:text-sm prose-ol:text-win-text prose-ol:my-1
          prose-li:text-sm prose-li:text-win-text prose-li:my-0.5
          prose-strong:text-win-text
          prose-em:text-win-text-secondary">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{content}</ReactMarkdown>
        </div>
      </div>
      {/* Send to chat button */}
      {activeTerminalId && (
        <button
          onClick={handleSend}
          className="flex items-center gap-1.5 rounded-full border border-win-border bg-win-card px-3 py-1
            text-[11px] font-medium text-win-text-secondary hover:bg-win-accent hover:text-white hover:border-win-accent transition-colors cursor-pointer"
        >
          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <line x1="19" y1="12" x2="5" y2="12" />
            <polyline points="12 19 5 12 12 5" />
          </svg>
          Send
        </button>
      )}
    </div>
  )
}

export default function TipsPanel() {
  const activeProject = useProjectStore((s) => s.activeProject)
  const activeTerminalId = useTerminalStore((s) => s.activeTerminalId)
  const [content, setContent] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const prevPathRef = useRef<string | null>(null)

  useEffect(() => {
    if (!activeProject) {
      setContent(null)
      prevPathRef.current = null
      return
    }

    const projectPath = activeProject.path
    const tipsPath = `${projectPath}/tips.md`

    const load = async () => {
      setLoading(true)
      try {
        const text = await api.readFile(tipsPath)
        setContent(text)
      } catch {
        setContent(null)
      } finally {
        setLoading(false)
      }
    }

    load()

    api.fsWatch(projectPath)
    prevPathRef.current = projectPath

    const unsub = api.onFsChanged((rootPath, changedDir) => {
      if (rootPath !== projectPath) return
      const normalizedChanged = changedDir.replace(/\\/g, '/')
      const normalizedRoot = projectPath.replace(/\\/g, '/')
      if (normalizedChanged === normalizedRoot || changedDir === projectPath) {
        api.readFile(tipsPath).then((text) => {
          setContent(text)
          if (text !== null) {
            useSettingsStore.getState().setRightPanelActiveTab('tips')
          }
        }).catch(() => setContent(null))
      }
    })

    return () => {
      unsub()
      void api.fsUnwatch(projectPath)
    }
  }, [activeProject])

  const messages = useMemo(() => {
    if (!content) return []
    return content
      .split(/\n---\n/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0)
  }, [content])

  if (!activeProject) {
    return (
      <div className="p-4 text-sm text-win-text-tertiary">
        Select a project to see suggestions.
      </div>
    )
  }

  if (loading) {
    return (
      <div className="p-4 text-sm text-win-text-tertiary">Loading tips...</div>
    )
  }

  if (content === null) {
    return (
      <div className="flex flex-col items-center gap-2 p-6 text-center">
        <div className="text-win-text-tertiary">
          <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
            <polyline points="14 2 14 8 20 8" />
            <line x1="16" y1="13" x2="8" y2="13" />
            <line x1="16" y1="17" x2="8" y2="17" />
            <polyline points="10 9 9 9 8 9" />
          </svg>
        </div>
        <p className="text-sm text-win-text-secondary">No tips.md found.</p>
        <p className="text-xs text-win-text-tertiary">
          Create a <code className="rounded bg-win-hover px-1 border border-win-border">tips.md</code> in your project root to see tips here.
        </p>
      </div>
    )
  }

  if (messages.length === 0) {
    return (
      <div className="flex flex-col items-center gap-2 p-6 text-center">
        <div className="text-win-text-tertiary">
          <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
          </svg>
        </div>
        <p className="text-sm text-win-text-secondary">No suggestions yet</p>
        <p className="text-xs text-win-text-tertiary">
          Suggestions will appear here as you chat with your AI assistant.
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-3 p-4 pr-3 overflow-y-auto">
      {messages.map((msg, i) => (
        <ChatBubble key={i} content={msg} index={i} activeTerminalId={activeTerminalId} />
      ))}
    </div>
  )
}
