mod app;
mod application;
mod domain;
mod infrastructure;
mod interface;
mod shared;
mod uninstall;
mod update;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Context;
use tracing_subscriber::EnvFilter;

use crate::app::App;
use crate::application::playback::PlaybackUseCase;
use crate::application::playlist::PlaylistUseCase;
use crate::application::ports::{AudioPlaybackPort, ConfigPort, DownloaderPort, I18nPort, MediaSearchPort};
use crate::application::search::SearchUseCase;
use crate::domain::audio_mode::AudioMode;
use crate::infrastructure::audio::mpv_backend::MpvAdapter;
use crate::infrastructure::audio::rodio_backend::{NoopAudioAdapter, RodioAdapter};
use crate::infrastructure::config::store::ConfigAdapter;
use crate::infrastructure::ytdlp::client::YtDlpAdapter;
use crate::interface::i18n::Translations;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Handle subcommands before starting the TUI
    let args: Vec<String> = std::env::args().collect();
    if let Some("uninstall") = args.get(1).map(|s| s.as_str()) {
        return crate::uninstall::run_uninstall();
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,symphonia=error")),
        )
        // The TUI owns stdout (alternate screen + raw mode), so logs must never
        // be written there: a warn/error mid-render would inject raw text into
        // the interface at the cursor position (e.g. inside the search input).
        // Redirect to a log file in the user config dir instead.
        .with_writer(Mutex::new(open_log_writer()))
        .with_ansi(false)
        .init();

    let mut config = ConfigAdapter::new().await.context("Failed to load config")?;
    let settings = config.settings().clone();
    let ytdlp = YtDlpAdapter::new();
    let audio: Box<dyn AudioPlaybackPort> = match RodioAdapter::new() {
        Ok(a) => Box::new(a),
        Err(e) => {
            tracing::warn!("Audio output unavailable ({}), running without sound. Use audio mode  \
                           to play audio once a device is available.", e);
            Box::new(NoopAudioAdapter)
        }
    };
    let mpv = MpvAdapter::new();

    let playlist = PlaylistUseCase::new();

    let downloader_port: Arc<dyn DownloaderPort> = Arc::new(ytdlp.clone());
    let search_port: Arc<dyn MediaSearchPort> = Arc::new(ytdlp);

    // Fall back to Audio if mpv is not installed (e.g. user had legacy config with audio_mode: true)
    // Also persist the corrected audio_mode to config so the warning goes away permanently.
    let initial_mode = if settings.audio_mode && !MpvAdapter::is_mpv_installed().await {
        tracing::warn!("Video mode configured but mpv is not installed. Falling back to Audio.");
        config.settings_mut().audio_mode = false;
        if let Err(e) = config.save_settings().await {
            tracing::warn!("Failed to persist corrected audio_mode: {}", e);
        }
        AudioMode::Audio
    } else {
        AudioMode::from_bool(settings.audio_mode)
    };
    let playback = PlaybackUseCase::new(downloader_port, audio, mpv, initial_mode);
    let search = SearchUseCase::new(search_port);
    let config_port: Box<dyn ConfigPort> = Box::new(config);

    // Determine language from settings, with system locale detection as default
    let language = if settings.language == "en" {
        Translations::detect_locale()
    } else {
        settings.language.clone()
    };
    let i18n: Arc<dyn I18nPort> = Arc::new(Translations::load(&language));

    let mut app = App::new(playback, search, playlist, config_port, i18n).await;

    match app.run().await {
        Ok(()) => {
            tracing::info!("Application exited cleanly");
            Ok(())
        }
        Err(e) => {
            tracing::error!("Application error: {}", e);
            eprintln!("Error: {}", e);
            Err(e)
        }
    }
}

/// Name of the log file inside the config dir.
const LOG_FILE_NAME: &str = "rgytui.log";
/// Name the log file is rotated to when it exceeds [`LOG_MAX_BYTES`].
const LOG_OLD_FILE_NAME: &str = "rgytui.log.old";
/// Cap for the log file; beyond this it is rotated so it cannot grow unboundedly.
const LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// Open (creating if needed) the log file the tracing subscriber writes to.
/// Logs live in the user config dir next to settings.json, so the TUI's
/// stdout stays pristine. On any failure fall back to the null device:
/// logging must never crash startup.
fn open_log_writer() -> Box<dyn std::io::Write + Send + Sync> {
    let config_dir = directories::ProjectDirs::from("com", "rgytui", "rgytui")
        .map(|d| d.config_dir().to_path_buf())
        .or_else(|| {
            // Same fallback as ConfigAdapter for sandboxed/container environments.
            std::env::current_dir()
                .ok()
                .map(|dir| dir.join(".rgytui"))
        });
    let Some(config_dir) = config_dir else {
        return Box::new(std::io::sink());
    };
    open_log_writer_in(&config_dir)
}

