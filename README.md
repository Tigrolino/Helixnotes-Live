# HelixNotes Live

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://github.com/Tigrolino/Helixnotes-Live/blob/main/LICENSE)
[![Fork of HelixNotes](https://img.shields.io/badge/fork%20of-HelixNotes-orange)](https://gitlab.com/ArkHost/HelixNotes)
[![Website](https://img.shields.io/badge/web-helixnotes.com-purple)](https://helixnotes.com)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS%20%7C%20Android-lightgrey)]()

A fork of [HelixNotes](https://gitlab.com/ArkHost/HelixNotes) adding **Live Notebook**: real-time
collaborative editing for shared notebooks, layered onto the same local-markdown app.

Your notes are still stored as standard Markdown files on your local filesystem.
No cloud, no lock-in - collaboration is opt-in and runs through a small, self-hosted relay server.

## AI notice

This fork was entirely coded with AI. I've reviewed every function and made sure I understand and
approve of the changes, but there may still be issues I haven't discovered - if you come across
anything that seems incorrect or broken, please report it to me.

I also want to be transparent about my views on AI. I don't fully support its use in every
context, and I believe there are areas where it shouldn't be used, such as the creation of images
or videos. However, I created this project entirely as a side project to collaborate with a friend
on another project of mine. This isn't meant to be an excuse for using AI, but I wanted to be
upfront about the reasoning behind it and give some context from my side.

## Live Notebook (this fork's addition)

A shared notebook shows up as a normal entry in the sidebar's notebook tree - same note list, same
editor, nothing new to learn. On top of that:

- Real-time collaborative editing (Yjs CRDT), with live cursors and a connected-users indicator
- Presence and connection-status indicators
- Pinning notes within a shared notebook
- Pasting or dropping images and files straight into a note - up to 95 MB, relayed through the
  collaboration server and optionally backed up to GitHub for durability

It's all powered by `collab-server/`, a small, self-hostable Node.js/TypeScript WebSocket relay
(Render works well - see [`collab-server/README.md`](collab-server/README.md) for local dev and
deployment instructions). It's gated by a single shared workspace password; there's no account
system.

## Features

- Markdown editor with toolbar, slash commands, source mode, code highlighting
- **Tasks view**: aggregate `- [ ]` checklists from across all notes, set priority and due dates, work in a list or a calendar (drag a task to reschedule)
- `[[Wiki-links]]` and graph view
- Full-text search (Tantivy), CJK-aware for Chinese, Japanese, and Korean
- Outline panel, daily notes with calendar view, tags with autocomplete, drag-and-drop
- Live KaTeX math editor (`/math`, `/imath`) with modal preview, double-click to edit
- Mermaid diagrams (opt-in render, copy as PNG, save as PNG/SVG)
- Encrypted secret blocks (`/secret`) stored as portable `helix-secret` markdown fences
- Insert date/time (`/date`, `/time`, `/now`), color swatches (`/color`), configurable week start
- Manual notebook sorting (drag to reorder above, into, or below)
- External `.md` viewer mode with import-to-vault flow
- PDF preview, Obsidian import, "Show in File Manager"
- AI writing tools (Ollama / OpenAI-compatible / Anthropic / OpenAI)
- **Optional WebDAV sync** to your own server (Nextcloud, ownCloud, a NAS): manual or automatic, with keep-both conflict copies
- Version history with diffs, automatic backups
- Multi-window, file associations, focus mode, view mode
- Themes (light, dark, and 14 palettes), accent colors, fonts, 80-200% interface scale
- Local plain-text files, no company cloud
- **Live Notebook**: real-time collaborative editing on shared notebooks - live cursors,
  presence, pinning, and image/file uploads (see above)

Full documentation: [helixnotes.com/docs](https://helixnotes.com/docs.html)

## Tech Stack

- **Frontend**: SvelteKit (Svelte 5) + TailwindCSS v4 + TipTap v3
- **Backend**: Rust (Tauri 2.0) + Tantivy (search) + Notify (file watcher)
- **Collaboration**: Node.js/TypeScript WebSocket relay (`collab-server/`) + Yjs CRDT
- **Platforms**: Linux (AppImage), Windows, macOS, Android

## Building from Source

### Prerequisites

- [Rust](https://rustup.rs/) (1.88+)
- [Node.js](https://nodejs.org/) (18+)
- [pnpm](https://pnpm.io/)
- System dependencies for Tauri: see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

### Development

```bash
pnpm install
pnpm tauri dev
```

### Verification

Run the frontend checks and tests, Rust tests and lints, and production frontend build with one command:

```bash
pnpm verify
```

### Production Build

```bash
pnpm tauri build
```

Live Notebook's relay server (`collab-server/`) is a separate Node project with its own setup and
deployment steps - see [`collab-server/README.md`](collab-server/README.md).

## License

[AGPL-3.0](https://github.com/Tigrolino/Helixnotes-Live/blob/main/LICENSE)
