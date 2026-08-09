//! Self-update logic for rgytui.
//!
//! Internal helpers used by the TUI upgrade popup. The app checks for new
//! versions at startup and offers to upgrade through a modal dialog.
//!
//! The binary location always matches the installer scripts:
//! `~/.local/bin` on Linux/macOS, `%LOCALAPPDATA%\rgytui\bin` on Windows.
//! `RGYTUI_HOME` overrides only the data/home directory (repo clones,
//! caches) — never the binary location, because the installers do not
//! relocate the binary for it.

use std::io::Read;
use std::path::Path;

use crate::paths::bin_dir;

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
    let content = delayed_replace_script(staging, dst, &log);
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

/// Build the content of the delayed-replace `.cmd` script (pure, no I/O).
#[cfg(windows)]
fn delayed_replace_script(staging: &Path, dst: &Path, log: &Path) -> String {
    let staging_s = staging.display();
    let dst_s = dst.display();
    let log_s = log.display();
    format!(
        "@echo off\r\n\
         title rgytui update\r\n\
         echo [%date% %time%] Waiting for rgytui to exit... > \"{log_s}\"\r\n\
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
    )
}

/// Stub for non-Windows platforms.
#[cfg(not(windows))]
fn schedule_delayed_replace(_staging: &Path, _dst: &Path) -> Result<(), anyhow::Error> {
    unreachable!()
}

/// Windows-only: sanity-check a staged update before rescheduling the
/// delayed replace. A crash or a failed `std::fs::write` (e.g. disk full)
/// can leave a 0-byte or partial `rgytui.new.exe`; copying that over the
/// live binary would brick the next launch. We require a plausible size
/// (never smaller than half of the live binary, which a real release
/// update is not) and a valid PE header ("MZ" DOS stub and "PE\0\0"
/// signature at the `e_lfanew` offset), matching how the updater produces
/// staging.
#[cfg(windows)]
fn staging_is_valid(staging: &Path, dst: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = match std::fs::File::open(staging) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    // A real rgytui.exe is hundreds of KB. Reject anything far smaller
    // than the binary it would replace (covers 0-byte, tiny, and partial
    // files that still carry a header-complete first chunk) and enforce
    // an absolute floor so an update never replaces a healthy binary with
    // a truncated image.
    let dst_size = std::fs::metadata(dst).map(|m| m.len()).unwrap_or(0);
    if size < (dst_size / 2).max(1024) {
        return false;
    }
    let mut dos = [0u8; 0x40];
    if file.read_exact(&mut dos).is_err() || &dos[0..2] != b"MZ" {
        return false;
    }
    let pe_offset =
        u64::from(u32::from_le_bytes([dos[0x3C], dos[0x3D], dos[0x3E], dos[0x3F]]));
    if pe_offset == 0 || pe_offset + 4 > size {
        return false;
    }
    if file.seek(SeekFrom::Start(pe_offset)).is_err() {
        return false;
    }
    let mut sig = [0u8; 4];
    !file.read_exact(&mut sig).is_err() && &sig == b"PE\0\0"
}