/// Open (creating if needed) the log file `rgytui.log` in `dir`, rotating a
/// too-large existing log to `rgytui.log.old` first. Returns a writer that also
/// rotates mid-session when the cap is exceeded; on any failure falls back to
/// the null device so logging never crashes startup.
fn open_log_writer_in(dir: &Path) -> Box<dyn std::io::Write + Send + Sync> {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("Warning: cannot create log directory {}: {e}", dir.display());
        return Box::new(std::io::sink());
    }
    let path = dir.join(LOG_FILE_NAME);
    // Startup rotation is still useful: it produces the .old file even when the
    // session never exceeds the cap again. This runs before the TUI takes raw
    // mode, so a stderr warning here cannot corrupt the interface.
    if let Err(e) = rotate_if_needed(&path, LOG_MAX_BYTES) {
        eprintln!("Warning: cannot rotate oversized log {}: {e}", path.display());
    }
    match RotatingLogWriter::open(path.clone()) {
        Ok(writer) => Box::new(writer),
        Err(e) => {
            eprintln!("Warning: cannot open log file {}: {e}", path.display());
            Box::new(std::io::sink())
        }
    }
}

/// Writer that rotates the active log file once [`LOG_MAX_BYTES`] have been
/// written during a session, so a long-lived TUI cannot grow the log
/// unboundedly. Best-effort: a failed rotation keeps appending to the current
/// file — logging must never crash or lose the active session's log.
struct RotatingLogWriter {
    file: Box<dyn std::io::Write + Send + Sync>,
    path: PathBuf,
    written: u64,
    max_bytes: u64,
    /// Latch set after a rotation attempt failed. Once set, no further rotation
    /// attempts are made: a persistent failure (e.g. another process locking
    /// the log) must not turn every log write into a rename retry storm, and
    /// must not spam stderr inside the raw-mode TUI.
    rotation_failed: bool,
}

impl RotatingLogWriter {
    /// Open an append-only writer for `path` with the production cap.
    fn open(path: PathBuf) -> std::io::Result<Self> {
        Self::open_with_cap(path, LOG_MAX_BYTES)
    }

    /// Open an append-only writer for `path` with an explicit cap (tests use a
    /// tiny cap so they stay fast).
    fn open_with_cap(path: PathBuf, max_bytes: u64) -> std::io::Result<Self> {
        let file = open_log_file(&path)?;
        let written = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            file: Box::new(file),
            path,
            written,
            max_bytes,
            rotation_failed: false,
        })
    }

    /// Rotate the active file through the shared rotation helper and reopen it
    /// in append mode. The current handle is closed first so the rename is
    /// allowed on Windows (open files are locked there). Best-effort: if the
    /// rotation fails the oversized file stays in place and we keep appending
    /// to it; if it cannot be reopened at all, writes fall back to the null
    /// device until the next retry. Either way logging never crashes. Only a
    /// genuine rename failure latches (so later writes do not re-attempt a
    /// failing rename); benign outcomes (no file, file below the cap) and a
    /// failed reopen are NOT latched — the reopen is re-attempted on later
    /// writes so a transient failure cannot silently drop the session's logs.
    /// Note: this runs inside the tracing writer (behind the subscriber's
    /// Mutex), so it must NEVER emit tracing events itself (deadlock) nor
    /// stderr text mid-session (the raw-mode TUI owns the terminal) — a failed
    /// rotation is reported silently and only once, via the latch.
    fn rotate(&mut self) {
        if self.rotation_failed {
            return;
        }
        let _ = self.file.flush();
        // Close the current handle before renaming (see the Windows note above).
        self.file = Box::new(std::io::sink());
        let rotated = rotate_if_needed(&self.path, self.max_bytes);
        match open_log_file(&self.path) {
            Ok(file) => {
                self.file = Box::new(file);
                self.written = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
            }
            Err(_) => {
                // Reopen failed (e.g. the rename left the file locked, or the
                // filesystem is unavailable). Keep the sink for now but force
                // the next write to retry the reopen: written is left at the
                // cap so the very next write re-enters rotate() and attempts
                // open_log_file again, silently, with no stderr.
                self.written = self.max_bytes;
            }
        }
        if rotated.is_err() {
            // Rename failed (e.g. another process holds the log open on
            // Windows). Do not retry on every subsequent write; keep appending
            // to the oversized file instead.
            self.rotation_failed = true;
        }
    }
}

