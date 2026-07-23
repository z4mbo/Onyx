import { createEffect, createMemo, For, onCleanup, Show, type Component, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
import { AlertCircle, ChevronRight, Circle, TerminalSquare } from "lucide-solid"
import { tokenizeCode } from "../lib/highlight"
import type { AgentSession, Message } from "../lib/types"

/** Renders code as inert highlighted spans, or plain text when unknown. */
function highlightCode(code: string, hint?: string): JSX.Element {
  const tokens = tokenizeCode(code, hint)
  if (!tokens) return code
  return tokens.map((token) => (token.type ? <span class={`syn-${token.type}`}>{token.text}</span> : token.text))
}

type MarkdownBlock =
  | { kind: "paragraph"; content: string }
  | { kind: "heading"; content: string; level: 1 | 2 | 3 | 4 | 5 | 6 }
  | { kind: "code"; content: string; language?: string }
  | { kind: "quote"; content: string }
  | { kind: "list"; items: string[]; ordered: boolean; start?: number }
  | { kind: "rule" }

const FENCE = /^ {0,3}(`{3,}|~{3,})(.*)$/
const HEADING = /^ {0,3}(#{1,6})\s+(.+)$/
const QUOTE = /^ {0,3}>\s?(.*)$/
const UNORDERED_ITEM = /^\s*[-+*]\s+(.+)$/
const ORDERED_ITEM = /^\s*(\d+)[.)]\s+(.+)$/
const HORIZONTAL_RULE = /^ {0,3}(?:([-*_])\s*){3,}$/

function startsBlock(line: string) {
  return FENCE.test(line)
    || HEADING.test(line)
    || QUOTE.test(line)
    || UNORDERED_ITEM.test(line)
    || ORDERED_ITEM.test(line)
    || HORIZONTAL_RULE.test(line)
}

/**
 * Parses the Markdown subset used in transcripts without ever creating raw
 * HTML. Provider-supplied tags therefore stay inert text in the DOM.
 */
function safeText(value: unknown) {
  return typeof value === "string" ? value : value == null ? "" : String(value)
}

function parseMarkdown(source: unknown): MarkdownBlock[] {
  const lines = safeText(source).replace(/\r\n?/g, "\n").split("\n")
  const blocks: MarkdownBlock[] = []

  for (let index = 0; index < lines.length;) {
    const line = lines[index]
    if (!line.trim()) {
      index += 1
      continue
    }

    const fence = line.match(FENCE)
    if (fence) {
      const marker = fence[1]
      const language = fence[2].trim().split(/\s+/, 1)[0] || undefined
      const content: string[] = []
      index += 1
      while (index < lines.length) {
        const candidate = lines[index]
        const closing = candidate.match(/^ {0,3}(`+|~+)\s*$/)
        if (closing && closing[1][0] === marker[0] && closing[1].length >= marker.length) {
          index += 1
          break
        }
        content.push(candidate)
        index += 1
      }
      blocks.push({ kind: "code", content: content.join("\n"), language })
      continue
    }

    const heading = line.match(HEADING)
    if (heading) {
      blocks.push({
        kind: "heading",
        content: heading[2].replace(/\s+#+\s*$/, ""),
        level: heading[1].length as 1 | 2 | 3 | 4 | 5 | 6,
      })
      index += 1
      continue
    }

    if (HORIZONTAL_RULE.test(line)) {
      blocks.push({ kind: "rule" })
      index += 1
      continue
    }

    const quote = line.match(QUOTE)
    if (quote) {
      const content = [quote[1]]
      index += 1
      while (index < lines.length) {
        const next = lines[index].match(QUOTE)
        if (!next) break
        content.push(next[1])
        index += 1
      }
      blocks.push({ kind: "quote", content: content.join("\n") })
      continue
    }

    const unordered = line.match(UNORDERED_ITEM)
    const ordered = line.match(ORDERED_ITEM)
    if (unordered || ordered) {
      const isOrdered = !!ordered
      const items: string[] = []
      const start = ordered ? Number.parseInt(ordered[1], 10) : undefined
      while (index < lines.length) {
        const match = isOrdered ? lines[index].match(ORDERED_ITEM) : lines[index].match(UNORDERED_ITEM)
        if (!match) break
        items.push(match[isOrdered ? 2 : 1])
        index += 1
      }
      blocks.push({ kind: "list", items, ordered: isOrdered, start })
      continue
    }

    const paragraph = [line]
    index += 1
    while (index < lines.length && lines[index].trim() && !startsBlock(lines[index])) {
      paragraph.push(lines[index])
      index += 1
    }
    blocks.push({ kind: "paragraph", content: paragraph.join("\n") })
  }

  return blocks
}