/// Windows-only: if a previous upgrade left a staging file behind (the
/// 300s deferred path or a copy that never ran), reschedule the delayed
/// replace so the next exit applies it. Invalid staging (0-byte, partial,
/// implausibly small, or missing the PE header) is removed instead of
/// being applied, so a corrupt leftover can never clobber the live
/// binary. No-op on Unix, where the binary is replaced in place.
pub fn resume_pending_upgrade() -> Result<(), anyhow::Error> {
    #[cfg(windows)]
    {
        let dst = bin_dir().join(binary_name());
        let staging = dst.with_extension("new.exe");
        if staging.exists() {
            if !staging_is_valid(&staging, &dst) {
                tracing::warn!(
                    "Ignoring invalid pending upgrade staging file {}; removing it",
                    staging.display()
                );
                if let Err(e) = std::fs::remove_file(&staging) {
                    tracing::warn!(
                        "Failed to remove invalid staging file {}: {e}; it will be re-checked next launch",
                        staging.display()
                    );
                }
                return Ok(());
            }
            tracing::info!("Pending upgrade staging found, rescheduling delayed replace...");
            schedule_delayed_replace(&staging, &dst)?;
        }
    }
    Ok(())
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

    #[cfg(windows)]
    #[test]
    fn delayed_replace_script_waits_for_exit_then_copies() {
        let script = delayed_replace_script(
            Path::new(r"C:\rgytui\rgytui.new.exe"),
            Path::new(r"C:\rgytui\rgytui.exe"),
            Path::new(r"C:\rgytui\rgytui.update.log"),
        );
        // Core contract: wait for the process to exit before copying.
        assert!(
            script.contains("tasklist /FI \"IMAGENAME eq rgytui.exe\""),
            "script must wait for rgytui.exe to exit"
        );
        assert!(script.contains(":wait"), "script must have a wait loop");
        assert!(script.contains("goto wait"), "script must loop while running");
        // Deferred limit: 300 attempts, then give up for this session.
        assert!(script.contains("set /a tries"), "script must count attempts");
        assert!(script.contains("geq 300"), "script must cap the wait at 300 tries");
        assert!(script.contains(":deferred"), "script must have a deferred path");
        // Copy must target the given staging and destination paths.
        assert!(
            script.contains("copy /y \"C:\\rgytui\\rgytui.new.exe\""),
            "script must copy staging over the destination"
        );
        // Structure: labels, self-delete, no user interaction.
        for needle in [":copy", ":done", "del \"%~f0\""] {
            assert!(script.contains(needle), "script should contain {needle:?}");
        }
        assert!(!script.contains("pause"), "script must not pause waiting on the user");
        assert!(
            !script.contains("echo ✓"),
            "script must not contain an un-redirected checkmark echo"
        );
    }

    #[cfg(windows)]
    mod staging_is_valid_tests {
        use super::*;
        use std::path::PathBuf;

        fn write_temp(name: &str, bytes: &[u8]) -> PathBuf {
            let path = std::env::temp_dir().join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        }

        fn pe_bytes(size: usize) -> Vec<u8> {
            let mut bytes = vec![0u8; size];
            bytes[0..2].copy_from_slice(b"MZ");
            // e_lfanew at 0x3C -> point just past the DOS header
            bytes[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
            bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
            bytes
        }

        #[test]
        fn rejects_missing_file() {
            let path = std::env::temp_dir().join("rgytui-absent.new.exe");
            let dst = std::env::temp_dir().join("rgytui-absent-dst.exe");
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(&dst);
            assert!(!staging_is_valid(&path, &dst));
        }

        #[test]
        fn rejects_zero_byte_file() {
            let path = write_temp("rgytui-empty.new.exe", &[]);
            let dst = write_temp("rgytui-empty-dst.exe", &pe_bytes(4096));
            assert!(!staging_is_valid(&path, &dst));
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(dst);
        }

        #[test]
        fn rejects_tiny_file() {
            let path = write_temp("rgytui-tiny.new.exe", b"MZ");
            let dst = write_temp("rgytui-tiny-dst.exe", &pe_bytes(4096));
            assert!(!staging_is_valid(&path, &dst));
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(dst);
        }

        #[test]
        fn rejects_missing_pe_signature() {
            let mut bytes = vec![0u8; 2048];
            bytes[0..2].copy_from_slice(b"MZ");
            bytes[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
            // No "PE\0\0" signature at 0x80
            let path = write_temp("rgytui-nosig.new.exe", &bytes);
            let dst = write_temp("rgytui-nosig-dst.exe", &pe_bytes(4096));
            assert!(!staging_is_valid(&path, &dst));
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(dst);
        }

        #[test]
        fn rejects_partial_file_with_header_complete_first_chunk() {
            // Simulates a crash mid-write: the staging is far smaller than
            // the live binary but still carries MZ + e_lfanew + PE\0\0 in
            // its first bytes. The size guard must reject it.
            let path = write_temp("rgytui-partial.new.exe", &pe_bytes(2048));
            let dst = write_temp("rgytui-partial-dst.exe", &pe_bytes(8192));
            assert!(!staging_is_valid(&path, &dst));
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(dst);
        }

        #[test]
        fn rejects_partial_file_shorter_than_absolute_floor() {
            // A header-complete staging below the 1 KB floor is rejected
            // even when the dst is missing (fresh install edge case).
            let path = write_temp("rgytui-floor.new.exe", &pe_bytes(512));
            let dst = std::env::temp_dir().join("rgytui-floor-dst.exe");
            let _ = std::fs::remove_file(&dst);
            assert!(!staging_is_valid(&path, &dst));
            let _ = std::fs::remove_file(path);
        }

        #[test]
        fn accepts_valid_pe_file() {
            let path = write_temp("rgytui-valid.new.exe", &pe_bytes(4096));
            let dst = write_temp("rgytui-valid-dst.exe", &pe_bytes(4096));
            assert!(staging_is_valid(&path, &dst));
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(dst);
        }
    }
}