impl std::io::Write for RotatingLogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write(buf)?;
        self.written += n as u64;
        if self.written >= self.max_bytes {
            self.rotate();
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

/// Open (creating if needed) `path` for appending, with a restrictive `0o600`
/// mode on POSIX so the log file is never world-readable (Windows ignores the
/// mode; the cfg gate keeps the OpenOptions building platform-neutral).
fn open_log_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

/// Outcome of a rotation attempt.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RotateOutcome {
    /// The oversized log was renamed to `rgytui.log.old`.
    Rotated,
    /// Nothing to rotate: no existing log, or it is below the cap.
    NothingToRotate,
}

/// Rotate `path` to `rgytui.log.old` when it exceeds `max_bytes`, so the log
/// cannot grow unboundedly. Best-effort: never panics. `std::fs::rename`
/// replaces an existing destination on every platform (on Windows it maps to
/// MoveFileExW MOVEFILE_REPLACE_EXISTING / FileRenameInfoEx), so a single
/// rename is all that is needed. Returns the error when the oversized file
/// could not be renamed, so callers can surface or latch on the actual cause.
fn rotate_if_needed(path: &Path, max_bytes: u64) -> std::io::Result<RotateOutcome> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(RotateOutcome::NothingToRotate); // no existing log yet
    };
    if meta.len() < max_bytes {
        return Ok(RotateOutcome::NothingToRotate);
    }
    let old = path.with_file_name(LOG_OLD_FILE_NAME);
    std::fs::rename(path, &old)?;
    Ok(RotateOutcome::Rotated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn open_log_writer_in_writes_to_expected_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut writer = open_log_writer_in(dir.path());
        writer.write_all(b"hello from the log\n").unwrap();
        drop(writer); // flush + close before reading back

        let content = std::fs::read_to_string(dir.path().join(LOG_FILE_NAME)).unwrap();
        assert!(content.contains("hello from the log"));
    }

    #[test]
    fn open_log_writer_in_appends_to_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LOG_FILE_NAME), "first line\n").unwrap();

        let mut writer = open_log_writer_in(dir.path());
        writer.write_all(b"second line\n").unwrap();
        drop(writer);

        let content = std::fs::read_to_string(dir.path().join(LOG_FILE_NAME)).unwrap();
        assert!(content.starts_with("first line\n"), "must append, not truncate");
        assert!(content.ends_with("second line\n"));
    }

    #[test]
    fn open_log_writer_in_rotates_oversized_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE_NAME);
        std::fs::write(&path, vec![0u8; (LOG_MAX_BYTES + 1) as usize]).unwrap();
        let old = dir.path().join(LOG_OLD_FILE_NAME);
        assert!(!old.exists(), "no .old before rotation");

        let mut writer = open_log_writer_in(dir.path());
        writer.write_all(b"fresh").unwrap();
        drop(writer);

        assert!(old.exists(), "oversized log should be rotated to .old");
        assert_eq!(
            std::fs::metadata(&old).unwrap().len(),
            LOG_MAX_BYTES + 1,
            ".old must hold the rotated bytes"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "fresh",
            "a fresh log file must be created for new writes"
        );
    }

    #[test]
    fn open_log_writer_in_keeps_undersized_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE_NAME);
        std::fs::write(&path, b"small").unwrap();
        let old = dir.path().join(LOG_OLD_FILE_NAME);

        open_log_writer_in(dir.path());

        assert!(path.exists(), "undersized log must stay in place");
        assert!(!old.exists(), "undersized log must NOT be rotated");
    }

    #[test]
    fn open_log_writer_in_rotation_overwrites_existing_old_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE_NAME);
        let old = dir.path().join(LOG_OLD_FILE_NAME);
        std::fs::write(&path, vec![0u8; (LOG_MAX_BYTES + 1) as usize]).unwrap();
        std::fs::write(&old, b"stale").unwrap();

        open_log_writer_in(dir.path());

        assert_eq!(
            std::fs::metadata(&old).unwrap().len(),
            LOG_MAX_BYTES + 1,
            "a stale .old must be overwritten by the rotation"
        );
    }

    #[test]
    fn open_log_writer_in_falls_back_to_sink_when_dir_creation_fails() {
        // A path whose parent is a regular FILE cannot be created as a
        // directory, so create_dir_all fails and the writer falls back to the
        // null device (write must still succeed silently).
        let base = tempfile::tempdir().unwrap();
        let blocker = base.path().join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let dir = blocker.join("nested");

        let mut writer = open_log_writer_in(&dir);
        assert!(writer.write_all(b"data").is_ok());
    }

    #[test]
    fn rotating_writer_rotates_mid_session_past_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE_NAME);
        let old = dir.path().join(LOG_OLD_FILE_NAME);
        let mut writer = RotatingLogWriter::open_with_cap(path.clone(), 8).unwrap();

        writer.write_all(b"1234567").unwrap(); // below cap: no rotation yet
        assert!(!old.exists(), "below the cap the file must not rotate");

        writer.write_all(b"8").unwrap(); // exactly at cap: rotates
        assert!(old.exists(), "at the cap the file must rotate");

        writer.write_all(b"9").unwrap(); // continues into a fresh file
        drop(writer);

        assert_eq!(
            std::fs::read_to_string(&old).unwrap(),
            "12345678",
            "the capped bytes must land in .old"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "9",
            "post-rotation writes must land in the fresh file"
        );
    }

    #[test]
    fn rotating_writer_rotates_at_exact_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE_NAME);
        let old = dir.path().join(LOG_OLD_FILE_NAME);
        let mut writer = RotatingLogWriter::open_with_cap(path.clone(), 4).unwrap();

        writer.write_all(b"abcd").unwrap(); // exactly at cap
        drop(writer);

        assert!(old.exists(), "a file exactly at the cap must be rotated");
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "abcd");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "",
            "the fresh file must be empty after rotation"
        );
    }

    #[test]
    fn rotating_writer_latches_after_failed_rotation_and_keeps_appending() {
        // Force the rotation rename to fail deterministically: rename of a
        // file onto an existing DIRECTORY fails on every platform.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE_NAME);
        let old = dir.path().join(LOG_OLD_FILE_NAME);
        std::fs::create_dir(&old).unwrap();

        let mut writer = RotatingLogWriter::open_with_cap(path.clone(), 4).unwrap();
        writer.write_all(b"abcd").unwrap(); // hits the cap; rename fails
        assert!(
            writer.rotation_failed,
            "a failed rotation must latch so later writes stop retrying"
        );

        writer.write_all(b"efgh").unwrap(); // must NOT re-attempt rotation
        assert!(
            writer.rotation_failed,
            "the latch must persist across subsequent writes"
        );
        drop(writer);

        assert!(
            old.is_dir(),
            "the blocked .old must not have been replaced"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "abcdefgh",
            "all writes must keep landing in the active log after a failed rotation"
        );
    }

    #[test]
    fn rotating_writer_does_not_latch_on_missing_or_undersized_log() {
        // A deleted (or externally truncated) log is a BENIGN state, not a
        // rotation failure: rotate_if_needed reports NothingToRotate and the
        // writer must keep attempting rotation later instead of latching.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE_NAME);
        let old = dir.path().join(LOG_OLD_FILE_NAME);

        let mut writer = RotatingLogWriter::open_with_cap(path.clone(), 4).unwrap();
        writer.write_all(b"abcd").unwrap(); // hits the cap; rename succeeds
        assert!(
            !writer.rotation_failed,
            "a successful rotation must not set the latch"
        );
        assert!(old.exists(), "oversized log must be rotated to .old");
        drop(writer);

        // Now simulate a mid-session deletion: the next cap crossing sees no
        // file (NothingToRotate), which must NOT latch the writer.
        std::fs::remove_file(&path).unwrap();
        let mut writer = RotatingLogWriter::open_with_cap(path.clone(), 4).unwrap();
        writer.write_all(b"xy").unwrap(); // written 2
        writer.write_all(b"zw").unwrap(); // written 4 >= cap; file missing -> NothingToRotate
        assert!(
            !writer.rotation_failed,
            "NothingToRotate (missing log) must not latch rotation"
        );

        // And once a real oversized file exists again, rotation still happens.
        std::fs::write(&path, vec![b'a'; 6]).unwrap();
        writer.write_all(b"tail").unwrap(); // written >= cap; file 6 bytes -> Rotated
        drop(writer);
        assert!(
            old.exists(),
            "rotation must still work after a benign NothingToRotate"
        );
    }
}
