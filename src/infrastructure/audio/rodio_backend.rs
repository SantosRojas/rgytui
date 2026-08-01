use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::{Decoder, DeviceSinkBuilder, Player, Source};

use crate::application::ports::{AudioPlaybackPort, RouteChangeNotice};
use crate::domain::error::DomainError;
use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;

use crate::infrastructure::audio::spectrum::{SpectrumFrame, SpectrumSource};
use crate::shared::sync::lock_or_warn;

/// Max time to wait for the audio thread to drain the queue after `stop()`.
/// This is the freeze fix: rodio 0.22's `Player::append()` blocks indefinitely
/// on an internal `sleep_until_end()` channel when the queue still holds a
/// never-ending source (e.g. while paused) and the audio thread is dead. We
/// replace that unbounded wait with our own bounded one, so the main thread can
/// never block forever.
const FLUSH_DRAIN_TIMEOUT: Duration = Duration::from_millis(500);
/// Poll cadence while waiting for the queue to drain after `stop()`.
const DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(10);
/// Position frozen this long while state is Playing = the backend is dead or
/// stalled (e.g. the cpal/WASAPI device died or suspended after a long pause).
const STALL_TIMEOUT: Duration = Duration::from_secs(3);
/// Minimum interval between health checks, so the 50ms UI poll loop doesn't
/// hammer the backend with lock acquisitions.
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_millis(500);
/// Minimum interval between output-device identity checks (route-change
/// detection). Cheaper than `default_output_device()` being called every
/// health check; a Jack 3.5mm <-> Bluetooth switch is not time-critical.
const DEVICE_CHECK_INTERVAL: Duration = Duration::from_secs(1);
/// Retained-bytes cap for transparent device-loss recovery. Typical songs are
/// 2-8 MB; holding one song bounded by this cap is cheap. Larger files (rare)
/// fall back to the error path on device loss.
const MAX_RETAINED_BYTES: usize = 32 * 1024 * 1024;

/// Shared state fields wrapped in a single Mutex to reduce lock overhead.
/// `spectrum` lives separately because `SpectrumSource` holds a reference to it.
struct SharedState {
    state: PlayerState,
    volume: f32,
    duration: f64,
}

pub struct RodioAdapter {
    _handle: rodio::MixerDeviceSink,
    player: Player,
    shared: Arc<Mutex<SharedState>>,
    spectrum: Arc<Mutex<SpectrumFrame>>,
    spectrum_enabled: Arc<AtomicBool>,
    /// Timestamp of the last `check_health` call (rate limiting).
    last_health_check: Option<std::time::Instant>,
    /// Last observed playback position, used to detect a stalled backend.
    last_pos: f64,
    /// When the position last moved. `None` = never moved since the last
    /// play/pause cycle started.
    last_pos_moved: Option<std::time::Instant>,
    /// Device id of the default output device the current sink was opened on.
    /// Compared periodically against the live default to detect a route change
    /// (Jack 3.5mm <-> Bluetooth). When it differs while playing, the current
    /// sink points at a dead endpoint and must be rebuilt.
    sink_device_id: Option<String>,
    /// Timestamp of the last output-device identity check (rate limiting).
    last_device_check: Option<std::time::Instant>,
    /// Pending route change (Jack 3.5mm <-> Bluetooth): position (seconds) at
    /// which playback was paused on the freshly reopened sink. Set by
    /// `pause_for_route_change`; consumed only by `resume()` (rebuild from
    /// retained bytes).
    pending_route_change: Option<f64>,
    /// One-shot notice describing WHY a route change paused/stopped playback
    /// (recoverable vs restart-required), surfaced to the app exactly once via
    /// `take_route_change_notice`. Set by `pause_for_route_change`; independent
    /// from `pending_route_change`, which only `resume()` consumes.
    route_change_notice: Option<RouteChangeNotice>,
    /// Audio bytes of the currently playing song, retained so playback can be
    /// rebuilt transparently if the audio device is lost mid-play/pause.
    /// Dropped on stop(); bounded by MAX_RETAINED_BYTES.
    retained_bytes: Option<Vec<u8>>,
}

