# home-sync

> A declarative user environment manager for Windows — sync dotfiles and Scoop packages with a single config file.

## Why?

Windows users face a painful ritual every time they reinstall the OS, switch machines, or set up a new workstation:

- Manually copying config files (`.gitconfig`, editor settings, terminal profiles, etc.)
- Remembering and installing dozens of tools one by one
- No single source of truth for "my environment"

**home-sync** solves this with a declarative TOML config file. One command, and your entire user environment is restored.

## Features

- 📦 **Scoop Integration** — Declare buckets and packages; home-sync installs them automatically
- 🔗 **Dotfile Symlinks** — Map repo files to their Windows target paths with automatic backup
- 🚀 **One Command** — `home-sync sync` does everything
- 📋 **Status Check** — `home-sync status` shows what's linked and what's missing
- 🎯 **Selective Sync** — `--scoop-only` or `--dotfiles-only` flags
- 🪶 **Lightweight** — Single binary, no runtime dependencies

## Quick Start

### 1. Install

```bash
cargo install --path .
```

### 2. Initialize config

```bash
home-sync init
```

This creates a `config.toml` from the example template.

### 3. Edit `config.toml`

```toml
[scoop]
buckets = ["main", "extras", "versions"]
packages = ["git", "7zip", "ripgrep", "bat", "neovim"]

[[dotfiles]]
source = "dotfiles/.gitconfig"
target = "~/.gitconfig"

[[dotfiles]]
source = "dotfiles/settings.json"
target = "~/AppData/Roaming/Code/User/settings.json"
```

### 4. Add your dotfiles

Place your actual config files in the `dotfiles/` directory:

```
home-sync/
├── config.toml
├── dotfiles/
│   ├── .gitconfig
│   ├── settings.json
│   └── starship.toml
└── src/
```

### 5. Sync everything

```bash
home-sync sync
```

## Commands

| Command | Description |
|---------|-------------|
| `home-sync init` | Create a `config.toml` from the example template |
| `home-sync sync` | Full sync: install Scoop packages + link dotfiles |
| `home-sync sync --scoop-only` | Only sync Scoop packages |
| `home-sync sync --dotfiles-only` | Only sync dotfiles |
| `home-sync sync --dry-run` | Show what would be done without making changes |
| `home-sync status` | Show current environment status |

### Global Options

| Option | Description |
|--------|-------------|
| `-c, --config <PATH>` | Path to config file (default: `config.toml`) |

## How It Works

### Scoop Sync

1. Checks if Scoop is installed; if not, installs it via the official PowerShell installer
2. Adds any missing Scoop buckets
3. Installs any missing packages (already-installed packages are skipped)

### Dotfile Sync

1. For each `[[dotfiles]]` entry, resolves `~` to the user's home directory
2. Creates parent directories for the target path if needed
3. If the target already exists as the correct symlink → skip
4. If the target exists but is different → back it up with `.bak` extension
5. Creates a symlink from the repo source to the target path

## Example Config

See [`config.example.toml`](config.example.toml) for a full annotated example.

## Requirements

- Windows 10/11
- [Scoop](https://scoop.sh/) (auto-installed if missing)
- Developer Mode enabled (required for symlinks without admin privileges)

## License

MIT