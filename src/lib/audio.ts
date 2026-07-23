export interface CapturedAudio { audioBase64: string; format: string }

const MIME_TYPES = ["audio/webm;codecs=opus", "audio/webm", "audio/mp4", "audio/ogg;codecs=opus"]

export class SpeechCapture {
  private recorder: MediaRecorder | null = null
  private stream: MediaStream | null = null
  private chunks: Blob[] = []
  private context: AudioContext | null = null
  private frame: number | null = null

  get isRecording() { return this.recorder?.state === "recording" }

  async start(onLevel?: (level: number) => void) {
    if (this.isRecording) return
    if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") throw new Error("Audio capture is unavailable")
    this.cleanupStream()
    this.stream = await navigator.mediaDevices.getUserMedia({ audio: { autoGainControl: true, echoCancellation: true, noiseSuppression: true } })
    try {
      const mimeType = MIME_TYPES.find((value) => MediaRecorder.isTypeSupported(value))
      this.chunks = []
      this.recorder = mimeType ? new MediaRecorder(this.stream, { mimeType }) : new MediaRecorder(this.stream)
      this.recorder.ondataavailable = (event) => { if (event.data.size) this.chunks.push(event.data) }
      this.recorder.start(200)
      if (onLevel) this.monitor(this.stream, onLevel)
    } catch (error) {
      this.recorder = null
      this.cleanupStream()
      throw error
    }
  }

  async stop(): Promise<CapturedAudio> {
    const recorder = this.recorder
    if (!recorder || recorder.state === "inactive") throw new Error("No recording is active")
    const stopped = new Promise<void>((resolve) => recorder.addEventListener("stop", () => resolve(), { once: true }))
    try {
      recorder.stop()
      await stopped
    } catch (error) {
      this.chunks = []
      throw error
    } finally {
      if (this.recorder === recorder) this.recorder = null
      this.cleanupStream()
    }
    const mime = recorder.mimeType || this.chunks[0]?.type || "audio/webm"
    const blob = new Blob(this.chunks, { type: mime })
    this.chunks = []
    if (!blob.size) throw new Error("The recording is empty")
    if (blob.size > 24 * 1024 * 1024) throw new Error("The recording exceeds the 24 MiB limit")
    return { audioBase64: await toBase64(blob), format: mime.includes("mp4") ? "mp4" : mime.includes("ogg") ? "ogg" : "webm" }
  }

  cancel() {
    if (this.recorder && this.recorder.state !== "inactive") {
      this.recorder.ondataavailable = null
      this.recorder.stop()
    }
    this.recorder = null
    this.chunks = []
    this.cleanupStream()
  }

  private monitor(stream: MediaStream, onLevel: (level: number) => void) {
    try {
      const context = new AudioContext({ latencyHint: "interactive" })
      const analyser = context.createAnalyser()
      analyser.fftSize = 256
      context.createMediaStreamSource(stream).connect(analyser)
      const samples = new Float32Array(analyser.fftSize)
      this.context = context
      const sample = () => {
        analyser.getFloatTimeDomainData(samples)
        let sum = 0
        for (const value of samples) sum += value * value
        onLevel(Math.min(1, Math.sqrt(sum / samples.length) * 7))
        this.frame = requestAnimationFrame(sample)
      }
      this.frame = requestAnimationFrame(sample)
    } catch { /* waveform is optional */ }
  }

  private cleanupStream() {
    if (this.frame !== null) cancelAnimationFrame(this.frame)
    this.frame = null
    if (this.context && this.context.state !== "closed") void this.context.close()
    this.context = null
    this.stream?.getTracks().forEach((track) => track.stop())
    this.stream = null
  }
}

function toBase64(blob: Blob) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader()
    reader.onerror = () => reject(new Error("Unable to read the recording"))
    reader.onload = () => resolve(String(reader.result ?? "").split(",").at(-1) ?? "")
    reader.readAsDataURL(blob)
  })
}
