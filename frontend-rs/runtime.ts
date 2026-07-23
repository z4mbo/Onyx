import { invoke } from "@tauri-apps/api/core"
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi"
import { Webview } from "@tauri-apps/api/webview"
import { getCurrentWindow, type Theme } from "@tauri-apps/api/window"
import { open } from "@tauri-apps/plugin-dialog"
import { openUrl } from "@tauri-apps/plugin-opener"
import { relaunch } from "@tauri-apps/plugin-process"
import { check, type Update } from "@tauri-apps/plugin-updater"
import { FitAddon } from "@xterm/addon-fit"
import { Terminal } from "@xterm/xterm"
import { ConvexClient } from "convex/browser"
import { anyApi } from "convex/server"

type ProviderId = "chatgpt" | "claude" | "gemini" | "grok"
type Bounds = { x: number; y: number; width: number; height: number }
type TerminalHandle = {
  terminal: Terminal
  fit: FitAddon
  observer: ResizeObserver
  dataSubscription: { dispose(): void }
  resizeFrame: number
}

const providerDefinitions: Record<ProviderId, string> = {
  chatgpt: "https://chatgpt.com/",
  claude: "https://claude.ai/new",
  gemini: "https://gemini.google.com/app",
  grok: "https://grok.com/",
}
const providerViews = new Map<ProviderId, Webview>()
const providerCreating = new Map<ProviderId, Promise<Webview>>()
let activeProvider: ProviderId | null = null
let providerActivation = 0

const terminalHandles = new Map<number, TerminalHandle>()
const terminalHandlesBySession = new Map<string, Set<number>>()
const terminalReplay = new Map<string, string>()
const closedTerminals = new Set<string>()
let nextTerminalHandle = 1
const MAX_TERMINAL_REPLAY = 1_048_576

let pendingUpdate: Update | null = null
const convexUrl = import.meta.env.VITE_CONVEX_URL?.trim() ?? ""
let convex: ConvexClient | null = null

function providerLabel(provider: ProviderId) {
  return `provider-sidebar-${provider}`
}

function ready(view: Webview) {
  return new Promise<Webview>((resolve, reject) => {
    void view.once("tauri://created", () => resolve(view))
    void view.once<string>("tauri://error", (event) => {
      reject(new Error(event.payload || `Could not create ${view.label}`))
    })
  })
}

async function providerView(provider: ProviderId, bounds: Bounds) {
  const existing = providerViews.get(provider)
  if (existing) return existing
  const pending = providerCreating.get(provider)
  if (pending) return pending
  const url = providerDefinitions[provider]
  if (!url) throw new Error("Unknown official provider")
  const view = new Webview(getCurrentWindow(), providerLabel(provider), {
    url,
    x: bounds.x,
    y: bounds.y,
    width: bounds.width,
    height: bounds.height,
    focus: false,
    acceptFirstMouse: true,
    dragDropEnabled: false,
  })
  const promise = ready(view)
    .then((created) => {
      providerViews.set(provider, created)
      providerCreating.delete(provider)
      return created
    })
    .catch((error) => {
      providerCreating.delete(provider)
      throw error
    })
  providerCreating.set(provider, promise)
  return promise
}

async function positionProviderView(view: Webview, bounds: Bounds) {
  await view.setPosition(new LogicalPosition(bounds.x, bounds.y))
  await view.setSize(new LogicalSize(bounds.width, bounds.height))
}

async function showProvider(provider: ProviderId, bounds: Bounds) {
  const request = ++providerActivation
  if (activeProvider && activeProvider !== provider) {
    await providerViews.get(activeProvider)?.hide()
  }
  const view = await providerView(provider, bounds)
  if (request !== providerActivation) {
    await view.hide()
    return
  }
  await positionProviderView(view, bounds)
  await view.show()
  activeProvider = provider
}

async function positionProvider(provider: ProviderId, bounds: Bounds) {
  if (activeProvider !== provider) return
  const view = providerViews.get(provider)
  if (view) await positionProviderView(view, bounds)
}

async function focusProvider(provider: ProviderId) {
  if (activeProvider === provider) await providerViews.get(provider)?.setFocus()
}

async function hideProvider(provider?: ProviderId) {
  if (provider && activeProvider && provider !== activeProvider) return
  providerActivation += 1
  if (!activeProvider) return
  const previous = activeProvider
  activeProvider = null
  await providerViews.get(previous)?.hide()
}

