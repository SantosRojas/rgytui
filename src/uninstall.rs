//! Self-uninstall logic for rgytui.
//!
//! `rgytui uninstall` removes the installed binary, source repo, and
//! optionally cleans up PATH. On Windows it uses a delayed script because
//! you cannot delete a running `.exe`.
//!
//! Install location discovery follows the same rules as `update`:
//!
//! 1. `RGYTUI_HOME` environment variable, or
//! 2. Platform default:
//!    - Linux/macOS: `~/.local/share/rgytui/`
//!    - Windows: `%LOCALAPPDATA%\rgytui`

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

/// Run `rgytui uninstall` — remove binary, repo, and clean up PATH.
pub fn run_uninstall() -> Result<(), anyhow::Error> {
    let home = install_home();
    let bin_dir = home.join("bin");
    let repo_dir = home.join("repo");

    // Confirm
    eprintln!("This will remove rgytui and all its files from:");
    eprintln!("  {}", home.display());
    eprintln!();

    // Skip confirmation if --yes or -y is passed
    let args: Vec<String> = std::env::args().collect();
    let skip_prompt = args.iter().any(|a| a == "--yes" || a == "-y");
    if !skip_prompt {
        eprint!("Continue? [y/N] ");
        std::io::stderr().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if input.trim().to_lowercase() != "y" {
            eprintln!("Aborted.");
            return Ok(());
        }
    }

    // Ask about optional dependency removal (default no for both)
    let remove_ytdlp = if skip_prompt {
        false
    } else {
        eprint!("Remove yt-dlp? (used by rgytui for downloads) [y/N] ");
        std::io::stderr().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        input.trim().to_lowercase() == "y"
    };

    let remove_mpv = if skip_prompt {
        false
    } else {
        eprint!("Remove mpv? (used by rgytui for video playback) [y/N] ");
        std::io::stderr().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        input.trim().to_lowercase() == "y"
    };

    // Determine the binary name for the current platform
    let target_name = if cfg!(windows) { "rgytui.exe" } else { "rgytui" };
    let binary_path = bin_dir.join(target_name);

    #[cfg(windows)]
    {
        uninstall_windows(&home, &bin_dir, &repo_dir, &binary_path, remove_ytdlp, remove_mpv)?;
    }
    #[cfg(not(windows))]
    {
        uninstall_unix(&home, &bin_dir, &repo_dir, &binary_path, remove_ytdlp, remove_mpv)?;
    }

    Ok(())
}

// ── Windows: schedule delayed cleanup via .ps1 script ───────────────────────
//
// Instead of embedding PowerShell code inside cmd with fragile ^ escaping
// (cmd treats & | > < as special even inside "..." arguments), we write a
// standalone .ps1 file and call it with powershell -File. The .cmd wrapper
// is minimal: wait, run the .ps1, then self-delete.

