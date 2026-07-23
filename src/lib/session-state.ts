import type { AgentSession, Message, SessionEvent } from "./types"

function text(value: unknown) {
  return typeof value === "string" ? value : value == null ? "" : String(value)
}

function normalizeMessage(message: Message): Message {
  return {
    ...message,
    content: text((message as Partial<Message>).content),
  }
}

/**
 * Treat persisted and event-delivered sessions as untrusted runtime data.
 * Older builds and interrupted provider streams may leave a message without
 * string content even though the current TypeScript contract requires it.
 */
export function normalizeSession(session: AgentSession): AgentSession {
  const candidate = session as Partial<AgentSession>
  return {
    ...session,
    title: text(candidate.title),
    workspace: text(candidate.workspace),
    messages: Array.isArray(candidate.messages) ? candidate.messages.map(normalizeMessage) : [],
  }
}

export function sortSessions(items: AgentSession[]) {
  return items
    .map(normalizeSession)
    .sort((left, right) => Date.parse(right.updatedAt) - Date.parse(left.updatedAt))
}

export function replaceSession(items: AgentSession[], session: AgentSession) {
  const normalized = normalizeSession(session)
  return sortSessions([normalized, ...items.filter((item) => item.id !== normalized.id)])
}

function mergeMessages(current: Message[], incoming: Message[]) {
  const normalizedCurrent = current.map(normalizeMessage)
  const byId = new Map(normalizedCurrent.map((message) => [message.id, message]))
  const ordered = incoming.map(normalizeMessage).map((message) => {
    const existing = byId.get(message.id)
    byId.delete(message.id)
    if (!existing) return message

    // Command responses can race streamed deltas. Keep the already-rendered text
    // when the command snapshot is merely an older prefix of it.
    if (existing.content.startsWith(message.content)) return { ...message, content: existing.content }
    return message
  })

  for (const message of normalizedCurrent) {
    if (byId.has(message.id)) ordered.push(message)
  }
  return ordered
}

export function mergeCommandSession(items: AgentSession[], incoming: AgentSession) {
  const normalized = normalizeSession(incoming)
  const current = items.find((item) => item.id === normalized.id)
  if (!current) return replaceSession(items, normalized)
  return replaceSession(items, {
    ...normalized,
    messages: mergeMessages(current.messages, normalized.messages),
  })
}

export function applySessionEvent(items: AgentSession[], event: SessionEvent) {
  if (event.type === "snapshot") return replaceSession(items, event.session)
  if (event.type === "removed") return items.filter((session) => session.id !== event.sessionId)

  return items.map((session) => {
    if (session.id !== event.sessionId) return session
    if (event.type === "context_usage") {
      return { ...session, contextUsage: event.usage, updatedAt: new Date().toISOString() }
    }
    if (event.type === "activity") {
      if (session.messages.some((message) => message.id === event.message.id)) return session
      return { ...session, messages: [...session.messages, normalizeMessage(event.message)] }
    }

    const existing = session.messages.find((message) => message.id === event.messageId)
    const delta = text(event.delta)
    const messages = existing
      ? session.messages.map((message) =>
          message.id === event.messageId ? { ...message, content: text(message.content) + delta } : message,
        )
      : [
          ...session.messages,
          {
            id: event.messageId,
            role: "assistant" as const,
            kind: "text" as const,
            content: delta,
            createdAt: new Date().toISOString(),
          },
        ]
    return { ...session, messages }
  })
}

export function sessionEventId(event: SessionEvent) {
  return event.type === "snapshot" ? event.session.id : event.sessionId
}
