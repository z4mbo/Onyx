# Contributing to zAI

Thanks for helping make zAI safer and more useful. Keep changes focused,
reviewable, and portable across macOS, Windows, and Linux where practical.

## Set up

Install current stable Rust, Node.js 20 or newer, npm, and the Tauri 2 system
prerequisites for your operating system. Then run:

```sh
npm install
npm run dev
```

A provider CLI is optional for interface work. To exercise an adapter, install
and authenticate that provider using its official instructions. Never put API
keys, access tokens, transcripts, or local credential-store data in fixtures,
screenshots, issues, or commits.

## Make a change

1. Create a branch from the current default branch.
2. Keep Rust/Tauri orchestration under `src-tauri/` and view state under `src/`.
3. Add or update focused tests for behavior that can regress.
4. Check permission, workspace-containment, cancellation, and secret-handling
   implications for every provider or tool change.
5. Update the README and notices when behavior, commands, or attribution
   changes.

Provider adapters must translate native output into the shared event model;
provider-specific protocol objects should not leak into UI components. Preserve
session-resume identifiers, bound captured output, drain stdout and stderr
concurrently, and terminate the complete process group on cancellation.

OpenRouter tools must resolve paths beneath the canonical workspace. Any new
operation that writes data, deletes data, changes external state, or executes a
program must have an explicit approval boundary in the Rust runtime.

## Verify

Run the checks relevant to the change before opening a pull request:

```sh
npm run typecheck
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
```

For a release-facing change, also run `npm run build:desktop` on each affected
platform and smoke-test provider discovery, a new session, cancellation,
resume, and settings persistence.

## Pull requests

Describe what changed, why it is needed, how it was tested, and any security or
compatibility tradeoffs. Include screenshots for visible interface changes,
but redact usernames, workspace paths, messages, tokens, and account data.

Contributions are accepted under the repository's MIT License. Submit only
work you have the right to license. Do not copy proprietary protocol schemas,
logos, illustrations, or other assets. When adapting MIT-licensed material,
retain its copyright and license notice in `THIRD_PARTY_NOTICES.md` and
`licenses/`. Do not imply endorsement by a compatible provider or upstream
project.
