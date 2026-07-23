# Agent guide

This file is operational guidance for coding agents working in this repository.

## Objective

Onyx is an independent MIT-licensed Tauri desktop app that presents a consistent
workspace for Claude Code, Codex, Gemini CLI, Kimi Code, and OpenRouter. Keep
the OpenCode-informed visual language and T3 Code-informed provider separation
without importing upstream brands, logos, or provider protocol types into the
UI.

## Repository map

- `src/` — SolidJS interface, view state, and typed Tauri client
- `src-tauri/src/lib.rs` — Tauri commands, runtime state, and event emission
- `src-tauri/src/providers/` — CLI discovery, lifecycle, and output normalization
- `src-tauri/src/openrouter.rs` — OpenRouter model catalog and approved tool loop
- `src-tauri/src/storage.rs` — local session persistence
- `public/` and `src-tauri/icons/` — Onyx-owned visual assets
- `licenses/` and `THIRD_PARTY_NOTICES.md` — required upstream notices

## Invariants

- UI code consumes shared Onyx session events, never raw provider protocol
  objects.
- A session has one provider, model, canonical workspace, and at most one
  running turn.
- Child stdout and stderr must be drained concurrently and complete process
  groups must stop on cancel or app exit.
- Never expose API keys to the webview or logs. Store OpenRouter credentials
  only through the operating-system credential store.
- Resolve OpenRouter tool paths below the selected canonical workspace. Reads
  may be automatic; writes, deletions, shell execution, and other external
  mutations require explicit approval in Rust.
- Bound prompts, files, tool loops, output tails, directory walks, and approval
  wait time. Do not weaken an existing cap without documenting the tradeoff.
- Preserve platform-specific command invocation and do not assume a Unix shell
  for CLI adapters.
- Keep provider install and login behavior external; reuse the official CLI's
  credentials and configuration.

## Workflow

Search with `rg`/`rg --files`. Edit focused files and preserve unrelated user
changes. Prefer `apply_patch` for hand edits. Do not run destructive Git or
filesystem commands unless the user explicitly authorized their exact scope.

Before handing off code, run:

```sh
npm run typecheck
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
```

Run `npm run build:desktop` when packaging, permissions, icons, configuration,
or native dependencies change. Report checks that could not be run.

## Attribution

Do not remove or abbreviate the OpenCode and T3 Code notices. The pinned source
revisions and full MIT texts are recorded in `THIRD_PARTY_NOTICES.md` and
`licenses/`. If a change copies or substantially adapts another project, add
its exact provenance and license before merging. The current root license does
not relicense differently licensed content in historical Git revisions.
