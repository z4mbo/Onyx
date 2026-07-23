import type { VoiceHistoryItem } from "./types"

const KEY = "onyx.voice-history.v1"

export function loadVoiceHistory(): VoiceHistoryItem[] {
  try {
    const value = JSON.parse(localStorage.getItem(KEY) ?? "[]") as VoiceHistoryItem[]
    return Array.isArray(value) ? value.filter((item) => typeof item?.text === "string").slice(0, 120) : []
  } catch { return [] }
}

export function appendVoiceHistory(item: VoiceHistoryItem) {
  const next = [item, ...loadVoiceHistory().filter((entry) => entry.id !== item.id)].slice(0, 120)
  localStorage.setItem(KEY, JSON.stringify(next))
  window.dispatchEvent(new CustomEvent("onyx:voice-history"))
  window.dispatchEvent(new Event("onyx:cloud-data-changed"))
}

export function clearVoiceHistory() {
  localStorage.removeItem(KEY)
  window.dispatchEvent(new CustomEvent("onyx:voice-history"))
  window.dispatchEvent(new Event("onyx:cloud-data-changed"))
}