impl RodioAdapter {
    pub fn new() -> Result<Self, DomainError> {
        let mut handle = DeviceSinkBuilder::open_default_sink()
            .map_err(|e| DomainError::Audio(format!("Cannot open audio output: {}", e)))?;
        handle.log_on_drop(false);
        // Capture the id AFTER the sink opened, so the recorded identity matches
        // the endpoint the sink actually opened on (the default can flip
        // between the two calls).
        let device_id = Self::default_device_id();
        let player = Player::connect_new(handle.mixer());

        Ok(Self {
            _handle: handle,
            player,
            shared: Arc::new(Mutex::new(SharedState {
                state: PlayerState::Idle,
                volume: 0.8,
                duration: 0.0,
            })),
            spectrum: Arc::new(Mutex::new(SpectrumFrame::default())),
            spectrum_enabled: Arc::new(AtomicBool::new(true)),
            last_health_check: None,
            last_pos: 0.0,
            last_pos_moved: None,
            sink_device_id: device_id,
            last_device_check: None,
            pending_route_change: None,
            route_change_notice: None,
            retained_bytes: None,
        })
    }

    fn shared_mut(&self) -> std::sync::MutexGuard<'_, SharedState> {
        lock_or_warn(&self.shared, "rodio_shared")
    }

    /// Identity of the current default output device, used as the key for
    /// route-change detection: `DeviceId` is stable across reboots and
    /// reconnections (unlike `name()`). `None` when enumeration fails (backend
    /// still usable; the check just stays inert until an id is available).
    fn default_device_id() -> Option<String> {
        rodio::cpal::default_host()
            .default_output_device()
            .and_then(|device| device.id().ok().map(|id| id.to_string()))
    }

    /// Rate-limited check whether the default output device changed since the
    /// sink was opened (Jack 3.5mm <-> Bluetooth route switch). Also bumps
    /// `last_device_check` so callers can cheaply gate on it.
    fn device_changed(&mut self) -> bool {
        let now = std::time::Instant::now();
        if let Some(last) = self.last_device_check
            && now.duration_since(last) < DEVICE_CHECK_INTERVAL
        {
            return false;
        }
        self.last_device_check = Some(now);
        let current = Self::default_device_id();
        Self::device_identity_changed(&self.sink_device_id, &current)
    }

    /// Pure comparison for the route-change check: a change is only a change
    /// when a recorded sink id exists AND differs from the current default. A
    /// `None` recorded id (transient enumeration failure at open time) keeps
    /// the check inert — there is no baseline to compare, so a later-resolved
    /// default must not trigger a spurious reopen.
    fn device_identity_changed(recorded: &Option<String>, current: &Option<String>) -> bool {
        match (recorded, current) {
            (Some(recorded), Some(current)) => recorded != current,
            _ => false,
        }
    }

    /// Pure predicate for the route-change check: only watch while actually
    /// playing AND the sink still holds audio. When the sink is empty the song
    /// ended naturally and auto-advance owns the flow — recovering here would
    /// race it by re-creating the finished song.
    fn should_check_device(state: PlayerState, sink_empty: bool) -> bool {
        state == PlayerState::Playing && !sink_empty
    }

    /// Pure predicate for the retained-bytes cap: keep a song's bytes only if they
    /// fit within MAX_RETAINED_BYTES, so device-loss recovery stays memory-light.
    fn should_retain(data_len: usize) -> bool {
        data_len <= MAX_RETAINED_BYTES
    }

    /// Start playing `source` through the rodio player.
    ///
    /// This is the single choke point for starting playback and replaces the
    /// previous `player.stop(); player.append(source);` sequence. rodio's
    /// `append()` blocks forever on `sleep_until_end()` when a previously
    /// appended source never ended (a paused Pausable emits 0.0 forever) and
    /// the audio thread is dead — exactly the "freeze after long pause" bug.
    /// We bound the drain wait ourselves and, if the queue does not empty, we
    /// rebuild the backend so the fresh player (empty queue, stopped=false)
    /// makes `append()` non-blocking.
    fn play_source<S>(&mut self, source: S) -> Result<(), DomainError>
    where
        S: rodio::Source<Item = f32> + Send + 'static,
    {
        // Start the watchdog tracking from scratch for this play cycle.
        self.last_pos = 0.0;
        self.last_pos_moved = None;
        self.last_health_check = None;
        // A fresh play cycle supersedes any pending route-change resume; the
        // user explicitly asked for a new source, so there is nothing to pause-for.
        self.pending_route_change = None;
        self.route_change_notice = None;

        self.player.stop();

        if !self.wait_drained(FLUSH_DRAIN_TIMEOUT) {
            // The queue is not draining: the audio thread is dead or stalled
            // (e.g. the cpal/WASAPI device died during a long pause). Reopen
            // the backend to get a fresh player; append() below is then
            // guaranteed non-blocking because sound_count == 0.
            tracing::warn!(
                "Audio queue did not drain within {:?}; reopening audio backend",
                FLUSH_DRAIN_TIMEOUT
            );
            self.reopen_backend()?;
        } else {
            // Force a fresh device comparison at play start so a route change
            // inside the 1s rate-limit window is not masked: a stale
            // `last_device_check` could otherwise append to the dead sink and
            // recreate the mute until the next health check.
            self.last_device_check = None;
            if self.device_changed() {
                // Route change while starting playback: the old sink points at a
                // dead endpoint, so appending here would play into silence. Reopen
                // first so the fresh source lands on the live default device.
                tracing::warn!(
                    "Audio output device changed while starting playback; reopening backend"
                );
                self.reopen_backend()?;
            }
        }

        self.player.append(source);
        Ok(())
    }

    /// Rebuild playback from retained bytes after `reopen_backend()`, resuming at
    /// `seek_to` seconds. Best-effort: if seeking is not supported by the codec we
    /// restart the song from 0 (logged) rather than fail. On success the adapter is
    /// left in Playing state with fresh watchdog tracking.
    fn rebuild_after_reopen(&mut self, data: &[u8], seek_to: f64) -> Result<(), DomainError> {
        // Owned Vec required for a 'static source; one copy on a rare recovery event.
        let mut decoder = Decoder::new(std::io::Cursor::new(data.to_vec()))
            .map_err(|e| DomainError::Audio(format!("Decode error: {}", e)))?;

        if seek_to > 0.0
            && let Err(e) = decoder.try_seek(Duration::from_secs_f64(seek_to))
        {
            tracing::warn!(
                "Seek after device-loss recovery failed ({}); restarting song from the beginning",
                e
            );
        }

        let (source, new_spectrum) = SpectrumSource::new(decoder, self.spectrum_enabled.clone());
        self.spectrum = new_spectrum;

        // Fresh player from reopen_backend(): queue empty, so no drain wait. This
        // also resets the watchdog tracking for the new play cycle.
        self.play_source(source)?;

        let mut s = self.shared_mut();
        self.player.set_volume(s.volume);
        s.state = PlayerState::Playing;

        Ok(())
    }

    /// Poll `Player::len()` until the queue is empty or the deadline passes.
    /// This replaces rodio's unbounded internal wait inside `append()` — the
    /// main loop may pause here for at most `timeout`, NEVER forever.
    fn wait_drained(&self, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while self.player.len() > 0 {
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(DRAIN_POLL_INTERVAL);
        }
        true
    }

    /// Rebuild the device handle and player after the backend was detected as
    /// dead/stalled (or after a route change). The `Arc` fields (`shared`,
    /// `spectrum`, `spectrum_enabled`) are untouched, so volume and the
    /// spectrum flag survive a reopen. Records the new device identity so the
    /// route-change check compares against the live endpoint.
    fn reopen_backend(&mut self) -> Result<(), DomainError> {
        let mut handle = DeviceSinkBuilder::open_default_sink()
            .map_err(|e| DomainError::Audio(format!("Cannot open audio output: {}", e)))?;
        handle.log_on_drop(false);
        // Capture the id AFTER the sink opened so the recorded identity matches
        // the endpoint the new sink actually opened on.
        let device_id = Self::default_device_id();
        let player = Player::connect_new(handle.mixer());
        self._handle = handle;
        self.player = player;
        self.sink_device_id = device_id;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn play_file(&mut self, path: &Path, _song: Song) -> Result<(), DomainError> {
        // File-path playback is unused dead code and streams straight from disk,
        // so there are no in-memory bytes to retain for device-loss recovery.
        self.retained_bytes = None;

        let file = std::fs::File::open(path)?;
        let decoder =
            Decoder::new(file).map_err(|e| DomainError::Audio(format!("Decode error: {}", e)))?;

        let total_duration = decoder
            .total_duration()
            .unwrap_or(Duration::from_secs(0))
            .as_secs_f64();

        let (source, new_spectrum) = SpectrumSource::new(decoder, self.spectrum_enabled.clone());
        self.spectrum = new_spectrum;

        self.play_source(source)?;

        let mut s = self.shared_mut();
        self.player.set_volume(s.volume);
        s.state = PlayerState::Playing;
        s.duration = total_duration;

        Ok(())
    }

    /// Play audio from in-memory bytes (used when download completes in background).
    pub fn play_bytes(&mut self, data: Vec<u8>, _song: Song) -> Result<(), DomainError> {
        // Retain a copy only when the song fits the cap AND decodes successfully,
        // so a device-loss event later can rebuild playback transparently. We clone
        // once here (kept as retained_bytes), never a second time for the decoder.
        let should_retain = Self::should_retain(data.len());
        let retained = should_retain.then(|| data.clone());
        let cursor = std::io::Cursor::new(data);
        let decoder =
            Decoder::new(cursor).map_err(|e| DomainError::Audio(format!("Decode error: {}", e)))?;
        self.retained_bytes = retained;

        let total_duration = decoder
            .total_duration()
            .unwrap_or(Duration::from_secs(0))
            .as_secs_f64();

        let (source, new_spectrum) = SpectrumSource::new(decoder, self.spectrum_enabled.clone());
        self.spectrum = new_spectrum;

        self.play_source(source)?;

        let mut s = self.shared_mut();
        self.player.set_volume(s.volume);
        s.state = PlayerState::Playing;
        s.duration = total_duration;

        Ok(())
    }

    pub fn get_spectrum(&self) -> SpectrumFrame {
        *lock_or_warn(&self.spectrum, "spectrum")
    }

    pub fn pause(&mut self) -> Result<(), DomainError> {
        self.player.pause();
        self.shared_mut().state = PlayerState::Paused;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), DomainError> {
        if let Some(seek_to) = self.pending_route_change.take() {
            // Route change paused us on a freshly reopened sink: rebuild the
            // track from retained bytes so resume continues where it left off.
            if let Some(data) = self.retained_bytes.take() {
                match self.rebuild_after_reopen(&data, seek_to) {
                    Ok(()) => {
                        self.retained_bytes = Some(data);
                        return Ok(());
                    }
                    Err(e) => {
                        self.reset_health_state();
                        return Err(DomainError::Audio(format!(
                            "Audio device lost. Playback stopped. ({e})"
                        )));
                    }
                }
            }
            // No retained bytes (song > cap or file playback): cannot rebuild.
            self.reset_health_state();
            return Err(DomainError::Audio(
                "Audio device lost. Playback stopped.".into(),
            ));
        }
        self.player.play();
        self.shared_mut().state = PlayerState::Playing;
        // Start the watchdog tracking from scratch for this play cycle.
        self.last_pos = 0.0;
        self.last_pos_moved = None;
        self.last_health_check = None;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), DomainError> {
        self.player.stop();
        self.shared_mut().state = PlayerState::Stopped;
        // Playback ended/cleared: nothing left to recover, so release the bytes.
        self.retained_bytes = None;
        // A stop supersedes any pending route-change resume.
        self.pending_route_change = None;
        self.route_change_notice = None;
        // Stop tracking a stall: a fresh play cycle starts from zero.
        self.last_pos = 0.0;
        self.last_pos_moved = None;
        self.last_health_check = None;
        Ok(())
    }

    pub fn set_volume(&mut self, vol: f32) {
        let vol = vol.clamp(0.0, 1.0);
        self.player.set_volume(vol);
        self.shared_mut().volume = vol;
    }

    pub fn volume(&self) -> f32 {
        self.shared_mut().volume
    }

    pub fn state(&self) -> PlayerState {
        self.shared_mut().state
    }

    pub fn current_position(&self) -> f64 {
        self.player.get_pos().as_secs_f64()
    }

    pub fn current_duration(&self) -> f64 {
        self.shared_mut().duration
    }

    pub fn is_sink_empty(&self) -> bool {
        self.player.empty()
    }

    pub fn set_spectrum_enabled(&mut self, enabled: bool) {
        self.spectrum_enabled.store(enabled, Ordering::Relaxed);
    }

    /// Route change (Jack <-> Bluetooth): switch the sink to the live default
    /// device but DO NOT auto-resume — a fortuitous Bluetooth disconnect must
    /// never spill sound through the laptop speakers. When the song's bytes are
    /// retained, playback is left Paused with `pending_route_change` set so the
    /// user can resume explicitly from where it left off. Without retained
    /// bytes (song > cap or file playback) the track cannot be rebuilt, so
    /// playback is Stopped with NO resume promise and the app must ask the user
    /// to play the song again. The notice always surfaces to the app so the UI
    /// message matches what actually happened. Returns Ok after the reopen; on
    /// reopen failure the state is reset and the error propagates (unrecoverable).
    fn pause_for_route_change(&mut self, seek_to: f64) -> Result<(), DomainError> {
        if let Err(e) = self.reopen_backend() {
            self.reset_health_state();
            return Err(DomainError::Audio(format!(
                "Audio device lost. Playback stopped. ({e})"
            )));
        }
        let (state, resume_pos, notice) =
            Self::route_change_plan(self.retained_bytes.is_some(), seek_to);
        self.shared_mut().state = state;
        self.pending_route_change = resume_pos;
        self.route_change_notice = Some(notice);
        self.last_pos = resume_pos.unwrap_or(0.0);
        self.last_pos_moved = None;
        self.last_health_check = None;
        Ok(())
    }

    /// Pure decision for what a route change does to the current playback:
    /// when the song's bytes are retained, playback is left Paused with a
    /// resume position (ResumeAvailable — the user can continue from here);
    /// otherwise the track cannot be rebuilt, so playback is Stopped with NO
    /// resume promise (RestartRequired — the user must play the song again).
    /// Kept as a pure function so the retained/non-retained branch decision is
    /// testable without a real audio device.
    fn route_change_plan(
        retained: bool,
        seek_to: f64,
    ) -> (PlayerState, Option<f64>, RouteChangeNotice) {
        if retained {
            (PlayerState::Paused, Some(seek_to), RouteChangeNotice::ResumeAvailable)
        } else {
            (PlayerState::Stopped, None, RouteChangeNotice::RestartRequired)
        }
    }

    /// Reopen the backend and rebuild playback from retained bytes at
    /// `seek_to`. Transparent when the rebuild succeeds. When it fails (no
    /// retained bytes, rebuild error, or reopen error) the internal state is
    /// reset to Stopped BEFORE the error is returned, so the app clears the
    /// song and unblocks instead of staying wedged in a stale Playing state.
    fn recover_backend(&mut self, seek_to: f64, reason: &str) -> Result<(), DomainError> {
        tracing::warn!("{reason}; reopening audio backend");
        if let Err(e) = self.reopen_backend() {
            self.reset_health_state();
            return Err(DomainError::Audio(format!(
                "Audio device lost. Playback stopped. ({e})"
            )));
        }
        // Take the bytes out so rebuild_after_reopen can borrow self mutably,
        // then put them back on success so a later device loss can also recover.
        let retained = self.retained_bytes.take();
        let rebuild_err = if let Some(data) = retained {
            match self.rebuild_after_reopen(&data, seek_to) {
                Ok(()) => {
                    self.retained_bytes = Some(data);
                    // Transparent recovery: playback resumed at seek_to. No error surfaced.
                    return Ok(());
                }
                Err(e) => Some(e),
            }
        } else {
            None
        };
        self.reset_health_state();
        // Preserve the underlying rebuild failure (decode/seek) when present so
        // the log and UI notification carry the real diagnostics.
        match rebuild_err {
            Some(e) => Err(DomainError::Audio(format!(
                "Audio device lost. Playback stopped. ({e})"
            ))),
            None => Err(DomainError::Audio(
                "Audio device lost. Playback stopped.".into(),
            )),
        }
    }

    /// Reset the watchdog tracking and shared state after an unrecoverable
    /// backend failure so the next play cycle starts from a clean slate.
    fn reset_health_state(&mut self) {
        self.shared_mut().state = PlayerState::Stopped;
        self.last_pos = 0.0;
        self.last_pos_moved = None;
        self.last_health_check = None;
    }
}

