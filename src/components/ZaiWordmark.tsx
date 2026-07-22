import { Show, createUniqueId, type Component, type ComponentProps } from "solid-js"

export interface ZaiWordmarkProps extends Pick<ComponentProps<"svg">, "class" | "aria-label"> {}

/**
 * Original zAI block wordmark for the oversized new-session backdrop.
 *
 * Its 720×129 canvas and low-opacity vertical fade match the surrounding v2
 * layout proportions without reusing OpenCode logo geometry or assets.
 */
export const ZaiWordmark: Component<ZaiWordmarkProps> = (props) => {
  const maskId = `zai-wordmark-mask-${createUniqueId()}`
  const gradientId = `zai-wordmark-gradient-${createUniqueId()}`
  const label = () => props["aria-label"]

  return (
    <svg
      class={`zai-wordmark${props.class ? ` ${props.class}` : ""}`}
      data-component="zai-wordmark"
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 720 129"
      fill="none"
      preserveAspectRatio="xMidYMid meet"
      role={label() ? "img" : undefined}
      aria-label={label()}
      aria-hidden={label() ? undefined : "true"}
    >
      <Show when={label()}>{(title) => <title>{title()}</title>}</Show>
      <g opacity="0.6" mask={`url(#${maskId})`}>
        <g opacity="0.16" fill="currentColor">
          {/* Lowercase z: intentionally original, rectilinear zAI geometry. */}
          <path opacity="0.7" d="M0 36H210V54H0V36ZM168 54H210L42 92H0L168 54ZM0 92H210V110H0V92Z" />

          {/* Capital A. */}
          <path opacity="0.7" d="M255 36H285V110H255V36ZM435 36H465V110H435V36ZM285 18H435V36H285V18ZM285 64H435V82H285V64Z" />

          {/* Capital I. */}
          <path opacity="0.7" d="M510 18H720V36H510V18ZM600 36H630V92H600V36ZM510 92H720V110H510V92Z" />
        </g>
      </g>
      <defs>
        <mask id={maskId} maskUnits="userSpaceOnUse" x="0" y="0" width="720" height="129">
          <rect width="720" height="129" fill={`url(#${gradientId})`} />
        </mask>
        <linearGradient id={gradientId} x1="360" y1="68" x2="360" y2="129" gradientUnits="userSpaceOnUse">
          <stop stop-color="white" stop-opacity="0.7" />
          <stop offset="1" stop-color="white" stop-opacity="0" />
        </linearGradient>
      </defs>
    </svg>
  )
}
