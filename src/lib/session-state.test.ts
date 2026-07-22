import { describe, expect, it } from "vitest"
import { applySessionEvent, mergeCommandSession, replaceSession, sortSessions } from "./session-state"
import type { AgentSession, Message } from "./types"

const at = (second: number) => `2026-07-22T18:00:${second.toString().padStart(2, "0")}.000Z`

function message(id: string, content: string, role: Message["role"] = "assistant"): Message {
  return { id, role, kind: "text", content, createdAt: at(0) }
}

function session(id: string, messages: Message[] = [], updated = at(0)): AgentSession {
  return {
    id,
    title: `Session ${id}`,
    provider: "claude",
    model: "default",
    workspace: "/tmp/project",
    providerSessionId: null,
    status: "idle",
    messages,
    createdAt: at(0),
    updatedAt: updated,
  }
}

describe("session state", () => {
  it("sorts and replaces sessions without duplicates", () => {
    const result = replaceSession([session("old", [], at(1)), session("same", [], at(2))], session("same", [], at(3)))
    expect(result.map((item) => item.id)).toEqual(["same", "old"])
    expect(sortSessions(result)).toEqual(result)
  })

  it("treats event snapshots as canonical", () => {
    const stale = session("a", [message("m", "streamed")])
    const canonical = session("a", [message("m", "done")], at(2))
    const result = applySessionEvent([stale], { type: "snapshot", session: canonical })
    expect(result[0]).toEqual(canonical)
  })

  it("appends deltas and creates a stable placeholder when needed", () => {
    const first = applySessionEvent([session("a")], { type: "delta", sessionId: "a", messageId: "m", delta: "hel" })
    const second = applySessionEvent(first, { type: "delta", sessionId: "a", messageId: "m", delta: "lo" })
    expect(second[0].messages).toHaveLength(1)
    expect(second[0].messages[0].content).toBe("hello")
  })

  it("deduplicates activity messages and honors removal", () => {
    const event = { type: "activity" as const, sessionId: "a", message: message("tool", "ran command", "tool") }
    const once = applySessionEvent([session("a")], event)
    const twice = applySessionEvent(once, event)
    expect(twice[0].messages).toHaveLength(1)
    expect(applySessionEvent(twice, { type: "removed", sessionId: "a" })).toEqual([])
  })

  it("does not let a late command snapshot erase streamed text or activity", () => {
    const current = session("a", [message("assistant", "streamed response"), message("tool", "checked files", "tool")])
    const command = { ...session("a", [message("assistant", "streamed")], at(2)), status: "running" as const }
    const result = mergeCommandSession([current], command)
    expect(result[0].messages.map((item) => [item.id, item.content])).toEqual([
      ["assistant", "streamed response"],
      ["tool", "checked files"],
    ])
  })
})
