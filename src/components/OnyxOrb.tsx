import type { Component } from "solid-js"

export const OnyxOrb: Component<{ class?: string; label?: string }> = (props) => (
  <svg
    class={`onyx-orb${props.class ? ` ${props.class}` : ""}`}
    viewBox="0 0 64 64"
    role={props.label ? "img" : undefined}
    aria-label={props.label}
    aria-hidden={props.label ? undefined : "true"}
  >
    <defs>
      <radialGradient id="onyx-orb-core" cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse" gradientTransform="translate(22 17) rotate(49) scale(55)">
        <stop stop-color="#b9fbff" />
        <stop offset=".24" stop-color="#62d9f4" />
        <stop offset=".56" stop-color="#6f72ef" />
        <stop offset=".82" stop-color="#633ac6" />
        <stop offset="1" stop-color="#25194e" />
      </radialGradient>
      <radialGradient id="onyx-orb-glow" cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse" gradientTransform="translate(20 16) rotate(57) scale(31)">
        <stop stop-color="#fff" stop-opacity=".82" />
        <stop offset=".38" stop-color="#c9fcff" stop-opacity=".24" />
        <stop offset="1" stop-color="#fff" stop-opacity="0" />
      </radialGradient>
      <linearGradient id="onyx-orb-rim" x1="12" y1="9" x2="51" y2="56" gradientUnits="userSpaceOnUse">
        <stop stop-color="#eaffff" stop-opacity=".7" />
        <stop offset=".46" stop-color="#8ddcf5" stop-opacity=".16" />
        <stop offset="1" stop-color="#160b37" stop-opacity=".65" />
      </linearGradient>
    </defs>
    <circle cx="32" cy="32" r="29" fill="url(#onyx-orb-core)" />
    <circle cx="32" cy="32" r="29" fill="url(#onyx-orb-glow)" />
    <circle cx="32" cy="32" r="28.5" fill="none" stroke="url(#onyx-orb-rim)" />
    <path d="M13 34c7-3 10-2 16 1 8 4 14 4 23-1" fill="none" stroke="#c8f8ff" stroke-opacity=".18" stroke-width="2" stroke-linecap="round" />
    <ellipse cx="22" cy="16" rx="7" ry="4.5" fill="#fff" fill-opacity=".28" transform="rotate(-25 22 16)" />
  </svg>
)
