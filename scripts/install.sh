#!/usr/bin/env bash
#
# rgytui installer — Linux / macOS
#
# Installs build deps, runtime deps (yt-dlp + mpv), builds rgytui
# from source, and symlinks it into ~/.local/bin/.
#
# Usage:
#   curl -fsSL <url> | bash
#   ./scripts/install.sh

set -euo pipefail

REPO_URL="https://github.com/rojasape/rgytui.git"
INSTALL_ROOT="${RGYTUI_HOME:-$HOME/.local/share/rgytui}"
REPO_DIR="$INSTALL_ROOT/repo"
BIN_DIR="$HOME/.local/bin"
RGYTUI_BIN="$BIN_DIR/rgytui"

# ── Platform detection ────────────────────────────────────────────────────────

detect_os() {
    case "$(uname -s)" in
        Linux)  echo "linux" ;;
        Darwin) echo "macos" ;;
        *)      echo "" ;;
    esac
}

detect_pkg_manager() {
    if command -v apt &>/dev/null; then echo "apt"
    elif command -v dnf &>/dev/null; then echo "dnf"
    elif command -v pacman &>/dev/null; then echo "pacman"
    elif command -v brew &>/dev/null; then echo "brew"
    else echo ""; fi
}

# ── Installers ────────────────────────────────────────────────────────────────

install_apt() {
    echo ":: [apt] Installing build dependencies..."
    sudo apt update
    sudo apt install -y \
        pkg-config libwayland-dev libxkbcommon-dev \
        libasound2-dev libgtk-3-dev

    echo ":: [apt] Installing runtime dependencies (yt-dlp, mpv)..."
    sudo apt install -y yt-dlp mpv
}

install_dnf() {
    echo ":: [dnf] Installing build dependencies..."
    sudo dnf install -y \
        pkg-config wayland-devel libxkbcommon-devel \
        alsa-lib-devel gtk3-devel

    echo ":: [dnf] Installing runtime dependencies (yt-dlp, mpv)..."
    sudo dnf install -y yt-dlp mpv
}

install_pacman() {
    echo ":: [pacman] Installing build dependencies..."
    sudo pacman -S --needed \
        pkg-config wayland libxkbcommon alsa-lib gtk3

    echo ":: [pacman] Installing runtime dependencies (yt-dlp, mpv)..."
    sudo pacman -S --needed yt-dlp mpv
}

install_brew() {
    echo ":: [brew] Installing dependencies..."
    brew install pkg-config yt-dlp mpv
}

# ── Ensure Rust is installed ─────────────────────────────────────────────────

ensure_rust() {
    if command -v cargo &>/dev/null; then
        echo ":: Rust already installed (cargo found)."
        return
    fi
    echo ":: Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # Source cargo env for the rest of the script
    . "$HOME/.cargo/env"
    echo "  ✓ Rust installed."
}

# ── Build & install rgytui ───────────────────────────────────────────────────

build_rgytui() {
    if [ ! -d "$REPO_DIR" ]; then
        echo ":: Cloning rgytui into $REPO_DIR..."
        mkdir -p "$INSTALL_ROOT"
        git clone "$REPO_URL" "$REPO_DIR"
    else
        echo ":: Repository already exists at $REPO_DIR, updating..."
        git -C "$REPO_DIR" fetch --ff-only
        git -C "$REPO_DIR" pull --ff-only
    fi

    echo ":: Building rgytui (release)..."
    cargo build --release --manifest-path "$REPO_DIR/Cargo.toml"

    echo ":: Installing to $RGYTUI_BIN..."
    mkdir -p "$BIN_DIR"
    ln -sf "$REPO_DIR/target/release/rgytui" "$RGYTUI_BIN"

    echo ""
    echo "✓ rgytui installed successfully!"
    echo "  Binary: $RGYTUI_BIN"
    echo ""
    echo "  Make sure $BIN_DIR is in your PATH."
    echo "  Run 'rgytui' to start."
}

# ── Main ──────────────────────────────────────────────────────────────────────

OS=$(detect_os)

if [ "$OS" = "linux" ]; then
    PKG=$(detect_pkg_manager)
    case "$PKG" in
        apt)    install_apt ;;
        dnf)    install_dnf ;;
        pacman) install_pacman ;;
        *)      echo "ERROR: unsupported package manager (apt/dnf/pacman). Install deps manually."; exit 1 ;;
    esac
elif [ "$OS" = "macos" ]; then
    if command -v brew &>/dev/null; then
        install_brew
    else
        echo "ERROR: Homebrew is required. Install it from https://brew.sh"
        exit 1
    fi
else
    echo "ERROR: unsupported OS. This script is for Linux and macOS."
    exit 1
fi

ensure_rust
build_rgytui
