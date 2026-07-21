import { readFile, writeFile } from 'fs/promises'
import { join } from 'path'
import { ensureDir } from '../util/paths'

export interface McpServerEntry {
  command: string
  args?: string[]
  env?: Record<string, string>
}

export interface McpConfig {
  mcpServers: Record<string, McpServerEntry>
}

/**
 * Path to the .mcp.json file in a project directory.
 */
function mcpJsonPath(projectDir: string): string {
  return join(projectDir, '.mcp.json')
}

/**
 * Path to the Gemini settings.json file in a project directory.
 * Gemini CLI uses .gemini/settings.json for MCP configuration.
 */
function geminiSettingsPath(projectDir: string): string {
  return join(projectDir, '.gemini', 'settings.json')
}

/**
 * Path to Kimi Code's project-level MCP configuration.
 * Current Kimi Code releases load .kimi-code/mcp.json from the working tree.
 */
function kimiMcpPath(projectDir: string): string {
  return join(projectDir, '.kimi-code', 'mcp.json')
}

/**
 * Reads .mcp.json from a project directory.
 * Returns an empty config if the file does not exist.
 */
export async function readMcpConfig(projectDir: string): Promise<McpConfig> {
  try {
    const raw = await readFile(mcpJsonPath(projectDir), 'utf-8')
    return JSON.parse(raw) as McpConfig
  } catch {
    return { mcpServers: {} }
  }
}

/**
 * Writes the full .mcp.json config to a project directory.
 * Also syncs the engine-specific Gemini and Kimi Code formats.
 */
export async function writeMcpConfig(
  projectDir: string,
  config: McpConfig
): Promise<void> {
  await writeFile(
    mcpJsonPath(projectDir),
    JSON.stringify(config, null, 2),
    'utf-8'
  )

  // Also write Gemini-format settings.json
  await syncGeminiSettings(projectDir, config)
  await syncKimiMcpConfig(projectDir, config)
}

/**
 * Adds an MCP server to the project config.
 */
export async function addMcpServer(
  projectDir: string,
  name: string,
  server: McpServerEntry
): Promise<McpConfig> {
  const config = await readMcpConfig(projectDir)

  if (config.mcpServers[name]) {
    throw new Error(`MCP server "${name}" already exists`)
  }

  config.mcpServers[name] = server
  await writeMcpConfig(projectDir, config)
  return config
}

/**
 * Removes an MCP server from the project config.
 */
export async function removeMcpServer(
  projectDir: string,
  name: string
): Promise<McpConfig> {
  const config = await readMcpConfig(projectDir)

  if (!config.mcpServers[name]) {
    throw new Error(`MCP server "${name}" not found`)
  }

  delete config.mcpServers[name]
  await writeMcpConfig(projectDir, config)
  return config
}

/**
 * Updates an existing MCP server in the project config.
 */
export async function updateMcpServer(
  projectDir: string,
  name: string,
  server: McpServerEntry
): Promise<McpConfig> {
  const config = await readMcpConfig(projectDir)

  if (!config.mcpServers[name]) {
    throw new Error(`MCP server "${name}" not found`)
  }

  config.mcpServers[name] = server
  await writeMcpConfig(projectDir, config)
  return config
}

/**
 * Syncs the MCP config to Gemini's settings.json format.
 * Gemini CLI expects: { mcpServers: { name: { command, args, env } } }
 * under .gemini/settings.json — the structure is compatible.
 */
async function syncGeminiSettings(
  projectDir: string,
  config: McpConfig
): Promise<void> {
  const settingsDir = join(projectDir, '.gemini')
  const settingsFile = geminiSettingsPath(projectDir)

  try {
    await ensureDir(settingsDir)

    // Read existing settings to preserve other Gemini-specific fields
    let existingSettings: Record<string, unknown> = {}
    try {
      const raw = await readFile(settingsFile, 'utf-8')
      existingSettings = JSON.parse(raw)
    } catch {
      // File does not exist yet, start fresh
    }

    const merged = {
      ...existingSettings,
      mcpServers: config.mcpServers
    }

    await writeFile(settingsFile, JSON.stringify(merged, null, 2), 'utf-8')
  } catch {
    // Non-critical: Gemini sync failure should not block Claude config
  }
}

/**
 * Syncs the project MCP list to Kimi Code's well-known mcp.json format while
 * preserving any future top-level Kimi-specific fields.
 */
async function syncKimiMcpConfig(
  projectDir: string,
  config: McpConfig
): Promise<void> {
  const settingsDir = join(projectDir, '.kimi-code')
  const settingsFile = kimiMcpPath(projectDir)

  try {
    await ensureDir(settingsDir)

    let existingSettings: Record<string, unknown> = {}
    try {
      const raw = await readFile(settingsFile, 'utf-8')
      existingSettings = JSON.parse(raw)
    } catch {
      // File does not exist yet, start fresh.
    }

    await writeFile(
      settingsFile,
      JSON.stringify({ ...existingSettings, mcpServers: config.mcpServers }, null, 2),
      'utf-8'
    )
  } catch {
    // Non-critical: Kimi Code sync failure should not block the shared config.
  }
}
