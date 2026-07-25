# Changelog

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
