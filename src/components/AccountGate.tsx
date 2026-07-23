import { createEffect, createSignal, onCleanup, onMount, Show, type ParentComponent } from "solid-js"
import {
  accountErrorMessage,
  accountSnapshot,
  beginEmailSignIn,
  beginSocialSignIn,
  initializeAccount,
  pullCloudSnapshotIntoDevice,
  resendEmailSignIn,
  startAutomaticCloudSync,
  subscribeAccount,
  verifyEmailSignIn,
} from "../lib/account"
import { OnyxOrb } from "./OnyxOrb"
import "./account.css"

const DASHBOARD_ICONS_REVISION = "46b860c70e866212311aef2f98da3775c17f5068"
const DASHBOARD_ICONS_BASE = `https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons@${DASHBOARD_ICONS_REVISION}/svg`

export const AccountGate: ParentComponent = (props) => {
  const developmentBypass = import.meta.env.DEV && import.meta.env.VITE_ONYX_DEV_BYPASS_ACCOUNT === "1"
  const [account, setAccount] = createSignal(accountSnapshot())
  const [step, setStep] = createSignal<"email" | "code">("email")
  const [email, setEmail] = createSignal("")
  const [code, setCode] = createSignal("")
  const [busy, setBusy] = createSignal<"google" | "apple" | "email" | "code" | "resend" | null>(null)
  const [message, setMessage] = createSignal<string | null>(null)
  let hydrated = false
  let stopSync: () => void = () => undefined

  onMount(() => {
    const unsubscribe = subscribeAccount(setAccount)
    void initializeAccount()
    onCleanup(() => {
      stopSync()
      unsubscribe()
    })
  })

  createEffect(() => {
    if (!account().profile || !account().cloud.authenticated || hydrated) return
    hydrated = true
    void pullCloudSnapshotIntoDevice()
      .catch((cause) => setMessage(accountErrorMessage(cause)))
    stopSync = startAutomaticCloudSync()
  })

  const socialSignIn = async (provider: "google" | "apple") => {
    setMessage(null)
    setBusy(provider)
    try {
      await beginSocialSignIn(provider)
      setMessage("Finish signing in in the browser. Onyx will reopen automatically.")
    } catch (cause) {
      setMessage(accountErrorMessage(cause))
    } finally {
      setBusy(null)
    }
  }

  const submitEmail = async (event: SubmitEvent) => {
    event.preventDefault()
    const identifier = email().trim()
    if (!identifier) return
    setMessage(null)
    setBusy("email")
    try {
      await beginEmailSignIn(identifier)
      setEmail(identifier)
      setCode("")
      setStep("code")
    } catch (cause) {
      setMessage(accountErrorMessage(cause))
    } finally {
      setBusy(null)
    }
  }

  const submitCode = async (event: SubmitEvent) => {
    event.preventDefault()
    if (code().length < 6) return
    setMessage(null)
    setBusy("code")
    try {
      await verifyEmailSignIn(code())
    } catch (cause) {
      setMessage(accountErrorMessage(cause))
    } finally {
      setBusy(null)
    }
  }

  const resendCode = async () => {
    setMessage(null)
    setBusy("resend")
    try {
      await resendEmailSignIn()
      setMessage("A new verification code was sent.")
    } catch (cause) {
      setMessage(accountErrorMessage(cause))
    } finally {
      setBusy(null)
    }
  }

  const startOver = () => {
    setStep("email")
    setCode("")
    setMessage(null)
  }

  return (
    <Show when={account().profile || developmentBypass} fallback={
      <main class="onyx-account-gate">
        <section class="onyx-account-gate__content">
          <header class="onyx-account-gate__intro">
            <OnyxOrb label="Onyx" />
            <h1>{step() === "code" ? "Check your email" : "Welcome to Onyx"}</h1>
            <p>{account().configured
              ? step() === "code"
                ? <>Enter the six-digit code sent to <strong>{email()}</strong>.</>
                : "Sign in to sync your coding sessions, chats, voice history, and preferences."
              : "This Onyx build needs its Clerk and Convex deployment values before accounts can be used."}</p>
          </header>

          <Show when={account().configured} fallback={
            <code class="onyx-account-gate__configuration">VITE_CLERK_PUBLISHABLE_KEY<br />VITE_CONVEX_URL</code>
          }>
            <Show when={!account().loading} fallback={<div class="onyx-account-gate__loading">Preparing secure sign in…</div>}>
              <div class="onyx-account-gate__auth" aria-label="Sign in to Onyx">
                <Show when={step() === "email"} fallback={
                  <form class="onyx-account-gate__form" onSubmit={submitCode}>
                    <label for="onyx-account-code">Verification code</label>
                    <input
                      id="onyx-account-code"
                      class="onyx-account-gate__code"
                      type="text"
                      inputmode="numeric"
                      autocomplete="one-time-code"
                      maxlength={6}
                      placeholder="000000"
                      value={code()}
                      autofocus
                      onInput={(event) => setCode(event.currentTarget.value.replace(/\D/g, "").slice(0, 6))}
                    />
                    <button class="onyx-account-gate__primary" type="submit" disabled={Boolean(busy()) || code().length < 6}>
                      {busy() === "code" ? "Verifying…" : "Verify and continue"}
                    </button>
                    <div class="onyx-account-gate__code-actions">
                      <button type="button" onClick={() => void resendCode()} disabled={Boolean(busy())}>
                        {busy() === "resend" ? "Sending…" : "Resend code"}
                      </button>
                      <span aria-hidden="true">·</span>
                      <button type="button" onClick={startOver} disabled={Boolean(busy())}>Use another email</button>
                    </div>
                  </form>
                }>
                  <div class="onyx-account-gate__providers">
                    <button type="button" onClick={() => void socialSignIn("apple")} disabled={Boolean(busy())}>
                      <img src={`${DASHBOARD_ICONS_BASE}/apple.svg`} alt="" draggable={false} />
                      <span>{busy() === "apple" ? "Opening…" : "Continue with Apple"}</span>
                    </button>
                    <button type="button" onClick={() => void socialSignIn("google")} disabled={Boolean(busy())}>
                      <img src={`${DASHBOARD_ICONS_BASE}/google.svg`} alt="" draggable={false} />
                      <span>{busy() === "google" ? "Opening…" : "Continue with Google"}</span>
                    </button>
                  </div>

                  <div class="onyx-account-gate__divider"><span>or continue with email</span></div>

                  <form class="onyx-account-gate__form" onSubmit={submitEmail}>
                    <label for="onyx-account-email">Email address</label>
                    <input
                      id="onyx-account-email"
                      type="email"
                      inputmode="email"
                      autocomplete="email"
                      autocapitalize="none"
                      spellcheck={false}
                      placeholder="you@example.com"
                      value={email()}
                      onInput={(event) => setEmail(event.currentTarget.value)}
                    />
                    <button class="onyx-account-gate__primary" type="submit" disabled={Boolean(busy()) || !email().trim()}>
                      {busy() === "email" ? "Sending code…" : "Continue with email"}
                    </button>
                  </form>
                </Show>
                <div
                  id="clerk-captcha"
                  data-cl-theme="auto"
                  data-cl-size="flexible"
                  data-cl-language="auto"
                />
              </div>
            </Show>
          </Show>

          <Show when={account().error || message()}>
            <p class="onyx-account-gate__message" role="status" aria-live="polite">
              {account().error ?? message()}
            </p>
          </Show>
        </section>
      </main>
    }>
      {props.children}
    </Show>
  )
}