impl AudioPlaybackPort for RodioAdapter {
    fn play_file(&mut self, path: &Path, song: Song) -> Result<(), DomainError> {
        self.play_file(path, song)
    }

    fn play_bytes(&mut self, data: Vec<u8>, song: Song) -> Result<(), DomainError> {
        self.play_bytes(data, song)
    }

    fn pause(&mut self) -> Result<(), DomainError> {
        self.pause()
    }

    fn resume(&mut self) -> Result<(), DomainError> {
        self.resume()
    }

    fn stop(&mut self) -> Result<(), DomainError> {
        self.stop()
    }

    fn set_volume(&mut self, vol: f32) {
        self.set_volume(vol);
    }

    fn volume(&self) -> f32 {
        self.volume()
    }

    fn state(&self) -> PlayerState {
        self.state()
    }

    fn current_position(&self) -> f64 {
        self.current_position()
    }

    fn current_duration(&self) -> f64 {
        self.current_duration()
    }

    fn is_sink_empty(&self) -> bool {
        self.is_sink_empty()
    }

    fn get_spectrum(&self) -> SpectrumFrame {
        self.get_spectrum()
    }

    fn set_spectrum_enabled(&mut self, enabled: bool) {
        self.set_spectrum_enabled(enabled);
    }

    fn take_route_change_notice(&mut self) -> Option<RouteChangeNotice> {
        self.route_change_notice.take()
    }

