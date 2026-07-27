use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::{Decoder, DeviceSinkBuilder, Player, Source};

use crate::application::ports::AudioPlaybackPort;
use crate::domain::error::DomainError;
use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;

use crate::infrastructure::audio::spectrum::{SpectrumFrame, SpectrumSource};
use crate::shared::sync::lock_or_warn;

/// Shared state fields wrapped in a single Mutex to reduce lock overhead.
/// `spectrum` lives separately because `SpectrumSource` holds a reference to it.
struct SharedState {
    state: PlayerState,
    volume: f32,
    position: f64,
    duration: f64,
}

pub struct RodioAdapter {
    _handle: rodio::MixerDeviceSink,
    player: Player,
    shared: Arc<Mutex<SharedState>>,
    spectrum: Arc<Mutex<SpectrumFrame>>,
    spectrum_enabled: Arc<AtomicBool>,
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
            shared: Arc::new(Mutex::new(SharedState {
                state: PlayerState::Idle,
                volume: 0.8,
                position: 0.0,
                duration: 0.0,
            })),
            spectrum: Arc::new(Mutex::new(SpectrumFrame::default())),
            spectrum_enabled: Arc::new(AtomicBool::new(true)),
        })
    }

    fn shared_mut(&self) -> std::sync::MutexGuard<'_, SharedState> {
        lock_or_warn(&self.shared, "rodio_shared")
    }

    #[allow(dead_code)]
    pub fn play_file(&mut self, path: &Path, _song: Song) -> Result<(), DomainError> {
        let file = std::fs::File::open(path)?;
        let decoder =
            Decoder::new(file).map_err(|e| DomainError::Audio(format!("Decode error: {}", e)))?;

        let total_duration = decoder
            .total_duration()
            .unwrap_or(Duration::from_secs(0))
            .as_secs_f64();

        let (source, new_spectrum) = SpectrumSource::new(decoder, self.spectrum_enabled.clone());
        self.spectrum = new_spectrum;

        self.player.stop();
        self.player.append(source);

        let mut s = self.shared_mut();
        self.player.set_volume(s.volume);
        s.state = PlayerState::Playing;
        s.position = 0.0;
        s.duration = total_duration;

        Ok(())
    }

    /// Play audio from in-memory bytes (used when download completes in background).
    pub fn play_bytes(&mut self, data: Vec<u8>, _song: Song) -> Result<(), DomainError> {
        let cursor = std::io::Cursor::new(data);
        let decoder =
            Decoder::new(cursor).map_err(|e| DomainError::Audio(format!("Decode error: {}", e)))?;

        let total_duration = decoder
            .total_duration()
            .unwrap_or(Duration::from_secs(0))
            .as_secs_f64();

        let (source, new_spectrum) = SpectrumSource::new(decoder, self.spectrum_enabled.clone());
        self.spectrum = new_spectrum;

        self.player.stop();
        self.player.append(source);

        let mut s = self.shared_mut();
        self.player.set_volume(s.volume);
        s.state = PlayerState::Playing;
        s.position = 0.0;
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
        self.player.play();
        self.shared_mut().state = PlayerState::Playing;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), DomainError> {
        self.player.stop();
        let mut s = self.shared_mut();
        s.state = PlayerState::Stopped;
        s.position = 0.0;
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
        let pos = self.player.get_pos().as_secs_f64();
        self.shared_mut().position = pos;
        pos
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
