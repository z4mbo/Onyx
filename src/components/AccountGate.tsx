import { createEffect, createSignal, onCleanup, onMount, Show, type ParentComponent } from "solid-js"
import {
  accountSnapshot,
  initializeAccount,
  mountSignIn,
  pullCloudSnapshotIntoDevice,
  startAutomaticCloudSync,
  subscribeAccount,
  unmountSignIn,
} from "../lib/account"
import { OnyxOrb } from "./OnyxOrb"
import "./account.css"

export const AccountGate: ParentComponent = (props) => {
  const developmentBypass = import.meta.env.DEV && import.meta.env.VITE_ONYX_DEV_BYPASS_ACCOUNT === "1"
  const [account, setAccount] = createSignal(accountSnapshot())
  const [message, setMessage] = createSignal<string | null>(null)
  let hydrated = false
  let stopSync: () => void = () => undefined
  let signInElement: HTMLDivElement | undefined
  let signInMounted = false

  onMount(() => {
    const unsubscribe = subscribeAccount(setAccount)
    void initializeAccount().then(async () => {
      if (!signInElement || accountSnapshot().profile || !accountSnapshot().configured) return
      try {
        await mountSignIn(signInElement)
        signInMounted = true
      } catch (cause) {
        setMessage(cause instanceof Error ? cause.message : String(cause))
      }
    })
    onCleanup(() => {
      stopSync()
      if (signInElement && signInMounted) unmountSignIn(signInElement)
      unsubscribe()
    })
  })

  createEffect(() => {
    if (!account().profile || !account().cloud.authenticated || hydrated) return
    hydrated = true
    void pullCloudSnapshotIntoDevice()
      .catch((cause) => setMessage(cause instanceof Error ? cause.message : String(cause)))
    stopSync = startAutomaticCloudSync()
  })

  return (
    <Show when={account().profile || developmentBypass} fallback={
      <main class="onyx-account-gate">
        <section>
          <OnyxOrb label="Onyx" />
          <h1>Welcome to Onyx</h1>
          <p>{account().configured
            ? "Sign in to keep your coding sessions, chats, voice history, and preferences with you."
            : "This Onyx build needs its Clerk and Convex deployment values before accounts can be used."}</p>
          <Show when={account().configured} fallback={
            <code>VITE_CLERK_PUBLISHABLE_KEY<br />VITE_CONVEX_URL</code>
          }>
            <Show when={!account().loading} fallback={<div class="onyx-account-gate__loading">Loading sign in…</div>}>
              <div ref={signInElement} class="onyx-account-gate__sign-in" aria-label="Sign in to Onyx" />
            </Show>
          </Show>
          <Show when={account().error || message()}><small>{account().error ?? message()}</small></Show>
        </section>
      </main>
    }>
      {props.children}
    </Show>
  )
}