    /// Watchdog: detect a dead/stalled audio backend (e.g. the WASAPI/cpal
    /// device dying after a long pause, or a Jack/Bluetooth route change) and
    /// recover by reopening the backend.
    /// Called periodically from the UI poll loop; rate-limited internally.
    fn check_health(&mut self) -> Result<(), DomainError> {
        let now = std::time::Instant::now();
        if let Some(last) = self.last_health_check
            && now.duration_since(last) < HEALTH_CHECK_INTERVAL
        {
            return Ok(());
        }
        self.last_health_check = Some(now);

        // Only watchdog actual playback. While paused the position is frozen
        // by design, and when the sink is empty the song ended naturally
        // (auto-advance handles it).
        if self.shared_mut().state != PlayerState::Playing {
            return Ok(());
        }

        // Route-change detection: a switched default output device (Jack 3.5mm
        // <-> Bluetooth) invalidates the current sink even when the stream
        // keeps pumping silence, which the frozen-position heuristic below
        // would never catch (position keeps moving). Unlike the stall watchdog
        // (which recovers transparently), a route change switches the sink to
        // the live default and PAUSES: a fortuitous Bluetooth disconnect must
        // never spill sound through the laptop speakers, so the user resumes
        // explicitly. Seek target = the live position, because after a paused
        // route-switch + resume `last_pos` is 0 (resume() zeroes it) and the
        // live get_pos() is the true position; max() guards a dead sink
        // reporting 0.
        let sink_empty = self.player.empty();
        if Self::should_check_device(PlayerState::Playing, sink_empty) && self.device_changed() {
            let seek_to = self.player.get_pos().as_secs_f64().max(self.last_pos);
            return self.pause_for_route_change(seek_to);
        }

        if sink_empty {
            // Natural end of the song; auto-advance owns the flow.
            return Ok(());
        }

        let pos = self.player.get_pos().as_secs_f64();
        if (pos - self.last_pos).abs() > 1e-9 {
            self.last_pos = pos;
            self.last_pos_moved = Some(now);
            return Ok(());
        }

        let moved_at = match self.last_pos_moved {
            Some(t) => t,
            None => {
                // Position has not moved yet since playback started — seed the
                // watchdog so we start counting from the first check.
                self.last_pos_moved = Some(now);
                return Ok(());
            }
        };

        if now.duration_since(moved_at) < STALL_TIMEOUT {
            return Ok(());
        }

        // Position frozen while Playing for longer than STALL_TIMEOUT: the
        // backend is dead. Try to recover transparently from the retained bytes.
        tracing::warn!(
            "Audio position frozen for {:?} while playing; backend presumed dead, recovering",
            STALL_TIMEOUT
        );
        let seek_to = self.last_pos; // capture BEFORE recover resets tracking
        self.recover_backend(seek_to, "Audio position frozen while playing; backend presumed dead")
    }
}

