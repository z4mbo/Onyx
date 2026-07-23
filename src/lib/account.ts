import { Clerk } from "@clerk/clerk-js"
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

const publishableKey = import.meta.env.VITE_CLERK_PUBLISHABLE_KEY?.trim() ?? ""
const convexUrl = import.meta.env.VITE_CONVEX_URL?.trim() ?? ""
const tauri = "__TAURI_INTERNALS__" in window
let clerk: Clerk | null = null
let convex: ConvexClient | null = null
let initialized: Promise<void> | null = null
let oauthListenerReady: Promise<void> | null = null
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

interface ClerkApiError {
  code?: string
  message?: string
  longMessage?: string
}

function clerkErrors(cause: unknown): ClerkApiError[] {
  if (!cause || typeof cause !== "object" || !("errors" in cause)) return []
  const errors = (cause as { errors?: unknown }).errors
  return Array.isArray(errors) ? errors.filter((item): item is ClerkApiError => Boolean(item) && typeof item === "object") : []
}

function clerkErrorCode(cause: unknown) {
  return clerkErrors(cause)[0]?.code
}

export function accountErrorMessage(cause: unknown) {
  const first = clerkErrors(cause)[0]
  if (first?.longMessage) return first.longMessage
  if (first?.message) return first.message
  if (cause instanceof Error) return cause.message
  return String(cause)
}

function requireClerk() {
  if (!clerk?.client) throw new Error("Onyx accounts are not ready yet")
  return clerk
}

function emailFactor() {
  const factor = requireClerk().client!.signIn.supportedFirstFactors?.find(
    (item) => item.strategy === "email_code",
  )
  if (!factor || !("emailAddressId" in factor)) {
    throw new Error("Email verification codes are not enabled for this Clerk application")
  }
  return factor
}

async function completeSession(sessionId: string | null, flow: string) {
  if (!sessionId) throw new Error(`${flow} did not create a session`)
  await requireClerk().setActive({
    session: sessionId,
    navigate: async () => undefined,
  })
}

async function handleOauthCallback(callbackUrl: string) {
  const instance = requireClerk()
  const callback = new URL(callbackUrl)
  const previous = `${window.location.pathname}${window.location.search}${window.location.hash}`
  const callbackLocation = `${window.location.pathname}${callback.search}${callback.hash}`
  window.history.replaceState(window.history.state, "", callbackLocation)
  try {
    await instance.handleRedirectCallback(
      {
        signInFallbackRedirectUrl: "/",
        signUpFallbackRedirectUrl: "/",
        transferable: true,
        reloadResource: "signIn",
      },
      async () => undefined,
    )
  } finally {
    window.history.replaceState(window.history.state, "", previous)
  }
}

function installOauthListener() {
  if (!tauri || oauthListenerReady) return oauthListenerReady ?? Promise.resolve()
  oauthListenerReady = listen<string>("onyx://oauth-callback", (event) => {
    void handleOauthCallback(event.payload).catch((cause) => {
      emit({ error: accountErrorMessage(cause) })
    })
  }).then(() => undefined)
  return oauthListenerReady
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
      await clerk.load({
        allowedRedirectOrigins: [
          window.location.origin,
          /^http:\/\/127\.0\.0\.1:\d+$/,
        ],
      })
      await installOauthListener()
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
  window.dispatchEvent(new Event("onyx:account-sign-in"))
}

export async function beginSocialSignIn(provider: "google" | "apple") {
  await initializeAccount()
  const instance = requireClerk()
  const callbackUrl = tauri
    ? await invoke<string>("start_oauth_callback")
    : new URL("/", window.location.href).toString()
  const redirectUrl = instance.buildUrlWithAuth(callbackUrl)
  const attempt = await instance.client!.signIn.create({
    strategy: `oauth_${provider}`,
    redirectUrl,
    actionCompleteRedirectUrl: callbackUrl,
    signUpIfMissing: true,
  })
  const target = attempt.firstFactorVerification.externalVerificationRedirectURL
  if (!target) throw new Error(`${provider === "google" ? "Google" : "Apple"} sign-in is not enabled for this Clerk application`)
  if (tauri) await openUrl(target.toString())
  else window.location.assign(target.toString())
}

export async function beginEmailSignIn(identifier: string) {
  await initializeAccount()
  const instance = requireClerk()
  await instance.client!.signIn.create({
    identifier: identifier.trim(),
    signUpIfMissing: true,
  })
  const factor = emailFactor()
  await instance.client!.signIn.prepareFirstFactor({
    strategy: "email_code",
    emailAddressId: factor.emailAddressId,
  })
}

export async function resendEmailSignIn() {
  const instance = requireClerk()
  const factor = emailFactor()
  await instance.client!.signIn.prepareFirstFactor({
    strategy: "email_code",
    emailAddressId: factor.emailAddressId,
  })
}

export async function verifyEmailSignIn(code: string) {
  const instance = requireClerk()
  try {
    const attempt = await instance.client!.signIn.attemptFirstFactor({
      strategy: "email_code",
      code: code.trim(),
    })
    if (attempt.status !== "complete") {
      if (attempt.status === "needs_second_factor" || attempt.status === "needs_client_trust") {
        throw new Error("This account requires an additional verification step that Onyx does not support yet")
      }
      throw new Error(`Sign-in could not be completed (${attempt.status ?? "unknown status"})`)
    }
    await completeSession(attempt.createdSessionId, "Sign-in")
  } catch (cause) {
    if (clerkErrorCode(cause) !== "sign_up_if_missing_transfer") throw cause
    const signUp = await instance.client!.signUp.create({ transfer: true })
    if (signUp.status !== "complete") {
      const requirements = signUp.missingFields.filter((field) => field !== "email_address")
      const suffix = requirements.length > 0 ? `: ${requirements.join(", ")}` : ""
      throw new Error(`Your account needs additional information before it can be created${suffix}`)
    }
    await completeSession(signUp.createdSessionId, "Sign-up")
  }
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
