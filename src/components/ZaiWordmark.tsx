import type { Component, ComponentProps } from "solid-js"

export interface ZaiWordmarkProps extends Pick<ComponentProps<"svg">, "class" | "aria-label"> {}

export const ZaiWordmark: Component<ZaiWordmarkProps> = (props) => (
  <svg class={`zai-wordmark${props.class ? ` ${props.class}` : ""}`} viewBox="0 0 720 150" role="img" aria-label={props["aria-label"] ?? "Onyx"}>
    <defs>
      <radialGradient id="onyx-wordmark-orb" cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse" gradientTransform="translate(92 48) rotate(48) scale(92)">
        <stop stop-color="#c8feff" />
        <stop offset=".24" stop-color="#61d8f3" />
        <stop offset=".57" stop-color="#7072ef" />
        <stop offset=".84" stop-color="#6138c1" />
        <stop offset="1" stop-color="#211746" />
      </radialGradient>
      <linearGradient id="onyx-wordmark-fade" x1="391" y1="22" x2="391" y2="137" gradientUnits="userSpaceOnUse">
        <stop stop-color="currentColor" stop-opacity=".2" />
        <stop offset="1" stop-color="currentColor" stop-opacity=".04" />
      </linearGradient>
    </defs>
    <circle cx="102" cy="75" r="43" fill="url(#onyx-wordmark-orb)" />
    <ellipse cx="87" cy="58" rx="11" ry="7" fill="#fff" fill-opacity=".32" transform="rotate(-24 87 58)" />
    <text x="170" y="112" fill="url(#onyx-wordmark-fade)" font-family="DM Sans, Inter, system-ui, sans-serif" font-size="112" font-weight="650" letter-spacing="-7">Onyx</text>
  </svg>
)
