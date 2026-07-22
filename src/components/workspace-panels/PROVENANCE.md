# Workspace panel visual provenance

These components are a SolidJS/zAI implementation visually informed by T3
Code at revision `9a0a07167f0623c3a7db0ffeff2e3939760309df`:

- [`RightPanelTabs.tsx`](https://github.com/pingdotgg/t3code/blob/9a0a07167f0623c3a7db0ffeff2e3939760309df/apps/web/src/components/RightPanelTabs.tsx#L353-L495) — 52px panel bar, 28px tabs, Browser/Terminal/Files/Diff surface menu, pending and close affordances.
- [`PanelLayoutControls.tsx`](https://github.com/pingdotgg/t3code/blob/9a0a07167f0623c3a7db0ffeff2e3939760309df/apps/web/src/components/chat/PanelLayoutControls.tsx#L18-L79) — paired bottom/right panel toggles and Lucide glyph choices.
- [`ThreadTerminalDrawer.tsx`](https://github.com/pingdotgg/t3code/blob/9a0a07167f0623c3a7db0ffeff2e3939760309df/apps/web/src/components/ThreadTerminalDrawer.tsx#L1258-L1453) — resizable drawer and split/new/close terminal action strip.
- [`ChatHeader.tsx`](https://github.com/pingdotgg/t3code/blob/9a0a07167f0623c3a7db0ffeff2e3939760309df/apps/web/src/components/chat/ChatHeader.tsx#L82-L153), [`OpenInPicker.tsx`](https://github.com/pingdotgg/t3code/blob/9a0a07167f0623c3a7db0ffeff2e3939760309df/apps/web/src/components/chat/OpenInPicker.tsx#L259-L311), and [`GitActionsControl.tsx`](https://github.com/pingdotgg/t3code/blob/9a0a07167f0623c3a7db0ffeff2e3939760309df/apps/web/src/components/GitActionsControl.tsx#L1655-L1815) — compact titlebar action grouping and icon proportions.
- [`index.css`](https://github.com/pingdotgg/t3code/blob/9a0a07167f0623c3a7db0ffeff2e3939760309df/apps/web/src/index.css#L78-L107) — 52px workspace topbar metric.

No T3 Code application code or upstream brand asset is imported at runtime.
The implementation uses the already-licensed `lucide-solid` dependency and
zAI/OpenCode theme tokens. T3 Code's full MIT notice remains required in the
repository-level third-party notices.
