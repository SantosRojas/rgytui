//! Self-update logic for rgytui.
//!
//! Internal helpers used by the TUI upgrade popup. The app checks for new
//! versions at startup and offers to upgrade through a modal dialog.
//!
//! The binary location is determined by:
//!
//! 1. `RGYTUI_HOME` environment variable (binary in `$RGYTUI_HOME/bin`), or
//! 2. A well-known default that matches the installer scripts:
//!    - Linux/macOS: `~/.local/bin/`
//!    - Windows: `%LOCALAPPDATA%\rgytui\bin`

use std::io::Read;
use std::path::{Path, PathBuf};

const GH_OWNER: &str = "SantosRojas";
const GH_REPO: &str = "rgytui";
const UA: &str = concat!("rgytui-updater/", env!("CARGO_PKG_VERSION"));

/// Check the latest release on GitHub and return `(tag_name, download_url)`
/// for the current platform. Returns `None` if already up to date.
pub fn check_latest() -> Result<Option<(String, String)>, anyhow::Error> {
    let asset_name = detect_asset_name()?;
    let release = fetch_latest_release()?;

    // Skip if already at latest version
    if let Ok(current) = current_version()
        && current == release.tag_name
    {
        return Ok(None);
    }

    // Find matching asset for this platform
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No binary found for your platform ({}) in release {}",
                asset_name,
                release.tag_name
            )
        })?;

    Ok(Some((release.tag_name, asset.browser_download_url.clone())))
}

/// Download, extract, and install the binary from a given release URL.
pub fn perform_upgrade(version: &str, url: &str) -> Result<(), anyhow::Error> {
    let bin_dir = bin_dir();
    std::fs::create_dir_all(&bin_dir)?;

    let target_name = binary_name();
    let dst = bin_dir.join(&target_name);
    let asset_name = detect_asset_name()?;
    let ext = asset_extension(&asset_name);

    let archive_bytes = download(url)?;
    let binary_bytes = extract_binary(&archive_bytes, ext, &target_name)?;
    install_binary(&binary_bytes, &dst, version)?;

    Ok(())
}

// ── Platform detection ────────────────────────────────────────────────────────

fn detect_asset_name() -> Result<String, anyhow::Error> {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    match os {
        "linux" => match arch {
            "x86_64" => Ok("rgytui-x86_64-unknown-linux-gnu.tar.gz".into()),
            _ => anyhow::bail!("Unsupported architecture '{arch}' for Linux. Only x86_64 is available."),
        },
        "macos" => match arch {
            "x86_64" => Ok("rgytui-x86_64-apple-darwin.tar.gz".into()),
            "aarch64" => Ok("rgytui-aarch64-apple-darwin.tar.gz".into()),
            _ => anyhow::bail!("Unsupported architecture '{arch}' for macOS. Only x86_64 and aarch64 are available."),
        },
        "windows" => match arch {
            "x86_64" => Ok("rgytui-x86_64-pc-windows-msvc.zip".into()),
            _ => anyhow::bail!("Unsupported architecture '{arch}' for Windows. Only x86_64 is available."),
        },
        _ => anyhow::bail!("Unsupported OS '{os}'. Only Linux, macOS, and Windows are supported."),
    }
}

fn asset_extension(name: &str) -> &str {
    if name.ends_with(".tar.gz") {
        ".tar.gz"
    } else if name.ends_with(".zip") {
        ".zip"
    } else {
        ""
    }
}

fn binary_name() -> String {
    if cfg!(windows) {
        "rgytui.exe".into()
    } else {
        "rgytui".into()
    }
}

/// Current version from Cargo.toml — lets us skip a download when already current.
fn current_version() -> Result<String, anyhow::Error> {
    Ok(format!("v{}", env!("CARGO_PKG_VERSION")))
}

// ── GitHub API ────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn fetch_latest_release() -> Result<Release, anyhow::Error> {
    let url = format!(
        "https://api.github.com/repos/{GH_OWNER}/{GH_REPO}/releases/latest"
    );
    let mut response = ureq::get(&url)
        .header("User-Agent", UA)
        .header("Accept", "application/json")
        .call()
        .map_err(|e| anyhow::anyhow!("Failed to fetch latest release: {e}"))?;

    if response.status() != 200 {
        let body = response
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|_| "<no body>".into());
        anyhow::bail!("GitHub API returned HTTP {}:\n{}", response.status(), body);
    }

    let release: Release = response
        .body_mut()
        .read_json()
        .map_err(|e| anyhow::anyhow!("Failed to parse release JSON: {e}"))?;

    Ok(release)
}

