import { GitCommit, LoaderCircle, X } from "lucide-solid"
import { For, Show, createEffect, createSignal, onCleanup, onMount, type Component } from "solid-js"
import type { RepoSummary } from "../../lib/types"

export interface GitCommitDialogProps {
  open: boolean
  summary: RepoSummary | null
  busy?: boolean
  onClose: () => void
  onCommit: (message: string | null) => void | Promise<void>
}

/** Explicit staging/commit confirmation for the T3-style Git action. */
export const GitCommitDialog: Component<GitCommitDialogProps> = (props) => {
  const [message, setMessage] = createSignal("")
  let dialog: HTMLElement | undefined
  let input: HTMLInputElement | undefined
  let restoreFocus: HTMLElement | null = null

  createEffect(() => {
    if (!props.open) return
    restoreFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
    setMessage("")
    queueMicrotask(() => input?.focus())
  })

  onMount(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!props.open) return
      if (event.key === "Escape" && !props.busy) {
        event.preventDefault()
        props.onClose()
        return
      }
      if (event.key !== "Tab" || !dialog) return
      const controls = Array.from(
        dialog.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), [tabindex="0"]'),
      )
      if (controls.length === 0) return
      const first = controls[0]
      const last = controls[controls.length - 1]
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }
    window.addEventListener("keydown", onKeyDown)
    onCleanup(() => window.removeEventListener("keydown", onKeyDown))
  })

  createEffect(() => {
    if (props.open) return
    const target = restoreFocus
    restoreFocus = null
    queueMicrotask(() => target?.focus())
  })

  const submit = async () => {
    if (props.busy) return
    const value = message().trim()
    await props.onCommit(value || null)
  }

  return (
    <Show when={props.open}>
      <div
        class="zai-git-dialog-scrim"
        onMouseDown={(event) => {
          if (event.target === event.currentTarget && !props.busy) props.onClose()
        }}
      >
        <section
          ref={dialog}
          class="zai-git-dialog"
          role="dialog"
          aria-modal="true"
          aria-labelledby="zai-git-commit-title"
          aria-describedby="zai-git-commit-description"
          aria-busy={props.busy}
        >
          <header>
            <span class="zai-git-dialog__icon" aria-hidden="true"><GitCommit /></span>
            <div>
              <h2 id="zai-git-commit-title">Commit changes</h2>
              <p id="zai-git-commit-description">Review and commit every current workspace change.</p>
            </div>
            <button type="button" aria-label="Close" disabled={props.busy} onClick={props.onClose}><X aria-hidden="true" /></button>
          </header>

          <div class="zai-git-dialog__body">
            <div class="zai-git-dialog__summary">
              <span>{props.summary?.changedFiles.length ?? 0} changed files</span>
              <span>{props.summary?.stagedCount ?? 0} staged</span>
              <span>{props.summary?.untrackedCount ?? 0} untracked</span>
            </div>
            <div class="zai-git-dialog__files">
              <For each={props.summary?.changedFiles ?? []}>
                {(file) => <div><code>{file.status}</code><span title={file.path}>{file.path}</span></div>}
              </For>
            </div>
            <label>
              <span>Commit message <small>optional</small></span>
              <input
                ref={input}
                value={message()}
                maxlength={240}
                autocomplete="off"
                placeholder="Leave blank to generate a concise workspace message"
                disabled={props.busy}
                onInput={(event) => setMessage(event.currentTarget.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && !event.isComposing) {
                    event.preventDefault()
                    void submit()
                  }
                }}
              />
            </label>
          </div>

          <footer>
            <button type="button" class="zai-git-dialog__cancel" disabled={props.busy} onClick={props.onClose}>Cancel</button>
            <button
              type="button"
              class="zai-git-dialog__commit"
              disabled={props.busy || (props.summary?.changedFiles.length ?? 0) === 0}
              onClick={() => void submit()}
            >
              <Show when={props.busy}><LoaderCircle class="spin" aria-hidden="true" /></Show>
              {props.busy ? "Committing…" : "Commit all changes"}
            </button>
          </footer>
        </section>
      </div>
    </Show>
  )
}
