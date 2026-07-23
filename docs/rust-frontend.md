# Rust frontend

Onyx ships its production interface as Leptos components compiled to
WebAssembly. `frontend-rs/` owns UI state, rendering, the typed Tauri client,
session-event reduction, workspace panels, settings, and the two voice overlay
windows. Tauri remains the native shell and owns provider processes,
credentials, persistence, approvals, terminals, updates, and child webviews.

The small `frontend-rs/runtime.ts` module is browser API glue for Tauri plugins,
xterm, audio capture, and Convex. It does not own application screens or
session state. Existing CSS remains shared with the former Solid implementation
so the production switch does not alter spacing, typography, color, or motion.

## Commands

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
npm run dev
npm run dev:web
npm run build
npm run test:rust-ui
npm run build:desktop
```

`npm run dev` launches the native Rust UI. `npm run dev:web` serves the same
WebAssembly bundle on port 1430 for rendering diagnostics; native commands only
work inside Tauri. `npm run dev:solid` is a legacy reference command and is not
used by production or packaging.
