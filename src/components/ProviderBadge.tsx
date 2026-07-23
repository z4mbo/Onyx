import type { Component } from "solid-js"
import { brandForRuntime, providerBrands } from "../lib/providers"
import type { ProviderBrand, ProviderId } from "../lib/types"

const DASHBOARD_ICONS_REVISION = "46b860c70e866212311aef2f98da3775c17f5068"
const DASHBOARD_ICONS_BASE = `https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons@${DASHBOARD_ICONS_REVISION}/svg`
const iconSlugs: Record<ProviderBrand, { light: string; dark?: string }> = {
  openai: { light: "openai", dark: "openai-light" },
  anthropic: { light: "anthropic", dark: "anthropic-dark" },
  google: { light: "google-gemini" },
  xai: { light: "grok", dark: "grok-dark" },
  moonshot: { light: "kimi-ai" },
  openrouter: { light: "open-router", dark: "open-router-dark" },
}

export const ProviderBadge: Component<{
  provider?: ProviderId
  brand?: ProviderBrand
  size?: "sm" | "md"
}> = (props) => {
  const brand = () => props.brand ?? brandForRuntime(props.provider ?? "openrouter")
  const meta = () => providerBrands.find((item) => item.id === brand())!
  const icons = () => iconSlugs[brand()]
  return (
    <span
      class={`provider-badge ${props.size === "sm" ? "provider-badge-sm" : ""}`}
      data-provider={props.provider}
      data-brand={brand()}
      style={{ "--provider-color": meta().color }}
      aria-label={meta().name}
    >
      <img class="provider-badge__icon provider-badge__icon--light" src={`${DASHBOARD_ICONS_BASE}/${icons().light}.svg`} alt="" draggable={false} />
      <img class="provider-badge__icon provider-badge__icon--dark" src={`${DASHBOARD_ICONS_BASE}/${icons().dark ?? icons().light}.svg`} alt="" draggable={false} />
    </span>
  )
}
