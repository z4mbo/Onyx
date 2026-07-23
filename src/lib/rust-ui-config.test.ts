import { describe, expect, it } from "vitest";
import rustUiConfig from "../../src-tauri/tauri.rust.conf.json";

function directive(csp: string, name: string): string[] {
  const value = csp
    .split(";")
    .map((item) => item.trim())
    .find((item) => item === name || item.startsWith(`${name} `));
  return value?.split(/\s+/).slice(1) ?? [];
}

describe("Rust UI desktop security policy", () => {
  it("allows the packaged WebView to compile and fetch its own WASM", () => {
    const csp = rustUiConfig.app.security.csp;

    expect(directive(csp, "script-src")).toContain("'wasm-unsafe-eval'");
    expect(directive(csp, "connect-src")).toContain("'self'");
  });
});