function safeLink(value: string) {
  try {
    const url = new URL(value)
    return ["http:", "https:", "mailto:"].includes(url.protocol) ? url.href : undefined
  } catch {
    return undefined
  }
}

function textWithBreaks(value: string): JSX.Element[] {
  const output: JSX.Element[] = []
  value.split("\n").forEach((line, index) => {
    if (index > 0) output.push(<br />)
    if (line) output.push(line)
  })
  return output
}

function renderInline(source: string): JSX.Element[] {
  const pattern = /(`[^`\n]+`|\[[^\]\n]+\]\([^\n)]+\)|\*\*[^*\n]+\*\*|__[^_\n]+__|~~[^~\n]+~~|\*[^*\n]+\*|_[^_\n]+_)/g
  const output: JSX.Element[] = []
  let cursor = 0

  for (const match of source.matchAll(pattern)) {
    const token = match[0]
    const offset = match.index
    if (offset > cursor) output.push(...textWithBreaks(source.slice(cursor, offset)))

    if (token.startsWith("`")) {
      output.push(<code>{token.slice(1, -1)}</code>)
    } else if (token.startsWith("[")) {
      const link = token.match(/^\[([^\]]+)\]\((\S+?)(?:\s+["']([^"']*)["'])?\)$/)
      const href = link ? safeLink(link[2]) : undefined
      if (link && href) {
        output.push(
          <a href={href} title={link[3]} target="_blank" rel="noopener noreferrer">
            {renderInline(link[1])}
          </a>,
        )
      } else {
        output.push(token)
      }
    } else if (token.startsWith("**") || token.startsWith("__")) {
      output.push(<strong>{renderInline(token.slice(2, -2))}</strong>)
    } else if (token.startsWith("~~")) {
      output.push(<del>{renderInline(token.slice(2, -2))}</del>)
    } else {
      output.push(<em>{renderInline(token.slice(1, -1))}</em>)
    }
    cursor = offset + token.length
  }

  if (cursor < source.length) output.push(...textWithBreaks(source.slice(cursor)))
  return output
}

function MarkdownBlockView(props: { block: MarkdownBlock }) {
  return (
    <Show
      when={props.block.kind}
      keyed
    >
      {(kind) => {
        switch (kind) {
          case "heading": {
            const block = props.block as Extract<MarkdownBlock, { kind: "heading" }>
            return (
              <Dynamic component={`h${block.level}` as "h1"}>
                {renderInline(block.content)}
              </Dynamic>
            )
          }
          case "code": {
            const block = props.block as Extract<MarkdownBlock, { kind: "code" }>
            return <pre><code data-language={block.language}>{highlightCode(block.content, block.language)}</code></pre>
          }
          case "quote": {
            const block = props.block as Extract<MarkdownBlock, { kind: "quote" }>
            return <blockquote>{renderInline(block.content)}</blockquote>
          }
          case "list": {
            const block = props.block as Extract<MarkdownBlock, { kind: "list" }>
            return (
              <Show
                when={block.ordered}
                fallback={<ul><For each={block.items}>{(item) => <li>{renderInline(item)}</li>}</For></ul>}
              >
                <ol start={block.start}><For each={block.items}>{(item) => <li>{renderInline(item)}</li>}</For></ol>
              </Show>
            )
          }
          case "rule":
            return <hr />
          default: {
            const block = props.block as Extract<MarkdownBlock, { kind: "paragraph" }>
            return <p>{renderInline(block.content)}</p>
          }
        }
      }}
    </Show>
  )
}

export function MarkdownContent(props: { content: unknown }) {
  const blocks = createMemo(() => parseMarkdown(props.content))
  return (
    <div class="zai-message-markdown">
      <For each={blocks()}>{(block) => <MarkdownBlockView block={block} />}</For>
    </div>
  )
}

function toolTitle(message: Message) {
  return safeText(message.content).split("\n", 1)[0] || "Tool activity"
}

