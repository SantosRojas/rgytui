# rgytui 🎵

> **R**ust **G**TK? No — **R**ust **G**orgeous **Y**ou**T**ube **UI**.
>
> A modern TUI application to search, stream, and play YouTube music — right from your terminal.

<p align="center">
  <img src="https://img.shields.io/badge/rustc-1.85%2B-orange" alt="Rustc"/>
  <img src="https://img.shields.io/github/v/release/rojasape/rgytui" alt="Version"/>
  <img src="https://img.shields.io/github/actions/workflow/status/rojasape/rgytui/ci.yml?branch=main" alt="CI"/>
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="License"/>
</p>

---

## Features ✨

- **🔍 Search YouTube** — Find songs directly from the terminal
- **▶ Audio Playback** — Stream audio via `yt-dlp` + `rodio`
- **🎬 Video Mode** — Spawn `mpv` for video playback
- **📋 Queue Management** — Build and persist playlists
- **🎚 Volume Control** — Real-time volume adjustment
- **⏩ Auto-next** — Automatically advances the queue
- **💾 Persistent State** — Remembers volume, queue across sessions
- **⌨️ Keyboard-driven** — Full TUI with vi-style navigation
- **🐧 Cross-platform** — Windows, Linux, macOS

---

## Installation 📦

One command. Everything included: build dependencies, yt-dlp, mpv, and rgytui itself.

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/rojasape/rgytui/main/scripts/install.sh | bash
```

The script will:
1. Install build dependencies (pkg-config, Wayland, ALSA, GTK3)
2. Install runtime dependencies (yt-dlp, mpv)
3. Clone the repository to `~/.local/share/rgytui/repo`
4. Build `rgytui --release`
5. Symlink to `~/.local/bin/rgytui`

> Make sure `~/.local/bin` is in your PATH.

### Windows

Run **PowerShell as Administrator**:

```powershell
Set-ExecutionPolicy RemoteSigned -Scope Process -Force
iex (curl -fsSL https://raw.githubusercontent.com/rojasape/rgytui/main/scripts/install.ps1)
```

The script will:
1. Install yt-dlp and mpv (via `winget` if available, or direct download for yt-dlp)
2. Detect if they're in PATH — if not, find the `.exe` and add it automatically
3. Clone the repository to `%LOCALAPPDATA%\rgytui\repo`
4. Build `rgytui --release`
5. Copy to `%LOCALAPPDATA%\rgytui\bin\` and add to user PATH

> No winget? No problem — yt-dlp is downloaded directly from GitHub. mpv requires manual install for video mode if winget is not available.

### Updating

```bash
rgytui update
```

Pulls the latest source, rebuilds, and replaces the installed binary. No Rust toolchain required after the first install — `rgytui update` handles everything.

---

## Quick Start 🚀

```bash
# Start the TUI
rgytui

# Inside the app:
/                  # Focus search
Type query + Enter # Search YouTube
↓ ↑               # Navigate results
Enter             # Play selected song
Space             # Play / Pause
q                 # Quit
?                 # Help
```

---

## Key Bindings ⌨️

| Key | Action |
|---|---|
| `/` | Focus search input |
| `Enter` | Search / Play selected |
| `↑` / `↓` | Navigate list |
| `Tab` | Switch focus (search ↔ results) |
| `Space` | Play / Pause |
| `s` | Stop playback |
| `n` | Next track |
| `p` | Previous track |
| `+` / `=` | Volume up |
| `-` | Volume down |
| `a` | Add selected to queue |
| `v` | Toggle audio / video mode |
| `?` | Toggle help screen |
| `Esc` | Back / Unfocus |
| `q` | Quit |

---

## Dependencies 📋

| Dependency | Required | Purpose |
|---|---|---|
| [yt-dlp](https://github.com/yt-dlp/yt-dlp) | ✅ Yes | Extract stream URLs and metadata from YouTube |
| [mpv](https://mpv.io) | ❌ No (video mode) | External video playback |

All Rust dependencies are managed by Cargo:
`ratatui` · `crossterm` · `tokio` · `reqwest` · `rodio` · `serde_json` · `tempfile`

---

## Build from Source 🔧

```bash
git clone https://github.com/rojasape/rgytui.git
cd rgytui

# Debug build
cargo build

# Release build
cargo build --release

# Run
./target/release/rgytui
```

---

## Architecture 🏗

```
┌─────────────────────────────────────────────────────┐
│                   TUI (ratatui)                      │
│         Search · Player · Help · Components          │
└──────────────────────┬──────────────────────────────┘
                       │ Events (tokio mpsc)
┌──────────────────────▼──────────────────────────────┐
│              Use Cases (Application)                 │
│       SearchUseCase · PlaybackUseCase · PlaylistUC   │
└──────┬──────────────────────────────┬───────────────┘
       │                              │
┌──────▼──────────────┐   ┌──────────▼────────────┐
│    Domain Model     │   │   Infrastructure       │
│  Song · Playlist   │   │  YtDlpClient           │
│  PlayerState (FSM) │   │  RodioBackend          │
│  DomainError       │   │  MpvBackend            │
└────────────────────┘   │  ConfigStore           │
                         └───────────────────────┘
```

### Audio Pipeline

```
yt-dlp --dump-json ──► Song metadata
yt-dlp -f bestaudio -g URL ──► Stream URL
reqwest GET ──► tempfile ──► rodio::Decoder ──► rodio::Player ──► Audio
```

The application follows [Hexagonal Architecture](https://alistair.cockburn.us/hexagonal-architecture/) (Ports & Adapters):
- **Domain** — Pure business logic, zero external dependencies
- **Application** — Use cases that orchestrate domain logic
- **Infrastructure** — Adapters for external systems (yt-dlp, audio, HTTP, filesystem)
- **Interface** — TUI presentation layer powered by `ratatui`

---

## Configuration ⚙️

Settings are persisted to `%APPDATA%\rgytui\settings.json` (Windows) or
`~/.config/rgytui/settings.json` (Linux/macOS):

```json
{
  "volume": 0.8,
  "audio_mode": true,
  "default_search_limit": 10
}
```

Playlists are saved to `playlist.json` in the same directory.

---

## Contributing 🤝

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing`)
5. Open a Pull Request

### Development

```bash
cargo check      # Verify compilation
cargo test       # Run tests
cargo clippy     # Lint
cargo fmt        # Format code
```

---

## License 📄

MIT — see [LICENSE](LICENSE)

---

<p align="center">
  Made with ❤️ and Rust
</p>
