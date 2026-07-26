# Third-party notices

Onyx is an independent project. Its interface uses the MIT-licensed `@opencode-ai/ui` package at version 1.18.4 and substantially adapts layout and component ideas from OpenCode's production UI. Its persistent provider sessions, normalized runtime events, composer controls, usage-limit display, and workspace interactions were informed by T3 Code. Workspace-panel geometry was initially referenced at T3 Code revision `9a0a07167f0623c3a7db0ffeff2e3939760309df`; provider behavior was re-audited at `78a0ea55c1d9edce8bcd2b3caff9510b4093e6d3`. T3 Code is not shipped as an Onyx runtime dependency.

Onyx owns and uses its own logo, wordmark, and application icons. OpenCode, T3 Code, and their associated names and marks remain the property of their respective owners. Attribution does not imply affiliation, sponsorship, or endorsement.

The general chat interaction is informed by the publicly accessible T3 Chat product. No T3 Chat source code, backend service, or brand asset is included.

## OpenCode

- Project: <https://github.com/anomalyco/opencode>
- Package: `@opencode-ai/ui` 1.18.4
- Pinned revision: [`411eff73f026d4950c07947c4d983788cb615baa`](https://github.com/anomalyco/opencode/tree/411eff73f026d4950c07947c4d983788cb615baa)
- Copyright (c) 2025 opencode
- License: MIT; complete text at [licenses/OpenCode-MIT.txt](licenses/OpenCode-MIT.txt)

## T3 Code

- Project: <https://github.com/pingdotgg/t3code>
- Pinned revision: [`78a0ea55c1d9edce8bcd2b3caff9510b4093e6d3`](https://github.com/pingdotgg/t3code/tree/78a0ea55c1d9edce8bcd2b3caff9510b4093e6d3)
- Use: behavioral and architectural reference; not bundled as a runtime dependency
- Copyright (c) 2026 T3 Tools Inc.
- License: MIT; complete text at [licenses/T3Code-MIT.txt](licenses/T3Code-MIT.txt)

## Other distributed dependencies and assets

- Dashboard Icons provider artwork: Apache License 2.0; pinned revision [`46b860c70e866212311aef2f98da3775c17f5068`](https://github.com/homarr-labs/dashboard-icons/tree/46b860c70e866212311aef2f98da3775c17f5068); Copyright (c) 2024 Bjorn Lammers, Meier Lukas, Thomas Camlong and Homarr Labs; [licenses/Dashboard-Icons-Apache-2.0.txt](licenses/Dashboard-Icons-Apache-2.0.txt)
- Inter: SIL Open Font License 1.1, [licenses/Inter-OFL-1.1.txt](licenses/Inter-OFL-1.1.txt)
- DM Sans: SIL Open Font License 1.1, [licenses/DM-Sans-OFL-1.1.txt](licenses/DM-Sans-OFL-1.1.txt)
- JetBrains Mono: SIL Open Font License 1.1, [licenses/JetBrains-Mono-OFL-1.1.txt](licenses/JetBrains-Mono-OFL-1.1.txt)
- xterm.js and addon-fit: MIT, [licenses/xterm-MIT.txt](licenses/xterm-MIT.txt)
- portable-pty: MIT, [licenses/portable-pty-MIT.txt](licenses/portable-pty-MIT.txt)

Claude/Claude Code, Codex/OpenAI, Gemini, Kimi/Moonshot AI, xAI, and OpenRouter are names of compatible third-party tools and services. Their licenses, subscriptions, terms, privacy policies, and charges apply independently.

The root [LICENSE](LICENSE), this notice, and the complete `licenses/` directory are embedded as Tauri bundle resources.
