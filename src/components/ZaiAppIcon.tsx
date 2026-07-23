import type { Component } from "solid-js"
import { OnyxOrb } from "./OnyxOrb"

export const ZaiAppIcon: Component<{ class?: string }> = (props) => (
  <OnyxOrb class={`zai-app-icon${props.class ? ` ${props.class}` : ""}`} label="Onyx" />
)