#[cfg(windows)]
fn uninstall_windows(
    home: &PathBuf,
    bin_dir: &PathBuf,
    repo_dir: &PathBuf,
    _binary_path: &PathBuf,
    remove_ytdlp: bool,
    remove_mpv: bool,
) -> Result<(), anyhow::Error> {
    let home_s = home.display();
    let bin_s = bin_dir.display();
    let repo_s = repo_dir.display();

    // Build optional dependency removal blocks (PowerShell code)
    let dep_block = {
        let mut s = String::new();
        if remove_ytdlp {
            s.push_str(&format!(
                r"
Write-Host '  :: Removing yt-dlp...'
$ids = @('yt-dlp.yt-dlp','ytdlp.yt-dlp')
$removed = $false
foreach ($id in $ids) {{
    $r = & winget uninstall --id $id --silent --accept-source-agreements 2>&1
    if ($LASTEXITCODE -eq 0) {{ $removed = $true; break }}
}}
if (-not $removed -and (Test-Path '{bin_s}\yt-dlp.exe')) {{
    Remove-Item -Force '{bin_s}\yt-dlp.exe'
    Write-Host '  Removed yt-dlp.exe from bin dir'
}}
",
            ));
        }
        if remove_mpv {
            s.push_str(&format!(
                r"
Write-Host '  :: Removing mpv...'
$ids = @('shinchiro.mpv','mpv-player.mpv-CI.MSVC')
$removed = $false
foreach ($id in $ids) {{
    $r = & winget uninstall --id $id --silent --accept-source-agreements 2>&1
    if ($LASTEXITCODE -eq 0) {{ $removed = $true; break }}
}}
if (-not $removed) {{
    if (Test-Path '{bin_s}\mpv\mpv.com') {{ Remove-Item -Recurse -Force '{bin_s}\mpv'; Write-Host '  Removed mpv directory' }}
    if (Test-Path '{bin_s}\mpv.exe') {{ Remove-Item -Force '{bin_s}\mpv.exe'; Write-Host '  Removed mpv.exe' }}
}}
",
            ));
        }
        s
    };

    let ps1_script = format!(
        r##"
Write-Host ':: Waiting for rgytui to exit...'
Start-Sleep 3

Write-Host ':: Removing binary...'
Remove-Item -Force '{bin_s}\rgytui.exe' -ErrorAction SilentlyContinue

Write-Host ':: Removing repository...'
Remove-Item -Recurse -Force '{repo_s}' -ErrorAction SilentlyContinue
{dep_block}
Remove-Item -Recurse -Force '{home_s}' -ErrorAction SilentlyContinue

Write-Host ':: Cleaning PATH...'
$p = [Environment]::GetEnvironmentVariable('Path','User')
$p = ($p.Split(';') | Where-Object {{ $_ -ne '{bin_s}' -and $_ -ne '{bin_s}\mpv' }}) -join ';'
[Environment]::SetEnvironmentVariable('Path', $p, 'User')

Write-Host ''
Write-Host '✓ rgytui has been uninstalled.'
Write-Host '  You may need to restart your terminal for PATH changes.'
Start-Sleep 3
"##,
        bin_s = bin_s,
        repo_s = repo_s,
        home_s = home_s,
        dep_block = dep_block,
    );

    // Write cleanup .ps1 file (with BOM so PowerShell -File reads it as UTF-8)
    let ps1_path = std::env::temp_dir().join("rgytui-uninstall.ps1");
    let mut ps1_bytes = vec![0xEFu8, 0xBB, 0xBF]; // UTF-8 BOM
    ps1_bytes.extend_from_slice(ps1_script.as_bytes());
    std::fs::write(&ps1_path, ps1_bytes)?;

    // Write minimal .cmd wrapper: wait, run .ps1, self-delete
    let cmd_path = std::env::temp_dir().join("rgytui-uninstall.cmd");
    let cmd_content = format!(
        "@echo off\r\n\
         title rgytui uninstall\r\n\
         powershell -NoProfile -ExecutionPolicy RemoteSigned -File \"{ps1}\"\r\n\
         del \"%~f0\"\r\n",
        ps1 = ps1_path.display(),
    );
    std::fs::write(&cmd_path, cmd_content)?;

    // Spawn detached — don't wait for it
    let _ = Command::new("cmd")
        .args(["/c", "start", "/b", "", &cmd_path.to_string_lossy()])
        .spawn();

    eprintln!("✓ Uninstall scheduled. The binary will be removed after you exit.");
    eprintln!("  A cleanup window will appear briefly.");
    Ok(())
}

// ── Unix: delete immediately (Linux/macOS allow unlinking running binaries) ──

#[cfg(not(windows))]
fn uninstall_unix(
    home: &PathBuf,
    bin_dir: &PathBuf,
    repo_dir: &PathBuf,
    binary_path: &PathBuf,
    remove_ytdlp: bool,
    remove_mpv: bool,
) -> Result<(), anyhow::Error> {
    use std::fs;

    let binary_s = binary_path.display();
    let repo_s = repo_dir.display();
    let home_s = home.display();
    let bin_s = bin_dir.display();

    // Remove optional dependencies
    if remove_ytdlp {
        let ytdlp = bin_dir.join("yt-dlp");
        if ytdlp.exists() || ytdlp.is_symlink() {
            fs::remove_file(&ytdlp)?;
            eprintln!("  ✓ Removed yt-dlp");
        }
    }
    if remove_mpv {
        let mpv_bin = bin_dir.join("mpv");
        if mpv_bin.exists() {
            fs::remove_dir_all(&mpv_bin)?;
            eprintln!("  ✓ Removed mpv directory");
        } else {
            let mpv_exe = bin_dir.join("mpv");
            if mpv_exe.exists() || mpv_exe.is_symlink() {
                fs::remove_file(&mpv_exe)?;
                eprintln!("  ✓ Removed mpv");
            }
        }
    }

    // Remove binary (symlink or file)
    if binary_path.exists() || binary_path.is_symlink() {
        fs::remove_file(binary_path)?;
        eprintln!("  ✓ Removed {binary_s}");
    }

    // Remove repo
    if repo_dir.exists() {
        fs::remove_dir_all(repo_dir)?;
        eprintln!("  ✓ Removed {repo_s}");
    }

    // Remove the home directory (should now be empty)
    if home.exists() {
        // Only remove if it only contains the bin dir (or is empty)
        let remaining: Vec<_> = std::fs::read_dir(home)
            .unwrap_or_else(|_| panic!("Failed to read {}", home_s))
            .filter_map(|e| e.ok())
            .collect();
        if remaining.is_empty() {
            fs::remove_dir(home)?;
            eprintln!("  ✓ Removed {home_s}");
        } else {
            eprintln!("  ⚠  {home_s} still has files, not removed:");
            for entry in &remaining {
                eprintln!("     - {}", entry.path().display());
            }
        }
    }

    eprintln!();
    eprintln!("✓ rgytui uninstalled.");
    eprintln!("  If you added ~/.local/bin to your PATH manually, remove it there.");
    Ok(())
}

// ── Install home discovery (same logic as update.rs) ─────────────────────────

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
