import { dirname, resolve } from 'path'

const WINDOWS_RESERVED_NAME = /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?$/i
const INVALID_PROJECT_CHARACTERS = /[<>:"/\\|?*\u0000-\u001f\u007f]/

/** Returns a portable, single-component project name or throws. */
export function validateProjectName(input: unknown): string {
  if (typeof input !== 'string') throw new Error('Project name must be text.')
  const name = input.trim().normalize('NFC')
  if (!name || name.length > 100) {
    throw new Error('Project name must be between 1 and 100 characters.')
  }
  if (name === '.' || name === '..' || INVALID_PROJECT_CHARACTERS.test(name)) {
    throw new Error('Project name contains characters that cannot be used in a folder name.')
  }
  if (/[. ]$/.test(name) || WINDOWS_RESERVED_NAME.test(name)) {
    throw new Error('Choose a different project name.')
  }
  return name
}

/** Resolves a project directly under root and verifies it cannot escape root. */
export function resolveManagedProjectPath(root: string, input: unknown): string {
  const rootPath = resolve(root)
  const projectPath = resolve(rootPath, validateProjectName(input))
  if (dirname(projectPath) !== rootPath) {
    throw new Error('Project path is outside the managed projects folder.')
  }
  return projectPath
}
