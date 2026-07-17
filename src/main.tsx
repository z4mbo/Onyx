import { ClerkProvider } from "@clerk/react";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { AgentOverlay } from "./components/AgentOverlay";
import { Hud } from "./components/Hud";
import { OnyxApp } from "./components/OnyxApp";
import "./styles.css";

const view = new URLSearchParams(window.location.search).get("view") ?? "main";
document.documentElement.dataset.view = view;

const publishableKey = String(import.meta.env.VITE_CLERK_PUBLISHABLE_KEY ?? "").trim();
const mainView = publishableKey
  ? <ClerkProvider publishableKey={publishableKey} afterSignOutUrl="/"><OnyxApp /></ClerkProvider>
  : <OnyxApp />;
const content = view === "hud"
  ? <Hud />
  : view === "agent"
    ? <AgentOverlay />
    : mainView;

createRoot(document.getElementById("root")!).render(
  <StrictMode>{content}</StrictMode>,
);