function ToolMessage(props: { message: Message }) {
  const lines = () => safeText(props.message.content).split("\n")
  return (
    <details class="zai-tool-event">
      <summary>
        <TerminalSquare aria-hidden="true" size={14} />
        <span>{toolTitle(props.message)}</span>
        <ChevronRight aria-hidden="true" class="zai-tool-chevron" size={13} />
      </summary>
      <Show when={lines().length > 1}><pre>{highlightCode(lines().slice(1).join("\n"))}</pre></Show>
    </details>
  )
}

type TranscriptItem =
  | { kind: "single"; message: Message }
  | { kind: "tools"; id: string; messages: Message[] }

/**
 * Collapses consecutive tool activities into one group so a long agent run
 * reads as "N steps" instead of an endless list of command rows.
 */
function groupMessages(messages: Message[]): TranscriptItem[] {
  const items: TranscriptItem[] = []
  for (const message of messages) {
    const last = items.at(-1)
    if (message.kind === "tool") {
      if (last?.kind === "tools") last.messages.push(message)
      else items.push({ kind: "tools", id: message.id, messages: [message] })
    } else {
      items.push({ kind: "single", message })
    }
  }
  return items
}

function ToolGroup(props: { messages: Message[] }) {
  const latest = () => props.messages[props.messages.length - 1]
  return (
    <Show
      when={props.messages.length > 1}
      fallback={<ToolMessage message={props.messages[0]} />}
    >
      <details class="zai-tool-group">
        <summary>
          <TerminalSquare aria-hidden="true" size={14} />
          <span class="zai-tool-group__count">{props.messages.length} steps</span>
          <span class="zai-tool-group__latest">{toolTitle(latest())}</span>
          <ChevronRight aria-hidden="true" class="zai-tool-chevron" size={13} />
        </summary>
        <div class="zai-tool-group__list">
          <For each={props.messages}>{(message) => <ToolMessage message={message} />}</For>
        </div>
      </details>
    </Show>
  )
}

export const Transcript: Component<{ session: AgentSession }> = (props) => {
  let scroller: HTMLDivElement | undefined
  let pinned = true
  let scrollFrame: number | undefined
  const items = createMemo(() => groupMessages(props.session.messages))

  const cancelScheduledScroll = () => {
    if (scrollFrame === undefined) return
    cancelAnimationFrame(scrollFrame)
    scrollFrame = undefined
  }

  createEffect(() => {
    props.session.messages.map((message) => safeText(message.content)).join("")
    cancelScheduledScroll()
    if (!pinned) return
    scrollFrame = requestAnimationFrame(() => {
      scrollFrame = undefined
      if (pinned && scroller) scroller.scrollTop = scroller.scrollHeight
    })
  })

  onCleanup(cancelScheduledScroll)

  return (
    <div
      ref={scroller}
      classList={{ "zai-transcript": true, "zai-transcript--running": props.session.status === "running" }}
      role="log"
      aria-label="Conversation"
      aria-live="polite"
      aria-relevant="additions text"
      aria-atomic="false"
      aria-busy={props.session.status === "running"}
      onScroll={() => {
        if (!scroller) return
        pinned = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80
        if (!pinned) cancelScheduledScroll()
      }}
    >
      <div class="zai-transcript__inner">
        <Show
          when={props.session.messages.length > 0}
          fallback={<div class="zai-transcript__empty">Start a conversation with {props.session.provider}.</div>}
        >
          <For each={items()}>
            {(item) => (
              <Show
                when={item.kind === "single"}
                fallback={<ToolGroup messages={(item as Extract<TranscriptItem, { kind: "tools" }>).messages} />}
              >
                {(() => {
                  const message = (item as Extract<TranscriptItem, { kind: "single" }>).message
                  return (
                    <article
                      class={`zai-message zai-message--${message.role} zai-message--${message.kind}`}
                      data-component={message.role === "user" ? "user-message" : "assistant-message"}
                    >
                      <Show when={message.kind === "error"}>
                        <AlertCircle aria-hidden="true" class="zai-message__alert" size={15} />
                      </Show>
                      <div class="zai-message__content" data-slot={message.role === "user" ? "user-message-text" : undefined}>
                        <MarkdownContent content={message.content} />
                      </div>
                    </article>
                  )
                })()}
              </Show>
            )}
          </For>
        </Show>

        <Show when={props.session.status === "running"}>
          <div class="zai-agent-working">
            <Circle aria-hidden="true" class="zai-agent-working__dot" size={8} fill="currentColor" />
            <span>Working…</span>
          </div>
        </Show>
      </div>
    </div>
  )
}