/// A no-op audio backend used when the system has no audio output available.
/// Allows the application to start and function (search, browse) without sound.
pub struct NoopAudioAdapter;

impl AudioPlaybackPort for NoopAudioAdapter {
    fn play_file(&mut self, _path: &Path, _song: Song) -> Result<(), DomainError> {
        Err(DomainError::Audio("No audio output available".into()))
    }
    fn play_bytes(&mut self, _data: Vec<u8>, _song: Song) -> Result<(), DomainError> {
        Err(DomainError::Audio("No audio output available".into()))
    }
    fn pause(&mut self) -> Result<(), DomainError> { Ok(()) }
    fn resume(&mut self) -> Result<(), DomainError> { Ok(()) }
    fn stop(&mut self) -> Result<(), DomainError> { Ok(()) }
    fn set_volume(&mut self, _vol: f32) {}
    fn volume(&self) -> f32 { 0.8 }
    fn state(&self) -> PlayerState { PlayerState::Stopped }
    fn current_position(&self) -> f64 { 0.0 }
    fn current_duration(&self) -> f64 { 0.0 }
    fn is_sink_empty(&self) -> bool { true }
    fn get_spectrum(&self) -> SpectrumFrame { SpectrumFrame::default() }
    fn set_spectrum_enabled(&mut self, _enabled: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_change_plan_retained_bytes_pauses_with_resume_position() {
        // Song bytes retained: playback pauses and keeps the resume position,
        // and the notice promises a recoverable resume.
        let (state, resume_pos, notice) = RodioAdapter::route_change_plan(true, 42.5);
        assert_eq!(state, PlayerState::Paused);
        assert_eq!(
            resume_pos,
            Some(42.5),
            "retained bytes must preserve the resume position"
        );
        assert_eq!(notice, RouteChangeNotice::ResumeAvailable);
    }

    #[test]
    fn route_change_plan_without_retained_bytes_stops_without_resume() {
        // No retained bytes (song > cap or file playback): playback stops with
        // NO resume promise — the notice must not advertise a recoverable
        // resume that resume() could never fulfill.
        let (state, resume_pos, notice) = RodioAdapter::route_change_plan(false, 42.5);
        assert_eq!(state, PlayerState::Stopped);
        assert_eq!(
            resume_pos,
            None,
            "no retained bytes means no resume position is kept"
        );
        assert_eq!(notice, RouteChangeNotice::RestartRequired);
    }

    #[test]
    fn should_retain_respects_cap() {
        assert!(RodioAdapter::should_retain(0));
        assert!(RodioAdapter::should_retain(super::MAX_RETAINED_BYTES));
        assert!(!RodioAdapter::should_retain(super::MAX_RETAINED_BYTES + 1));
    }

    #[test]
    fn should_check_device_only_while_playing_with_content() {
        use crate::domain::player_state::PlayerState;
        // Route-change check is only meaningful while audio is actually
        // flowing: paused playback is covered by the stall watchdog on resume,
        // and an empty sink means the song ended naturally (auto-advance owns
        // the flow — recovering here would race it).
        assert!(RodioAdapter::should_check_device(PlayerState::Playing, false));
        assert!(!RodioAdapter::should_check_device(PlayerState::Playing, true));
        assert!(!RodioAdapter::should_check_device(PlayerState::Paused, false));
        assert!(!RodioAdapter::should_check_device(PlayerState::Stopped, false));
        assert!(!RodioAdapter::should_check_device(PlayerState::Idle, false));
    }

    #[test]
    fn device_identity_changed_is_inert_without_a_recorded_baseline() {
        // No recorded sink id (transient enumeration failure at open time):
        // the check stays inert even when the default later resolves —
        // there is no baseline to compare, so no spurious reopen.
        assert!(!RodioAdapter::device_identity_changed(&None, &Some("current".into())));
        assert!(!RodioAdapter::device_identity_changed(&None, &None));
        // Recorded id but no current default: cannot prove a change, inert.
        assert!(!RodioAdapter::device_identity_changed(&Some("previous".into()), &None));
        // Same id: no change. Different id: route switch detected.
        assert!(!RodioAdapter::device_identity_changed(
            &Some("speakers".into()),
            &Some("speakers".into())
        ));
        assert!(RodioAdapter::device_identity_changed(
            &Some("speakers".into()),
            &Some("headset".into())
        ));
    }
}
