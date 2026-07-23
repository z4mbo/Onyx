import { describe, expect, it } from "vitest";
import productionConfig from "../../src-tauri/tauri.conf.json";

function directive(csp: string, name: string): string[] {
  const value = csp
    .split(";")
    .map((item) => item.trim())
    .find((item) => item === name || item.startsWith(`${name} `));
  return value?.split(/\s+/).slice(1) ?? [];
}

describe("production Rust UI desktop configuration", () => {
  it("allows the packaged WebView to compile and fetch its own WASM", () => {
    const csp = productionConfig.app.security.csp;

    expect(directive(csp, "script-src")).toContain("'wasm-unsafe-eval'");
    expect(directive(csp, "connect-src")).toContain("'self'");
  });

  it("ships the Rust bundle by default", () => {
    expect(productionConfig.build.frontendDist).toBe("../dist-rust");
    expect(productionConfig.build.devUrl).toBe("http://localhost:1430");
    expect(productionConfig.app.withGlobalTauri).toBe(true);
  });
});
