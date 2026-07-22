import { Match, Switch, type Component } from "solid-js"
import { Braces, Diamond, Moon, Route, Sparkles } from "lucide-solid"
import { providerMeta } from "../lib/providers"
import type { ProviderId } from "../lib/types"

const ProviderGlyph: Component<{ provider: ProviderId }> = (props) => (
  <Switch>
    <Match when={props.provider === "claude"}><Sparkles aria-hidden="true" /></Match>
    <Match when={props.provider === "codex"}><Braces aria-hidden="true" /></Match>
    <Match when={props.provider === "gemini"}><Diamond aria-hidden="true" /></Match>
    <Match when={props.provider === "kimi"}><Moon aria-hidden="true" /></Match>
    <Match when={props.provider === "openrouter"}><Route aria-hidden="true" /></Match>
  </Switch>
)

export const ProviderBadge: Component<{ provider: ProviderId; size?: "sm" | "md" }> = (props) => {
  const meta = () => providerMeta[props.provider]
  return (
    <span
      class={`provider-badge ${props.size === "sm" ? "provider-badge-sm" : ""}`}
      data-provider={props.provider}
      style={{ "--provider-color": meta().color }}
      aria-label={meta().name}
    >
      <ProviderGlyph provider={props.provider} />
    </span>
  )
}
