# Rust frontend migration

The Rust frontend is a parallel Leptos/WebAssembly implementation. The shipped
Solid frontend remains the production entrypoint until every gate below passes.
This keeps the current application releasable throughout the migration.

## Architecture

- `frontend-rs/` owns Rust UI state, components, Tauri IPC, and browser-preview
  fallbacks.
- Tauri remains the native shell and backend. Provider processes, credential
  handling, storage, updater behavior, child webviews, terminals, and security
  approvals stay in `src-tauri/`.
- Existing CSS is compiled into one deterministic stylesheet for the Rust
  preview. Reusing the exact tokens and class names prevents styling drift
  while components are ported.
- The TypeScript frontend is removed only after the Rust frontend reaches full
  parity and becomes the production entrypoint in a dedicated change.

## Non-regression gates

The production switch is blocked until all of these are true:

- Every main route works: home, draft, active session, settings, voice history,
  account gate, recovery state, HUD, and voice-agent overlay.
- Provider discovery, model selection, Build/Plan, access modes, streaming,
  steering, cancellation, approvals, usage, persistence, and deletion match.
- Workspace files, diff, browser child webviews, provider child webviews, Git
  actions, editor launch, and all terminal layouts match.
- Keyboard navigation, focus restoration, clipboard/text selection, drag
  regions, reduced motion, dark/light theme, and platform shortcuts match.
- Screenshot comparisons pass for light/dark at 1024×768, 1280×800, and
  1536×960 on each supported desktop renderer, with intentional rasterization
  differences documented.
- Long transcripts and terminal streams remain responsive under profiling.
- The repository's TypeScript, Vite, Rust formatting, Rust tests, and desktop
  packaging checks pass before and after the entrypoint switch.

## Commands

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
npm run dev:rust
npm run build:rust
npm run test:rust-ui
npm run dev:desktop:rust
npm run build:desktop:rust
```

The browser preview runs on port 1430. The desktop preview merges
`src-tauri/tauri.rust.conf.json` over the production Tauri configuration, so
it exercises the same native commands and permissions without replacing
`npm run dev` or `npm run build:desktop`.
