import "./workspace-panels.css"
import "./git-dialog.css"

export { BottomTerminalPanel, type BottomTerminalPanelProps } from "./BottomTerminalPanel"
export { PanelLayoutControls, type PanelLayoutControlsProps } from "./PanelLayoutControls"
export { RightWorkspacePanel, type RightWorkspacePanelProps } from "./RightWorkspacePanel"
export { SurfaceIcon, type SurfaceIconProps } from "./SurfaceIcon"
export { GitCommitDialog, type GitCommitDialogProps } from "./GitCommitDialog"
export {
  TerminalViewport,
  type TerminalViewportProps,
  clearTerminalViewport,
  forgetTerminalViewport,
  startTerminalViewportBridge,
} from "./TerminalViewport"
export { WorkspaceSurfaceView, type WorkspaceSurfaceViewProps } from "./WorkspaceSurfaceView"
export {
  WorkspaceTopbarActions,
  type WorkspaceGitActionName,
  type WorkspaceOpenOption,
  type WorkspaceTopbarActionsProps,
} from "./WorkspaceTopbarActions"
export {
  ALL_WORKSPACE_SURFACE_KINDS,
  DEFAULT_SURFACE_AVAILABILITY,
  WORKSPACE_SURFACE_COPY,
  type SurfaceRenderer,
  type TerminalRenderer,
  type WorkspaceActionState,
  type WorkspaceSurface,
  type WorkspaceSurfaceAvailability,
  type WorkspaceSurfaceKind,
  type WorkspaceTerminal,
} from "./types"
