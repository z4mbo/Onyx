import { ErrorBoundary } from "solid-js"
import { render } from "solid-js/web"
import App from "./App"
import { AgentOverlay } from "./components/AgentOverlay"
import { AccountGate } from "./components/AccountGate"
import { Hud } from "./components/Hud"
import { applyDocumentTheme } from "./lib/theme"
import "./styles.css"

const windowName = new URLSearchParams(window.location.search).get("window")
if (windowName) document.documentElement.dataset.window = windowName
applyDocumentTheme()
const syncTheme = () => applyDocumentTheme()
window.addEventListener("focus", syncTheme)
window.addEventListener("storage", syncTheme)
document.addEventListener("visibilitychange", syncTheme)
render(
  () => (
    <ErrorBoundary fallback={(cause) => (
      <main class="onyx-recovery" role="alert">
        <div>
          <strong>Onyx needs to reload</strong>
          <p>{cause instanceof Error ? cause.message : "An unexpected interface error occurred."}</p>
          <button onClick={() => window.location.reload()}>Reload Onyx</button>
        </div>
      </main>
    )}>
      {windowName === "hud"
        ? <Hud />
        : windowName === "agent"
          ? <AgentOverlay />
          : <AccountGate><App /></AccountGate>}
    </ErrorBoundary>
  ),
  document.getElementById("root")!,
)
