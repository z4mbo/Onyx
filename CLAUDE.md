# CLAUDE.md

This file is repository guidance for AI coding assistants working on zAI.

## Project overview

zAI is an Electron desktop workspace for local AI coding CLIs. It supports Claude Code, Gemini CLI, Codex CLI, and Kimi Code. OpenRouter is a provider-backed engine that launches through Kimi Code with the user's selected OpenRouter model; it is not a separately detected CLI.

The application combines chat and raw terminal views with project management, file browsing, Git, skills, agents, MCP connections, and a project canvas. It targets Windows 11 x64 and macOS x64/arm64.

zAI is GPL-3.0 and is based on Bruno Pigat's `friendly-terminal`; preserve `LICENSE` and `ATTRIBUTION.md` when redistributing changes.

## Commands

```bash
npm run dev                 # electron-vite development mode
npm run typecheck           # TypeScript checks for Node and web projects
npm test                    # Node test runner
npm run build               # compile application into out/
npm run rebuild             # rebuild native modules for Electron
npm run build:win           # Windows x64 NSIS installer
npm run build:mac:x64       # Intel macOS DMG + ZIP
npm run build:mac:arm64     # Apple-silicon macOS DMG + ZIP
npm run build:mac           # both macOS architectures
```

`node-pty` is native. Rebuild it after installing dependencies and package it on the target OS/architecture. Do not treat a successful renderer-only build as proof that a packaged terminal works.

## Electron architecture

1. `src/main/` owns Electron lifecycle, OS integration, native processes, storage, and IPC handlers.
2. `src/preload/` exposes a narrow typed API through `contextBridge`; do not expose raw `ipcRenderer` or Node primitives.
3. `src/renderer/` is the React UI. It communicates with privileged code only through the preload API.

For a new IPC operation, update the main handler, preload implementation, preload types, and renderer wrapper together. Validate renderer-supplied filesystem paths and command arguments in the main process.

## Key subsystems

### AI engines and terminals

- `src/main/ai-engines/` detects CLI executables and maps engine intents to commands.
- `src/main/pty/` owns `node-pty` processes and their BrowserWindow ownership.
- `src/renderer/hooks/useTerminal.ts` owns xterm lifecycle, PTY I/O, engine startup, and paste handling.
- `src/renderer/lib/constants.ts` is the renderer source of truth for engine labels, instruction files, colors, and config directories.

Supported engine IDs are `claude`, `gemini`, `codex`, `kimi`, and `openrouter`. Keep the registry, command dictionary, preload types, renderer constants, settings store, setup UI, and terminal/chat parsing exhaustive when adding or changing an ID.

OpenRouter is a selectable zAI engine profile but depends on Kimi Code 0.6.0 or newer for execution. The Electron main process encrypts its API key with `safeStorage`; the renderer and settings APIs must never receive plaintext after the key is saved. Main fetches tool-capable models from OpenRouter's authenticated `/api/v1/models/user` endpoint. At launch it injects temporary `KIMI_MODEL_*` environment variables only into that OpenRouter terminal's local process tree and terminates active OpenRouter PTYs when credentials are replaced or cleared. Never place provider keys in project instruction files, MCP files, logs, renderer URLs, terminal metadata, or committed fixtures. Error messages may name a missing field but must not include credential values.

### Project storage

- `src/main/util/paths.ts` chooses the managed-project root.
- Development uses `<repo>/projects`.
- Packaged builds use the user's `Documents/zAI Projects` directory.
- The first packaged access performs a copy-only migration from legacy install-adjacent and old app-data locations. It never deletes the source or overwrites a same-named destination; a userData migration-state file prevents a deliberately deleted migrated project from reappearing.
- `src/main/project/project-manager.ts` creates, imports, lists, and removes managed project entries.

Imported folders are represented by directory links/junctions. Be especially careful with recursive deletion and migration code: deleting a managed link must never delete the external target.

### Project instructions, skills, and agents

- Claude reads `CLAUDE.md`.
- Gemini reads `GEMINI.md`.
- Codex and Kimi Code use `AGENTS.md`.
- Shared default skills are copied into engine-specific directories and `.agents/skills` without overwriting existing user files.

Default assets live in `resources/default-projects/` and are copied from `process.resourcesPath` in packaged builds. Existing project content always wins over bundled defaults.

### MCP

The shared project configuration is `.mcp.json` with a top-level `mcpServers` object. `src/main/project/mcp-config.ts` preserves unrelated engine settings while synchronizing servers to:

- `.gemini/settings.json`; and
- `.kimi-code/mcp.json`.

Kimi Code's current project-level shape is the well-known `{ "mcpServers": { ... } }` JSON format. Do not sync to the legacy `~/.kimi` layout.

The bundled `gui-control` server lives under `resources/default-projects/mcp-servers/`. Its project entry launches `process.execPath` with `ELECTRON_RUN_AS_NODE=1`, allowing the packaged Electron runtime to execute it on Windows and macOS without relying on a separate `node` binary. Its `ZAI_*` environment names supersede `YFT_*`; the server accepts old names only for existing-project compatibility.

MCP stdio entries execute local commands. Keep approvals enabled by default and never silently add broad wildcard permissions for third-party servers.

### Files and disks

- `src/main/filesystem/disk-service.ts` returns `DiskInfo { name, mount, free, size }`.
- Windows mounts use absolute roots such as `C:\\`.
- macOS exposes Home, `/`, and entries mounted under `/Volumes`.
- `tree-service.ts` and `fs-ipc.ts` implement browsing and watched-file updates.

Keep the `DiskInfo` contract identical in main, preload, renderer API, and hooks.

### State and UI

Zustand stores in `src/renderer/stores/` own projects, terminals, chat sessions, and settings. Each terminal retains the engine/provider configuration with which it was created. Avoid changing an already running PTY when the user changes a default setting.

The canvas iframe communicates through the deliberately narrow `window.yft` bridge. That internal bridge name is retained for project compatibility and is not a product label. Canvas file requests must remain within the real path of the active project. Canvas cannot write to OpenRouter PTYs; for other engines it can only prefill printable text, leaving submission to the user.

### Window and PTY shutdown

On Windows, destroying a BrowserWindow while its ConPTY processes are alive can terminate Electron. Secondary-window close handling must kill owned PTYs before destruction and allow native cleanup time. The last window may detach callbacks and let the application-level quit path perform final cleanup. Do not replace this ordering with an unconditional `window.destroy()`.

## Packaging and releases

`electron-builder.config.ts` defines:

- app ID `io.github.z4mbo.zai` and product name `zAI`;
- Windows x64 NSIS packaging;
- macOS DMG and ZIP packaging with hardened-runtime entitlements;
- native `node-pty` unpacking; and
- Windows/macOS installer artifact naming.

`.github/workflows/release.yml` verifies Windows and macOS, then uses native Windows x64, macOS arm64, and macOS Intel runners for tag artifacts. A `v*` tag publishes all artifacts to one GitHub release. Updates are manual because private GitHub Releases require authentication that must not be embedded in a desktop client.

macOS signing/notarization requires repository secrets. Hardened-runtime configuration alone does not make an unsigned artifact notarized; keep the README's Gatekeeper warning until signed releases are verified.

## Working rules

- Preserve user files and unrelated worktree changes.
- Prefer additive migration with explicit conflict handling over destructive moves.
- Keep secrets in main-process/local configuration and redact them from errors.
- Run typecheck, tests, and the production build before release work.
- For native or packaging changes, also smoke-test an installed artifact on each target architecture.
- Update README and this file when engine setup, storage, packaging, or security behavior changes.
