# Onyx

Onyx is a Rust-native desktop workspace that combines coding agents, official provider chats in the right sidebar, and system-wide voice assistance. Its coding surface keeps the OpenCode desktop visual language while using T3 Code-style provider drivers and session behavior.

> Onyx is an independent MIT-licensed project. It is not affiliated with or endorsed by OpenCode, T3 Tools, Anthropic, OpenAI, Google, Moonshot AI, xAI, OpenRouter, Clerk, or Convex.

## What is included

- Project-grouped coding sessions with tabbed workspaces.
- Claude Code, Codex, Gemini CLI, and Kimi Code through their installed CLIs and existing subscriptions.
- OpenRouter text, image, video, transcription, speech synthesis, and model discovery through a key stored in the operating-system keychain.
- Native `gpt-image-2`, transcription, and speech through an optional OpenAI API key stored in the operating-system keychain. OpenAI API usage is billed separately from ChatGPT subscriptions.
- Provider-specific model, reasoning, service-tier, access, and Build/Plan controls. Codex models and 5-hour/weekly usage windows are read from the Codex app-server when the account reports them.
- T3 Code-inspired Open, Commit, Push, Create PR, bottom-terminal, and multi-tab right-panel controls. Commit drafts a message with `claude -p` when Claude Code is installed and none is provided.
- T3 Code-style approvals: Deny, Allow for session (Claude Code persists the matching permission rule; Codex answers `approved_for_session`), or Allow once. In Plan mode the proposed plan is captured into the transcript for review instead of surfacing as a tool approval.
- Persistent, signed-in ChatGPT, Claude, Gemini, and Grok child webviews inside the session sidebar, without scraping private website APIs.
- Hold `Control+Shift` for dictation and `Control+Option` for the voice agent on macOS (`Ctrl+Shift` and `Ctrl+Alt` on Windows). Voice history is stored locally.
- Optional Clerk sign-in and Convex cloud sync scaffolding. The app stays local-first when these services are not configured.
- macOS, Windows, and Linux builds. Windows terminals can use native shells, the default WSL distribution, or a selected WSL distribution.

## Providers

| Brand | Runtime | Authentication |
| --- | --- | --- |
| OpenAI | Codex CLI/app-server | Existing Codex login |
| OpenAI media/audio | OpenAI HTTPS API | OpenAI API key |
| Anthropic | Claude Code CLI | Existing Claude login |
| Google | Gemini CLI | Existing Gemini login |
| Moonshot AI | Kimi Code CLI | Existing Kimi login |
| xAI | OpenRouter | OpenRouter API key |
| OpenRouter | OpenRouter HTTPS API | OpenRouter API key |

Only OpenRouter and OpenAI API credentials are entered in Onyx. Local CLIs own their credentials, subscriptions, model availability, and terms. Website subscriptions remain available in provider-controlled web-app windows; Onyx does not extract cookies or call private website endpoints.

## Run and test

Prerequisites:

- Current stable Rust
- Node.js 22 and npm
- [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system
- At least one supported CLI, or an OpenRouter key

```sh
git clone https://github.com/z4mbo/Onyx.git
cd Onyx
npm ci --legacy-peer-deps
npm run dev
```

The first native launch compiles Rust dependencies and may take several minutes. `npm run dev:web` is a UI-only browser preview with mocked native operations.

### Coding-agent smoke test

1. Verify the CLI you want to use from the same terminal: `claude --version`, `codex --version`, `gemini --version`, or `kimi --version`.
2. Launch `npm run dev`, choose a disposable project, and create a session.
3. Select provider, model, reasoning, Standard/Fast when available, access level, and Build or Plan.
4. Send a prompt, stop a running turn, and verify the session remains grouped under its project.
5. Test Open, terminal tabs, Browser/Files/Diff right-panel tabs, and Git actions in a disposable repository.
6. Click **No Git** to initialize a new repository when the selected project is not already a repository.

### Subscription chat and media smoke test

1. Open a coding session and choose **Chat** in the right panel.
2. Choose ChatGPT, Claude, Gemini, or Grok in the sidebar and sign in on the provider's official site.
3. Close and reopen the sidebar and verify the signed-in session persists inside Onyx.
4. In ChatGPT, verify subscription-backed image creation works through the official site. A ChatGPT subscription is separate from the optional, separately billed OpenAI API key.

### Voice smoke test

1. Connect OpenRouter or an OpenAI API key and set a transcription model in Settings → Voice. OpenRouter has no dedicated speech-to-text models: Onyx sends dictation audio through chat completions on an audio-input model such as the default `google/gemini-2.5-flash` (any model tagged with audio input on openrouter.ai/models works). The OpenAI provider uses `gpt-4o-mini-transcribe` and friends directly.
2. Click **Enable & test** under Settings → Voice, then grant microphone and accessibility/input permissions when macOS or Windows prompts.
3. Hold `Control+Shift` on macOS (`Ctrl+Shift` on Windows), speak, then release. The transcript should be inserted into the focused application and appear in Voice history.
4. Hold `Control+Option` on macOS (`Ctrl+Alt` on Windows), ask a question, then release. The compact overlay should expand with the answer and, when **Speak responses** is enabled, play it through the configured OpenRouter or OpenAI speech model.
5. Closing the main window should leave Onyx in the tray so the shortcuts remain available.

Linux currently exposes the Voice dashboard and chat but does not install a global modifier-only hold listener; macOS and Windows implement the native hold gestures. This limitation is shown here rather than silently claiming parity.

#### Dictation troubleshooting (macOS)

- Nothing happens while holding `Control+Shift`: macOS Input Monitoring is missing. Enable Onyx under System Settings → Privacy & Security → Input Monitoring, then relaunch.
- The overlay records but nothing pastes: the transcript is still copied to the clipboard (`⌘V` pastes it). For automatic pasting, enable Onyx under Privacy & Security → Accessibility.
- Grants are tied to the app's code signature. `npm run tauri dev` rebuilds are ad-hoc signed, so macOS silently drops Input Monitoring/Accessibility grants after every rebuild — re-toggle them, or use `npm run tauri build` (which signs with your Apple identity when one exists) for day-to-day voice use.

### Windows and WSL

Open Settings → General → Windows terminal and select:

- **Windows native** for PowerShell/cmd.
- **Default WSL distribution**.
- **Specific WSL distribution**, populated from `wsl.exe --list --quiet`.

WSL must already be installed and configured. Onyx does not install or convert distributions.

## Accounts and cloud sync

Production builds require Clerk sign-in on first launch. Onyx uses the system browser with OAuth 2.0 Authorization Code + PKCE, then stores its refresh credentials in the operating-system keychain. Authentication never runs inside the Tauri WebView, so Google, Apple, email verification, consent, and anti-bot checks run on a supported HTTPS page.

The public OAuth client and Clerk issuer are compiled into the desktop app; no Clerk publishable key or client secret is shipped. To enable cloud sync, copy `.env.example` to `.env.local`, set `VITE_CONVEX_URL`, and configure the Convex deployment:

```sh
npx convex dev
```

Set `CLERK_JWT_ISSUER_DOMAIN` in Convex to the Clerk issuer documented in `.env.example`. `convex/auth.config.ts` accepts the Onyx public OAuth client audience (and the legacy `convex` JWT-template audience). The included Convex functions authenticate every request and scope snapshots to the Clerk subject.

The custom Onyx sign-in screen provides individual Google, Apple, and email entry points, then opens the secure browser flow. The Account page provides sign-in/account status and a **Sync now** action that uploads coding sessions, chats, voice history, and preferences to the authenticated user's Convex snapshot. No subscription or payment behavior is implemented.

## Checks and packages

```sh
npm test
npm run typecheck
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build:desktop
```

GitHub Actions runs tests and creates unsigned desktop bundles for macOS, Windows, and Linux. Production distribution still requires your own Apple/Windows signing credentials.

## Releases and auto-updates

Installed apps check GitHub Releases for signed updates (game-launcher style): a banner appears in the app when a newer build exists, and Settings → General → Updates has a manual "Check for updates" button. Updates are verified against the updater public key in `src-tauri/tauri.conf.json` before install.

To ship an update:

1. Bump `version` in `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml` (keep them equal).
2. Commit, tag `vX.Y.Z`, and push the tag. `.github/workflows/release.yml` builds macOS (Apple Silicon + Intel) and Windows bundles, signs the updater artifacts, publishes the GitHub release, and uploads `latest.json`.
3. Installed apps pick the release up automatically from `https://github.com/z4mbo/Onyx/releases/latest/download/latest.json`.

One-time setup: the updater keypair lives at `~/.tauri/onyx-updater.key` (private, no password) and `~/.tauri/onyx-updater.key.pub`. Add the private key's contents as the `TAURI_SIGNING_PRIVATE_KEY` repository secret and an empty `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secret. If the private key is lost, generate a new pair (`npx tauri signer generate -w ~/.tauri/onyx-updater.key`), update `pubkey` in `tauri.conf.json`, and ship one manual release — older installs can then no longer verify updates and must be reinstalled once.

## Security notes

- Coding agents and terminals can modify files and execute programs. Use clean Git worktrees and review diffs.
- Supervised, Auto-accept edits, and Full access are translated to each runtime's supported permission and sandbox options; the upstream CLI remains authoritative.
- OpenRouter tools are constrained to the canonical project directory and bounded by prompt, output, file, tool-loop, and timeout limits.
- OpenRouter keys are validated and stored in the OS credential manager; they are never returned to the webview. Keys saved by the earlier zAI build are migrated to the Onyx credential service on first use.
- OpenAI API keys are validated and stored in the OS credential manager; they are never returned to the webview. ChatGPT subscriptions are not API credentials.
- Chat, voice history, and coding sessions are local by default. Local session JSON is not encrypted. Clerk/Convex sync is opt-in and requires deployment configuration.
- Browser panels load user-selected HTTP/HTTPS sites in sandboxed frames; those sites' terms and privacy policies apply.
- Official provider chats run as isolated child webviews inside the right sidebar. Onyx does not read their cookies, scrape their DOM, or relay private network calls into unified chat.

## Attribution and license

The UI package and production layout reference are from OpenCode at [`411eff73f026d4950c07947c4d983788cb615baa`](https://github.com/anomalyco/opencode/tree/411eff73f026d4950c07947c4d983788cb615baa). Provider behavior, composer controls, usage limits, and workspace interaction references are from T3 Code at [`78a0ea55c1d9edce8bcd2b3caff9510b4093e6d3`](https://github.com/pingdotgg/t3code/tree/78a0ea55c1d9edce8bcd2b3caff9510b4093e6d3).

The general chat interaction is informed by the public T3 Chat product. Onyx does not include T3 Chat source code, backend services, or brand assets.

Both projects are MIT-licensed. Their complete notices are preserved in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and [licenses/](licenses/), which are included in packaged apps. Onyx uses its own logo and does not redistribute the OpenCode or T3 Code marks.

Onyx itself is released under the [MIT License](LICENSE).
