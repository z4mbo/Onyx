import { randomBytes } from 'crypto'
import { ipcMain, protocol } from 'electron'
import { buildCanvasDocument } from './canvas-document'

const CANVAS_SCHEME = 'zai-canvas'
const MAX_DOCUMENT_BYTES = 2 * 1024 * 1024
const MAX_DOCUMENTS_PER_RENDERER = 8
const documents = new Map<string, { content: string; ownerId: number }>()
const ownerTokens = new Map<number, string[]>()
const observedOwners = new Set<number>()
let protocolRegistered = false

const CANVAS_CSP = [
  "default-src 'none'",
  "script-src 'unsafe-inline'",
  "style-src 'unsafe-inline'",
  'img-src data:',
  'font-src data:',
  'media-src data:',
  "connect-src 'none'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'"
].join('; ')

function forgetToken(token: string): void {
  const document = documents.get(token)
  if (!document) return
  documents.delete(token)
  const tokens = ownerTokens.get(document.ownerId)?.filter((candidate) => candidate !== token) ?? []
  if (tokens.length > 0) ownerTokens.set(document.ownerId, tokens)
  else ownerTokens.delete(document.ownerId)
}

function forgetOwner(ownerId: number): void {
  for (const token of ownerTokens.get(ownerId) ?? []) documents.delete(token)
  ownerTokens.delete(ownerId)
  observedOwners.delete(ownerId)
}

/** Used by navigation guards to bind a Canvas URL to its renderer owner. */
export function isOwnedCanvasDocumentUrl(urlInput: string, ownerId: number): boolean {
  try {
    const url = new URL(urlInput)
    const token = url.protocol === `${CANVAS_SCHEME}:` && url.hostname === 'document'
      ? url.pathname.slice(1)
      : ''
    return documents.get(token)?.ownerId === ownerId
  } catch {
    return false
  }
}

/** Must run synchronously before Electron's ready event. */
export function registerCanvasScheme(): void {
  protocol.registerSchemesAsPrivileged([
    {
      scheme: CANVAS_SCHEME,
      privileges: { standard: true, secure: true }
    }
  ])
}

/** Registers the isolated Canvas document protocol and its narrow lifecycle IPC. */
export function registerCanvasProtocol(): void {
  if (protocolRegistered) return
  protocolRegistered = true

  protocol.handle(CANVAS_SCHEME, (request) => {
    const url = new URL(request.url)
    const token = url.hostname === 'document' ? url.pathname.slice(1) : ''
    const document = /^[a-f0-9]{48}$/.test(token) ? documents.get(token) : undefined
    if (!document) {
      return new Response('Canvas document not found.', {
        status: 404,
        headers: { 'Content-Type': 'text/plain; charset=utf-8' }
      })
    }

    return new Response(document.content, {
      status: 200,
      headers: {
        'Cache-Control': 'no-store',
        'Content-Security-Policy': CANVAS_CSP,
        'Content-Type': 'text/html; charset=utf-8',
        'Referrer-Policy': 'no-referrer',
        'X-Content-Type-Options': 'nosniff'
      }
    })
  })

  ipcMain.handle('canvas:create-document', (event, contentInput: unknown) => {
    if (typeof contentInput !== 'string' || Buffer.byteLength(contentInput, 'utf-8') > MAX_DOCUMENT_BYTES) {
      throw new Error('Canvas HTML must be text no larger than 2 MB.')
    }

    const ownerId = event.sender.id
    if (!observedOwners.has(ownerId)) {
      observedOwners.add(ownerId)
      event.sender.once('destroyed', () => forgetOwner(ownerId))
    }

    const token = randomBytes(24).toString('hex')
    documents.set(token, { content: buildCanvasDocument(contentInput), ownerId })
    const tokens = [...(ownerTokens.get(ownerId) ?? []), token]
    while (tokens.length > MAX_DOCUMENTS_PER_RENDERER) {
      const expiredToken = tokens.shift()
      if (expiredToken) documents.delete(expiredToken)
    }
    ownerTokens.set(ownerId, tokens)
    return { token, url: `${CANVAS_SCHEME}://document/${token}` }
  })

  ipcMain.handle('canvas:dispose-document', (event, tokenInput: unknown) => {
    if (typeof tokenInput !== 'string') return
    const document = documents.get(tokenInput)
    if (document?.ownerId === event.sender.id) forgetToken(tokenInput)
  })
}
