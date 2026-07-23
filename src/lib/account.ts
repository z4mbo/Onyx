import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { openUrl } from "@tauri-apps/plugin-opener"
import { ConvexClient } from "convex/browser"
import { anyApi } from "convex/server"
import { api } from "./api"
import type { AccountProfile, CloudStatus } from "./types"

export interface AccountState {
  configured: boolean
  loading: boolean
  profile: AccountProfile | null
  cloud: CloudStatus
  error: string | null
}

const convexUrl = import.meta.env.VITE_CONVEX_URL?.trim() ?? ""
const tauri = "__TAURI_INTERNALS__" in window
let convex: ConvexClient | null = null
let initialized: Promise<void> | null = null
let accountListenerReady: Promise<void> | null = null
let state: AccountState = {
  configured: tauri,
  loading: tauri,
  profile: null,
  cloud: {
    configured: Boolean(convexUrl),
    authenticated: false,
    syncing: false,
    lastSyncedAt: null,
    error: null,
  },
  error: null,
}
const listeners = new Set<(value: AccountState) => void>()

interface OAuthStart {
  authorizeUrl: string
}

interface AccountEvent {
  profile: AccountProfile | null
  error: string | null
}

export function accountErrorMessage(cause: unknown) {
  if (cause instanceof Error) return cause.message
  return String(cause)
}

function installAccountListener() {
  if (!tauri || accountListenerReady) return accountListenerReady ?? Promise.resolve()
  accountListenerReady = listen<AccountEvent>("onyx://account-changed", (event) => {
    if (event.payload.error) {
      emit({ loading: false, error: event.payload.error })
      return
    }
    emit({ loading: false, profile: event.payload.profile, error: null })
    configureConvexAuth()
  }).then(() => undefined)
  return accountListenerReady
}

function emit(update: Partial<AccountState>) {
  state = { ...state, ...update }
  listeners.forEach((listener) => listener(state))
}

function configureConvexAuth() {
  if (!tauri || !convexUrl) return
  convex ??= new ConvexClient(convexUrl)
  convex.setAuth(
    async ({ forceRefreshToken }) => {
      if (!state.profile) return null
      return await invoke<string | null>("clerk_account_token", {
        forceRefresh: forceRefreshToken,
      })
    },
    (authenticated) => {
      state = { ...state, cloud: { ...state.cloud, authenticated } }
      listeners.forEach((listener) => listener(state))
    },
  )
}

export function accountSnapshot() {
  return state
}

export function subscribeAccount(listener: (value: AccountState) => void) {
  listeners.add(listener)
  listener(state)
  return () => listeners.delete(listener)
}

export function initializeAccount() {
  if (initialized) return initialized
  initialized = (async () => {
    if (!tauri) {
      emit({ configured: false, loading: false })
      return
    }
    try {
      await installAccountListener()
      const profile = await invoke<AccountProfile | null>("clerk_account_profile")
      emit({ loading: false, profile, error: null })
      configureConvexAuth()
    } catch (cause) {
      emit({ loading: false, error: accountErrorMessage(cause) })
    }
  })()
  return initialized
}

export async function openSignIn() {
  await initializeAccount()
  if (!tauri) throw new Error("Account sign-in is available in the Onyx desktop app")
  window.dispatchEvent(new Event("onyx:account-sign-in"))
}

async function beginBrowserSignIn(loginHint?: string) {
  await initializeAccount()
  if (!tauri) throw new Error("Account sign-in is available in the Onyx desktop app")
  const flow = await invoke<OAuthStart>("start_clerk_oauth", {
    loginHint: loginHint?.trim() || null,
  })
  await openUrl(flow.authorizeUrl)
}

export async function beginSocialSignIn(_provider: "google" | "apple") {
  await beginBrowserSignIn()
}

export async function beginEmailSignIn(identifier: string) {
  await beginBrowserSignIn(identifier)
}

