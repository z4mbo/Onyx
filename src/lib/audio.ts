export interface CapturedAudio {
  audioBase64: string;
  format: string;
  mimeType: string;
}

export type AudioLevelHandler = (level: number) => void;

const MIME_TYPES = [
  "audio/webm;codecs=opus",
  "audio/webm",
  "audio/mp4;codecs=mp4a.40.2",
  "audio/mp4",
  "audio/ogg;codecs=opus",
];

export class SpeechCapture {
  private recorder: MediaRecorder | null = null;
  private stream: MediaStream | null = null;
  private chunks: Blob[] = [];
  private audioContext: AudioContext | null = null;
  private analyser: AnalyserNode | null = null;
  private audioSource: MediaStreamAudioSourceNode | null = null;
  private levelFrame: number | null = null;
  private smoothedLevel = 0;

  get isRecording(): boolean {
    return this.recorder?.state === "recording";
  }

  async start(onLevel?: AudioLevelHandler): Promise<void> {
    if (this.isRecording) return;
    if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
      throw new Error("La registrazione audio non è disponibile su questo sistema.");
    }
    this.stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        autoGainControl: true,
        echoCancellation: true,
        noiseSuppression: true,
      },
    });
    const mimeType = MIME_TYPES.find((candidate) => MediaRecorder.isTypeSupported(candidate));
    this.chunks = [];
    this.recorder = mimeType
      ? new MediaRecorder(this.stream, { mimeType })
      : new MediaRecorder(this.stream);
    this.recorder.addEventListener("dataavailable", (event) => {
      if (event.data.size > 0) this.chunks.push(event.data);
    });
    this.recorder.start(250);
    this.startLevelMonitoring(this.stream, onLevel);
  }

  async stop(): Promise<CapturedAudio> {
    const recorder = this.recorder;
    if (!recorder || recorder.state === "inactive") {
      throw new Error("Non c'è una registrazione attiva.");
    }
    const stopped = new Promise<void>((resolve) => {
      recorder.addEventListener("stop", () => resolve(), { once: true });
    });
    recorder.stop();
    await stopped;
    this.stopLevelMonitoring();
    this.stopTracks();

    const mimeType = recorder.mimeType || this.chunks[0]?.type || "audio/webm";
    const blob = new Blob(this.chunks, { type: mimeType });
    this.recorder = null;
    this.chunks = [];
    if (blob.size === 0) throw new Error("La registrazione è vuota.");
    const wav = await recordingToWav(blob);
    return {
      audioBase64: await blobToBase64(wav),
      format: "wav",
      mimeType: "audio/wav",
    };
  }

  cancel(): void {
    if (this.recorder && this.recorder.state !== "inactive") this.recorder.stop();
    this.recorder = null;
    this.chunks = [];
    this.stopLevelMonitoring();
    this.stopTracks();
  }

  private startLevelMonitoring(stream: MediaStream, onLevel?: AudioLevelHandler): void {
    this.stopLevelMonitoring();
    if (!onLevel) return;

    const AudioContextConstructor = window.AudioContext
      ?? (window as typeof window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!AudioContextConstructor) return;

    try {
      const context = new AudioContextConstructor({ latencyHint: "interactive" });
      const analyser = context.createAnalyser();
      const source = context.createMediaStreamSource(stream);
      const samples = new Float32Array(512);

      analyser.fftSize = samples.length;
      analyser.smoothingTimeConstant = 0.25;
      source.connect(analyser);

      this.audioContext = context;
      this.analyser = analyser;
      this.audioSource = source;
      this.smoothedLevel = 0;
      void context.resume().catch(() => undefined);
      let lastEmittedAt = 0;

      const sampleLevel = (timestamp: number) => {
        if (this.analyser !== analyser) return;

        analyser.getFloatTimeDomainData(samples);
        let sumSquares = 0;
        for (const sample of samples) sumSquares += sample * sample;
        const rms = Math.sqrt(sumSquares / samples.length);

        // Remove the usual microphone noise floor, then expand normal speech so
        // quiet and loud voices both produce an obvious but stable deformation.
        const normalized = Math.min(1, Math.max(0, (rms - 0.008) / 0.16));
        const responsiveLevel = Math.pow(normalized, 0.58);
        const smoothing = responsiveLevel > this.smoothedLevel ? 0.58 : 0.16;
        this.smoothedLevel += (responsiveLevel - this.smoothedLevel) * smoothing;
        if (this.smoothedLevel < 0.008) this.smoothedLevel = 0;

        if (timestamp - lastEmittedAt >= 32) {
          lastEmittedAt = timestamp;
          onLevel(this.smoothedLevel);
        }
        this.levelFrame = window.requestAnimationFrame(sampleLevel);
      };

      this.levelFrame = window.requestAnimationFrame(sampleLevel);
    } catch {
      this.stopLevelMonitoring();
    }
  }

  private stopLevelMonitoring(): void {
    if (this.levelFrame !== null) window.cancelAnimationFrame(this.levelFrame);
    this.levelFrame = null;
    this.audioSource?.disconnect();
    this.analyser?.disconnect();
    const context = this.audioContext;
    this.audioSource = null;
    this.analyser = null;
    this.audioContext = null;
    this.smoothedLevel = 0;
    if (context && context.state !== "closed") void context.close().catch(() => undefined);
  }

  private stopTracks(): void {
    this.stream?.getTracks().forEach((track) => track.stop());
    this.stream = null;
  }
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("Impossibile leggere la registrazione."));
    reader.onload = () => {
      const result = String(reader.result ?? "");
      resolve(result.includes(",") ? result.slice(result.indexOf(",") + 1) : result);
    };
    reader.readAsDataURL(blob);
  });
}

