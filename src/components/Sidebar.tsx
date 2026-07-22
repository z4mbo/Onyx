import { createMemo, createSignal, For, Show, type Component } from "solid-js"
import { ChevronDown, MessageSquarePlus, Search, Settings2 } from "lucide-solid"
import { ProviderBadge } from "./ProviderBadge"
import { workspaceName } from "../lib/providers"
import type { AgentSession } from "../lib/types"

export const Sidebar: Component<{
  sessions: AgentSession[]
  currentId: string | null
  onSelect: (id: string) => void
  onNew: () => void
  onSettings: () => void
}> = (props) => {
  const [query, setQuery] = createSignal("")
  const filtered = createMemo(() => {
    const value = query().trim().toLowerCase()
    if (!value) return props.sessions
    return props.sessions.filter((session) =>
      `${session.title} ${session.workspace} ${session.provider}`.toLowerCase().includes(value),
    )
  })

  return (
  <aside class="sidebar">
    <div class="sidebar-drag" data-tauri-drag-region />
    <div class="brand-row">
      <img src="/zai.svg" class="brand-icon" alt="" />
      <span class="brand-name">zAI</span>
      <button class="icon-button brand-menu" aria-label="Workspace menu"><ChevronDown size={14} /></button>
    </div>

    <button class="new-session-button" onClick={props.onNew}>
      <MessageSquarePlus size={15} />
      <span>New session</span>
      <kbd>⌘N</kbd>
    </button>

    <div class="sidebar-search">
      <Search size={14} />
      <input
        aria-label="Search sessions"
        placeholder="Search"
        value={query()}
        onInput={(event) => setQuery(event.currentTarget.value)}
      />
    </div>

    <div class="sidebar-section-label">Sessions</div>
    <div class="session-list">
      <Show when={filtered().length > 0} fallback={<div class="empty-list">{query() ? "No matching sessions" : "No sessions yet"}</div>}>
        <For each={filtered()}>
          {(session) => (
            <button
              class="session-row"
              classList={{ active: session.id === props.currentId }}
              onClick={() => props.onSelect(session.id)}
            >
              <ProviderBadge provider={session.provider} size="sm" />
              <span class="session-row-copy">
                <span class="session-row-title">{session.title}</span>
                <span class="session-row-meta">
                  {workspaceName(session.workspace)}
                  <Show when={session.status === "running"}><span class="running-dot" /></Show>
                </span>
              </span>
            </button>
          )}
        </For>
      </Show>
    </div>

    <button class="sidebar-settings" onClick={props.onSettings}>
      <Settings2 size={15} />
      <span>Settings</span>
    </button>
  </aside>
  )
}
