import { app } from 'electron'
import { join, dirname, resolve } from 'path'
import {
  cp,
  lstat,
  mkdir,
  readdir,
  readFile,
  readlink,
  rename,
  rm,
  symlink,
  writeFile
} from 'fs/promises'

let projectsPreparation: Promise<string> | null = null

/**
 * Returns the base directory for all projects.
 * In dev: <repo>/projects/
 * In prod: <Documents>/zAI Projects/
 */
export function getProjectsDir(): string {
  if (app.isPackaged) {
    // Install locations and macOS app bundles are not safe places for user data.
    return join(app.getPath('documents'), 'zAI Projects')
  }
  // In dev, use the repo root
  return join(app.getAppPath(), 'projects')
}

/**
 * Ensures the projects directory exists and copies projects from locations used
 * by older builds. Migration is intentionally copy-only: legacy data is never
 * deleted automatically, and a same-named project already in Documents wins.
 */
export async function prepareProjectsDir(): Promise<string> {
  if (!projectsPreparation) {
    projectsPreparation = prepareProjectsDirOnce().catch((error) => {
      projectsPreparation = null
      throw error
    })
  }
  return projectsPreparation
}

async function prepareProjectsDirOnce(): Promise<string> {
  const projectsDir = getProjectsDir()
  await ensureDir(projectsDir)

  if (!app.isPackaged) return projectsDir

  const migrationStatePath = join(app.getPath('userData'), 'project-migration-v1.json')
  const migratedEntries = await readMigrationState(migrationStatePath)
  for (const legacyDir of getLegacyProjectsDirs()) {
    if (resolve(legacyDir) === resolve(projectsDir)) continue
    await copyLegacyProjects(legacyDir, projectsDir, migratedEntries, migrationStatePath)
  }

  return projectsDir
}

function getLegacyProjectsDirs(): string[] {
  const appDataDir = app.getPath('appData')
  return [
    // v0.2 and earlier stored projects beside the executable. On macOS this
    // resolves inside the .app bundle; on Windows it may be under Program Files.
    join(dirname(app.getPath('exe')), 'projects'),
    // Also cover builds that followed the earlier documented userData layout.
    join(appDataDir, 'Your Friendly Terminal', 'projects'),
    join(appDataDir, 'your-friendly-terminal', 'projects'),
    join(appDataDir, 'friendly-terminal', 'projects')
  ]
}

async function copyLegacyProjects(
  sourceDir: string,
  projectsDir: string,
  migratedEntries: Set<string>,
  migrationStatePath: string
): Promise<void> {
  let entries: string[]
  try {
    entries = await readdir(sourceDir)
  } catch {
    return
  }

  for (const entry of entries) {
    const sourcePath = join(sourceDir, entry)
    const destinationPath = join(projectsDir, entry)
    const migrationKey = `${resolve(sourceDir)}\0${entry}`
    if (migratedEntries.has(migrationKey)) continue

    let destinationExists = false
    try {
      await lstat(destinationPath)
      destinationExists = true
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') {
        console.warn(`[paths] Could not inspect project destination "${entry}":`, error)
        continue
      }
    }

    if (destinationExists) {
      console.warn(
        `[paths] Skipped legacy project "${entry}" because a project with that name already exists`
      )
      await markMigrationComplete(migrationKey, migratedEntries, migrationStatePath).catch((error) => {
        console.warn(`[paths] Could not save migration state for "${entry}":`, error)
      })
      continue
    }

    const temporaryPath = join(projectsDir, `.${entry}.migration-${process.pid}`)
    try {
      const info = await lstat(sourcePath)
      await rm(temporaryPath, { recursive: true, force: true })

      if (info.isSymbolicLink()) {
        const target = await readlink(sourcePath)
        await symlink(target, temporaryPath, process.platform === 'win32' ? 'junction' : 'dir')
      } else if (info.isDirectory()) {
        await cp(sourcePath, temporaryPath, {
          recursive: true,
          force: false,
          errorOnExist: true,
          preserveTimestamps: true
        })
      } else {
        continue
      }

      await rename(temporaryPath, destinationPath)
      await markMigrationComplete(migrationKey, migratedEntries, migrationStatePath).catch((error) => {
        console.warn(`[paths] Could not save migration state for "${entry}":`, error)
      })
      console.log(`[paths] Migrated legacy project "${entry}" to ${destinationPath}`)
    } catch (error) {
      await rm(temporaryPath, { recursive: true, force: true }).catch(() => undefined)
      console.warn(`[paths] Could not migrate legacy project "${entry}":`, error)
    }
  }
}

async function readMigrationState(statePath: string): Promise<Set<string>> {
  try {
    const parsed = JSON.parse(await readFile(statePath, 'utf-8')) as { completed?: unknown }
    if (!Array.isArray(parsed.completed)) return new Set()
    return new Set(parsed.completed.filter((entry): entry is string => typeof entry === 'string'))
  } catch {
    return new Set()
  }
}

async function markMigrationComplete(
  migrationKey: string,
  migratedEntries: Set<string>,
  statePath: string
): Promise<void> {
  migratedEntries.add(migrationKey)
  await ensureDir(dirname(statePath))

  const temporaryStatePath = `${statePath}.${process.pid}.tmp`
  await writeFile(
    temporaryStatePath,
    JSON.stringify({ completed: [...migratedEntries].sort() }, null, 2),
    'utf-8'
  )
  await rename(temporaryStatePath, statePath)
}

/**
 * Returns the directory for a specific project by slug.
 */
export function getProjectDir(slug: string): string {
  return join(getProjectsDir(), slug)
}

/**
 * Creates a directory (and parents) if it does not exist.
 * Silently succeeds if the directory already exists.
 */
export async function ensureDir(dir: string): Promise<void> {
  await mkdir(dir, { recursive: true })
}
