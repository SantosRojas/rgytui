//! Self-update logic for rgytui.
//!
//! `rgytui update` pulls the latest source, builds it, and replaces the
//! installed binary.  The install location is determined by:
//!
//! 1. `RGYTUI_HOME` environment variable, or
//! 2. A well-known default:
//!    - Linux/macOS: `~/.local/share/rgytui/`
//!    - Windows: `%LOCALAPPDATA%\rgytui`

use std::path::{Path, PathBuf};
use std::process::Command;

/// Run `rgytui update` — pull, build, install.
pub fn run_update() -> Result<(), anyhow::Error> {
    let home = install_home();
    let repo = home.join("repo");

    if !repo.join(".git").exists() {
        anyhow::bail!(
            "Repository not found at {}.\n\
             Install rgytui first: see https://github.com/rojasape/rgytui",
            repo.display()
        );
    }

    // ── 1. git fetch + pull ────────────────────────────────────────────────
    eprintln!(":: Updating source...");
    run_git(&["fetch", "--ff-only"], &repo)?;
    run_git(&["pull", "--ff-only"], &repo)?;

    let head = run_git_capture(&["rev-parse", "--short", "HEAD"], &repo)?;
    eprintln!("   master @ {head}");

    // ── 2. cargo build ─────────────────────────────────────────────────────
    eprintln!(":: Building rgytui (release)...");
    let status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(&repo)
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to run cargo: {e}"))?;

    if !status.success() {
        anyhow::bail!("cargo build failed");
    }

    // ── 3. Install binary ─────────────────────────────────────────────────
    let target_name = if cfg!(windows) { "rgytui.exe" } else { "rgytui" };
    let src = repo.join("target").join("release").join(target_name);
    let bin_dir = home.join("bin");
    let dst = bin_dir.join(target_name);

    std::fs::create_dir_all(&bin_dir)?;

    // On Windows we may not be able to overwrite a running binary.
    // Try a normal copy first, then a delayed rename via batch file.
    if let Err(e) = std::fs::copy(&src, &dst) {
        if cfg!(windows) {
            eprintln!(":: Direct copy failed ({e}), scheduling delayed replace...");
            schedule_delayed_replace(&src, &dst)?;
            eprintln!("✓ Build complete. Restart rgytui to use the new version.");
        } else {
            anyhow::bail!("Failed to install binary: {e}");
        }
    } else {
        eprintln!("✓ rgytui updated to {head}.");
        eprintln!("  Next launch will use the new version.");
    }

    Ok(())
}

/// Default install root for the current platform.
fn install_home() -> PathBuf {
    if let Ok(val) = std::env::var("RGYTUI_HOME") {
        return PathBuf::from(val);
    }
    default_install_home()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn default_install_home() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".local/share/rgytui")
}

#[cfg(windows)]
fn default_install_home() -> PathBuf {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| "C:\\".into());
    PathBuf::from(local).join("rgytui")
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn default_install_home() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".local/share/rgytui")
}

// ── Git helpers ──────────────────────────────────────────────────────────────

fn run_git(args: &[&str], cwd: &Path) -> Result<(), anyhow::Error> {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to run git: {e}"))?;

    if !status.success() {
        anyhow::bail!("git {} failed", args.join(" "));
    }
    Ok(())
}

fn run_git_capture(args: &[&str], cwd: &Path) -> Result<String, anyhow::Error> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run git: {e}"))?;

    if !output.status.success() {
        anyhow::bail!("git {} failed", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// ── Windows delayed replace ──────────────────────────────────────────────────

/// On Windows, a running .exe cannot be overwritten. We write a small cmd
/// script that waits a few seconds, copies, then self-destructs.
#[cfg(windows)]
fn schedule_delayed_replace(src: &Path, dst: &Path) -> Result<(), anyhow::Error> {
    let update_script = dst.with_extension("update.cmd");
    let src_s = src.display();
    let dst_s = dst.display();
    let content = format!(
        "@echo off\r\n\
         timeout /t 2 /nobreak >nul\r\n\
         copy /y \"{src_s}\" \"{dst_s}\" >nul\r\n\
         if errorlevel 1 (\r\n\
             echo Failed to update. Close all rgytui instances and try again.\r\n\
             pause\r\n\
         ) else (\r\n\
             echo ✓ rgytui updated.\r\n\
         )\r\n\
         del \"%~f0\"\r\n"
    );
    std::fs::write(&update_script, content)?;

    // Spawn detached — don't wait for it
    let _ = Command::new("cmd")
        .args(["/c", "start", "/b", "", &update_script.to_string_lossy()])
        .spawn();

    Ok(())
}

/// Stub for non-Windows platforms — never called.
#[cfg(not(windows))]
fn schedule_delayed_replace(_src: &Path, _dst: &Path) -> Result<(), anyhow::Error> {
    unreachable!()
}
