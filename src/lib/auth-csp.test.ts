/// <reference types="node" />

import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { describe, expect, it } from "vitest"

interface TauriConfig {
  app: {
    security: {
      csp: string
    }
  }
}

function directive(csp: string, name: string) {
  return csp
    .split(";")
    .map((value) => value.trim())
    .find((value) => value.startsWith(`${name} `))
}

describe("desktop authentication CSP", () => {
  const config = JSON.parse(
    readFileSync(resolve(process.cwd(), "src-tauri/tauri.conf.json"), "utf8"),
  ) as TauriConfig
  const csp = config.app.security.csp

  it("allows Clerk and Turnstile to load the bot-protection challenge", () => {
    expect(directive(csp, "script-src")).toContain("https://challenges.cloudflare.com")
    expect(directive(csp, "frame-src")).toContain("https://challenges.cloudflare.com")
    expect(directive(csp, "connect-src")).toContain("https://challenges.cloudflare.com")
    expect(directive(csp, "script-src")).toContain("https://*.clerk.accounts.dev")
    expect(directive(csp, "connect-src")).toContain("https://*.clerk.accounts.dev")
  })

  it("keeps the worker policy required by Clerk", () => {
    expect(directive(csp, "worker-src")).toContain("'self'")
    expect(directive(csp, "worker-src")).toContain("blob:")
  })
})
