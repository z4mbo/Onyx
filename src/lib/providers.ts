import type { ProviderId } from "./types"

export const providerMeta: Record<ProviderId, { name: string; color: string; models: string[] }> = {
  claude: {
    name: "Claude Code",
    color: "#d97757",
    models: ["default", "sonnet", "opus", "haiku"],
  },
  codex: {
    name: "Codex",
    color: "#111111",
    models: ["default"],
  },
  gemini: {
    name: "Gemini CLI",
    color: "#4285f4",
    models: ["default"],
  },
  kimi: {
    name: "Kimi Code",
    color: "#5b5bd6",
    models: ["default"],
  },
  openrouter: {
    name: "OpenRouter",
    color: "#6d5dfc",
    models: [],
  },
}

export function workspaceName(path: string) {
  const segments = path.replaceAll("\\", "/").split("/").filter(Boolean)
  return segments.at(-1) ?? path
}
