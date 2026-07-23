import type { JSX } from "solid-js"

/** Surfaces that can live side-by-side as tabs in the workspace inspector. */
export type WorkspaceSurfaceKind = "browser" | "terminal" | "files" | "diff"

/**
 * Provider-neutral description of a right-panel tab.
 *
 * The resource identifier deliberately stays opaque so the parent can map a
 * browser webview, terminal process, file path, or diff snapshot without
 * leaking backend protocol objects into the interface.
 */
export interface WorkspaceSurface {
  id: string
  kind: WorkspaceSurfaceKind
  title: string
  resourceId?: string
  pending?: boolean
  dirty?: boolean
}

export interface WorkspaceSurfaceAvailability {
  browser: boolean
  terminal: boolean
  files: boolean
  diff: boolean
}

export interface WorkspaceTerminal {
  id: string
  title: string
  cwd?: string
  status?: "starting" | "running" | "exited"
  /** Optional presentation-only fallback while a native terminal is not mounted. */
  lines?: readonly string[]
}

export interface WorkspaceActionState {
  disabled?: boolean
  busy?: boolean
  label?: string
  hint?: string
}

export type SurfaceRenderer = (surface: WorkspaceSurface) => JSX.Element
export type TerminalRenderer = (terminal: WorkspaceTerminal) => JSX.Element

export const ALL_WORKSPACE_SURFACE_KINDS: readonly WorkspaceSurfaceKind[] = [
  "browser",
  "terminal",
  "files",
  "diff",
]

export const DEFAULT_SURFACE_AVAILABILITY: WorkspaceSurfaceAvailability = {
  browser: true,
  terminal: true,
  files: true,
  diff: true,
}

export const WORKSPACE_SURFACE_COPY: Record<
  WorkspaceSurfaceKind,
  { label: string; description: string }
> = {
  browser: { label: "Browser", description: "Open a local app or URL." },
  terminal: { label: "Terminal", description: "Start a shell in this workspace." },
  files: { label: "Files", description: "Browse and read workspace files." },
  diff: { label: "Diff", description: "Review changes in this workspace." },
}
