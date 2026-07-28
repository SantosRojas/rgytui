#!/usr/bin/env bash
#
# rgytui installer — Linux / macOS
#
# Downloads pre-built binary from GitHub Releases.
# Falls back to source build with --build-from-source flag.
#
# Usage:
#   curl -fsSL <url> | bash
#   ./scripts/install.sh [--build-from-source] [--nightly] [--force] [--help]

set -euo pipefail

REPO_OWNER="SantosRojas"
REPO_NAME="rgytui"
REPO_URL="${RGYTUI_REPO:-https://github.com/${REPO_OWNER}/${REPO_NAME}.git}"
INSTALL_ROOT="${RGYTUI_HOME:-$HOME/.local/share/rgytui}"
REPO_DIR="$INSTALL_ROOT/repo"
BIN_DIR="$HOME/.local/bin"
RGYTUI_BIN="$BIN_DIR/rgytui"

# ── Flags ─────────────────────────────────────────────────────────────────────

BUILD_FROM_SOURCE=false
NIGHTLY=false
FORCE=false

# ── Platform detection ────────────────────────────────────────────────────────

detect_os() {
    case "$(uname -s)" in
        Linux)  echo "linux" ;;
        Darwin) echo "macos" ;;
        *)      echo "" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64) echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *) echo "" ;;
    esac
}

detect_pkg_manager() {
    if command -v apt &>/dev/null; then echo "apt"
    elif command -v dnf &>/dev/null; then echo "dnf"
    elif command -v pacman &>/dev/null; then echo "pacman"
    elif command -v brew &>/dev/null; then echo "brew"
    else echo ""; fi
}

# ── Package managers ──────────────────────────────────────────────────────────

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

# ── Runtime dependencies only ─────────────────────────────────────────────────

install_runtime_deps() {
    if [ "$OS" = "linux" ]; then
        local pkg
        pkg=$(detect_pkg_manager)
        case "$pkg" in
            apt)    sudo apt update; sudo apt install -y yt-dlp mpv ;;
            dnf)    sudo dnf install -y yt-dlp mpv ;;
            pacman) sudo pacman -S --needed yt-dlp mpv ;;
            *)      echo ":: WARNING: unsupported package manager. Install yt-dlp and mpv manually." ;;
        esac
    elif [ "$OS" = "macos" ]; then
        if command -v brew &>/dev/null; then
            brew install yt-dlp mpv
        else
            echo ":: WARNING: Homebrew not found. Install yt-dlp and mpv manually."
        fi
    fi
}

# ── Ensure Rust is installed ─────────────────────────────────────────────────

ensure_rust() {
    if command -v cargo &>/dev/null; then
        echo ":: Rust already installed (cargo found)."
        return
    fi
    echo ":: Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    . "$HOME/.cargo/env"
    echo "  ✓ Rust installed."
}

# ── Build from source ────────────────────────────────────────────────────────

