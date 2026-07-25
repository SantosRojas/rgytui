#!/usr/bin/env bash
#
# Install system dependencies required to build rgytui on Linux.
#
# Supported distributions:
#   - Debian / Ubuntu / Pop!_OS / Mint / WSL (apt)
#   - Fedora / RHEL / CentOS (dnf)
#   - Arch / Manjaro / EndeavourOS (pacman)
#
# Run this once before `cargo build`:
#   ./scripts/install-linux-deps.sh

set -euo pipefail

detect_pkg_manager() {
    if command -v apt &>/dev/null; then
        echo "apt"
    elif command -v dnf &>/dev/null; then
        echo "dnf"
    elif command -v pacman &>/dev/null; then
        echo "pacman"
    else
        echo ""
    fi
}

install_apt() {
    echo ":: Detected apt (Debian/Ubuntu)"
    sudo apt update
    sudo apt install -y \
        pkg-config \
        libwayland-dev \
        libxkbcommon-dev \
        libasound2-dev \
        libgtk-3-dev
}

install_dnf() {
    echo ":: Detected dnf (Fedora/RHEL)"
    sudo dnf install -y \
        pkg-config \
        wayland-devel \
        libxkbcommon-devel \
        alsa-lib-devel \
        gtk3-devel
}

install_pacman() {
    echo ":: Detected pacman (Arch)"
    sudo pacman -S --needed \
        pkg-config \
        wayland \
        libxkbcommon \
        alsa-lib \
        gtk3
}

# ── main ──────────────────────────────────────────────────────────────────────

case "$(detect_pkg_manager)" in
    apt)    install_apt ;;
    dnf)    install_dnf ;;
    pacman) install_pacman ;;
    *)
        echo "ERROR: unsupported package manager."
        echo ""
        echo "Install the following packages manually:"
        echo "  - pkg-config"
        echo "  - wayland client dev headers"
        echo "  - libxkbcommon dev headers"
        echo "  - alsa (libasound) dev headers"
        echo "  - GTK3 dev headers (optional, for file dialogs)"
        exit 1
        ;;
esac

echo ""
echo "✓ Linux system dependencies installed."
echo "  Run 'cargo build' to compile rgytui."
