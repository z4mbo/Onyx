import { createEffect, For, Show, type Component } from "solid-js"
import { AlertCircle, ChevronRight, LoaderCircle, TerminalSquare } from "lucide-solid"
import type { AgentSession, Message } from "../lib/types"
import { ProviderBadge } from "./ProviderBadge"

function ToolMessage(props: { message: Message }) {
  const lines = () => props.message.content.split("\n")
  return (
    <details class="tool-event">
      <summary><TerminalSquare size={14} /><span>{lines()[0] || "Tool activity"}</span><ChevronRight class="tool-chevron" size={13} /></summary>
      <Show when={lines().length > 1}><pre>{lines().slice(1).join("\n")}</pre></Show>
    </details>
  )
}

export const Transcript: Component<{ session: AgentSession }> = (props) => {
  let end!: HTMLDivElement
  createEffect(() => {
    props.session.messages.map((message) => message.content).join("")
    queueMicrotask(() => end?.scrollIntoView({ block: "end" }))
  })

  return (
    <div class="transcript">
      <div class="transcript-inner">
        <For each={props.session.messages}>
          {(message) => (
            <Show
              when={message.kind !== "tool"}
              fallback={<ToolMessage message={message} />}
            >
              <article class={`message message-${message.role} message-${message.kind}`}>
                <Show when={message.role === "assistant"}>
                  <div class="message-avatar"><ProviderBadge provider={props.session.provider} size="sm" /></div>
                </Show>
                <Show when={message.kind === "error"}><AlertCircle class="message-alert" size={15} /></Show>
                <div class="message-content">{message.content}</div>
              </article>
            </Show>
          )}
        </For>
        <Show when={props.session.status === "running"}>
          <div class="agent-working"><ProviderBadge provider={props.session.provider} size="sm" /><LoaderCircle class="spin" size={14} /><span>Working…</span></div>
        </Show>
        <div ref={end} class="transcript-end" />
      </div>
    </div>
  )
}
