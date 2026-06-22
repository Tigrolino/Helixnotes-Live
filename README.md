# HelixNotes

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://codeberg.org/ArkHost/HelixNotes/src/branch/main/LICENSE)
[![Latest Release](https://img.shields.io/badge/release-v1.3.2-green)](https://codeberg.org/ArkHost/HelixNotes/releases)
[![Website](https://img.shields.io/badge/web-helixnotes.com-purple)](https://helixnotes.com)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS%20%7C%20Android-lightgrey)]()

A local markdown note-taking app built with Tauri, SvelteKit, and Rust.

Your notes are stored as standard Markdown files on your local filesystem.
No cloud, no lock-in.

## Download (v1.3.2)

### Linux

#### Arch / Manjaro (AUR)

```bash
yay -S helixnotes-appimage-bin
```

#### Debian / Ubuntu / Mint (APT)

```bash
curl -fsSL https://repo.arkhost.com/gpg.key | sudo gpg --dearmor -o /usr/share/keyrings/arkhost.gpg && echo "deb [signed-by=/usr/share/keyrings/arkhost.gpg arch=amd64] https://repo.arkhost.com stable main" | sudo tee /etc/apt/sources.list.d/helixnotes.list && sudo apt update && sudo apt install helix-notes
```

#### Solus (EOPKG)

```bash
sudo eopkg it helixnotes
```

#### NixOS

<details>
<summary>flake.nix</summary>

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    helix-notes = {
      url = "git+https://codeberg.org/ArkHost/HelixNotes";
      # inputs.nixpkgs.follows = "nixpkgs";
    }
  };

  outputs = {
    nixpkgs,
    helix-notes,
    ...
  }: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
  in {
    nixosConfigurations.default = nixpkgs.lib.nixosSystem {
      system = system;
      specialArgs = { inherit helix-notes; };

      modules = [
        /path/to/configuration.nix
      ];
    };
  };
}
```
</details>


<details>
<summary>configuration.nix</summary>

```nix
{
  config,
  lib,
  pkgs,
  helix-notes,
  ...
}:
{
  users.users.<USERNAME> = {
    packages = with pkgs; [
      (helix-notes.packages.${pkgs.stdenv.hostPlatform.system}.default)
    ];
  };
}
```
</details>

#### AppImage (Arch, Fedora 43+, openSUSE Tumbleweed)

[Download AppImage](https://download.helixnotes.com/releases/v1.3.2/HelixNotes_1.3.2_amd64.AppImage)

#### .deb (manual)

[Download .deb](https://codeberg.org/ArkHost/HelixNotes/releases/download/v1.3.2/HelixNotes_1.3.2_amd64.deb) (Ubuntu 22.04+)

### Windows

[Download Installer](https://codeberg.org/ArkHost/HelixNotes/releases/download/v1.3.2/HelixNotes_1.3.2_x64-setup.exe) (Windows 10/11)

### macOS

[Download .dmg](https://codeberg.org/ArkHost/HelixNotes/releases/download/v1.3.2/HelixNotes_1.3.2_x64.dmg) (Intel, runs on Apple Silicon via Rosetta)

### Android

[Download APK](https://codeberg.org/ArkHost/HelixNotes/releases/download/v1.3.2/HelixNotes_1.3.2_android.apk)

---

All releases: [codeberg.org/ArkHost/HelixNotes/releases](https://codeberg.org/ArkHost/HelixNotes/releases)

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

Full documentation: [helixnotes.com/docs](https://helixnotes.com/docs.html)

## Tech Stack

- **Frontend**: SvelteKit (Svelte 5) + TailwindCSS v4 + TipTap v3
- **Backend**: Rust (Tauri 2.0) + Tantivy (search) + Notify (file watcher)
- **Platforms**: Linux (AppImage), Windows, macOS, Android

## Building from Source

### Prerequisites

- [Rust](https://rustup.rs/) (1.77+)
- [Node.js](https://nodejs.org/) (18+)
- [pnpm](https://pnpm.io/)
- System dependencies for Tauri: see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

### Development

```bash
pnpm install
pnpm tauri dev
```

### Production Build

```bash
pnpm tauri build
```

## Screenshots

![Editor](https://cdn.helixnotes.com/assets/screenshots/screenshot-1.png)
![Tasks calendar](https://cdn.helixnotes.com/assets/screenshots/screenshot-2.png)
![Graph view](https://cdn.helixnotes.com/assets/screenshots/screenshot-7.png)
![Daily notes](https://cdn.helixnotes.com/assets/screenshots/screenshot-4.png)
![Themes](https://cdn.helixnotes.com/assets/screenshots/screenshot-6.png)

## License

[AGPL-3.0](https://codeberg.org/ArkHost/HelixNotes/src/branch/main/LICENSE)
