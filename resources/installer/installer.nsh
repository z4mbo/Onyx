; ────────────────────────────────────────────────────────────────────
; zAI — Custom NSIS installer include
; Uses the zAI navy/cyan brand and concise product copy
; ────────────────────────────────────────────────────────────────────

; ── Welcome page ───────────────────────────────────────────────────
!define MUI_WELCOMEPAGE_TITLE "Welcome to zAI"
!define MUI_WELCOMEPAGE_TEXT "This wizard will guide you through the installation of zAI.$\r$\n$\r$\nWork with Claude Code, Gemini CLI, OpenAI Codex, Kimi Code, and OpenRouter models from one desktop app.$\r$\n$\r$\nClick Next to continue."

; ── Finish page ────────────────────────────────────────────────────
!define MUI_FINISHPAGE_TITLE "You're All Set!"
!define MUI_FINISHPAGE_TEXT "zAI has been installed on your computer.$\r$\n$\r$\nStart building with AI — just describe what you want in plain English.$\r$\n$\r$\nClick Finish to close this wizard."
!define MUI_FINISHPAGE_RUN_TEXT "Launch zAI"

; ── Uninstaller welcome ───────────────────────────────────────────
!define MUI_UNWELCOMEPAGE_TITLE "Uninstall zAI"
!define MUI_UNWELCOMEPAGE_TEXT "This wizard will remove zAI from your computer.$\r$\n$\r$\nYour projects and files will not be affected.$\r$\n$\r$\nClick Next to continue."

; ── Uninstaller finish ────────────────────────────────────────────
!define MUI_UNCONFIRMPAGE_TEXT_TOP "zAI will be uninstalled from the following folder."
!define MUI_FINISHPAGE_NOAUTOCLOSE
!define MUI_UNFINISHPAGE_NOAUTOCLOSE
