import { PanelBottom, PanelRight } from "lucide-solid"
import type { Component } from "solid-js"

export interface PanelLayoutControlsProps {
  bottomPanelOpen: boolean
  rightPanelOpen: boolean
  bottomPanelAvailable?: boolean
  rightPanelAvailable?: boolean
  bottomPanelShortcut?: string
  rightPanelShortcut?: string
  onToggleBottomPanel: () => void
  onToggleRightPanel: () => void
}

/** The paired 28px layout toggles from the right edge of T3 Code's top bar. */
export const PanelLayoutControls: Component<PanelLayoutControlsProps> = (props) => {
  const bottomLabel = () =>
    `Toggle terminal drawer${props.bottomPanelShortcut ? ` (${props.bottomPanelShortcut})` : ""}`
  const rightLabel = () =>
    `Toggle right panel${props.rightPanelShortcut ? ` (${props.rightPanelShortcut})` : ""}`

  return (
    <div class="zai-layout-controls" data-slot="workspace-layout-controls">
      <button
        type="button"
        class="zai-workspace-icon-button"
        data-active={props.bottomPanelOpen ? "true" : "false"}
        aria-label={bottomLabel()}
        aria-pressed={props.bottomPanelOpen}
        title={bottomLabel()}
        disabled={props.bottomPanelAvailable === false}
        onClick={props.onToggleBottomPanel}
      >
        <PanelBottom aria-hidden="true" />
      </button>
      <button
        type="button"
        class="zai-workspace-icon-button"
        data-active={props.rightPanelOpen ? "true" : "false"}
        aria-label={rightLabel()}
        aria-pressed={props.rightPanelOpen}
        title={rightLabel()}
        disabled={props.rightPanelAvailable === false}
        onClick={props.onToggleRightPanel}
      >
        <PanelRight aria-hidden="true" />
      </button>
    </div>
  )
}
