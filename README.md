<p align="center">
  <img src="public/onyx.svg" width="112" height="112" alt="Onyx logo">
</p>

<h1 align="center">Onyx</h1>

<p align="center">
  A Rust-first desktop workspace for coding agents, voice, Git, and local tools.
</p>

<p align="center">
  <a href="CHANGELOG.md"><img alt="Version 0.3.3" src="https://img.shields.io/badge/version-0.3.3-635bdb"></a>
  <img alt="Rust and Leptos" src="https://img.shields.io/badge/UI-Rust%20%C2%B7%20Leptos-dea584">
  <img alt="Tauri 2" src="https://img.shields.io/badge/desktop-Tauri%202-24c8db">
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-2f855a"></a>
</p>

Onyx gives Claude Code, Codex, Gemini CLI, and Kimi Code a consistent graphical workspace without replacing their official runtimes. Provider credentials, configuration, model availability, approvals, and session continuity remain owned by each installed CLI; Onyx turns their events into one native interface.

> Onyx is independent and is not affiliated with or endorsed by OpenCode, T3 Tools, Anthropic, OpenAI, Google, Moonshot AI, xAI, OpenRouter, Clerk, or Convex.

## Onyx 0.3.3

| Area | What changed |
| --- | --- |
| Home | Onyx opens on the real Home screen without creating a draft tab. |
| Sessions | Name the draft directly in its tab; right-click any session tab to rename or permanently delete it. Closing a tab never deletes history. |
| CLI fidelity | Codex models come from `model/list`, Claude models/effort/Fast from `initialize`, and Kimi uses its official ACP IDE transport for model, thinking, mode, permissions, streaming, resume, and slash commands. **CLI** still opens the real provider TUI inside Onyx. |
| Conversation | Sent prompts remain visible with profile and timestamp. Follow-ups queue above the composer and can steer Codex/Claude through their native live-input protocols. |
| Navigation | Every user prompt has a compact rail marker; hover to animate and preview it, then click to jump back to it. |
| Workspace | Terminal, Files, Diff, Browser, Git actions, and the active agent are available without leaving the session. |
| Provider chats | The standalone Chat page is gone; ChatGPT, Claude, Gemini, and Grok remain inside each session workspace as internal Onyx child webviews. |
| Voice | Dictation, agent, speech, and voice choices are dropdowns; retired TTS defaults migrate to a current model and errors retain their actionable cause. |
| Updates | Onyx updates retain signed progress and release notes; installed coding agents also expose their official **Update** command in an internal terminal. |
| Performance | The unused standalone chat/image/video stack was removed; xterm/FitAddon and optional Convex code remain lazy-loaded. |

## How it is built

| Layer | Implementation |
| --- | --- |
| Interface and application state | Rust, Leptos, and WebAssembly in `frontend-rs/src/` |
| Desktop shell and backend | Rust and Tauri 2 in `src-tauri/src/` |
| CLI providers and terminal lifecycle | Rust processes, normalized events, bounded output, and process-group cleanup |
| Webview integration | A small `frontend-rs/runtime.js` bridge for Tauri browser APIs, audio capture, lazy-loaded xterm, and optional Convex |
| Optional cloud sync | Clerk authentication plus a Convex backend |

There are **zero tracked TypeScript files**. Onyx is not literally JavaScript-free: WebAssembly boot code, a narrow WebView runtime bridge, xterm, and the optional Convex backend use the JavaScript ecosystem where the platform requires it. Application screens, session state, provider orchestration, persistence, permissions, and update logic remain in Rust.

## Providers

| Provider | Path | Authentication |
| --- | --- | --- |
| Codex | Official `codex app-server` session | Existing Codex login |
| Claude Code | Installed `claude` CLI | Existing Claude login |
| Gemini | Installed `gemini` CLI | Existing Gemini login |
| Kimi Code | Official `kimi acp` session, with bounded CLI fallback | Existing Kimi login |
| OpenRouter | Native Rust HTTP client and approved tool loop | API key stored in the OS credential manager |
| OpenAI audio | Native transcription and speech routes | API key stored in the OS credential manager |