function terminalTheme(element: HTMLElement) {
  const styles = getComputedStyle(element)
  return {
    background: "transparent",
    foreground: styles.color || "#e8e8e8",
    cursor: styles.color || "#e8e8e8",
    cursorAccent: styles.backgroundColor || "#111",
    selectionBackground: "rgba(120, 150, 255, 0.28)",
    black: "#202020",
    red: "#ff6b7a",
    green: "#77d991",
    yellow: "#e8c56b",
    blue: "#7db4ff",
    magenta: "#c79cff",
    cyan: "#69d9df",
    white: "#d7d7d7",
    brightBlack: "#777",
    brightRed: "#ff9aa5",
    brightGreen: "#a7e9b7",
    brightYellow: "#f6dc9b",
    brightBlue: "#a8ccff",
    brightMagenta: "#dfc0ff",
    brightCyan: "#9cebef",
    brightWhite: "#fff",
  }
}

function mountTerminal(
  element: HTMLElement,
  sessionId: string,
  onData: (data: string) => void,
  onResize: (cols: number, rows: number) => void,
  autofocus: boolean,
) {
  const terminal = new Terminal({
    allowTransparency: true,
    convertEol: false,
    cursorBlink: true,
    cursorStyle: "block",
    fontFamily: '"JetBrains Mono", "SFMono-Regular", Menlo, Consolas, monospace',
    fontSize: 12,
    lineHeight: 1.25,
    minimumContrastRatio: 4.5,
    scrollback: 5_000,
    theme: terminalTheme(element),
  })
  const fit = new FitAddon()
  terminal.loadAddon(fit)
  terminal.open(element)
  const replay = terminalReplay.get(sessionId)
  if (replay) terminal.write(replay)
  if (closedTerminals.has(sessionId)) terminal.options.disableStdin = true

  const handleId = nextTerminalHandle++
  const resize = () => {
    const handle = terminalHandles.get(handleId)
    if (!handle) return
    cancelAnimationFrame(handle.resizeFrame)
    handle.resizeFrame = requestAnimationFrame(() => {
      if (!element.isConnected) return
      try {
        fit.fit()
        if (terminal.cols > 0 && terminal.rows > 0) {
          onResize(terminal.cols, terminal.rows)
        }
      } catch {
        // A subsequent ResizeObserver notification retries after layout settles.
      }
    })
  }
  const observer = new ResizeObserver(resize)
  observer.observe(element)
  const dataSubscription = terminal.onData((data) => {
    if (!closedTerminals.has(sessionId)) onData(data)
  })
  terminalHandles.set(handleId, {
    terminal,
    fit,
    observer,
    dataSubscription,
    resizeFrame: 0,
  })
  const sessionHandles = terminalHandlesBySession.get(sessionId) ?? new Set()
  sessionHandles.add(handleId)
  terminalHandlesBySession.set(sessionId, sessionHandles)
  resize()
  if (autofocus) queueMicrotask(() => terminal.focus())
  return handleId
}

function unmountTerminal(handleId: number, sessionId: string) {
  const handle = terminalHandles.get(handleId)
  if (!handle) return
  cancelAnimationFrame(handle.resizeFrame)
  handle.observer.disconnect()
  handle.dataSubscription.dispose()
  handle.terminal.dispose()
  terminalHandles.delete(handleId)
  const sessionHandles = terminalHandlesBySession.get(sessionId)
  sessionHandles?.delete(handleId)
  if (sessionHandles?.size === 0) terminalHandlesBySession.delete(sessionId)
}

function appendTerminalReplay(sessionId: string, data: string) {
  const next = `${terminalReplay.get(sessionId) ?? ""}${data}`
  terminalReplay.set(
    sessionId,
    next.length > MAX_TERMINAL_REPLAY
      ? next.slice(next.length - MAX_TERMINAL_REPLAY)
      : next,
  )
}

function writeTerminal(sessionId: string, data: string) {
  appendTerminalReplay(sessionId, data)
  terminalHandlesBySession.get(sessionId)?.forEach((handleId) => {
    terminalHandles.get(handleId)?.terminal.write(data)
  })
}

