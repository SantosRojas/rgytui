//! Shared path resolution for install / update / uninstall.
//!
//! The rgytui binary ALWAYS lives where the installer scripts put it:
//! `~/.local/bin` on Linux/macOS and `%LOCALAPPDATA%\rgytui\bin` on
//! Windows. `RGYTUI_HOME` overrides the data/home directory only (repo
//! clones, caches) — never the binary location, because the installers
//! do not relocate the binary for it.

use std::path::{Path, PathBuf};

/// Directory where the rgytui binary lives. Matches the installer scripts.
pub fn bin_dir() -> PathBuf {
    default_bin_dir()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn default_bin_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    unix_bin_dir(Path::new(&home))
}

#[cfg(windows)]
fn default_bin_dir() -> PathBuf {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| "C:\\".into());
    windows_bin_dir(Path::new(&local))
}

/// Fallback for targets that are not Linux, macOS, or Windows (e.g.
/// FreeBSD): keep the unix-style `~/.local/bin` layout. Implemented
/// inline (not via [`unix_bin_dir`]) because that helper is cfg-gated to
/// Linux/macOS and would not compile on these targets.
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn default_bin_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    Path::new(&home).join(".local/bin")
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn unix_bin_dir(home: &Path) -> PathBuf {
    home.join(".local/bin")
}

#[cfg(windows)]
pub fn windows_bin_dir(local: &Path) -> PathBuf {
    local.join("rgytui").join("bin")
}
