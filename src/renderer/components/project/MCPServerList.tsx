import { useState, useCallback } from 'react'
import { useProjectStore } from '@/stores/project-store'
import type { McpServer, MCPServer } from '@/lib/api'
import * as api from '@/lib/api'
import MCPServerForm from '@/components/project/MCPServerForm'

interface MCPServerEntry {
  name: string
  server: McpServer
  enabled: boolean
}

/**
 * Displays the list of configured MCP servers for the active project.
 */
export default function MCPServerList() {
  const activeProject = useProjectStore((s) => s.activeProject)

  const [servers, setServers] = useState<MCPServerEntry[]>([])
  const [editingServer, setEditingServer] = useState<MCPServerEntry | null>(null)
  const [showForm, setShowForm] = useState(false)

  const loadServers = useCallback(async () => {
    if (!activeProject) return
    try {
      const result = await api.listMcpServers(activeProject.name)
      const entries: MCPServerEntry[] = Object.entries(result as Record<string, McpServer>).map(
        ([name, server]) => ({
          name,
          server,
          enabled: true
        })
      )
      setServers(entries)
    } catch {
      setServers([])
    }
  }, [activeProject])

  const handleAdd = useCallback(() => {
    setEditingServer(null)
    setShowForm(true)
  }, [])

  const handleEdit = useCallback((entry: MCPServerEntry) => {
    setEditingServer(entry)
    setShowForm(true)
  }, [])

  const handleRemove = useCallback(
    async (name: string) => {
      if (!activeProject) return
      await api.removeMcpServer(activeProject.name, name)
      await loadServers()
    },
    [activeProject, loadServers]
  )

  const handleSave = useCallback(async (server: MCPServer) => {
    if (!activeProject) return
    const value: McpServer = {
      command: server.command,
      args: server.args,
      env: server.env
    }
    if (editingServer) {
      await api.updateMcpServer(activeProject.name, editingServer.name, value)
    } else {
      await api.addMcpServer(activeProject.name, server.name, value)
    }
    setShowForm(false)
    setEditingServer(null)
    await loadServers()
  }, [activeProject, editingServer, loadServers])

  const handleCancel = useCallback(() => {
    setShowForm(false)
    setEditingServer(null)
  }, [])

  if (!activeProject) {
    return (
      <div className="p-4 text-xs text-win-text-tertiary">
        Select a project to manage MCP servers.
      </div>
    )
  }

  if (showForm) {
    const formServer: MCPServer | null = editingServer
      ? {
          id: editingServer.name,
          name: editingServer.name,
          enabled: editingServer.enabled,
          ...editingServer.server
        }
      : null
    return <MCPServerForm server={formServer} onSave={handleSave} onCancel={handleCancel} />
  }

  return (
    <div className="flex flex-col gap-2 p-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium text-win-text">MCP Servers</h3>
        <button
          onClick={handleAdd}
          className="rounded-md border border-win-border bg-win-card px-2.5 py-1 text-xs text-win-text-secondary hover:bg-win-hover hover:text-win-text transition-colors"
        >
          + Add Server
        </button>
      </div>

      {servers.length === 0 && (
        <p className="py-4 text-center text-xs text-win-text-tertiary">
          No MCP servers configured.
        </p>
      )}

      <div className="flex flex-col gap-1">
        {servers.map((entry) => (
          <div
            key={entry.name}
            className="flex items-center justify-between rounded-lg border border-win-border bg-win-card px-4 py-3 hover:bg-win-hover transition-colors"
          >
            <div className="flex flex-col gap-0.5 overflow-hidden">
              <div className="flex items-center gap-2">
                <span
                  className={`h-2 w-2 shrink-0 rounded-full ${
                    entry.enabled ? 'bg-green-500' : 'bg-win-text-tertiary'
                  }`}
                />
                <span className="text-xs font-medium text-win-text truncate">
                  {entry.name}
                </span>
              </div>
              <span className="pl-4 text-[10px] text-win-text-tertiary truncate">
                {entry.server.command} {(entry.server.args ?? []).join(' ')}
              </span>
            </div>

            <div className="flex items-center gap-1 shrink-0 ml-2">
              <button
                onClick={() => handleEdit(entry)}
                className="rounded-md px-1.5 py-0.5 text-[10px] text-win-text-tertiary hover:bg-win-hover hover:text-win-text transition-colors"
              >
                Edit
              </button>
              <button
                onClick={() => handleRemove(entry.name)}
                className="rounded-md px-1.5 py-0.5 text-[10px] text-red-500 hover:bg-red-50 hover:text-red-600 transition-colors"
              >
                Remove
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
