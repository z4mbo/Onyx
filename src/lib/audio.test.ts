import { describe, expect, it } from "vitest"
import { downsample, encodeWav } from "./audio"

describe("dictation audio capture", () => {
  it("encodes 16-bit mono PCM wav that transcription providers accept", () => {
    const samples = new Float32Array([0, 0.5, -0.5, 1, -1, 2, -2])
    const wav = new DataView(encodeWav(samples, 16_000))
    const ascii = (offset: number, length: number) =>
      Array.from({ length }, (_, index) => String.fromCharCode(wav.getUint8(offset + index))).join("")
    expect(ascii(0, 4)).toBe("RIFF")
    expect(ascii(8, 4)).toBe("WAVE")
    expect(wav.getUint16(20, true)).toBe(1)
    expect(wav.getUint16(22, true)).toBe(1)
    expect(wav.getUint32(24, true)).toBe(16_000)
    expect(wav.getUint16(34, true)).toBe(16)
    expect(wav.getUint32(40, true)).toBe(samples.length * 2)
    expect(wav.byteLength).toBe(44 + samples.length * 2)
    expect(wav.getInt16(44, true)).toBe(0)
    // Out-of-range samples clip instead of wrapping into noise.
    expect(wav.getInt16(44 + 5 * 2, true)).toBe(0x7fff)
    expect(wav.getInt16(44 + 6 * 2, true)).toBe(-0x8000)
  })

  it("downsamples to the transcription rate and passes lower rates through", () => {
    const input = new Float32Array(48_000)
    expect(downsample(input, 48_000, 16_000).length).toBe(16_000)
    const low = new Float32Array([0.1, 0.2])
    expect(downsample(low, 8_000, 16_000)).toBe(low)
  })
})
