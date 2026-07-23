import { render } from "solid-js/web"
import App from "./App"
import { AgentOverlay } from "./components/AgentOverlay"
import { AccountGate } from "./components/AccountGate"
import { Hud } from "./components/Hud"
import "./styles.css"

const windowName = new URLSearchParams(window.location.search).get("window")
if (windowName) document.documentElement.dataset.window = windowName
render(
  () => windowName === "hud"
    ? <Hud />
    : windowName === "agent"
      ? <AgentOverlay />
      : <AccountGate><App /></AccountGate>,
  document.getElementById("root")!,
)
