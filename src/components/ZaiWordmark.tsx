import type { Component, ComponentProps } from "solid-js"

export interface ZaiWordmarkProps extends Pick<ComponentProps<"svg">, "class" | "aria-label"> {}

export const ZaiWordmark: Component<ZaiWordmarkProps> = (props) => (
  <svg class={`zai-wordmark${props.class ? ` ${props.class}` : ""}`} viewBox="0 0 720 150" role="img" aria-label={props["aria-label"] ?? "Onyx"}>
    <defs>
      <linearGradient id="onyx-wordmark-fade" x1="391" y1="22" x2="391" y2="137" gradientUnits="userSpaceOnUse">
        <stop stop-color="currentColor" stop-opacity=".2" />
        <stop offset="1" stop-color="currentColor" stop-opacity=".04" />
      </linearGradient>
    </defs>
    <text x="360" y="112" text-anchor="middle" fill="url(#onyx-wordmark-fade)" font-family="DM Sans, Inter, system-ui, sans-serif" font-size="112" font-weight="650" letter-spacing="-7">Onyx</text>
  </svg>
)
