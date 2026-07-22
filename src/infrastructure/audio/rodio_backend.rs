use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use rodio::{Decoder, DeviceSinkBuilder, Player, Source};

use crate::application::ports::AudioPlaybackPort;
use crate::domain::error::DomainError;
use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;
use crate::infrastructure::audio::spectrum::{SpectrumFrame, SpectrumSource};

/// Lock a `Mutex<T>` and return a guard, logging a warning if the mutex was poisoned.
/// This ensures thread-panic data corruption is never silently swallowed.
fn lock_or_warn<'a, T>(m: &'a Mutex<T>, name: &str) -> MutexGuard<'a, T> {
    m.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("Mutex '{}' was poisoned — recovering. Data may be stale.", name);
        poisoned.into_inner()
    })
}

pub struct RodioAdapter {
    _handle: rodio::MixerDeviceSink,
    player: Player,
    state: Arc<Mutex<PlayerState>>,
    volume: Arc<Mutex<f32>>,
    current_song: Arc<Mutex<Option<Song>>>,
    position: Arc<Mutex<f64>>,
    duration: Arc<Mutex<f64>>,
    spectrum: Arc<Mutex<SpectrumFrame>>,
}

impl RodioAdapter {
    pub fn new() -> Result<Self, DomainError> {
        let mut handle = DeviceSinkBuilder::open_default_sink()
            .map_err(|e| DomainError::Audio(format!("Cannot open audio output: {}", e)))?;
        handle.log_on_drop(false);
        let player = Player::connect_new(handle.mixer());

        Ok(Self {
            _handle: handle,
            player,
            state: Arc::new(Mutex::new(PlayerState::Idle)),
            volume: Arc::new(Mutex::new(0.8)),
            current_song: Arc::new(Mutex::new(None)),
            position: Arc::new(Mutex::new(0.0)),
            duration: Arc::new(Mutex::new(0.0)),
            spectrum: Arc::new(Mutex::new(SpectrumFrame::default())),
        })
    }

    pub fn play_file(&mut self, path: &Path, song: Song) -> Result<(), DomainError> {
        let file = std::fs::File::open(path)?;
        let decoder =
            Decoder::new(file).map_err(|e| DomainError::Audio(format!("Decode error: {}", e)))?;

        let total_duration = decoder
            .total_duration()
            .unwrap_or(Duration::from_secs(0))
            .as_secs_f64();

        let (source, frame) = SpectrumSource::new(decoder);
        self.spectrum = frame;

        self.player.stop();
        self.player.append(source);
        self.player.set_volume(*lock_or_warn(&self.volume, "volume"));

        *lock_or_warn(&self.state, "state") = PlayerState::Playing;
        *lock_or_warn(&self.current_song, "current_song") = Some(song);
        *lock_or_warn(&self.duration, "duration") = total_duration;
        *lock_or_warn(&self.position, "position") = 0.0;

        Ok(())
    }

    /// Play audio from in-memory bytes (used when download completes in background).
    pub fn play_bytes(&mut self, data: Vec<u8>, song: Song) -> Result<(), DomainError> {
        let cursor = std::io::Cursor::new(data);
        let decoder =
            Decoder::new(cursor).map_err(|e| DomainError::Audio(format!("Decode error: {}", e)))?;

        let total_duration = decoder
            .total_duration()
            .unwrap_or(Duration::from_secs(0))
            .as_secs_f64();

        let (source, frame) = SpectrumSource::new(decoder);
        self.spectrum = frame;

        self.player.stop();
        self.player.append(source);
        self.player.set_volume(*lock_or_warn(&self.volume, "volume"));

        *lock_or_warn(&self.state, "state") = PlayerState::Playing;
        *lock_or_warn(&self.current_song, "current_song") = Some(song);
        *lock_or_warn(&self.duration, "duration") = total_duration;
        *lock_or_warn(&self.position, "position") = 0.0;

        Ok(())
    }

    pub fn get_spectrum(&self) -> SpectrumFrame {
        *lock_or_warn(&self.spectrum, "spectrum")
    }

    pub fn pause(&mut self) -> Result<(), DomainError> {
        self.player.pause();
        *lock_or_warn(&self.state, "state") = PlayerState::Paused;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), DomainError> {
        self.player.play();
        *lock_or_warn(&self.state, "state") = PlayerState::Playing;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), DomainError> {
        self.player.stop();
        *lock_or_warn(&self.state, "state") = PlayerState::Stopped;
        *lock_or_warn(&self.position, "position") = 0.0;
        Ok(())
    }

    pub fn set_volume(&mut self, vol: f32) {
        let vol = vol.clamp(0.0, 1.0);
        self.player.set_volume(vol);
        *lock_or_warn(&self.volume, "volume") = vol;
    }

    pub fn volume(&self) -> f32 {
        *lock_or_warn(&self.volume, "volume")
    }

    pub fn state(&self) -> PlayerState {
        *lock_or_warn(&self.state, "state")
    }

    pub fn current_position(&self) -> f64 {
        let pos = self.player.get_pos().as_secs_f64();
        *lock_or_warn(&self.position, "position") = pos;
        pos
    }

    pub fn current_duration(&self) -> f64 {
        *lock_or_warn(&self.duration, "duration")
    }

    #[allow(dead_code)]
    pub fn current_song(&self) -> Option<Song> {
        lock_or_warn(&self.current_song, "current_song").as_ref().cloned()
    }

    pub fn is_sink_empty(&self) -> bool {
        self.player.empty()
    }

    #[allow(dead_code)]
    pub fn has_sink(&self) -> bool {
        !self.player.empty()
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

    fn has_sink(&self) -> bool {
        self.has_sink()
    }

    fn get_spectrum(&self) -> SpectrumFrame {
        self.get_spectrum()
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
    fn has_sink(&self) -> bool { false }
    fn get_spectrum(&self) -> SpectrumFrame { SpectrumFrame::default() }
}