Onyx does not install CLIs or take ownership of their logins. Official provider websites keep their own signed-in WebView sessions and are not merged into the native API chat.

## Core experience

- Project-grouped sessions with provider, model, reasoning, access, and Build/Plan controls.
- Streaming command output, FIFO follow-up queue, native steering where supported, stop/interrupt, approvals, and structured provider questions.
- Embedded official CLI sessions for native commands plus per-provider update actions.
- A prompt rail for long conversations with hover previews and click-to-jump navigation.
- Bottom terminal tabs and right-panel Chat, Browser, Files, Terminal, and Diff surfaces.
- Official provider chats in internal session webviews, plus native transcription and speech routes for Voice.
- Local-first history for sessions, voice, and preferences, with optional authenticated sync.
- A tray launcher that keeps global voice shortcuts available when the main window is hidden.

### Voice

| Gesture | Result |
| --- | --- |
| Hold `Control+Shift` | Record, transcribe, and insert dictation into the focused app |
| Hold `Control+Option` | Ask the configured voice agent about the active app |

The Voice settings select four independent roles: **Dictation model**, **Agent model**, **Speech model**, and **Voice**. Agent answers are read aloud only when **Speak responses** is enabled.

macOS requires Microphone, Input Monitoring, and Accessibility permissions for the full global-shortcut and text-insertion flow. Windows uses `Ctrl+Shift` and `Ctrl+Alt`. Linux currently exposes the Voice interface but does not install the global modifier-only listener.

## Run locally

Prerequisites:

- Current stable Rust with the `wasm32-unknown-unknown` target
- Node.js 22 and npm
- Trunk `0.21.14`
- [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for the host OS
- At least one supported CLI, or an OpenRouter/OpenAI API key for the native routes

```sh
git clone https://github.com/z4mbo/Onyx.git
cd Onyx
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
npm ci
npm run dev
```

`npm run dev:web` serves the same Rust/Leptos interface in a browser for rendering diagnostics; native Tauri commands are unavailable there.

### Verification

```sh
npm run typecheck
npm test
npm run build
cargo fmt --manifest-path frontend-rs/Cargo.toml -- --check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path frontend-rs/Cargo.toml --target wasm32-unknown-unknown -- -D warnings
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

## Accounts and optional Convex sync

Packaged releases currently use Clerk for Onyx account sign-in. Session, voice, and preference data is stored locally by default; Convex sync activates only when `VITE_CONVEX_URL` and the matching deployment authentication are configured.

```sh
cp .env.example .env.local
npx convex dev
```

Set `CLERK_JWT_ISSUER_DOMAIN` in Convex to the Clerk issuer documented in `.env.example`. Sync requests are authenticated and scoped to the Clerk subject. Convex is retained as the optional hosted backend; it is not required by the Rust CLI runtime, terminal, or local persistence layers.

## macOS releases

macOS release artifacts are built **only on a trusted local Mac**:

```sh
npm run release:macos:local
```

[`scripts/release-macos-local.sh`](scripts/release-macos-local.sh) requires:

- a Tauri updater signing key through `TAURI_SIGNING_PRIVATE_KEY` or `TAURI_SIGNING_PRIVATE_KEY_PATH`;
- a declared `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`;
- an installed **Developer ID Application** identity in `APPLE_SIGNING_IDENTITY`;
- either App Store Connect API credentials or Apple ID notarization credentials;
- both Apple targets for the default universal build, or an explicit `ONYX_MACOS_TARGET`.

With Tauri CLI 2.11, the build wrapper safely maps
`TAURI_SIGNING_PRIVATE_KEY_PATH` to the path-valued
`TAURI_SIGNING_PRIVATE_KEY` variable consumed by `tauri build`; it never reads
or prints the private key contents. Set `ONYX_NOTARIZATION_TIMEOUT` to override
the bounded `30m` notarization wait.

The script builds and signs the app, explicitly submits the DMG to Apple,
requires an `Accepted` result, staples and validates its ticket, and only then
runs the Gatekeeper checks. Artifacts remain under
`src-tauri/target/<target>/release/bundle/`. The command deliberately does not
upload or publish anything.

Pushing the matching `v0.3.3` tag builds the Windows updater in a draft
GitHub release. After that draft exists, publish the already-verified local
macOS artifacts with:

```sh
npm run release:macos:publish -- --target universal-apple-darwin
```

[`scripts/publish-macos-release.sh`](scripts/publish-macos-release.sh) refuses
dirty or mismatched source trees, requires the Windows manifest and provenance
in the draft, validates every updater signature, preserves existing non-macOS
entries, merges the Darwin targets into `latest.json`, uploads the artifacts,
and publishes only after the complete manifest verifies. Use `--dry-run` to
exercise validation without changing GitHub.

The maintainer machine does not currently have the Developer ID Application identity and notarization setup needed for a Gatekeeper-trusted public build. The script therefore stops instead of presenting an ad-hoc development artifact as a production release.

GitHub Actions runs checks and creates Windows/Linux desktop bundles. Tagged release automation is Windows-only; macOS is intentionally absent from CI.

## Updates and current distribution status

Onyx uses Tauri's signed updater. When a newer version is reachable, an **Update** button appears beside the profile, and installation shows progress plus release notes before restart. Every updater archive must match the public key embedded in `src-tauri/tauri.conf.json`.

The configured endpoint is:

```text
https://github.com/z4mbo/Onyx/releases/latest/download/latest.json
```

The repository and its Releases endpoint are public, so installed copies can
read this URL without a GitHub login. The endpoint starts serving updates as
soon as the first complete release publishes `latest.json`; until then GitHub
correctly returns `404`.

The local build and publish commands provide the complete signed-release path.
A valid Developer ID identity and notarization credentials are still required
before publishing a Gatekeeper-trusted macOS update.

## Security notes

- Coding agents and terminals can edit files and execute programs. Work in Git and review changes.
- CLI credentials and configuration stay with the official provider tools.
- OpenRouter and OpenAI API keys are validated and stored in the OS credential manager, never returned to the WebView.
- OpenRouter file tools are constrained to the canonical workspace; mutations require explicit Rust-side approval.
- Local session JSON is not encrypted. Convex sync is optional and subject to the configured deployment's policies.
- Internal browser panels load third-party websites; their terms, privacy policies, and authentication rules still apply.

## Screenshots

<p align="center">
  <img src="docs/screenshots/onyx-home.png" width="820" alt="Onyx Home with project-grouped coding sessions">
</p>

<p align="center"><em>Home stays focused on projects, searchable sessions, and one-click creation.</em></p>

<p align="center">
  <img src="docs/screenshots/onyx-voice-settings.jpg" width="820" alt="Onyx Voice settings">
</p>

<p align="center"><em>Native Rust settings with separate dictation, agent, speech, and voice selectors.</em></p>

## Attribution and license

Onyx's visual language references the MIT-licensed OpenCode UI at [`411eff73f026d4950c07947c4d983788cb615baa`](https://github.com/anomalyco/opencode/tree/411eff73f026d4950c07947c4d983788cb615baa). Provider separation, persistent-session behavior, composer controls, and workspace interactions were informed by T3 Code at [`78a0ea55c1d9edce8bcd2b3caff9510b4093e6d3`](https://github.com/pingdotgg/t3code/tree/78a0ea55c1d9edce8bcd2b3caff9510b4093e6d3).

Onyx does not ship T3 Code as a runtime dependency and does not redistribute upstream brands or logos. Complete pinned provenance and license texts are preserved in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and [`licenses/`](licenses/).

Onyx is released under the [MIT License](LICENSE).
