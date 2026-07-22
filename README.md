# zAI

zAI is a Rust-native desktop workspace for local coding-agent CLIs and
OpenRouter models. It keeps one consistent conversation interface while each
provider runs through an isolated adapter in the selected project directory.

The desktop shell and orchestration layer are written in Rust with Tauri. The
interface is SolidJS and follows the compact workspace layout and interaction
language that made [OpenCode](https://github.com/anomalyco/opencode) useful,
while the provider boundary and normalized event stream are inspired by
[T3 Code](https://github.com/pingdotgg/t3code).

> [!IMPORTANT]
> zAI is an independent community project. It is not affiliated with or
> endorsed by OpenCode, T3 Tools, Anthropic, OpenAI, Google, Moonshot AI, or
> OpenRouter.

## Providers

| Provider | Integration | Authentication |
| --- | --- | --- |
| Claude Code | `claude --print` with streaming JSON and session resume | Existing Claude Code login/configuration |
| Codex | `codex exec --json` with resumable threads and workspace-write sandboxing | Existing Codex login/configuration |
| Gemini CLI | `gemini --prompt` with streaming JSON, `auto_edit`, and session resume | Existing Gemini CLI login/configuration |
| Kimi Code | `kimi --prompt` with streaming JSON and session resume | Existing Kimi Code login/configuration |
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

## Security model

Coding agents can change files and execute programs. Use zAI on a clean Git
worktree, review diffs, and keep important data backed up.

- The OpenRouter key is validated before storage, placed in the operating
  system keychain/credential manager, and never returned to the webview.
- OpenRouter read, list, and literal-search tools are constrained to the
  selected canonical workspace. File writes and shell commands require an
  explicit in-app approval and time out after ten minutes.
- OpenRouter file reads are capped at 256 KiB, writes at 1 MiB, and a response
  can perform at most 12 tool rounds.
- Prompts sent through command-line arguments are capped at 24 KiB for portable
  process creation. OpenRouter prompts use HTTPS and retain the 256 KiB cap.
- Local CLIs are separate trusted programs. zAI launches them in the selected
  workspace, but their native permission policy remains authoritative. Claude
  currently uses `acceptEdits`; Codex uses its `workspace-write` sandbox.
  Gemini runs headlessly in its documented `auto_edit` mode: edits are allowed,
  while tools still covered by an `ask_user` policy are denied in headless mode.
  Selecting a workspace in zAI marks it trusted for that Gemini process. Kimi's
  prompt mode is autonomous by upstream design. An in-app OpenRouter approval
  does not govern any of those CLI providers.
- Provider CLIs and OpenRouter necessarily send prompts, context, and selected
  tool results to their respective services. Their privacy policies, terms,
  quotas, and charges apply. zAI adds no analytics service of its own.
- The webview content-security policy limits network connections to OpenRouter;
  local CLIs make their own network connections outside the webview.

## Develop

Prerequisites:

- Current stable [Rust](https://www.rust-lang.org/tools/install)
- Node.js 20 or newer and npm
- The [Tauri 2 system prerequisites](https://v2.tauri.app/start/prerequisites/)
  for your operating system
- At least one provider CLI, or an OpenRouter API key, to run an agent

Install JavaScript dependencies and start the desktop app:

```sh
npm install
npm run dev
```

The browser-only interface can be previewed with `npm run dev:web`. Native
commands require the Tauri desktop runtime.

Run the static checks and production builds:

```sh
npm run typecheck
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
npm run build:desktop
```

Tauri installers and application bundles are emitted below
`src-tauri/target/release/bundle/` unless Cargo's target directory is
overridden.

## Attribution

The OpenCode and T3 Code source trees were reviewed at pinned revisions so the
design and architectural influences are reproducible:

- [OpenCode `0a601cf334b9a83cc2854108a2b860f25e6e7e8e`](https://github.com/anomalyco/opencode/tree/0a601cf334b9a83cc2854108a2b860f25e6e7e8e), plus its historical Tauri desktop at [`6f7d63e9ceaacc5debbfcba18bf8391a90e59e8f`](https://github.com/anomalyco/opencode/tree/6f7d63e9ceaacc5debbfcba18bf8391a90e59e8f)
- [T3 Code `32c6012dabdbd0eb178b25ea4225d889ec8f6475`](https://github.com/pingdotgg/t3code/tree/32c6012dabdbd0eb178b25ea4225d889ec8f6475)

Both upstream projects are MIT-licensed. Their copyright notices and complete
license texts are preserved in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
and [`licenses/`](licenses/). No upstream logo or brand asset is used.

## Contributing and license

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow. The
current zAI rewrite is released under the [MIT License](LICENSE). Historical
revisions remain governed by the licenses included in those revisions.