async function recordingToWav(blob: Blob): Promise<Blob> {
  const AudioContextConstructor = window.AudioContext
    ?? (window as typeof window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!AudioContextConstructor) {
    throw new Error("La preparazione dell'audio non è disponibile su questo sistema.");
  }
  const context = new AudioContextConstructor();
  try {
    const decoded = await context.decodeAudioData(await blob.arrayBuffer());
    const samples = await renderMono16Khz(decoded);
    return new Blob([encodeWavPcm16(samples)], { type: "audio/wav" });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`Non riesco a preparare l'audio per la trascrizione: ${detail}`);
  } finally {
    if (context.state !== "closed") await context.close().catch(() => undefined);
  }
}

async function renderMono16Khz(audio: AudioBuffer): Promise<Float32Array> {
  if (typeof OfflineAudioContext === "undefined") return downmixAndResample(audio, 16_000);
  const frameCount = Math.max(1, Math.ceil(audio.duration * 16_000));
  const offline = new OfflineAudioContext(1, frameCount, 16_000);
  const mono = offline.createBuffer(1, audio.length, audio.sampleRate);
  const target = mono.getChannelData(0);
  for (let channel = 0; channel < audio.numberOfChannels; channel += 1) {
    const source = audio.getChannelData(channel);
    for (let index = 0; index < source.length; index += 1) {
      target[index] += source[index] / audio.numberOfChannels;
    }
  }
  const node = offline.createBufferSource();
  node.buffer = mono;
  node.connect(offline.destination);
  node.start();
  const rendered = await offline.startRendering();
  return rendered.getChannelData(0).slice();
}

function downmixAndResample(audio: AudioBuffer, targetRate: number): Float32Array {
  const mono = new Float32Array(audio.length);
  for (let channel = 0; channel < audio.numberOfChannels; channel += 1) {
    const source = audio.getChannelData(channel);
    for (let index = 0; index < source.length; index += 1) {
      mono[index] += source[index] / audio.numberOfChannels;
    }
  }
  if (audio.sampleRate === targetRate) return mono;
  const outputLength = Math.max(1, Math.round(mono.length * targetRate / audio.sampleRate));
  const output = new Float32Array(outputLength);
  const ratio = audio.sampleRate / targetRate;
  for (let index = 0; index < outputLength; index += 1) {
    const position = index * ratio;
    const left = Math.floor(position);
    const right = Math.min(left + 1, mono.length - 1);
    const mix = position - left;
    output[index] = mono[left] * (1 - mix) + mono[right] * mix;
  }
  return output;
}

export function encodeWavPcm16(samples: Float32Array, sampleRate = 16_000): ArrayBuffer {
  const buffer = new ArrayBuffer(44 + samples.length * 2);
  const view = new DataView(buffer);
  writeAscii(view, 0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  writeAscii(view, 8, "WAVE");
  writeAscii(view, 12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  writeAscii(view, 36, "data");
  view.setUint32(40, samples.length * 2, true);
  for (let index = 0; index < samples.length; index += 1) {
    const sample = Math.max(-1, Math.min(1, samples[index]));
    view.setInt16(44 + index * 2, sample < 0 ? sample * 0x8000 : sample * 0x7fff, true);
  }
  return buffer;
}

function writeAscii(view: DataView, offset: number, value: string): void {
  for (let index = 0; index < value.length; index += 1) {
    view.setUint8(offset + index, value.charCodeAt(index));
  }
}
