import { readdir, stat, lstat, mkdir, rm, writeFile, readFile, symlink } from 'fs/promises'
import { join, basename } from 'path'
import { existsSync } from 'fs'
import { app } from 'electron'
import { getProjectsDir, prepareProjectsDir, ensureDir } from '../util/paths'
import { copyDefaultSkills } from './skills-manager'
import { copyDefaultAgents } from './agents-manager'
import { createDefaultInstructionFiles } from './engine-instructions'
import { readMcpConfig, writeMcpConfig } from './mcp-config'
import { getGuiPortFilePath } from '../gui-control/gui-server'
import { resolveManagedProjectPath, validateProjectName } from './project-name'

export interface Project {
  name: string
  path: string
  createdAt: string
  imported: boolean
}

/**
 * Lists all projects by scanning the projects directory.
 * Every subdirectory = a project.
 */
export async function listProjects(): Promise<Project[]> {
  const projectsDir = await prepareProjectsDir()

  let entries: string[]
  try {
    entries = await readdir(projectsDir)
  } catch {
    return []
  }

  const projects: Project[] = []
  for (const entry of entries) {
    const fullPath = join(projectsDir, entry)
    try {
      const linkInfo = await lstat(fullPath)
      const isImported = linkInfo.isSymbolicLink()
      const info = await stat(fullPath)
      if (info.isDirectory()) {
        // Upgrade projects created by older zAI/upstream builds. Every operation
        // is idempotent and preserves existing user-authored files/settings.
        await ensureDefaultStructure(fullPath)
        projects.push({
          name: entry,
          path: fullPath,
          createdAt: info.birthtime.toISOString(),
          imported: isImported
        })
      }
    } catch {
      // Skip entries we can't stat
    }
  }

  projects.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }))
  return projects
}

/**
 * Creates a new project folder.
 */
export async function createProject(name: string): Promise<Project> {
  const projectsDir = await prepareProjectsDir()
  const safeName = validateProjectName(name)
  const projectDir = resolveManagedProjectPath(projectsDir, safeName)
  await mkdir(projectDir, { recursive: false })

  // Create empty tips.md — tips will be populated by the tip-creator skill
  const tipsPath = join(projectDir, 'tips.md')
  await writeFile(tipsPath, '', 'utf-8')

  // Copy default skills into .claude/skills/
  await copyDefaultSkills(projectDir)

  // Copy default agents into .claude/agents/ and .gemini/agents/
  await copyDefaultAgents(projectDir)

  // Create default instruction files (CLAUDE.md, GEMINI.md)
  await createDefaultInstructionFiles(projectDir, name)

  // Add default gui-control MCP server
  await addDefaultGuiMcp(projectDir)

  // Set default permissions for each engine
  await createDefaultPermissions(projectDir)

  const info = await stat(projectDir)
  console.log(`[project-manager] created project "${name}" at ${projectDir}`)

  return {
    name: safeName,
    path: projectDir,
    createdAt: info.birthtime.toISOString(),
    imported: false
  }
}

/**
 * Imports an existing folder as a project.
 * Creates a directory junction in the projects dir pointing to the external folder,
 * then fills in any missing default structure (tips.md, skills, agents, MCP, permissions, instruction files).
 */
export async function importProject(folderPath: string): Promise<Project> {
  const projectsDir = await prepareProjectsDir()
  const name = validateProjectName(basename(folderPath))
  const linkPath = resolveManagedProjectPath(projectsDir, name)

  // Check if a project with this name already exists
  if (existsSync(linkPath)) {
    throw new Error(`A project named "${name}" already exists`)
  }

  // Create a directory junction (works without admin on Windows, like a symlink for dirs)
  await symlink(folderPath, linkPath, 'junction')

  // Now fill in any missing default structure
  await ensureDefaultStructure(folderPath)

  const info = await stat(folderPath)
  console.log(`[project-manager] imported project "${name}" from ${folderPath}`)

  return {
    name,
    path: linkPath,
    createdAt: info.birthtime.toISOString(),
    imported: true
  }
}

/**
 * Ensures a project directory has all the default structure.
 * Checks for each piece and only creates what's missing.
 */
async function ensureDefaultStructure(projectDir: string): Promise<void> {
  // 1. tips.md
  const tipsPath = join(projectDir, 'tips.md')
  if (!existsSync(tipsPath)) {
    await writeFile(tipsPath, '', 'utf-8')
  }

  // 2. Default skills (only copies missing ones)
  await copyDefaultSkills(projectDir)

  // 3. Default agents (only copies missing ones)
  await copyDefaultAgents(projectDir)

  // 4. Instruction files (CLAUDE.md, GEMINI.md) — only if missing
  await createDefaultInstructionFiles(projectDir, basename(projectDir))

  // 5. GUI-control MCP server
  await addDefaultGuiMcp(projectDir)

  // 6. Default permissions
  await createDefaultPermissions(projectDir)
}

/**
 * Deletes a project folder.
 */
