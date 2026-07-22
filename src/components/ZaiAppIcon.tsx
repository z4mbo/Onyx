import type { Component } from "solid-js"

/** Original zAI retro mark, shared visually with the generated desktop icons. */
export const ZaiAppIcon: Component<{ class?: string }> = (props) => (
  <svg
    class={`zai-app-icon${props.class ? ` ${props.class}` : ""}`}
    viewBox="0 0 512 512"
    role="img"
    aria-label="zAI"
    shape-rendering="crispEdges"
  >
    <rect x="4" y="4" width="504" height="504" rx="100" fill="#050505" />
    <g fill="#fff">
      <path d="M32 144h156v28H32zm128 32h28v28h-28zm-32 32h28v28h-28zm-32 32h28v28H96zm-32 32h28v28H64zm-32 32h156v28H32z" />
      <path d="M220 144h92v28h-92zm-28 32h28v156h-28zm120 0h28v156h-28zm28 128H192v-28h148z" />
      <path d="M364 144h116v28H364zm44 32h28v128h-28zm-44 128h116v28H364z" />
    </g>
  </svg>
)
