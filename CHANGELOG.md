# Changelog

## 0.3.3 — Native session tabs and CLI control

- Session deletion now removes persisted sessions from both Home and the tab context menu; closing a tab remains a non-destructive action.
- New session names are entered directly in the tab, and every session tab supports right-click rename and delete actions.
- Messages sent during an active turn are queued FIFO above the composer and can be steered immediately when the official provider transport supports it.
- Codex command activities use stable upstream item IDs and native output-delta events, keeping the running command expanded with live output before collapsing on completion.
- Installed runtimes expose **Open CLI** and **Update** actions inside an embedded Onyx terminal using each provider’s official command.
- The session header can resume the official provider CLI in the bottom terminal, preserving access to native slash commands and interactive controls.
- Fallback UI catalogs no longer invent provider model or reasoning choices; unavailable capabilities stay disabled until the installed CLI reports them.

## 0.3.2 — Home and Codex session reliability

- Onyx now opens on the real Home screen without creating a draft session automatically.
- New Session opens its own tab, focuses the required session name, and keeps the prompt composer directly below it.
- The standalone Chat page has been removed; official ChatGPT, Claude, Gemini, and Grok web apps remain available inside each session workspace.
- Unused standalone chat, image, and video UI/runtime code has been removed to reduce the shipped app.
- Codex CLI fallback sessions now place the sandbox option before `exec resume`, matching the current official CLI parser.

## 0.3.1 — Conversation and voice reliability

- Sent prompts are always visible as conversation messages with the signed-in profile and a local timestamp.
- The prompt rail stays compact until hover, animates to the active prompt, and keeps its click-to-jump preview.
- The Environment popover has been removed while the focused workspace tools remain available.
- Agent Voice migrates the retired OpenRouter speech default to a current model and compatible voice.
- Speech failures now preserve a short, actionable provider error instead of hiding the cause behind a generic notice.
- Empty or non-audio speech responses are rejected before playback.

## 0.3.0 — Native workspace update

- Session names are chosen before creation and can be renamed at any time.
- Session deletion is immediate and reliable, including sessions with an active CLI runtime.
- Every prompt has a compact navigation marker with hover preview and click-to-jump.
- Agent sessions preserve the official CLI runtime and interactive question flow.
- ChatGPT, Claude, Gemini, and Grok web apps open inside the Onyx workspace.
- Voice models and voices use curated dropdowns with separate transcription, agent, and speech roles.
- A live Environment panel brings Git changes, branch, local workspace, other running sessions, and sources together.
- Signed updates now include progress, release notes, and a matching “What’s new” experience.
- Startup, persistence, bundle size, and macOS launcher behavior received focused performance work.