function exitTerminal(sessionId: string, exitCode?: number | null, error?: string | null) {
  closedTerminals.add(sessionId)
  const suffix = error
    ? `\r\n\u001b[31m[terminal] ${error}\u001b[0m\r\n`
    : `\r\n\u001b[2m[process exited${exitCode == null ? "" : ` with code ${exitCode}`}]\u001b[0m\r\n`
  writeTerminal(sessionId, suffix)
  terminalHandlesBySession.get(sessionId)?.forEach((handleId) => {
    const terminal = terminalHandles.get(handleId)?.terminal
    if (terminal) terminal.options.disableStdin = true
  })
}

function clearTerminal(sessionId: string) {
  terminalReplay.set(sessionId, "")
  terminalHandlesBySession.get(sessionId)?.forEach((handleId) => {
    terminalHandles.get(handleId)?.terminal.clear()
  })
}

function forgetTerminal(sessionId: string) {
  terminalReplay.delete(sessionId)
  closedTerminals.delete(sessionId)
}

export interface CapturedAudio {
  audioBase64: string
  format: string
}

const TARGET_SAMPLE_RATE = 16_000
const MAX_CAPTURE_SECONDS = 5 * 60

class SpeechCapture {
  private stream: MediaStream | null = null
  private context: AudioContext | null = null
  private processor: ScriptProcessorNode | null = null
  private chunks: Float32Array[] = []
  private sampleRate = TARGET_SAMPLE_RATE
  private recording = false

  get active() {
    return this.recording
  }

  async requestAccess() {
    this.assertAvailable()
    let stream: MediaStream | null = null
    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: true })
    } finally {
      stream?.getTracks().forEach((track) => track.stop())
    }
  }

  async start(onLevel?: (level: number) => void) {
    if (this.recording) return
    this.assertAvailable()
    this.cleanup()
    this.stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        autoGainControl: true,
        echoCancellation: true,
        noiseSuppression: true,
      },
    })
    try {
      const context = new AudioContext({ latencyHint: "interactive" })
      if (context.state === "suspended") await context.resume()
      const source = context.createMediaStreamSource(this.stream)
      const processor = context.createScriptProcessor(4096, 1, 1)
      const maxSamples = context.sampleRate * MAX_CAPTURE_SECONDS
      this.sampleRate = context.sampleRate
      this.chunks = []
      let collected = 0
      processor.onaudioprocess = (event) => {
        const samples = event.inputBuffer.getChannelData(0)
        if (collected < maxSamples) {
          this.chunks.push(new Float32Array(samples))
          collected += samples.length
        }
        if (onLevel) {
          let sum = 0
          for (const value of samples) sum += value * value
          onLevel(Math.min(1, Math.sqrt(sum / samples.length) * 7))
        }
      }
      source.connect(processor)
      processor.connect(context.destination)
      this.context = context
      this.processor = processor
      this.recording = true
    } catch (error) {
      this.cleanup()
      throw error
    }
  }

  async stop(): Promise<CapturedAudio> {
    if (!this.recording) throw new Error("No recording is active")
    const chunks = this.chunks
    const sampleRate = this.sampleRate
    this.cleanup()
    const samples = downsample(concat(chunks), sampleRate, TARGET_SAMPLE_RATE)
    if (!samples.length) throw new Error("The recording is empty")
    const blob = new Blob([encodeWav(samples, TARGET_SAMPLE_RATE)])
    return { audioBase64: await toBase64(blob), format: "wav" }
  }

  cancel() {
    this.cleanup()
  }

  private assertAvailable() {
    if (!navigator.mediaDevices?.getUserMedia || typeof AudioContext === "undefined") {
      throw new Error("Audio capture is unavailable in this window.")
    }
  }

  private cleanup() {
    this.recording = false
    this.chunks = []
    if (this.processor) {
      this.processor.onaudioprocess = null
      this.processor.disconnect()
    }
    this.processor = null
    if (this.context && this.context.state !== "closed") void this.context.close()
    this.context = null
    this.stream?.getTracks().forEach((track) => track.stop())
    this.stream = null
  }
}

function concat(chunks: Float32Array[]) {
  const merged = new Float32Array(chunks.reduce((total, chunk) => total + chunk.length, 0))
  let offset = 0
  for (const chunk of chunks) {
    merged.set(chunk, offset)
    offset += chunk.length
  }
  return merged
}

function downsample(samples: Float32Array, from: number, to: number) {
  if (from <= to || !samples.length) return samples
  const ratio = from / to
  const output = new Float32Array(Math.floor(samples.length / ratio))
  for (let index = 0; index < output.length; index += 1) {
    const position = index * ratio
    const left = Math.floor(position)
    const right = Math.min(left + 1, samples.length - 1)
    output[index] = samples[left] + (samples[right] - samples[left]) * (position - left)
  }
  return output
}