export async function signOut() {
  if (tauri) await invoke<void>("clerk_sign_out")
  emit({ profile: null, error: null })
  configureConvexAuth()
}

export async function pushCloudSnapshot(payload: unknown) {
  await initializeAccount()
  if (!convex || !state.profile) throw new Error("Sign in and configure Convex before syncing")
  state = { ...state, cloud: { ...state.cloud, syncing: true, error: null } }
  listeners.forEach((listener) => listener(state))
  try {
    await convex.mutation(anyApi.sync.upsertSnapshot, { payload: JSON.stringify(payload) })
    state = { ...state, cloud: { ...state.cloud, syncing: false, lastSyncedAt: new Date().toISOString(), error: null } }
  } catch (cause) {
    state = { ...state, cloud: { ...state.cloud, syncing: false, error: cause instanceof Error ? cause.message : String(cause) } }
    throw cause
  } finally {
    listeners.forEach((listener) => listener(state))
  }
}

export async function pullCloudSnapshot(): Promise<string | null> {
  await initializeAccount()
  if (!convex || !state.profile) throw new Error("Sign in and configure Convex before syncing")
  return await convex.query(anyApi.sync.latestSnapshot, {}) as string | null
}

function readStoredJson(key: string, fallback: unknown) {
  try {
    const value = localStorage.getItem(key)
    return value ? JSON.parse(value) : fallback
  } catch {
    return fallback
  }
}

async function currentCloudSnapshot() {
  return {
    version: 1,
    exportedAt: new Date().toISOString(),
    sessions: await api.listSessions(),
    chats: readStoredJson("onyx.chat.threads.v1", []),
    voiceHistory: readStoredJson("onyx.voice-history.v1", []),
    preferences: {
      colorScheme: localStorage.getItem("onyx.color-scheme"),
      desktop: readStoredJson("onyx.desktop-preferences.v1", null),
      favoriteModels: readStoredJson("onyx.chat.favorite-models.v1", []),
    },
  }
}

export async function pushCurrentCloudSnapshot() {
  await pushCloudSnapshot(await currentCloudSnapshot())
}

export async function pullCloudSnapshotIntoDevice() {
  const raw = await pullCloudSnapshot()
  if (!raw) return false
  const snapshot = JSON.parse(raw) as {
    chats?: unknown
    voiceHistory?: unknown
    preferences?: { colorScheme?: string | null; desktop?: unknown; favoriteModels?: unknown }
  }
  if (Array.isArray(snapshot.chats)) localStorage.setItem("onyx.chat.threads.v1", JSON.stringify(snapshot.chats))
  if (Array.isArray(snapshot.voiceHistory)) localStorage.setItem("onyx.voice-history.v1", JSON.stringify(snapshot.voiceHistory))
  if (snapshot.preferences?.colorScheme) localStorage.setItem("onyx.color-scheme", snapshot.preferences.colorScheme)
  if (snapshot.preferences?.desktop) localStorage.setItem("onyx.desktop-preferences.v1", JSON.stringify(snapshot.preferences.desktop))
  if (Array.isArray(snapshot.preferences?.favoriteModels)) localStorage.setItem("onyx.chat.favorite-models.v1", JSON.stringify(snapshot.preferences.favoriteModels))
  window.dispatchEvent(new Event("onyx:cloud-hydrated"))
  window.dispatchEvent(new Event("onyx:voice-history"))
  return true
}

export function startAutomaticCloudSync() {
  let timer: number | undefined
  const schedule = () => {
    window.clearTimeout(timer)
    timer = window.setTimeout(() => {
      void pushCurrentCloudSnapshot().catch(() => undefined)
    }, 1_500)
  }
  window.addEventListener("onyx:cloud-data-changed", schedule)
  const interval = window.setInterval(schedule, 60_000)
  schedule()
  return () => {
    window.removeEventListener("onyx:cloud-data-changed", schedule)
    window.clearInterval(interval)
    window.clearTimeout(timer)
  }
}