fn download(url: &str) -> Result<Vec<u8>, anyhow::Error> {
    let mut response = ureq::get(url)
        .header("User-Agent", UA)
        .call()
        .map_err(|e| anyhow::anyhow!("Failed to download {url}: {e}"))?;

    if response.status() != 200 {
        anyhow::bail!("Download returned HTTP {}", response.status());
    }

    let body = response
        .body_mut()
        .with_config()
        .limit(100 * 1024 * 1024) // 100 MB safety limit
        .read_to_vec()
        .map_err(|e| anyhow::anyhow!("Failed to read download: {e}"))?;

    Ok(body)
}

// ── Extraction ────────────────────────────────────────────────────────────────

fn extract_binary(
    archive: &[u8],
    ext: &str,
    binary_name: &str,
) -> Result<Vec<u8>, anyhow::Error> {
    match ext {
        ".tar.gz" => extract_from_tar_gz(archive, binary_name),
        ".zip" => extract_from_zip(archive, binary_name),
        _ => anyhow::bail!("Unknown archive format '{ext}'"),
    }
}

fn extract_from_tar_gz(archive: &[u8], binary_name: &str) -> Result<Vec<u8>, anyhow::Error> {
    let decoder = flate2::read::GzDecoder::new(archive);
    let mut tar = tar::Archive::new(decoder);
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        if path
            .file_name()
            .map(|n| n == binary_name)
            .unwrap_or(false)
        {
            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }
    anyhow::bail!("Binary '{binary_name}' not found in archive")
}

fn extract_from_zip(archive: &[u8], binary_name: &str) -> Result<Vec<u8>, anyhow::Error> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive))
        .map_err(|e| anyhow::anyhow!("Failed to read zip archive: {e}"))?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let name = file.name();
        let fname = std::path::Path::new(name)
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        if fname.as_ref() == binary_name {
            let mut buf = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }
    anyhow::bail!("Binary '{binary_name}' not found in zip archive")
}

// ── Install ───────────────────────────────────────────────────────────────────

fn install_binary(
    binary_bytes: &[u8],
    dst: &Path,
    version: &str,
) -> Result<(), anyhow::Error> {
    if let Err(e) = std::fs::write(dst, binary_bytes) {
        if cfg!(windows) {
            tracing::warn!("Direct write failed ({e}), scheduling delayed replace...");
            // Write to a staging file first, then schedule delayed copy
            let staging = dst.with_extension("new.exe");
            std::fs::write(&staging, binary_bytes)?;
            schedule_delayed_replace(&staging, dst)?;
            tracing::info!("Downloaded {version}. Restart rgytui to use it.");
        } else {
            anyhow::bail!("Failed to install binary to {}: {e}", dst.display());
        }
    } else {
        #[cfg(not(windows))]
        make_executable(dst)?;
        tracing::info!("rgytui updated to {version}.");
        tracing::info!("Next launch will use the new version.");
    }
    Ok(())
}

#[cfg(not(windows))]
fn make_executable(path: &Path) -> Result<(), anyhow::Error> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::metadata(path)?;
    let mut perms = metadata.permissions();
    let mode = perms.mode();
    perms.set_mode(mode | 0o111); // +x
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

// ── Windows delayed replace ───────────────────────────────────────────────────

