import { describe, expect, it } from "vitest";

import { encodeWavPcm16 } from "./audio";

describe("transcription WAV encoder", () => {
  it("writes mono PCM16 audio at 16 kHz", () => {
    const wav = encodeWavPcm16(new Float32Array([-1, 0, 1]));
    const bytes = new Uint8Array(wav);
    const view = new DataView(wav);

    expect(new TextDecoder().decode(bytes.slice(0, 4))).toBe("RIFF");
    expect(new TextDecoder().decode(bytes.slice(8, 12))).toBe("WAVE");
    expect(view.getUint16(20, true)).toBe(1);
    expect(view.getUint16(22, true)).toBe(1);
    expect(view.getUint32(24, true)).toBe(16_000);
    expect(view.getUint16(34, true)).toBe(16);
    expect(view.getUint32(40, true)).toBe(6);
    expect(view.getInt16(44, true)).toBe(-32_768);
    expect(view.getInt16(46, true)).toBe(0);
    expect(view.getInt16(48, true)).toBe(32_767);
  });

  it("clips samples outside the valid PCM range", () => {
    const wav = encodeWavPcm16(new Float32Array([-2, 2]));
    const view = new DataView(wav);
    expect(view.getInt16(44, true)).toBe(-32_768);
    expect(view.getInt16(46, true)).toBe(32_767);
  });
});
