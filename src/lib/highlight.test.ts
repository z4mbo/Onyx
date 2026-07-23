import { describe, expect, it } from "vitest"
import { guessLanguage, tokenizeCode } from "./highlight"

describe("guessLanguage", () => {
  it("maps fence hints and aliases", () => {
    expect(guessLanguage("", "ts")).toBe("clike")
    expect(guessLanguage("", "sh")).toBe("bash")
    expect(guessLanguage("", "jsonc")).toBe("json")
    expect(guessLanguage("", "patch")).toBe("diff")
  })

  it("detects content without a hint", () => {
    expect(guessLanguage('{"a": 1}')).toBe("json")
    expect(guessLanguage("diff --git a/x b/x\n--- a/x\n+++ b/x")).toBe("diff")
    expect(guessLanguage("$ npm install\nnpm warn deprecated")).toBe("bash")
    expect(guessLanguage("plain prose with no code")).toBeUndefined()
  })
})

describe("tokenizeCode", () => {
  const text = (tokens: NonNullable<ReturnType<typeof tokenizeCode>>) =>
    tokens.map((token) => token.text).join("")

  it("round-trips content byte for byte", () => {
    const samples = [
      ['const x = "a\\"b" // note', "ts"],
      ["if [ -f x ]; then\n  echo 'hi' # comment\nfi", "bash"],
      ['{"key": [1, 2.5, true, null], "s": "v"}', "json"],
      ["diff --git a/f b/f\n@@ -1 +1 @@\n-old\n+new", "diff"],
    ] as const
    for (const [code, hint] of samples) {
      const tokens = tokenizeCode(code, hint)
      expect(tokens, `tokenized ${hint}`).not.toBeNull()
      expect(text(tokens!)).toBe(code)
    }
  })

  it("classifies keywords, strings, and numbers", () => {
    const tokens = tokenizeCode('const value = 42 // answer', "ts")!
    const byType = (type: string) => tokens.filter((token) => token.type === type).map((token) => token.text)
    expect(byType("kw")).toContain("const")
    expect(byType("num")).toContain("42")
    expect(byType("com")).toContain("// answer")
  })

  it("marks diff additions and deletions per line", () => {
    const tokens = tokenizeCode("+added\n-removed\n context", "diff")!
    expect(tokens.find((token) => token.type === "add")?.text).toBe("+added")
    expect(tokens.find((token) => token.type === "del")?.text).toBe("-removed")
  })

  it("keeps json keys distinct from string values", () => {
    const tokens = tokenizeCode('{"name": "onyx"}', "json")!
    expect(tokens.find((token) => token.type === "prop")?.text).toBe('"name"')
    expect(tokens.find((token) => token.type === "str")?.text).toBe('"onyx"')
  })

  it("returns null for unknown or oversized content", () => {
    expect(tokenizeCode("plain prose with no code")).toBeNull()
    expect(tokenizeCode("x".repeat(70_000), "ts")).toBeNull()
  })
})
