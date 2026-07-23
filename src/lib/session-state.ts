import type { AgentSession, Message, SessionEvent } from "./types"

export function sortSessions(items: AgentSession[]) {
  return [...items].sort((left, right) => Date.parse(right.updatedAt) - Date.parse(left.updatedAt))
}

export function replaceSession(items: AgentSession[], session: AgentSession) {
  return sortSessions([session, ...items.filter((item) => item.id !== session.id)])
}

function mergeMessages(current: Message[], incoming: Message[]) {
  const byId = new Map(current.map((message) => [message.id, message]))
  const ordered = incoming.map((message) => {
    const existing = byId.get(message.id)
    byId.delete(message.id)
    if (!existing) return message

    // Command responses can race streamed deltas. Keep the already-rendered text
    // when the command snapshot is merely an older prefix of it.
    if (existing.content.startsWith(message.content)) return { ...message, content: existing.content }
    return message
  })

  for (const message of current) {
    if (byId.has(message.id)) ordered.push(message)
  }
  return ordered
}

export function mergeCommandSession(items: AgentSession[], incoming: AgentSession) {
  const current = items.find((item) => item.id === incoming.id)
  if (!current) return replaceSession(items, incoming)
  return replaceSession(items, {
    ...incoming,
    messages: mergeMessages(current.messages, incoming.messages),
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
      return { ...session, messages: [...session.messages, event.message] }
    }

    const existing = session.messages.find((message) => message.id === event.messageId)
    const messages = existing
      ? session.messages.map((message) =>
          message.id === event.messageId ? { ...message, content: message.content + event.delta } : message,
        )
      : [
          ...session.messages,
          {
            id: event.messageId,
            role: "assistant" as const,
            kind: "text" as const,
            content: event.delta,
            createdAt: new Date().toISOString(),
          },
        ]
    return { ...session, messages }
  })
}

export function sessionEventId(event: SessionEvent) {
  return event.type === "snapshot" ? event.session.id : event.sessionId
}
