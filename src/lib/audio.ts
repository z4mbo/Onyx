export interface CapturedAudio { audioBase64: string; format: string }

/**
 * Overlay phases like "Transcribing" must never sit forever: a wedged network
 * request or blocked keychain read would otherwise leave the HUD stuck with no
 * feedback. Rejects with an actionable message once the deadline passes.
 */
export function withDeadline<T>(work: Promise<T>, seconds: number, label: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(
      () => reject(new Error(`${label} timed out after ${seconds} seconds. Check your connection and API keys in Settings.`)),
      seconds * 1000,
    )
    work.then(
      (value) => { window.clearTimeout(timer); resolve(value) },
      (error) => { window.clearTimeout(timer); reject(error) },
    )
  })
}

// Transcription rides OpenRouter/OpenAI APIs that expect wav/mp3, so Onyx
// captures raw PCM and encodes WAV itself instead of trusting MediaRecorder's
// per-platform codec lottery (Safari yields mp4/AAC, Chrome webm/opus).
const TARGET_SAMPLE_RATE = 16_000
const MAX_CAPTURE_SECONDS = 5 * 60

function audioCaptureError(error: unknown) {
  const name = error instanceof DOMException ? error.name : ""
  if (name === "NotAllowedError" || name === "SecurityError") {
    return new Error(
      "Microphone access is blocked. Enable Onyx in System Settings → Privacy & Security → Microphone, then relaunch Onyx.",
    )
  }
  if (name === "NotFoundError" || name === "DevicesNotFoundError") {
    return new Error("No microphone was found. Connect an input device and try again.")
  }
  if (name === "NotReadableError" || name === "TrackStartError") {
    return new Error("The microphone is busy or unavailable. Close other audio apps and try again.")
  }
  return error instanceof Error ? error : new Error(String(error))
}

function assertAudioCaptureAvailable() {
  if (!navigator.mediaDevices?.getUserMedia || typeof AudioContext === "undefined") {
    throw new Error("Audio capture is unavailable in this window.")
  }
}

/** Requests macOS/browser microphone permission from a focused, user-driven UI. */
export async function requestMicrophoneAccess() {
  assertAudioCaptureAvailable()
  let stream: MediaStream | null = null
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: true })
  } catch (error) {
    throw audioCaptureError(error)
  } finally {
    stream?.getTracks().forEach((track) => track.stop())
  }
}

export class SpeechCapture {
  private stream: MediaStream | null = null
  private context: AudioContext | null = null
  private processor: ScriptProcessorNode | null = null
  private chunks: Float32Array[] = []
  private sampleRate = TARGET_SAMPLE_RATE
  private recording = false

  get isRecording() { return this.recording }

  async start(onLevel?: (level: number) => void) {
    if (this.recording) return
    assertAudioCaptureAvailable()
    this.cleanup()
    try {
      this.stream = await navigator.mediaDevices.getUserMedia({ audio: { autoGainControl: true, echoCancellation: true, noiseSuppression: true } })
    } catch (error) {
      throw audioCaptureError(error)
    }
    try {
      const context = new AudioContext({ latencyHint: "interactive" })
      // The HUD records from a hotkey, not a click: WebKit can hand back a
      // suspended context here, which would capture pure silence.
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
      // ScriptProcessor only fires while routed to a destination; its output stays silent.
      processor.connect(context.destination)
      this.context = context
      this.processor = processor
      this.recording = true
    } catch (error) {
      this.cleanup()
      throw audioCaptureError(error)
    }
  }

  async stop(): Promise<CapturedAudio> {
    if (!this.recording) throw new Error("No recording is active")
    const chunks = this.chunks
    const sampleRate = this.sampleRate
    this.cleanup()
    const samples = downsample(concat(chunks), sampleRate, TARGET_SAMPLE_RATE)
    if (!samples.length) throw new Error("The recording is empty")
    return { audioBase64: await toBase64(new Blob([encodeWav(samples, TARGET_SAMPLE_RATE)])), format: "wav" }
  }

  cancel() {
    this.cleanup()
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

export function downsample(samples: Float32Array, from: number, to: number) {
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

export function encodeWav(samples: Float32Array, sampleRate: number) {
  const buffer = new ArrayBuffer(44 + samples.length * 2)
  const view = new DataView(buffer)
  const writeAscii = (offset: number, text: string) => {
    for (let index = 0; index < text.length; index += 1) view.setUint8(offset + index, text.charCodeAt(index))
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
