import { createEffect, createSignal, For, Show, type Component } from "solid-js"
import { File, Folder, GitBranch, PanelRightClose } from "lucide-solid"
import { api } from "../lib/api"
import { workspaceName } from "../lib/providers"
import type { WorkspaceEntry } from "../lib/types"

export const FileTree: Component<{ workspace: string; onClose: () => void }> = (props) => {
  const [entries, setEntries] = createSignal<WorkspaceEntry[]>([])

  createEffect(() => {
    const workspace = props.workspace
    api.workspaceEntries(workspace).then(setEntries).catch(() => setEntries([]))
  })

  return (
    <aside class="file-panel">
      <header class="file-panel-header">
        <div><Folder size={14} /><span>{workspaceName(props.workspace)}</span></div>
        <button class="icon-button" onClick={props.onClose} aria-label="Close file tree"><PanelRightClose size={15} /></button>
      </header>
      <div class="file-tree">
        <Show when={entries().length > 0} fallback={<div class="file-empty">No files to show</div>}>
          <For each={entries()}>
            {(entry) => (
              <div class="file-row" style={{ "padding-left": `${12 + Math.min(entry.depth, 3) * 14}px` }} title={entry.path}>
                <Show when={entry.isDirectory} fallback={<File size={13} />}><Folder size={13} /></Show>
                <span>{entry.name}</span>
              </div>
            )}
          </For>
        </Show>
      </div>
      <footer class="file-panel-footer"><GitBranch size={12} /> workspace</footer>
    </aside>
  )
}
