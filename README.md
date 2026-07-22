# zAI

zAI is a Rust-native desktop workspace for local coding-agent CLIs and
OpenRouter models. It keeps one consistent conversation interface while each
provider runs through an isolated adapter in the selected project directory.

The desktop shell and orchestration layer are written in Rust with Tauri. The
SolidJS interface uses [`@opencode-ai/ui` 1.18.4](https://www.npmjs.com/package/@opencode-ai/ui/v/1.18.4)
and substantially adapts layout and component ideas from the OpenCode
production interface. Its provider boundary, persistent-session behavior, and
normalized event stream are informed by T3 Code's driver architecture. Exact
source revisions and licenses are recorded below.

> [!IMPORTANT]
> zAI is an independent community project. It is not affiliated with or
> endorsed by OpenCode, T3 Tools, Anthropic, OpenAI, Google, Moonshot AI, or
> OpenRouter.

## Providers

| Provider | Integration | Authentication |
| --- | --- | --- |
| Claude Code | Claude Code's streaming CLI protocol when available, with a bounded CLI fallback | Existing Claude Code login/configuration |
| Codex | Codex app-server when available, with a bounded CLI fallback | Existing Codex login/configuration |
| Gemini CLI | Gemini's non-interactive streaming output | Existing Gemini CLI login/configuration |
| Kimi Code | Kimi's non-interactive streaming output | Existing Kimi Code login/configuration |
| OpenRouter | Direct HTTPS chat-completions API with model discovery and a bounded tool loop | API key stored in the operating-system credential store |

Only OpenRouter requires a credential to be entered in zAI. Local CLI adapters
discover the executable, report its version, and reuse that tool's own login,
settings, model access, and billing account. Providers are optional and can be
installed independently:

- [Claude Code](https://docs.anthropic.com/en/docs/claude-code)
- [Codex CLI](https://developers.openai.com/codex/cli)
- [Gemini CLI](https://github.com/google-gemini/gemini-cli)
- [Kimi Code](https://moonshotai.github.io/kimi-code/)
- [OpenRouter API keys](https://openrouter.ai/keys)

## How it works

```text
SolidJS interface
      │ Tauri commands + events
Rust session runtime
      ├── Claude adapter ── claude CLI
      ├── Codex adapter  ── codex CLI
      ├── Gemini adapter ── gemini CLI
      ├── Kimi adapter   ── kimi CLI
      └── OpenRouter adapter ── HTTPS API + approved local tools
```

Provider-specific JSONL is normalized into session deltas, tool activity, and
provider session IDs before it reaches the interface. A session is tied to one
provider, model, and canonical workspace. One turn runs at a time per session;
closing the app or cancelling a turn terminates its child process group.

Conversation history is stored as `sessions.json` in the platform-specific
Tauri application-data directory. It is local, but it is not encrypted; avoid
putting secrets in prompts or tool output.

Each session also has T3-style workspace controls: Open launches the selected
code editor, Git actions commit/push/create or open a pull request, the bottom
drawer hosts persistent PTYs, and the tabbed right panel can hold Browser,
Terminal, Files, and Diff surfaces at the same time. The panel state is local
to the open session tab.

## Security model

Coding agents can change files and execute programs. Use zAI on a clean Git
worktree, review diffs, and keep important data backed up.

- The OpenRouter key is validated before storage, placed in the operating
  system keychain/credential manager, and never returned to the webview.
- OpenRouter read, list, and literal-search tools are constrained to the
  selected canonical workspace. File writes and shell commands require an
  explicit in-app approval and time out after ten minutes.
- OpenRouter file reads are capped at 256 KiB, writes at 1 MiB, and a response
  can perform at most 12 tool rounds. Requests retain at most the newest 64
  text messages or 512 KiB of history; chat responses are capped at 8 MiB,
  assistant text at 1 MiB, and tool-call/result accumulation is separately
  bounded per call and per turn.
- Local-CLI prompts are capped at 24 KiB and OpenRouter prompts at 256 KiB.
  Claude and Codex normally receive prompts over their persistent stdin
  protocols; compatibility fallbacks and the Gemini/Kimi adapters pass the
  prompt directly to the executable without invoking a shell.
- Local CLIs are separate trusted programs. zAI launches them in the selected
  workspace, but their native permission and sandbox behavior remains
  authoritative. The persistent Claude adapter uses manual permission callbacks
  and the persistent Codex adapter uses `on-request` approval with the
  `workspace-write` sandbox; supported boolean requests are relayed to zAI's
  approval interface. If protocol initialization fails, Claude falls back to
  `acceptEdits` and Codex to `codex exec --json`, so those compatibility paths do
  not provide the same in-app approval bridge. Gemini runs headlessly in its
  `auto_edit` mode and zAI marks the selected workspace as trusted for that
  process. Kimi's
  [non-interactive prompt mode](https://moonshotai.github.io/kimi-code/en/reference/kimi-command.html#non-interactive-execution)
  uses its upstream automatic permission policy. These modes can write files or
  run tools without a zAI approval prompt, so use them only in workspaces you
  trust. An OpenRouter
  approval never grants permission to a local CLI.
- Provider CLIs and OpenRouter necessarily send prompts, context, and selected
  tool results to their respective services. Their privacy policies, terms,
  quotas, and charges apply. zAI adds no analytics service of its own.
- The integrated terminal is a full interactive shell running with the user's
  operating-system authority in the selected workspace. Closing its tab stops
  the PTY process tree, but commands entered there are otherwise unrestricted.
- Commit stages every current workspace change after an explicit confirmation.
  Push and Create PR perform external Git/GitHub mutations only after their
  corresponding topbar buttons are clicked; zAI never force-pushes.
- Browser surfaces accept only HTTP and HTTPS addresses and run in a sandboxed
  frame. Pages can still execute their own scripts and network requests, and
  their privacy policies apply. Some sites block embedding; use the
  external-open icon in that case. Because the surface is an iframe, navigation
  initiated inside a cross-origin page cannot always update zAI's address and
  history controls.
- The main webview's direct API connection policy is limited to OpenRouter;
  embedded Browser frames may load user-selected HTTP/HTTPS pages, and local
  CLIs make their own network connections outside the webview.

## Run from source

Prerequisites:

- Current stable [Rust](https://www.rust-lang.org/tools/install)
- Node.js 20.19 or newer (or Node.js 22.12 or newer) and npm
- The [Tauri 2 system prerequisites](https://v2.tauri.app/start/prerequisites/)
  for your operating system
- At least one provider CLI, or an OpenRouter API key, to run an agent

Clone the repository, install the locked JavaScript dependency set, and start
the Tauri development application:

```sh
git clone https://github.com/z4mbo/zAI.git
cd zAI
npm ci
npm run dev
```

`npm run dev` starts both the Vite frontend and the Rust/Tauri process. The
first run also compiles the native application and can take longer. Keep that
terminal open while testing.

For interface-only work, `npm run dev:web` starts a browser preview on port
1420. It uses the frontend's demo transport and does not exercise native CLI
processes, the operating-system credential store, or real persistence.

### Configure a provider

For Claude Code, Codex, Gemini, or Kimi:

1. Install the official CLI linked in [Providers](#providers).
2. Complete that CLI's normal login or API configuration outside zAI.
3. Confirm its executable is on `PATH` in the environment from which you run
   `npm run dev`.
4. Open zAI Settings → Providers and select **Refresh**. zAI reuses the CLI's
   existing credentials; it does not perform or store the CLI login itself.

You can verify discovery from the same shell before launching zAI:

```sh
claude --version
codex --version
gemini --version
kimi --version
```

Only the provider you intend to use needs to be installed. For OpenRouter,
create a key in the OpenRouter dashboard, open zAI Settings → Providers, enter
the key in the OpenRouter section, and connect. zAI validates the key, stores
it through the operating-system credential store, and then loads the model
catalog available to that key. Do not commit or paste API keys into source
files, `.env` files, screenshots, or issue reports.

### Test the desktop workspace

Use `npm run dev` for functional testing; `npm run dev:web` is a visual demo
and intentionally mocks native operations.

1. Choose a real project folder and send a prompt to create a session.
2. Confirm the chat composer matches the T3-style rounded glass layout and its
   provider, Build, access-policy, send, and stop controls are present.
3. Use the **Open** dropdown to select an installed editor. The main Open
   button remembers that choice.
4. Toggle the bottom panel with the first layout icon or `Cmd/Ctrl+J`. Run
   `pwd`, resize the drawer, create another terminal tab, and close it.
5. Toggle the right panel with the second layout icon or `Cmd/Ctrl+Shift+J`.
   Use **+** to keep Browser, Files, Diff, and Terminal tabs open together.
6. In Browser, enter a localhost or HTTPS URL. In Files, preview a UTF-8 file.
   In Diff, verify tracked and untracked changes appear.
7. Test Commit in a disposable Git repository first: its dialog lists every
   file and clearly says it will commit all changes. Push and Create PR require
   a configured remote; Create PR also requires an authenticated `gh` CLI.

### Checks and builds

Run the frontend checks and Rust tests:

```sh
npm test
npm run typecheck
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

`npm run build` validates and bundles the frontend only. Build the native
desktop application separately:

```sh
npm run build:desktop
```

Tauri installers and application bundles are emitted below
`src-tauri/target/release/bundle/` unless Cargo's target directory is
overridden.

## Attribution

The OpenCode and T3 Code source trees were reviewed at pinned revisions so the
adaptations and architectural influences are reproducible:

- OpenCode production UI reference for `@opencode-ai/ui` 1.18.4:
  [`411eff73f026d4950c07947c4d983788cb615baa`](https://github.com/anomalyco/opencode/tree/411eff73f026d4950c07947c4d983788cb615baa).
  zAI uses that published UI package and substantially adapts OpenCode UI
  layout and component ideas for the zAI desktop interface.
- T3 Code behavior and provider-driver architecture reference:
  [`9a0a07167f0623c3a7db0ffeff2e3939760309df`](https://github.com/pingdotgg/t3code/tree/9a0a07167f0623c3a7db0ffeff2e3939760309df).
  zAI is informed by its provider-instance separation, persistent session
  handling, normalized events, composer geometry/interactions, session header
  actions, panel toggles, terminal drawer, and right-panel tab model; T3 Code
  is not a zAI runtime dependency.

Both upstream projects are MIT-licensed. Their copyright notices and complete
license texts are preserved in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
and [`licenses/`](licenses/), and those files are embedded in packaged desktop
resources. The zAI project owns and uses its own logo,
wordmark, and application icons; it does not use the OpenCode or T3 Code logo
or wordmark. Attribution identifies provenance and does not imply sponsorship
or endorsement.

## Contributing and license

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow. The
current zAI rewrite is released under the [MIT License](LICENSE). Historical
revisions remain governed by the licenses included in those revisions.
