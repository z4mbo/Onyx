import { createMemo, createSignal, For, Show, type Component } from "solid-js"
import {
  CircleHelp,
  MessageSquare,
  Mic2,
  FolderPlus,
  GitBranch,
  Search,
  Settings,
  SquarePen,
  Trash2,
} from "lucide-solid"
import { workspaceName } from "../lib/providers"
import type { AgentSession } from "../lib/types"

type Project = { path: string; name: string; sessions: AgentSession[] }

export const HomeView: Component<{
  sessions: AgentSession[]
  draftWorkspace: string
  onNew: (workspace?: string) => void
  onSelect: (id: string) => void
  onDelete: (id: string) => void
  onChooseWorkspace: () => void
  onSettings: () => void
  onChat: () => void
  onVoice: () => void
}> = (props) => {
  const [query, setQuery] = createSignal("")

  const projects = createMemo<Project[]>(() => {
    const paths = new Map<string, AgentSession[]>()
    for (const session of props.sessions) {
      paths.set(session.workspace, [...(paths.get(session.workspace) ?? []), session])
    }
    if (props.draftWorkspace && !paths.has(props.draftWorkspace)) paths.set(props.draftWorkspace, [])
    return [...paths].map(([path, sessions]) => ({ path, name: workspaceName(path), sessions }))
  })

  const filteredProjects = createMemo(() => {
    const needle = query().trim().toLowerCase()
    return projects().map((project) => ({
      ...project,
      sessions: needle ? project.sessions.filter((session) =>
        `${session.title} ${session.workspace} ${session.provider} ${session.model ?? ""}`.toLowerCase().includes(needle),
      ) : project.sessions,
    })).filter((project) => project.sessions.length > 0)
  })

  return (
    <section class="zai-page-frame zai-home-view">
      <div class="zai-home-layout">
        <aside class="zai-home-sidebar">
          <div class="zai-home-projects-heading">
            <h2>Projects</h2>
            <button class="zai-icon-button" aria-label="Add project" onClick={props.onChooseWorkspace}>
              <FolderPlus size={17} stroke-width={1.7} />
            </button>
          </div>

          <div class="zai-home-project-list">
            <Show
              when={projects().length > 0}
              fallback={
                <button class="zai-home-project zai-home-project--empty" onClick={props.onChooseWorkspace}>
                  <span class="zai-project-avatar">+</span>
                  <span>Add project</span>
                </button>
              }
            >
              <For each={projects()}>
                {(project) => (
                  <div class="zai-home-project-row">
                    <button class="zai-home-project" onClick={() => props.onNew(project.path)}>
                      <span class="zai-project-avatar">{project.name.slice(0, 1).toUpperCase()}</span>
                      <span>{project.name}</span>
                    </button>
                    <button
                      class="zai-home-project-action"
                      aria-label={`New session in ${project.name}`}
                      onClick={() => props.onNew(project.path)}
                    >
                      <SquarePen size={14} />
                    </button>
                  </div>
                )}
              </For>
            </Show>
          </div>

          <nav class="zai-home-nav" aria-label="Application">
            <button onClick={props.onChat}><MessageSquare size={15} stroke-width={1.7} /><span>Chat</span></button>
            <button onClick={props.onVoice}><Mic2 size={15} stroke-width={1.7} /><span>Voice history</span></button>
            <button onClick={props.onSettings}><Settings size={15} stroke-width={1.7} /><span>Settings</span></button>
            <a href="https://github.com/z4mbo/Onyx#readme" target="_blank">
              <CircleHelp size={15} stroke-width={1.7} /><span>Help</span>
            </a>
          </nav>
        </aside>

        <div class="zai-home-main">
          <label class="zai-home-search">
            <Search size={16} stroke-width={1.6} />
            <input
              value={query()}
              onInput={(event) => setQuery(event.currentTarget.value)}
              placeholder="Search sessions"
              aria-label="Search sessions"
            />
          </label>

          <div class="zai-home-results">
            <Show
              when={filteredProjects().length > 0}
              fallback={
                <div class="zai-home-empty">
                  <strong>{query() ? "No matching sessions" : "Nothing here yet"}</strong>
                  <span>{query() ? "Try another search" : "Create a session to get started"}</span>
                  <Show when={!query()}>
                    <button class="zai-neutral-button" onClick={() => props.onNew()}>
                      <SquarePen size={15} /> New session
                    </button>
                  </Show>
                </div>
              }
            >
              <div class="zai-session-history">
                <For each={filteredProjects()}>{(project) => <section class="onyx-project-sessions">
                  <header><span class="zai-project-avatar">{project.name.slice(0, 1).toUpperCase()}</span><div><strong>{project.name}</strong><small>{project.sessions.length} session{project.sessions.length === 1 ? "" : "s"}</small></div><button onClick={() => props.onNew(project.path)} aria-label={`New session in ${project.name}`}><SquarePen size={14} /></button></header>
                  <For each={project.sessions}>{(session) => (
                      <div class="zai-session-history-item">
                        <button class="zai-session-history-row" onClick={() => props.onSelect(session.id)}>
                          <div class="zai-session-history-copy"><strong>{session.title}</strong><span>{session.model ?? "Default model"}</span></div>
                          <div class="zai-session-history-meta"><GitBranch size={13} /><span>{session.provider}</span></div>
                        </button>
                        <button type="button" class="zai-session-history-delete" aria-label={`Delete ${session.title}`} title="Delete session" onClick={() => props.onDelete(session.id)}><Trash2 aria-hidden="true" /></button>
                      </div>
                  )}</For>
                </section>}</For>
              </div>
            </Show>
          </div>
        </div>
      </div>
    </section>
  )
}
