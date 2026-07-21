---
name: ui-creator
description: Builds a project insight interface — navigable, informative, beside the terminal
user-invocable: true
---

# UI Creator

Build an interface that helps the user **navigate and understand what really matters** about their project. The terminal stays visible — your UI sits beside it, providing structure, previews, and insight.

## Purpose

Your UI is not a generic dashboard. It's a **project lens** — a living view that surfaces what's important:

- File structure with inline previews (the canvas can read files at runtime)
- Key patterns, dependencies, and architecture at a glance
- Important metrics, config summaries, or data overviews
- Clickable elements that prefill reviewed text in a non-OpenRouter terminal via `yft.sendToTerminal()`

## Design language — Windows 11 minimalist

Follow the app's design identity. **Not dark theme. Not generic AI output.**

```
Background:       #f8f9fa (light, clean)
Surface/cards:    #ffffff with border #e5e7eb
Text primary:     #1a1a1a
Text secondary:   #6b7280
Accent:           #0078d4 (Windows blue)
Font:             'Segoe UI', system-ui, sans-serif
Border radius:    8px for cards, 6px for buttons
Spacing:          Generous whitespace, 16-20px padding
Shadows:          Minimal — 0 1px 3px rgba(0,0,0,0.06)
```

Think: Windows 11 Settings app, Microsoft 365, or VS Code's Welcome tab. Light, bright, airy, with clear hierarchy and no visual noise.

## Layout modes

- **`full`** (primary) — Replaces sidebar + right panel. Terminal + toolbar stay in center-left. Your UI takes the right side (resizable). Use this for anything substantial.
- **`bottom`** — Below the terminal. Use for live feeds, logs, or monitoring views.
- **`panel`** — Small right sidebar tab. Use for quick status indicators only.

**Default to `full`.**

## When to update the UI

**Do NOT rebuild the UI on every response.** The interface should be stable — users rely on spatial memory to navigate it. Only update when:

- The user explicitly asks ("update the dashboard", "add X to the view")
- A genuinely important insight is discovered (new dependency found, critical pattern identified, major structural change)
- The user switches to a different area of the project that warrants a different view

Never update just because you answered a question. The UI is a persistent workspace, not a response artifact.

## `window.yft` API

The iframe runs in a sandbox but can communicate with the app:

### Actions (fire-and-forget)
- **`yft.sendToTerminal(text)`** — Prefills printable text in the active terminal. Control characters are removed, the user must press Enter, and OpenRouter terminals reject Canvas writes.
- **`yft.switchTab(tab)`** — Switches right panel tab
- **`yft.setMode(mode)`** — Changes layout mode (`panel`, `full`, `bottom`)

### Data access (async, returns Promises)
- **`yft.readFile(path)`** — Reads a file's content. Only relative paths contained in the project are accepted. Returns `string | null`.
- **`yft.readDir(path)`** — Lists a contained project directory by relative path. Returns array of `{ name, path, isDirectory, size, modified }` or `null`.

This means your UI can **read project files at runtime** — render file previews, show config contents, display code snippets, all live:

```html
<script>
  // Load and display a file when the user clicks it
  async function showFile(path) {
    const content = await yft.readFile(path);
    if (content) {
      document.getElementById('preview').textContent = content;
      document.getElementById('preview-title').textContent = path;
    }
  }

  // List project files on load
  async function init() {
    const entries = await yft.readDir('.');
    const list = document.getElementById('file-list');
    entries.filter(e => !e.isDirectory).forEach(e => {
      const li = document.createElement('div');
      li.textContent = e.name;
      li.onclick = () => showFile(e.name);
      li.className = 'file-item';
      list.appendChild(li);
    });
  }
  init();
</script>
```

## Template

