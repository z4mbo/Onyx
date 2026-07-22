import type { Component } from "solid-js"
import { providerMeta } from "../lib/providers"
import type { ProviderId } from "../lib/types"

export const ProviderBadge: Component<{ provider: ProviderId; size?: "sm" | "md" }> = (props) => {
  const meta = () => providerMeta[props.provider]
  return (
    <span
      class={`provider-badge ${props.size === "sm" ? "provider-badge-sm" : ""}`}
      style={{ "--provider-color": meta().color }}
      aria-label={meta().name}
    >
      {meta().short}
    </span>
  )
}
