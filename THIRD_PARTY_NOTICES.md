# Third-party notices

zAI is an independent project. Its interface uses the MIT-licensed
`@opencode-ai/ui` package at version 1.18.4 and substantially adapts layout and
component ideas from OpenCode's production UI. Its persistent-session behavior,
composer interactions, normalized event model, and provider-driver separation
were informed by T3 Code. T3 Code is not shipped as a zAI runtime dependency.

The zAI project owns and uses its own logo, wordmark, and application icons.
It does not redistribute the OpenCode or T3 Code logo or wordmark. OpenCode,
T3 Code, and their associated names and marks remain the property of their
respective owners. This notice records provenance and license compliance; it
does not imply affiliation, sponsorship, or endorsement.

The source revisions reviewed for this implementation were:

- [OpenCode production UI reference for `@opencode-ai/ui` 1.18.4 — `411eff73f026d4950c07947c4d983788cb615baa`](https://github.com/anomalyco/opencode/tree/411eff73f026d4950c07947c4d983788cb615baa)
- [T3 Code behavior and provider architecture reference — `9a0a07167f0623c3a7db0ffeff2e3939760309df`](https://github.com/pingdotgg/t3code/tree/9a0a07167f0623c3a7db0ffeff2e3939760309df)

## OpenCode

Project: <https://github.com/anomalyco/opencode>

Package used by zAI: `@opencode-ai/ui` version `1.18.4`

Pinned source revision: `411eff73f026d4950c07947c4d983788cb615baa`

Use in zAI: distributed UI dependency plus substantial adaptation of
production UI layout and component ideas.

Copyright (c) 2025 opencode

OpenCode is licensed under the MIT License. The complete notice supplied by
OpenCode is preserved at [licenses/OpenCode-MIT.txt](licenses/OpenCode-MIT.txt).

## T3 Code

Project: <https://github.com/pingdotgg/t3code>

Pinned source revision: `9a0a07167f0623c3a7db0ffeff2e3939760309df`

Use in zAI: behavioral and architectural reference for composer interactions,
provider-driver boundaries, provider instances, normalized runtime events, and
persistent sessions. T3 Code is not bundled as a runtime dependency.

Copyright (c) 2026 T3 Tools Inc.

T3 Code is licensed under the MIT License. The complete notice supplied by T3
Code is preserved at [licenses/T3Code-MIT.txt](licenses/T3Code-MIT.txt).

## Inter font

The interface embeds the Inter font distributed by `@opencode-ai/ui` 1.18.4.
Inter is copyright 2016 The Inter Project Authors and is licensed under the
SIL Open Font License 1.1. The complete notice is preserved at
[licenses/Inter-OFL-1.1.txt](licenses/Inter-OFL-1.1.txt).

## xterm.js

The interactive terminal embeds `@xterm/xterm` version 6.0.0 and
`@xterm/addon-fit` version 0.11.0 from the xterm.js project.

Copyright (c) 2017-2019, The xterm.js authors; copyright (c) 2014-2016,
SourceLair Private Company; copyright (c) 2012-2013, Christopher Jeffrey. The
addon-fit notice is copyright (c) 2019, The xterm.js authors.

Both packages are licensed under the MIT License. Their combined notices and
license text are preserved at [licenses/xterm-MIT.txt](licenses/xterm-MIT.txt).

## DM Sans font

The T3-informed composer and workspace controls embed DM Sans from
`@fontsource-variable/dm-sans` version 5.2.8.

Copyright 2014 The DM Sans Project Authors.

DM Sans is licensed under the SIL Open Font License 1.1. The complete notice
is preserved at
[licenses/DM-Sans-OFL-1.1.txt](licenses/DM-Sans-OFL-1.1.txt).

## JetBrains Mono font

Terminal, diff, file-preview, and permission details embed JetBrains Mono from
`@fontsource/jetbrains-mono` version 5.2.8.

Copyright 2020 The JetBrains Mono Project Authors.

JetBrains Mono is licensed under the SIL Open Font License 1.1. The complete
notice is preserved at
[licenses/JetBrains-Mono-OFL-1.1.txt](licenses/JetBrains-Mono-OFL-1.1.txt).

## portable-pty

The native terminal uses `portable-pty` version 0.9.0, a cross-platform PTY
library from the WezTerm project.

Copyright (c) 2018 Wez Furlong

`portable-pty` is licensed under the MIT License. The complete notice supplied
with the crate is preserved at
[licenses/portable-pty-MIT.txt](licenses/portable-pty-MIT.txt).

## Product and service names

Claude and Claude Code are associated with Anthropic. Codex and OpenAI are
associated with OpenAI. Gemini is associated with Google. Kimi and Kimi Code
are associated with Moonshot AI. OpenRouter is associated with OpenRouter.
OpenCode is associated with its respective maintainers, and T3 Code is
associated with T3 Tools Inc.

Use of these names identifies compatible tools and services only. zAI is not
affiliated with, sponsored by, endorsed by, or an official product of any of
those parties. Each CLI, model, API, and transitive dependency remains subject
to its own license and terms.

## Repository history

The root [LICENSE](LICENSE) governs the current independent rewrite. Earlier
revisions in this repository's Git history may contain differently licensed
works; the license files present in those revisions continue to govern them.

The root license, this notice, and the complete `licenses/` directory are also
declared as Tauri bundle resources so packaged desktop distributions carry
these notices with the application.