Windows 11 light-theme starting point:

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Project</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
      background: #f8f9fa;
      color: #1a1a1a;
      min-height: 100vh;
      font-size: 13px;
      line-height: 1.5;
    }
    .app { display: flex; flex-direction: column; min-height: 100vh; }
    .header {
      padding: 20px 24px 16px;
      border-bottom: 1px solid #e5e7eb;
      background: #ffffff;
    }
    .header h1 {
      font-size: 1.1rem;
      font-weight: 600;
      color: #1a1a1a;
      letter-spacing: -0.01em;
    }
    .header p { font-size: 0.75rem; color: #6b7280; margin-top: 2px; }
    .content {
      flex: 1;
      padding: 16px 20px;
      display: flex;
      flex-direction: column;
      gap: 12px;
      overflow-y: auto;
    }
    .card {
      background: #ffffff;
      border: 1px solid #e5e7eb;
      border-radius: 8px;
      padding: 16px;
      box-shadow: 0 1px 3px rgba(0,0,0,0.04);
    }
    .card h2 {
      font-size: 0.7rem;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      color: #6b7280;
      margin-bottom: 10px;
    }
    .file-item {
      padding: 6px 10px;
      border-radius: 6px;
      cursor: pointer;
      font-size: 12px;
      color: #374151;
      transition: background 0.1s;
    }
    .file-item:hover { background: #f0f1f3; }
    .file-item.active { background: #e8f0fe; color: #0078d4; }
    .preview {
      background: #fafbfc;
      border: 1px solid #e5e7eb;
      border-radius: 6px;
      padding: 12px 14px;
      font-family: 'Cascadia Code', 'Consolas', monospace;
      font-size: 11.5px;
      line-height: 1.6;
      white-space: pre-wrap;
      overflow-y: auto;
      color: #374151;
      max-height: 400px;
    }
    .badge {
      display: inline-block;
      padding: 2px 8px;
      border-radius: 10px;
      font-size: 11px;
      font-weight: 500;
      background: #e8f0fe;
      color: #0078d4;
    }
    .stat { font-size: 1.5rem; font-weight: 700; color: #1a1a1a; }
    .stat-label { font-size: 0.7rem; color: #9ca3af; margin-top: 2px; }
    button {
      background: #0078d4;
      color: white;
      border: none;
      border-radius: 6px;
      padding: 7px 14px;
      cursor: pointer;
      font-weight: 500;
      font-size: 12px;
      font-family: inherit;
      transition: background 0.15s;
    }
    button:hover { background: #106ebe; }
    .btn-subtle {
      background: transparent;
      color: #0078d4;
      border: 1px solid #d1d5db;
    }
    .btn-subtle:hover { background: #f0f1f3; }
  </style>
</head>
<body>
  <div class="app">
    <div class="header">
      <h1>Project Name</h1>
      <p>Quick insight into what matters</p>
    </div>
    <div class="content">
      <div class="card">
        <h2>Files</h2>
        <div id="file-list"></div>
      </div>
      <div class="card">
        <h2>Preview</h2>
        <div id="preview-title" style="font-size:11px;color:#6b7280;margin-bottom:6px;"></div>
        <div id="preview" class="preview">Select a file to preview</div>
      </div>
    </div>
  </div>
</body>
</html>
```

## Design principles

1. **Information density over decoration** — Every pixel should inform. No ornamental gradients, no hero images, no card shadows deeper than 3px.
2. **Hierarchy through typography, not color** — Use font weight (400/500/600), size (11-16px range), and color (#1a1a1a → #6b7280 → #9ca3af) to create hierarchy. Avoid colored backgrounds for emphasis.
3. **Spatial stability** — The layout must not jump around. Use fixed regions (file list left, preview right, or vertical scroll with pinned headers). Users build muscle memory.
4. **Interactivity that matters** — Clickable file previews, expandable sections, copy buttons on code blocks. Not animations for the sake of it.
5. **Windows 11 soul** — Light backgrounds, 1px borders in #e5e7eb, rounded corners (6-8px), Segoe UI, generous padding, zero clutter.
6. **No generic AI aesthetic** — No neon gradients, no dark mode, no card grids with random icons, no "dashboard" clichés. Look at Windows 11 Settings, File Explorer details pane, or Microsoft Loop for inspiration.

## What to show

Read the project before designing. Then pick what matters most:

- **Structure** — file tree, key directories, entry points
- **Content** — inline previews of important files (README, configs, main entry)
- **Dependencies** — package.json deps, import graphs, external services
- **Patterns** — repeated code structures, naming conventions, architecture style
- **Health** — test coverage hints, TODO counts, lint config presence
- **Context** — git branch, recent changes summary, open issues count

Not all of these. Pick the 3-4 that matter most for THIS specific project.

## Design autonomy

Never ask the user what to show or how it should look. Read the project, identify what's important, design the interface, render it. If the user wants changes, make them directly.

## Limitations

- No `localStorage` or `sessionStorage` (sandboxed iframe)
- No parent DOM access
- No navigation, popups, or new windows
- File reads are async and scoped to the project
- Max HTML size: 1MB