export async function deleteProject(name: string): Promise<boolean> {
  const projectDir = resolveManagedProjectPath(await prepareProjectsDir(), name)
  try {
    await rm(projectDir, { recursive: true, force: true })
    console.log(`[project-manager] deleted project "${name}"`)
    return true
  } catch {
    return false
  }
}

/**
 * Returns the path for a project by name.
 */
export function getProjectPath(name: string): string {
  return resolveManagedProjectPath(getProjectsDir(), name)
}

/**
 * Resolves the path to the gui-control MCP server script.
 * In dev: <repo>/resources/default-projects/mcp-servers/gui-control/index.js
 * In prod: <resourcesPath>/default-projects/mcp-servers/gui-control/index.js
 */
function getGuiMcpScriptPath(): string {
  if (app.isPackaged) {
    return join(process.resourcesPath, 'default-projects', 'mcp-servers', 'gui-control', 'index.js')
  }
  return join(app.getAppPath(), 'resources', 'default-projects', 'mcp-servers', 'gui-control', 'index.js')
}

/**
 * Adds the default gui-control MCP server entry to a project.
 */
async function addDefaultGuiMcp(projectDir: string): Promise<void> {
  try {
    const scriptPath = getGuiMcpScriptPath()
    const portFilePath = getGuiPortFilePath()

    const guiControlServer = {
      // Electron can act as the bundled Node runtime on both Windows and macOS,
      // so users do not need a separate `node` executable for this MCP server.
      command: process.execPath,
      args: [scriptPath],
      env: {
        ELECTRON_RUN_AS_NODE: '1',
        NODE_PATH: join(app.getAppPath(), 'node_modules'),
        ZAI_GUI_PORT_FILE: portFilePath,
        ZAI_PROJECT_PATH: projectDir
      }
    }
    const config = await readMcpConfig(projectDir)
    config.mcpServers['gui-control'] = guiControlServer
    await writeMcpConfig(projectDir, config)
    console.log(`[project-manager] Synchronized gui-control MCP server in ${projectDir}`)
  } catch (err) {
    // Don't fail project creation if MCP setup fails
    console.error('[project-manager] Failed to add gui-control MCP:', err)
  }
}

/**
 * Creates default permission settings for each engine.
 *
 * Claude: .claude/settings.local.json — allows all gui-control MCP tools
 * Gemini: .gemini/settings.json — sets trust:true on gui-control server
 * Codex: uses --full-auto flag; .agents/ dir is ensured by skills/agents copy
 * Kimi Code: keeps approvals enabled; its MCP config is synced to .kimi-code/.
 */
async function createDefaultPermissions(projectDir: string): Promise<void> {
  // Claude: allow all gui-control MCP tools via permissions.allow
  try {
    const claudeDir = join(projectDir, '.claude')
    await ensureDir(claudeDir)
    const claudeSettingsPath = join(claudeDir, 'settings.local.json')

    let existing: Record<string, unknown> = {}
    try {
      const raw = await readFile(claudeSettingsPath, 'utf-8')
      existing = JSON.parse(raw)
    } catch {
      // File doesn't exist yet
    }

    const permissions = (existing.permissions as Record<string, unknown>) || {}
    const currentAllow = (permissions.allow as string[]) || []

    // Add gui-control MCP wildcard if not already present
    const guiPattern = 'mcp__gui-control__*'
    if (!currentAllow.includes(guiPattern)) {
      currentAllow.push(guiPattern)
    }

    // Allow all tools to modify tips.md and canvas.html in the project root
    const filePatterns = [
      'Edit(tips.md)',
      'Write(tips.md)',
      'Bash(*tips.md*)',
      'Edit(canvas.html)',
      'Write(canvas.html)',
      'Bash(*canvas.html*)'
    ]
    for (const pattern of filePatterns) {
      if (!currentAllow.includes(pattern)) {
        currentAllow.push(pattern)
      }
    }

    existing.permissions = { ...permissions, allow: currentAllow }
    await writeFile(claudeSettingsPath, JSON.stringify(existing, null, 2), 'utf-8')
    console.log(`[project-manager] Created Claude default permissions at ${claudeSettingsPath}`)
  } catch (err) {
    console.error('[project-manager] Failed to create Claude permissions:', err)
  }

  // Gemini: set trust:true on gui-control MCP server
  try {
    const geminiDir = join(projectDir, '.gemini')
    const geminiSettingsPath = join(geminiDir, 'settings.json')

    let existing: Record<string, unknown> = {}
    try {
      const raw = await readFile(geminiSettingsPath, 'utf-8')
      existing = JSON.parse(raw)
    } catch {
      // File doesn't exist yet — will be created by MCP sync
      return
    }

    const mcpServers = (existing.mcpServers as Record<string, Record<string, unknown>>) || {}
    if (mcpServers['gui-control']) {
      mcpServers['gui-control'].trust = true
      existing.mcpServers = mcpServers
      await writeFile(geminiSettingsPath, JSON.stringify(existing, null, 2), 'utf-8')
      console.log(`[project-manager] Set Gemini gui-control trust:true at ${geminiSettingsPath}`)
    }
  } catch (err) {
    console.error('[project-manager] Failed to update Gemini permissions:', err)
  }
}
