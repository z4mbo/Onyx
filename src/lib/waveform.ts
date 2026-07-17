export const WAVE_HISTORY_LENGTH = 7;
export const WAVE_BAR_COUNT = WAVE_HISTORY_LENGTH * 2 - 1;

const MIN_HEIGHT = 3;
const MAX_HEIGHT = 24;
const AMPLITUDE = 18;
const GAINS = [0.45, 0.58, 0.7, 0.84, 0.96, 1.08, 1.15, 1.08, 0.96, 0.84, 0.7, 0.58, 0.45];

export function emptyWaveHistory(): number[] {
  return Array.from({ length: WAVE_HISTORY_LENGTH }, () => 0);
}

export function pushWaveLevel(history: readonly number[], level: number): number[] {
  const bounded = Number.isFinite(level) ? Math.min(1, Math.max(0, level)) : 0;
  return [bounded, ...history.slice(0, WAVE_HISTORY_LENGTH - 1)];
}

export function waveBarHeights(history: readonly number[]): number[] {
  const normalized = Array.from(
    { length: WAVE_HISTORY_LENGTH },
    (_, index) => Math.min(1, Math.max(0, history[index] ?? 0)),
  );
  const levels = [...normalized.slice(1).reverse(), normalized[0], ...normalized.slice(1)];
  return levels.map((level, index) => Math.min(
    MAX_HEIGHT,
    MIN_HEIGHT + Math.pow(level, 0.72) * AMPLITUDE * GAINS[index],
  ));
}