function encodeWav(samples: Float32Array, sampleRate: number) {
  const buffer = new ArrayBuffer(44 + samples.length * 2)
  const view = new DataView(buffer)
  const writeAscii = (offset: number, text: string) => {
    for (let index = 0; index < text.length; index += 1) {
      view.setUint8(offset + index, text.charCodeAt(index))
    }
  }
  writeAscii(0, "RIFF")
  view.setUint32(4, 36 + samples.length * 2, true)
  writeAscii(8, "WAVE")
  writeAscii(12, "fmt ")
  view.setUint32(16, 16, true)
  view.setUint16(20, 1, true)
  view.setUint16(22, 1, true)
  view.setUint32(24, sampleRate, true)
  view.setUint32(28, sampleRate * 2, true)
  view.setUint16(32, 2, true)
  view.setUint16(34, 16, true)
  writeAscii(36, "data")
  view.setUint32(40, samples.length * 2, true)
  for (let index = 0; index < samples.length; index += 1) {
    const clamped = Math.max(-1, Math.min(1, samples[index]))
    view.setInt16(44 + index * 2, clamped < 0 ? clamped * 0x8000 : clamped * 0x7fff, true)
  }
  return buffer
}

function toBase64(blob: Blob) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader()
    reader.onerror = () => reject(new Error("Unable to read the recording"))
    reader.onload = () => resolve(String(reader.result ?? "").split(",").at(-1) ?? "")
    reader.readAsDataURL(blob)
  })
}

const speechCapture = new SpeechCapture()

async function checkUpdate() {
  pendingUpdate = await check()
  return pendingUpdate ? { version: pendingUpdate.version } : null
}

async function installUpdate(onProgress?: (downloaded: number, total: number | null) => void) {
  if (!pendingUpdate) throw new Error("No update is ready to install")
  let downloaded = 0
  let total: number | null = null
  await pendingUpdate.downloadAndInstall((event) => {
    if (event.event === "Started") total = event.data.contentLength ?? null
    else if (event.event === "Progress") downloaded += event.data.chunkLength
    onProgress?.(downloaded, total)
  })
  await relaunch()
}

function cloudConfigured() {
  return Boolean(convexUrl)
}

function startCloudAuth(onAuthenticated: (authenticated: boolean) => void) {
  if (!convexUrl) return false
  convex ??= new ConvexClient(convexUrl)
  convex.setAuth(
    async ({ forceRefreshToken }) =>
      await invoke<string | null>("clerk_account_token", {
        forceRefresh: forceRefreshToken,
      }),
    onAuthenticated,
  )
  return true
}

async function pushCloud(payload: string) {
  if (!convex) throw new Error("Cloud sync is not configured")
  await convex.mutation(anyApi.sync.upsertSnapshot, { payload })
}

async function pullCloud() {
  if (!convex) throw new Error("Cloud sync is not configured")
  return await convex.query(anyApi.sync.latestSnapshot, {}) as string | null
}

const runtime = {
  openDirectory: () => open({ directory: true, multiple: false, title: "Choose a workspace" }),
  openFiles: () => open({ directory: false, multiple: true, title: "Attach files" }),
  openUrl,
  setWindowTheme: (theme: Theme | null) => getCurrentWindow().setTheme(theme),
  showProvider,
  positionProvider,
  focusProvider,
  hideProvider,
  mountTerminal,
  unmountTerminal,
  writeTerminal,
  exitTerminal,
  clearTerminal,
  forgetTerminal,
  requestMicrophoneAccess: () => speechCapture.requestAccess(),
  startAudioCapture: (onLevel: (level: number) => void) => speechCapture.start(onLevel),
  stopAudioCapture: () => speechCapture.stop(),
  cancelAudioCapture: () => speechCapture.cancel(),
  audioCaptureActive: () => speechCapture.active,
  playAudio: async (source: string) => {
    if (source) await new Audio(source).play()
  },
  copyText: (text: string) => navigator.clipboard.writeText(text),
  checkUpdate,
  installUpdate,
  cloudConfigured,
  startCloudAuth,
  pushCloud,
  pullCloud,
}

Object.assign(window, { __ONYX_RUNTIME__: runtime })

declare global {
  interface Window {
    __ONYX_RUNTIME__: typeof runtime
  }
}
