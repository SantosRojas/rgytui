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

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn default_bin_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    unix_bin_dir(Path::new(&home))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn unix_bin_dir(home: &Path) -> PathBuf {
    home.join(".local/bin")
}

#[cfg(windows)]
pub fn windows_bin_dir(local: &Path) -> PathBuf {
    local.join("rgytui").join("bin")
}