/// On Windows, a running .exe cannot be overwritten. We write a small cmd
/// script that waits until the app process exits, then copies the staged
/// binary over the destination and self-destructs (including the staging
/// file). All output goes to a log file so nothing is painted over the TUI.
#[cfg(windows)]
fn schedule_delayed_replace(staging: &Path, dst: &Path) -> Result<(), anyhow::Error> {
    let update_script = dst.with_extension("update.cmd");
    let log = dst.with_extension("update.log");
    let staging_s = staging.display();
    let dst_s = dst.display();
    let log_s = log.display();
    let content = format!(
        "@echo off\r\n\
         title rgytui update\r\n\
         echo [%date% %time%] Waiting for rgytui to exit... >> \"{log_s}\"\r\n\
         set /a tries=0\r\n\
         :wait\r\n\
         tasklist /FI \"IMAGENAME eq rgytui.exe\" 2>nul | find /I \"rgytui.exe\" >nul\r\n\
         if errorlevel 1 goto copy\r\n\
         set /a tries+=1\r\n\
         if %tries% geq 300 goto deferred\r\n\
         timeout /t 1 /nobreak >nul\r\n\
         goto wait\r\n\
         :copy\r\n\
         copy /y \"{staging_s}\" \"{dst_s}\" >> \"{log_s}\" 2>&1\r\n\
         if errorlevel 1 goto failed\r\n\
         echo [%date% %time%] OK: rgytui updated. >> \"{log_s}\"\r\n\
         del \"{staging_s}\" >> \"{log_s}\" 2>&1\r\n\
         goto done\r\n\
         :failed\r\n\
         echo [%date% %time%] ERROR: Failed to update. Close all rgytui instances and try again. >> \"{log_s}\"\r\n\
         goto done\r\n\
         :deferred\r\n\
         echo [%date% %time%] WARN: rgytui still running after 300s; update deferred to next start. >> \"{log_s}\"\r\n\
         :done\r\n\
         del \"%~f0\"\r\n"
    );
    std::fs::write(&update_script, content)?;

    let spawn = std::process::Command::new("cmd")
        .args(["/c", &update_script.to_string_lossy()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Err(e) = spawn {
        anyhow::bail!(
            "Failed to launch update script {}: {e}",
            update_script.display()
        );
    }
    Ok(())
}

/// Stub for non-Windows platforms.
#[cfg(not(windows))]
fn schedule_delayed_replace(_staging: &Path, _dst: &Path) -> Result<(), anyhow::Error> {
    unreachable!()
}

// ── Binary directory discovery ────────────────────────────────────────────────

/// Directory where the rgytui binary lives. Must match the installer:
/// `~/.local/bin` on Linux/macOS, `%LOCALAPPDATA%\rgytui\bin` on Windows.
/// An explicit `RGYTUI_HOME` override redirects to `$RGYTUI_HOME/bin`.
fn bin_dir() -> PathBuf {
    if let Ok(val) = std::env::var("RGYTUI_HOME") {
        return PathBuf::from(val).join("bin");
    }
    default_bin_dir()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn default_bin_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".local/bin")
}

#[cfg(windows)]
fn default_bin_dir() -> PathBuf {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| "C:\\".into());
    PathBuf::from(local).join("rgytui").join("bin")
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn default_bin_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".local/bin")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_asset_linux_x86_64() {
        // We can't mock std::env::consts, but we can test the extension helper
        assert_eq!(asset_extension("rgytui-x86_64-unknown-linux-gnu.tar.gz"), ".tar.gz");
    }

    #[test]
    fn detect_asset_windows_x86_64() {
        assert_eq!(asset_extension("rgytui-x86_64-pc-windows-msvc.zip"), ".zip");
    }

    #[test]
    fn detect_asset_macos_arm() {
        assert_eq!(asset_extension("rgytui-aarch64-apple-darwin.tar.gz"), ".tar.gz");
    }

    #[test]
    fn detect_asset_macos_intel() {
        assert_eq!(asset_extension("rgytui-x86_64-apple-darwin.tar.gz"), ".tar.gz");
    }

    #[test]
    fn binary_name_is_platform_specific() {
        let name = binary_name();
        if cfg!(windows) {
            assert_eq!(name, "rgytui.exe");
        } else {
            assert_eq!(name, "rgytui");
        }
    }

    #[test]
    fn current_version_returns_v_prefix() {
        let v = current_version().unwrap();
        assert!(v.starts_with('v'), "version should start with 'v'");
        assert!(!v.is_empty());
    }

    #[test]
    fn extract_binary_from_zip_fails_on_empty_archive() {
        let empty: Vec<u8> = vec![];
        let result = extract_from_zip(&empty, "rgytui.exe");
        assert!(result.is_err());
    }

    #[test]
    fn extract_binary_from_tar_gz_fails_on_empty_archive() {
        let empty: Vec<u8> = vec![];
        let result = extract_from_tar_gz(&empty, "rgytui");
        assert!(result.is_err());
    }
}
