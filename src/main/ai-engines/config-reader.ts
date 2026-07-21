import { readdir, readFile } from 'fs/promises'
import { join, basename, extname } from 'path'
import { homedir } from 'os'

export interface AgentEntry {
  name: string
  description: string
  model?: string
  tools?: string
  filePath: string
}

export interface SkillEntry {
  name: string
  description: string
  userInvocable?: boolean
  filePath: string
}

/**
 * Simple YAML frontmatter parser for markdown files.
 * Extracts key-value pairs between --- delimiters.
 */
function parseFrontmatter(content: string): Record<string, string> {
  const match = content.match(/^---\r?\n([\s\S]*?)\r?\n---/)
  if (!match) return {}

  const result: Record<string, string> = {}
  const lines = match[1].split(/\r?\n/)
  for (const line of lines) {
    const colonIdx = line.indexOf(':')
    if (colonIdx === -1) continue
    const key = line.slice(0, colonIdx).trim()
    let value = line.slice(colonIdx + 1).trim()
    // Strip surrounding quotes
    if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) {
      value = value.slice(1, -1)
    }
    if (key) result[key] = value
  }
  return result
}

/**
 * Extracts the first paragraph of markdown body (after frontmatter) as a description fallback.
 */
function extractFirstParagraph(content: string): string {
  let body = content
  const fmMatch = content.match(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/)
  if (fmMatch) {
    body = content.slice(fmMatch[0].length)
  }
  const trimmed = body.trim()
  const firstPara = trimmed.split(/\r?\n\r?\n/)[0]
  return firstPara?.replace(/^#+\s*/, '').trim().slice(0, 200) || ''
}

/**
 * Safely reads .md files from a directory, returning an empty array if the directory doesn't exist.
 */
async function readMdFiles(dirPath: string): Promise<{ name: string; content: string; filePath: string }[]> {
  try {
    const entries = await readdir(dirPath, { withFileTypes: true })
    const mdFiles = entries.filter((e) => e.isFile() && extname(e.name).toLowerCase() === '.md')

    const results = await Promise.all(
      mdFiles.map(async (entry) => {
        const filePath = join(dirPath, entry.name)
        try {
          const content = await readFile(filePath, 'utf-8')
          return { name: basename(entry.name, '.md'), content, filePath }
        } catch {
          return null
        }
      })
    )
    return results.filter((r): r is NonNullable<typeof r> => r !== null)
  } catch {
    return []
  }
}

/**
 * Reads agent definitions from engine-specific directories.
 * Checks both project-level and user-level directories.
 */
const ENGINE_DIR_MAP: Record<string, string> = {
  claude: '.claude',
  gemini: '.gemini',
  codex: '.agents',
  kimi: '.kimi-code',
  openrouter: '.kimi-code'
}

export async function readAgents(engineId: string, projectPath: string): Promise<AgentEntry[]> {
  // Kimi Code uses AGENTS.md for instructions and `.agents/skills` for skills;
  // it does not consume the Claude/Gemini markdown subagent directory format.
  if (engineId === 'kimi' || engineId === 'openrouter') return []

  const engineDir = ENGINE_DIR_MAP[engineId] ?? `.${engineId}`
  const dirs = [
    join(projectPath, engineDir, 'agents'),
    join(homedir(), engineDir, 'agents')
  ]

  const allFiles = new Map<string, { content: string; filePath: string }>()

  // Project-level takes precedence over user-level
  for (const dir of dirs) {
    const files = await readMdFiles(dir)
    for (const file of files) {
      if (!allFiles.has(file.name)) {
        allFiles.set(file.name, { content: file.content, filePath: file.filePath })
      }
    }
  }

  const agents: AgentEntry[] = []
  for (const [name, { content, filePath }] of allFiles) {
    const fm = parseFrontmatter(content)
    agents.push({
      name: fm.name || name,
      description: fm.description || extractFirstParagraph(content),
      model: fm.model || undefined,
      tools: fm.tools || undefined,
      filePath
    })
  }

  return agents.sort((a, b) => a.name.localeCompare(b.name))
}

/**
 * Reads skill definitions from engine-specific directories.
 * Checks SKILL.md in subdirectories and legacy command files.
 */
export async function readSkills(engineId: string, projectPath: string): Promise<SkillEntry[]> {
  const engineDirs = engineId === 'kimi' || engineId === 'openrouter'
    ? ['.agents', '.kimi-code']
    : [ENGINE_DIR_MAP[engineId] ?? `.${engineId}`]
  const dirs: { path: string; type: 'skills' | 'commands' }[] = [
    ...engineDirs.map((engineDir) => ({
      path: join(projectPath, engineDir, 'skills'),
      type: 'skills' as const
    })),
    ...engineDirs.map((engineDir) => ({
      path: join(projectPath, engineDir, 'commands'),
      type: 'commands' as const
    })),
    ...engineDirs.map((engineDir) => ({
      path: join(homedir(), engineDir, 'skills'),
      type: 'skills' as const
    })),
    ...engineDirs.map((engineDir) => ({
      path: join(homedir(), engineDir, 'commands'),
      type: 'commands' as const
    }))
  ]

  const allSkills = new Map<string, SkillEntry>()

  for (const { path: dir, type } of dirs) {
    try {
      const entries = await readdir(dir, { withFileTypes: true })

      if (type === 'skills') {
        // Skills are subdirectories containing SKILL.md
        const subdirs = entries.filter((e) => e.isDirectory())
        for (const subdir of subdirs) {
          if (allSkills.has(subdir.name)) continue
          const skillFile = join(dir, subdir.name, 'SKILL.md')
          try {
            const content = await readFile(skillFile, 'utf-8')
            const fm = parseFrontmatter(content)
            allSkills.set(subdir.name, {
              name: fm.name || subdir.name,
              description: fm.description || extractFirstParagraph(content),
              userInvocable: fm['user-invocable'] === 'true' || fm.userInvocable === 'true',
              filePath: skillFile
            })
          } catch {
            // No SKILL.md in this subdirectory, skip
          }
        }
      } else {
        // Legacy commands: .md files directly in the commands directory
        const mdFiles = entries.filter((e) => e.isFile() && extname(e.name).toLowerCase() === '.md')
        for (const file of mdFiles) {
          const name = basename(file.name, '.md')
          if (allSkills.has(name)) continue
          const filePath = join(dir, file.name)
          try {
            const content = await readFile(filePath, 'utf-8')
            const fm = parseFrontmatter(content)
            allSkills.set(name, {
              name: fm.name || name,
              description: fm.description || extractFirstParagraph(content),
              userInvocable: fm['user-invocable'] === 'true' || fm.userInvocable === 'true',
              filePath
            })
          } catch {
            // Skip unreadable files
          }
        }
      }
    } catch {
      // Directory doesn't exist, skip
    }
  }

  return Array.from(allSkills.values()).sort((a, b) => a.name.localeCompare(b.name))
}
