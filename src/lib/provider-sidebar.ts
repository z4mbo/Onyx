import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi"
import { Webview } from "@tauri-apps/api/webview"
import { getCurrentWindow } from "@tauri-apps/api/window"
import type { ProviderBrand } from "./types"

export type OfficialProviderId = "chatgpt" | "claude" | "gemini" | "grok"

export const officialProviders: readonly {
  id: OfficialProviderId
  name: string
  brand: ProviderBrand
  url: string
  detail: string
}[] = [
  { id: "chatgpt", name: "ChatGPT", brand: "openai", url: "https://chatgpt.com/", detail: "Chat, tools, and subscription image creation" },
  { id: "claude", name: "Claude", brand: "anthropic", url: "https://claude.ai/new", detail: "Chat, projects, and artifacts" },
  { id: "gemini", name: "Gemini", brand: "google", url: "https://gemini.google.com/app", detail: "Chat and Google tools" },
  { id: "grok", name: "Grok", brand: "xai", url: "https://grok.com/", detail: "Chat and media tools" },
] as const

export interface ProviderSidebarBounds {
  x: number
  y: number
  width: number
  height: number
}

const native = "__TAURI_INTERNALS__" in window
const views = new Map<OfficialProviderId, Webview>()
const creating = new Map<OfficialProviderId, Promise<Webview>>()
let active: OfficialProviderId | null = null
let activation = 0

const labelFor = (provider: OfficialProviderId) => `provider-sidebar-${provider}`

function ready(view: Webview) {
  return new Promise<Webview>((resolve, reject) => {
    void view.once("tauri://created", () => resolve(view))
    void view.once<string>("tauri://error", (event) => {
      reject(new Error(event.payload || `Could not create ${view.label}`))
    })
  })
}

async function providerView(provider: OfficialProviderId, bounds: ProviderSidebarBounds) {
  const existing = views.get(provider)
  if (existing) return existing
  const pending = creating.get(provider)
  if (pending) return pending
  const definition = officialProviders.find((item) => item.id === provider)
  if (!definition) throw new Error("Unknown official provider")
  const view = new Webview(getCurrentWindow(), labelFor(provider), {
    url: definition.url,
    x: bounds.x,
    y: bounds.y,
    width: bounds.width,
    height: bounds.height,
    focus: false,
    acceptFirstMouse: true,
    dragDropEnabled: false,
  })
  const promise = ready(view)
    .then((created) => {
      views.set(provider, created)
      creating.delete(provider)
      return created
    })
    .catch((error) => {
      creating.delete(provider)
      throw error
    })
  creating.set(provider, promise)
  return promise
}

async function position(view: Webview, bounds: ProviderSidebarBounds) {
  await view.setPosition(new LogicalPosition(bounds.x, bounds.y))
  await view.setSize(new LogicalSize(bounds.width, bounds.height))
}

export function boundsForProviderSidebar(element: HTMLElement): ProviderSidebarBounds {
  const bounds = element.getBoundingClientRect()
  return {
    x: Math.round(bounds.left),
    y: Math.round(bounds.top),
    width: Math.max(1, Math.round(bounds.width)),
    height: Math.max(1, Math.round(bounds.height)),
  }
}

export async function showProviderSidebar(
  provider: OfficialProviderId,
  bounds: ProviderSidebarBounds,
) {
  if (!native) return
  const request = ++activation
  if (active && active !== provider) await views.get(active)?.hide()
  const view = await providerView(provider, bounds)
  if (request !== activation) {
    await view.hide()
    return
  }
  await position(view, bounds)
  await view.show()
  active = provider
}

export async function positionProviderSidebar(
  provider: OfficialProviderId,
  bounds: ProviderSidebarBounds,
) {
  if (!native || active !== provider) return
  const view = views.get(provider)
  if (view) await position(view, bounds)
}

export async function focusProviderSidebar(provider: OfficialProviderId) {
  if (!native || active !== provider) return
  await views.get(provider)?.setFocus()
}

export async function hideProviderSidebar(provider?: OfficialProviderId) {
  if (!native || (provider && active && provider !== active)) return
  activation += 1
  if (!active) return
  const previous = active
  active = null
  await views.get(previous)?.hide()
}
