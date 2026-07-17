import { describe, expect, it } from "vitest";

import {
  WAVE_BAR_COUNT,
  emptyWaveHistory,
  pushWaveLevel,
  waveBarHeights,
} from "./waveform";

describe("voice waveform", () => {
  it("renders thirteen quiet bars at the minimum height", () => {
    expect(WAVE_BAR_COUNT).toBe(13);
    expect(waveBarHeights(emptyWaveHistory())).toEqual(Array(13).fill(3));
  });

  it("is symmetric and tallest at the center for a new voice peak", () => {
    const heights = waveBarHeights(pushWaveLevel(emptyWaveHistory(), 1));
    expect(heights).toHaveLength(13);
    expect(heights).toEqual([...heights].reverse());
    expect(heights[6]).toBeGreaterThan(heights[5]);
    expect(Math.max(...heights)).toBeLessThanOrEqual(24);
  });

  it("clamps invalid levels and propagates history outwards", () => {
    let history = pushWaveLevel(emptyWaveHistory(), 4);
    history = pushWaveLevel(history, Number.NaN);
    const heights = waveBarHeights(history);
    expect(heights[6]).toBe(3);
    expect(heights[5]).toBeGreaterThan(3);
    expect(heights[7]).toBe(heights[5]);
  });
});