build_rgytui() {
    if [ ! -d "$REPO_DIR" ]; then
        echo ":: Cloning rgytui into $REPO_DIR..."
        mkdir -p "$INSTALL_ROOT"
        git clone "$REPO_URL" "$REPO_DIR"
    else
        echo ":: Repository already exists at $REPO_DIR, updating..."
        git -C "$REPO_DIR" fetch
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

# ── Pre-built binary download ────────────────────────────────────────────────

get_target_triple() {
    local os="$1" arch="$2"
    if [ "$os" = "linux" ]; then
        echo "x86_64-unknown-linux-gnu"
    elif [ "$os" = "macos" ]; then
        if [ "$arch" = "x86_64" ]; then
            echo "x86_64-apple-darwin"
        elif [ "$arch" = "aarch64" ]; then
            echo "aarch64-apple-darwin"
        fi
    fi
}

download_prebuilt() {
    local os="$1" arch="$2"
    local target
    target=$(get_target_triple "$os" "$arch")
    local archive_name="rgytui-${target}.tar.gz"
    local release_url

    if [ "$NIGHTLY" = true ]; then
        release_url="https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/tags/nightly"
        echo ":: Querying nightly release..."
    else
        release_url="https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/latest"
        echo ":: Querying latest release..."
    fi

    local release_json
    release_json=$(curl -fsSL "$release_url" 2>/dev/null || true)
    if [ -z "$release_json" ]; then
        echo "ERROR: Failed to query GitHub API."
        echo "       No release found or rate limit exceeded."
        echo "       Try building from source:"
        echo "         $0 --build-from-source"
        exit 1
    fi

    local download_url
    download_url=$(echo "$release_json" | grep -o '"browser_download_url": "[^"]*"' | grep "$archive_name" | head -1 | cut -d'"' -f4)

    if [ -z "$download_url" ]; then
        echo "ERROR: Could not find asset '$archive_name' in the release."
        echo "       Available assets:"
        echo "$release_json" | grep -o '"browser_download_url": "[^"]*"' | cut -d'"' -f4 | sed 's/^/         - /'
        echo ""
        echo "       Try building from source:"
        echo "         $0 --build-from-source"
        exit 1
    fi

    echo ":: Downloading $archive_name..."
    local tmpdir
    tmpdir=$(mktemp -d)
    curl -fsSL "$download_url" -o "$tmpdir/$archive_name"
    echo "  ✓ Downloaded."

    echo ":: Extracting..."
    mkdir -p "$BIN_DIR"
    tar xzf "$tmpdir/$archive_name" -C "$BIN_DIR"
    chmod +x "$RGYTUI_BIN"
    rm -rf "$tmpdir"
    echo "  ✓ Installed to $RGYTUI_BIN"
}

# ── Usage ─────────────────────────────────────────────────────────────────────

usage() {
    cat <<EOF
Usage: $0 [options]

Options:
  --build-from-source   Build rgytui from source instead of downloading a binary
  --nightly             Download the nightly build instead of the latest release
  --force               Overwrite existing installation without prompting
  --help                Show this help message

Environment:
  RGYTUI_REPO           Git repository URL (default: $REPO_URL)
  RGYTUI_HOME           Install root directory (default: $INSTALL_ROOT)
EOF
}

# ── Parse flags ──────────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
    case "$1" in
        --build-from-source) BUILD_FROM_SOURCE=true; shift ;;
        --nightly)           NIGHTLY=true; shift ;;
        --force)             FORCE=true; shift ;;
        --help)              usage; exit 0 ;;
        *)                   echo "ERROR: unknown flag '$1'. Use --help for usage."; exit 1 ;;
    esac
done

# ── Main ──────────────────────────────────────────────────────────────────────

OS=$(detect_os)
ARCH=$(detect_arch)

if [ -z "$OS" ]; then
    echo "ERROR: unsupported OS ($(uname -s)). This script is for Linux and macOS."
    echo "       Try the Windows installer (install.ps1) or build from source."
    exit 1
fi

# ── Build from source ────────────────────────────────────────────────────────

if [ "$BUILD_FROM_SOURCE" = true ]; then
    echo "=== rgytui installer (source build) ==="
    echo ""

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
    fi

    ensure_rust
    build_rgytui
    exit 0
fi

# ── Pre-built download ────────────────────────────────────────────────────────

echo "=== rgytui installer (pre-built) ==="
echo ""

if [ -z "$ARCH" ]; then
    echo "ERROR: unsupported architecture ($(uname -m))."
    echo "       Supported: x86_64 (amd64), aarch64 (arm64)."
    echo "       Try building from source:"
    echo "         $0 --build-from-source"
    exit 1
fi

# Check for existing installation
if [ -f "$RGYTUI_BIN" ] && [ "$FORCE" = false ]; then
    if [ -t 0 ]; then
        echo ":: rgytui is already installed at $RGYTUI_BIN"
        read -r -p "  Overwrite? [y/N] " response
        case "$response" in
            [yY][eE][sS]|[yY]) ;;
            *)
                echo "  Installation cancelled."
                exit 0
                ;;
        esac
    else
        echo "ERROR: rgytui is already installed at $RGYTUI_BIN."
        echo "       Re-run with --force to overwrite:"
        echo "         $0 --force"
        exit 1
    fi
fi

download_prebuilt "$OS" "$ARCH"

echo ":: Installing runtime dependencies..."
install_runtime_deps "$OS"

echo ""
echo "✓ rgytui installed successfully!"
echo "  Binary: $RGYTUI_BIN"
echo ""
echo "  Make sure $BIN_DIR is in your PATH."
echo "  Then run 'rgytui' to start."
