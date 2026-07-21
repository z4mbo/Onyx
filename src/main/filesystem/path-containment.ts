import { realpath } from 'fs/promises'
import { isAbsolute, relative, resolve, posix, win32 } from 'path'

function assertContained(rootPath: string, targetPath: string): void {
  const pathFromRoot = relative(rootPath, targetPath)
  if (pathFromRoot === '..' || pathFromRoot.startsWith(`..${process.platform === 'win32' ? '\\' : '/'}`) || isAbsolute(pathFromRoot)) {
    throw new Error('Canvas paths must stay inside the active project.')
  }
}

/** Resolves a user-supplied relative path without allowing lexical traversal. */
export function resolvePathWithinRoot(rootInput: unknown, relativeInput: unknown): string {
  if (typeof rootInput !== 'string' || !rootInput || rootInput.length > 32_768) {
    throw new Error('The active project path is invalid.')
  }
  if (
    typeof relativeInput !== 'string' ||
    !relativeInput ||
    relativeInput.length > 4_096 ||
    relativeInput.includes('\0') ||
    posix.isAbsolute(relativeInput) ||
    win32.isAbsolute(relativeInput)
  ) {
    throw new Error('Canvas file requests must use a relative project path.')
  }

  const rootPath = resolve(rootInput)
  const targetPath = resolve(rootPath, relativeInput)
  assertContained(rootPath, targetPath)
  return targetPath
}

/** Also resolves symlinks so an in-project link cannot escape the project. */
export async function resolveExistingPathWithinRoot(
  rootInput: unknown,
  relativeInput: unknown
): Promise<string> {
  const lexicalTarget = resolvePathWithinRoot(rootInput, relativeInput)
  const [canonicalRoot, canonicalTarget] = await Promise.all([
    realpath(rootInput as string),
    realpath(lexicalTarget)
  ])
  assertContained(canonicalRoot, canonicalTarget)
  return canonicalTarget
}
