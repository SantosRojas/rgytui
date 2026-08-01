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
/// Temporary name the previous `.old` is parked under while a Windows rotation
/// replaces it (rename cannot overwrite an existing file there).
const LOG_OLD_PREV_FILE_NAME: &str = "rgytui.log.old.prev";
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
    // session never exceeds the cap again.
    rotate_if_needed(&path, LOG_MAX_BYTES);
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
        })
    }

    /// Rotate the active file through the shared rotation helper and reopen it
    /// in append mode. The current handle is closed first so the rename is
    /// allowed on Windows (open files are locked there). Best-effort: if the
    /// rotation fails the oversized file stays in place and we keep appending
    /// to it; if it cannot be reopened at all, writes fall back to the null
    /// device. Either way logging never crashes.
    fn rotate(&mut self) {
        let _ = self.file.flush();
        // Close the current handle before renaming (see the Windows note above).
        self.file = Box::new(std::io::sink());
        rotate_if_needed(&self.path, self.max_bytes);
        match open_log_file(&self.path) {
            Ok(file) => self.file = Box::new(file),
            Err(e) => eprintln!(
                "Warning: cannot reopen log file {} after rotation: {e}",
                self.path.display()
            ),
        }
        self.written = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
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

/// Rotate `path` to `rgytui.log.old` when it exceeds `max_bytes`, so the log
/// cannot grow unboundedly. Best-effort: never fails — a failed rotation just
/// leaves the oversized log in place (the writer appends to it as before).
fn rotate_if_needed(path: &Path, max_bytes: u64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return; // no existing log yet
    };
    if meta.len() < max_bytes {
        return;
    }
    let old = path.with_file_name(LOG_OLD_FILE_NAME);
    #[cfg(unix)]
    {
        // rename(2) atomically replaces an existing .old, so no dance needed.
        if let Err(e) = std::fs::rename(path, &old) {
            eprintln!("Warning: cannot rotate log file {}: {e}", path.display());
        }
    }
    #[cfg(not(unix))]
    {
        // On Windows rename cannot overwrite an existing file, so park the
        // previous .old aside first and restore it if the rotation itself
        // fails. Best-effort: never panics, a failed rotation just leaves the
        // oversized log in place.
        let prev = path.with_file_name(LOG_OLD_PREV_FILE_NAME);
        let _ = std::fs::remove_file(&prev); // drop a stale temp from a crash
        let _ = std::fs::rename(&old, &prev);
        if let Err(e) = std::fs::rename(path, &old) {
            eprintln!("Warning: cannot rotate log file {}: {e}", path.display());
            let _ = std::fs::rename(&prev, &old); // restore the parked .old
        } else {
            let _ = std::fs::remove_file(&prev); // success: drop the parked .old
        }
    }
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
}
