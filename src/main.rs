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
    /// Latch set after the rotation RENAME failed. Once set, the rename is not
    /// retried on every write: a persistent failure (e.g. another process
    /// locking the log) must not turn every log write into a rename retry
    /// storm, and must not spam stderr inside the raw-mode TUI. It only
    /// suppresses the rename, never the reopen — a writer left on the null
    /// device by a transient reopen failure must keep trying to recover (see
    /// [`Self::rotate`]).
    rotation_failed: bool,
    /// True while the writer targets the null device because the last reopen
    /// failed. While set, the next cap crossing re-attempts the reopen
    /// (silently), and a successful reopen clears a transient rename latch so
    /// rotation can resume.
    on_sink: bool,
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
            on_sink: false,
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
    /// The latch only suppresses the rename: while the writer is on the null
    /// device the reopen keeps being retried, and a successful reopen clears
    /// the latch so rotation can resume after a transient lock clears.
    /// Note: this runs inside the tracing writer (behind the subscriber's
    /// Mutex), so it must NEVER emit tracing events itself (deadlock) nor
    /// stderr text mid-session (the raw-mode TUI owns the terminal) — a failed
    /// rotation is reported silently and only once, via the latch.
    fn rotate(&mut self) {
        if self.rotation_failed && !self.on_sink {
            // Rename failed and we are still writing to a real file: keep
            // appending to the oversized file; do not re-attempt the rename
            // (retry storm) nor the reopen (already open).
            return;
        }
        let _ = self.file.flush();
        // Close the current handle before renaming (see the Windows note above).
        self.file = Box::new(std::io::sink());
        let rename_attempted = !self.rotation_failed;
        let was_on_sink = self.on_sink;
        if rename_attempted && rotate_if_needed(&self.path, self.max_bytes).is_err() {
            // Rename failed (e.g. another process holds the log open on
            // Windows). Do not retry it on every subsequent write; keep
            // appending to the oversized file instead.
            self.rotation_failed = true;
        }
        match open_log_file(&self.path) {
            Ok(file) => {
                self.file = Box::new(file);
                self.written = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
                self.on_sink = false;
                // Recovered from the null device: the transient failure that
                // put us there cleared, so a rename latch from that same
                // period may be cleared too and rotation can resume on later
                // writes.
                if was_on_sink {
                    self.rotation_failed = false;
                }
            }
            Err(_) => {
                // Reopen failed (e.g. the rename left the file locked, or the
                // filesystem is unavailable). Keep the sink for now but force
                // the next write to retry the reopen: written is left at the
                // cap so the very next write re-enters rotate() and attempts
                // open_log_file again, silently, with no stderr. Never latch
                // here — a transient reopen failure must not kill rotation.
                self.written = self.max_bytes;
                self.on_sink = true;
            }
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
        // NOTE: the log is truncated/removed WHILE the writer holds it open,
        // so the writer's in-memory `written` counter keeps growing past the
        // cap while the on-disk file is missing or below the cap — the exact
        // benign state the R3-1 latch bug mishandled.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE_NAME);
        let old = dir.path().join(LOG_OLD_FILE_NAME);

        let mut writer = RotatingLogWriter::open_with_cap(path.clone(), 4).unwrap();

        // --- Undersized: truncate the log below the cap while the writer is
        // open, then cross the cap again. The on-disk file stays below the
        // cap, so rotate_if_needed must report NothingToRotate (no latch).
        writer.write_all(b"ab").unwrap(); // written 2, on-disk 2 bytes
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(0)
            .unwrap();
        writer.write_all(b"cd").unwrap(); // written 4 >= cap; on-disk 2 bytes
        assert!(
            !writer.rotation_failed,
            "NothingToRotate (undersized log) must not latch rotation"
        );

        // --- Missing: delete the log while the writer is open, then cross the
        // cap again. rotate_if_needed sees no file -> NothingToRotate.
        std::fs::remove_file(&path).unwrap();
        writer.write_all(b"xy").unwrap(); // written 2 (in-memory)
        writer.write_all(b"zw").unwrap(); // written 4 >= cap; file missing
        assert!(
            !writer.rotation_failed,
            "NothingToRotate (missing log) must not latch rotation"
        );

        // And once a real oversized file exists again, rotation still happens.
        std::fs::write(&path, vec![b'a'; 6]).unwrap();
        writer.write_all(b"tail").unwrap(); // written >= cap; file 6 bytes
        drop(writer);
        assert!(
            old.exists(),
            "rotation must still work after a benign NothingToRotate"
        );
    }

    #[test]
    fn rotating_writer_retries_reopen_after_transient_failure() {
        // A failed reopen must NOT latch rotation: the writer falls back to
        // the null device but re-attempts the reopen on later cap crossings,
        // and recovers (writing to a real file again) once the path is
        // openable — a transient failure cannot silently drop the session's
        // logs for good. Simulate the failure by making the parent directory
        // disappear mid-session, so both metadata (NothingToRotate) and
        // open_log_file (NotFound) fail deterministically on every platform.
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("parent");
        std::fs::create_dir(&parent).unwrap();
        let path = parent.join(LOG_FILE_NAME);
        let old = parent.join(LOG_OLD_FILE_NAME);

        let mut writer = RotatingLogWriter::open_with_cap(path.clone(), 4).unwrap();
        writer.write_all(b"abcd").unwrap(); // at cap -> Rotated; fresh file
        assert!(old.exists(), "a successful rotation must produce .old");

        // Remove the log, its rotated .old, and the parent directory while
        // the writer is open (Windows defers the log deletion only while a
        // handle stays open, so the parent removal needs the .old gone too).
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(&old).unwrap();
        std::fs::remove_dir(&parent).unwrap();

        writer.write_all(b"xy").unwrap(); // written 2 (in-memory)
        writer.write_all(b"zw").unwrap(); // written 4 >= cap -> rotate() fails reopen
        assert!(
            writer.on_sink,
            "a failed reopen must leave the writer on the null device"
        );
        assert!(
            !writer.rotation_failed,
            "a failed reopen must NOT latch rotation"
        );

        // Restore the parent: the next cap crossing must retry the reopen and
        // recover onto a real file.
        std::fs::create_dir(&parent).unwrap();
        writer.write_all(b"efgh").unwrap(); // retries reopen -> succeeds
        assert!(
            !writer.on_sink,
            "the writer must recover from the null device once the path is openable"
        );

        writer.write_all(b"tail").unwrap(); // written >= cap -> rotates again
        drop(writer);
        assert!(
            old.exists(),
            "rotation must still work after a transient reopen failure"
        );
    }

    #[test]
    fn rotating_writer_recovers_from_combined_rename_and_reopen_failure() {
        // The R3-3 fix: when BOTH the rename and the reopen fail in the same
        // rotation, the writer must NOT stay on the null device forever. The
        // rename latch only suppresses the rename; the reopen is retried on
        // later cap crossings and, once the path is openable again, the writer
        // recovers onto a real file and clears the latch so rotation can
        // resume. A cap of 0 makes every write cross the cap; a NON-EMPTY
        // directory in `.old` makes the rename fail on every platform, and a
        // NON-EMPTY directory at `path` makes open_log_file fail too.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE_NAME);
        let old = dir.path().join(LOG_OLD_FILE_NAME);

        // Start with a real oversized log so the writer can be opened.
        std::fs::write(&path, b"abcd").unwrap();
        let mut writer = RotatingLogWriter::open_with_cap(path.clone(), 0).unwrap();

        // Turn both the log and the .old target into non-empty directories so
        // the rename fails (ENOTEMPTY/ACCESS_DENIED) and the reopen fails
        // (EISDIR/ACCESS_DENIED) in the same rotation.
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("inner"), b"x").unwrap();
        std::fs::create_dir(&old).unwrap();
        std::fs::write(old.join("stale"), b"x").unwrap();

        writer.write_all(b"x").unwrap(); // cross the cap -> combined failure
        assert!(
            writer.rotation_failed && writer.on_sink,
            "the combined failure must latch the rename AND put the writer on the sink"
        );

        // A second write in that state must still retry (not early-return),
        // and the rename must NOT be re-attempted while latched.
        writer.write_all(b"y").unwrap();
        assert!(
            writer.rotation_failed && writer.on_sink,
            "the latch must persist while the combined failure persists"
        );

        // Repair the log path: once it is a regular openable file again, the
        // next cap crossing must retry the reopen, recover onto the real file,
        // and clear the latch. (Under the base unconditional early return the
        // reopen would never be retried and on_sink would stay true.)
        std::fs::remove_dir_all(&path).unwrap();
        std::fs::write(&path, b"abc").unwrap();
        writer.write_all(b"z").unwrap(); // sink write that triggers the retry
        assert!(
            !writer.on_sink,
            "the writer must recover from the sink once the path is openable"
        );
        assert!(
            !writer.rotation_failed,
            "a recovered reopen must clear the transient rename latch"
        );

        // Subsequent writes must land in the real file again, and a later
        // rename failure (the .old directory is still there) must re-latch
        // into the append-to-real-file state, not the sink.
        writer.write_all(b"w").unwrap(); // rotates: rename fails again -> re-latch
        writer.write_all(b"final").unwrap(); // early-return: append to the real file
        drop(writer);
        assert!(
            std::fs::read_to_string(&path).unwrap().contains("wfinal"),
            "post-recovery writes must land in the real log file"
        );
    }
}
