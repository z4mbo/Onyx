import { FileDiff, Files, Globe2, TerminalSquare } from "lucide-solid"
import { Match, Switch, type Component } from "solid-js"
import type { WorkspaceSurfaceKind } from "./types"

export interface SurfaceIconProps {
  kind: WorkspaceSurfaceKind
  class?: string
}

/** Lucide glyphs used by T3 Code for the corresponding panel surfaces. */
export const SurfaceIcon: Component<SurfaceIconProps> = (props) => (
  <Switch>
    <Match when={props.kind === "browser"}>
      <Globe2 class={props.class} aria-hidden="true" />
    </Match>
    <Match when={props.kind === "terminal"}>
      <TerminalSquare class={props.class} aria-hidden="true" />
    </Match>
    <Match when={props.kind === "files"}>
      <Files class={props.class} aria-hidden="true" />
    </Match>
    <Match when={props.kind === "diff"}>
      <FileDiff class={props.class} aria-hidden="true" />
    </Match>
  </Switch>
)
