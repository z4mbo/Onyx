import { check, type Update } from "@tauri-apps/plugin-updater"
import { relaunch } from "@tauri-apps/plugin-process"

const tauri = "__TAURI_INTERNALS__" in window

export interface UpdateProgress {
  downloaded: number
  total: number | null
}

/**
 * Asks the release endpoint whether a newer build exists. Returns null when
 * the app is current or the check is unavailable (browser preview). Throws on
 * network or signature failures so explicit "Check for updates" clicks can
 * surface the reason; startup checks catch and stay silent.
 */
export async function fetchUpdate(): Promise<Update | null> {
  if (!tauri) return null
  return check()
}

/**
 * Downloads, verifies, and installs the update, then relaunches Onyx.
 * The returned promise only settles on failure or right before relaunch.
 */
export async function installUpdate(update: Update, onProgress?: (progress: UpdateProgress) => void) {
  let downloaded = 0
  let total: number | null = null
  await update.downloadAndInstall((event) => {
    if (event.event === "Started") {
      total = event.data.contentLength ?? null
    } else if (event.event === "Progress") {
      downloaded += event.data.chunkLength
    }
    onProgress?.({ downloaded, total })
  })
  await relaunch()
}
