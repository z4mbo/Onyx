import { Clerk } from "@clerk/clerk-js"
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

type ClerkLoadOptions = NonNullable<Parameters<Clerk["load"]>[0]>
type ClerkUIConstructor = NonNullable<ClerkLoadOptions["ui"]>["ClerkUI"]

declare global {
  interface Window {
    __internal_ClerkUICtor?: ClerkUIConstructor
  }
}

const publishableKey = import.meta.env.VITE_CLERK_PUBLISHABLE_KEY?.trim() ?? ""
const convexUrl = import.meta.env.VITE_CONVEX_URL?.trim() ?? ""
let clerk: Clerk | null = null
let convex: ConvexClient | null = null
let initialized: Promise<void> | null = null
let clerkUiLoading: Promise<ClerkUIConstructor> | null = null
let state: AccountState = {
  configured: Boolean(publishableKey),
  loading: Boolean(publishableKey),
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

function loadClerkUi(key: string): Promise<ClerkUIConstructor> {
  if (window.__internal_ClerkUICtor) return Promise.resolve(window.__internal_ClerkUICtor)
  if (clerkUiLoading) return clerkUiLoading

  clerkUiLoading = new Promise((resolve, reject) => {
    const encodedDomain = key.split("_")[2]
    if (!encodedDomain) {
      reject(new Error("The Clerk publishable key is invalid"))
      return
    }

    let clerkDomain = ""
    try {
      clerkDomain = window.atob(encodedDomain).slice(0, -1)
    } catch {
      reject(new Error("The Clerk publishable key is invalid"))
      return
    }

    const script = document.createElement("script")
    script.src = `https://${clerkDomain}/npm/@clerk/ui@1/dist/ui.browser.js`
    script.async = true
    script.crossOrigin = "anonymous"
    script.addEventListener("load", () => {
      if (window.__internal_ClerkUICtor) resolve(window.__internal_ClerkUICtor)
      else reject(new Error("Clerk sign-in controls did not load"))
    }, { once: true })
    script.addEventListener("error", () => reject(new Error("Clerk sign-in controls could not be loaded")), { once: true })
    document.head.appendChild(script)
  })
  return clerkUiLoading
}

function emit(update: Partial<AccountState>) {
  state = { ...state, ...update }
  listeners.forEach((listener) => listener(state))
}

function profileFromClerk(instance: Clerk): AccountProfile | null {
  const user = instance.user
  if (!user) return null
  return {
    id: user.id,
    name: user.fullName || user.username || user.primaryEmailAddress?.emailAddress || "Onyx user",
    email: user.primaryEmailAddress?.emailAddress ?? "",
    imageUrl: user.imageUrl || null,
  }
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
    if (!publishableKey) {
      emit({ loading: false })
      return
    }
    try {
      clerk = new Clerk(publishableKey)
      await clerk.load({ ui: { ClerkUI: await loadClerkUi(publishableKey) } })
      const refresh = () => emit({ loading: false, profile: clerk ? profileFromClerk(clerk) : null, error: null })
      clerk.addListener(refresh)
      refresh()
      if (convexUrl) {
        convex = new ConvexClient(convexUrl)
        convex.setAuth(async () => clerk?.session?.getToken({ template: "convex" }) ?? null, (authenticated) => {
          state = { ...state, cloud: { ...state.cloud, authenticated } }
          listeners.forEach((listener) => listener(state))
        })
      }
    } catch (cause) {
      emit({ loading: false, error: cause instanceof Error ? cause.message : String(cause) })
    }
  })()
  return initialized
}

export async function openSignIn() {
  await initializeAccount()
  if (!clerk) throw new Error("Add VITE_CLERK_PUBLISHABLE_KEY to enable accounts")
  clerk.openSignIn({ fallbackRedirectUrl: window.location.href })
}

export async function mountSignIn(node: HTMLDivElement) {
  await initializeAccount()
  if (!clerk) throw new Error("Add VITE_CLERK_PUBLISHABLE_KEY to enable accounts")
  clerk.mountSignIn(node, {
    fallbackRedirectUrl: window.location.href,
    appearance: {
      variables: {
        colorPrimary: "#18181b",
        colorText: "#20201f",
        colorTextSecondary: "#71716e",
        colorBackground: "#ffffff",
        borderRadius: "0.75rem",
        fontFamily: "Inter, system-ui, sans-serif",
      },
      elements: {
        rootBox: "onyx-clerk-root",
        cardBox: "onyx-clerk-card-box",
        card: "onyx-clerk-card",
      },
    },
  })
}

export function unmountSignIn(node: HTMLDivElement) {
  clerk?.unmountSignIn(node)
}

export async function openAccount() {
  await initializeAccount()
  if (!clerk) throw new Error("Accounts are not configured")
  clerk.openUserProfile()
}

export async function signOut() {
  await clerk?.signOut()
}

export async function pushCloudSnapshot(payload: unknown) {
  await initializeAccount()
  if (!convex || !clerk?.user) throw new Error("Sign in and configure Convex before syncing")
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
  if (!convex || !clerk?.user) throw new Error("Sign in and configure Convex before syncing")
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
