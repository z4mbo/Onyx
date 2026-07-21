import { writeFile } from 'fs/promises'
import { join } from 'path'
import { existsSync } from 'fs'

/**
 * Maps engine IDs to their instruction file names.
 * Claude Code reads CLAUDE.md, Gemini CLI reads GEMINI.md, and Codex/Kimi
 * Code share the cross-tool AGENTS.md convention.
 */
const ENGINE_INSTRUCTION_FILES: Record<string, string> = {
  claude: 'CLAUDE.md',
  gemini: 'GEMINI.md',
  codex: 'AGENTS.md',
  kimi: 'AGENTS.md',
  openrouter: 'AGENTS.md'
}

/**
 * Returns the default instruction content for a given engine.
 * All engines get the same behavioral rules.
 */
function getDefaultInstructions(engineId: string, projectName: string): string {
  const autoTipRule =
    'After every response, run the `tip-creator` skill to analyze the conversation and replace `tips.md` with fresh actionable next steps. Always overwrite the entire file — never append. Do this silently at the end — do not mention it to the user.'

  const canvasRule =
    'You have a `render_ui` MCP tool that builds a project insight interface beside the terminal. The terminal always stays visible. Use `mode: "full"` to replace the sidebar and right panel with your UI — this is the main mode. The UI should help the user navigate and understand what matters about the project: file previews, structure overviews, key metrics, dependency maps, important patterns. Follow the app\'s Windows 11 light/bright minimalist design language (light backgrounds, subtle borders, clean typography, Segoe UI). The canvas can read files inside the active project at runtime via relative paths passed to `yft.readFile(path)` and `yft.readDir(path)`. Never ask what the UI should look like — analyze the project and design the best interface. Do NOT rebuild the UI on every response — only update it when the user asks or when a genuinely important insight has been discovered. Use the `ui-creator` skill for guidance.'

  if (engineId === 'gemini') {
    return `# Project: ${projectName}

## Rules

- ${autoTipRule}
- ${canvasRule}
`
  }

  // Shared default.
  return `# Project: ${projectName}

## Rules

- ${autoTipRule}
- ${canvasRule}
`
}

/**
 * Creates default instruction files in the project root.
 * Existing files are not overwritten.
 */
export async function createDefaultInstructionFiles(
  projectDir: string,
  projectName: string
): Promise<void> {
  for (const [engineId, filename] of Object.entries(ENGINE_INSTRUCTION_FILES)) {
    const filePath = join(projectDir, filename)
    if (existsSync(filePath)) continue

    try {
      const content = getDefaultInstructions(engineId, projectName)
      await writeFile(filePath, content, 'utf-8')
    } catch {
      // Non-critical — skip if we can't write
    }
  }
}
