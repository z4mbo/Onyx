import { describe, expect, it } from "vitest"
import { normalizeBrowserUrl } from "./browser-url"

describe("normalizeBrowserUrl", () => {
  it("defaults public hostnames to HTTPS", () => {
    expect(normalizeBrowserUrl("example.com/docs")).toBe("https://example.com/docs")
  })

  it("keeps localhost previews on HTTP", () => {
    expect(normalizeBrowserUrl("localhost:3000")).toBe("http://localhost:3000/")
    expect(normalizeBrowserUrl("127.0.0.1:5173/app")).toBe("http://127.0.0.1:5173/app")
  })

  it("rejects blank and privileged schemes", () => {
    expect(() => normalizeBrowserUrl("  ")).toThrow("Enter a URL")
    expect(() => normalizeBrowserUrl("file:///etc/passwd")).toThrow("HTTP and HTTPS")
    expect(() => normalizeBrowserUrl("javascript:alert(1)")).toThrow("HTTP and HTTPS")
  })
})
