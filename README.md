# HelixNotes

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://gitlab.com/ArkHost/HelixNotes/-/blob/main/LICENSE)
[![Latest Release](https://img.shields.io/badge/release-v1.3.4-green)](https://gitlab.com/ArkHost/HelixNotes/-/releases/v1.3.4)
[![Website](https://img.shields.io/badge/web-helixnotes.com-purple)](https://helixnotes.com)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS%20%7C%20Android-lightgrey)]()

A local markdown note-taking app built with Tauri, SvelteKit, and Rust.

Your notes are stored as standard Markdown files on your local filesystem.
No cloud, no lock-in.

## Download (v1.3.4)

### Linux

#### AppImage

The AppImage works only on Fedora 43+, Arch Linux, and openSUSE Tumbleweed (x86_64).

[Download AppImage](https://download.helixnotes.com/releases/v1.3.4/HelixNotes_1.3.4_amd64.AppImage)

#### Distro-specific packages

##### Fedora 43+ (DNF)

```bash
sudo dnf config-manager addrepo \
  --from-repofile=https://repo.arkhost.com/helixnotes.repo
sudo dnf install helix-notes
```

##### Debian / Ubuntu / Mint (APT)

```bash
curl -fsSL https://repo.arkhost.com/gpg.key | sudo gpg --dearmor -o /usr/share/keyrings/arkhost.gpg && echo "deb [signed-by=/usr/share/keyrings/arkhost.gpg arch=amd64] https://repo.arkhost.com stable main" | sudo tee /etc/apt/sources.list.d/helixnotes.list && sudo apt update && sudo apt install helix-notes
```

##### Arch / Manjaro (AUR)

```bash
yay -S helixnotes-appimage-bin
```

##### Solus (EOPKG)

```bash
sudo eopkg it helixnotes
```

##### NixOS

<details>
<summary>flake.nix</summary>

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    helix-notes = {
      url = "git+https://gitlab.com/ArkHost/HelixNotes";
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

#### Manual package downloads

- [.deb](https://download.helixnotes.com/releases/v1.3.4/HelixNotes_1.3.4_amd64.deb) (Ubuntu 22.04+)
- [.rpm](https://download.helixnotes.com/releases/v1.3.4/HelixNotes-1.3.4-1.x86_64.rpm)

### Windows

[Download Installer](https://download.helixnotes.com/releases/v1.3.4/HelixNotes_1.3.4_x64-setup.exe) (Windows 10/11)

### macOS

[Download .dmg (Apple Silicon)](https://download.helixnotes.com/releases/v1.3.4/HelixNotes_1.3.4_aarch64.dmg) (M-series Macs)

> **"HelixNotes is damaged and can't be opened"?** The app isn't damaged. The macOS build isn't notarized by Apple yet, so Gatekeeper blocks it on Apple Silicon. Run this once in Terminal, then open it normally (you'll need to redo it after each update):
>
> ```bash
> xattr -cr /Applications/HelixNotes.app
> ```

### Android

[Download APK](https://download.helixnotes.com/releases/v1.3.4/HelixNotes_1.3.4_android.apk)

---

All releases: [gitlab.com/ArkHost/HelixNotes/-/releases](https://gitlab.com/ArkHost/HelixNotes/-/releases)

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

[AGPL-3.0](https://gitlab.com/ArkHost/HelixNotes/-/blob/main/LICENSE)
