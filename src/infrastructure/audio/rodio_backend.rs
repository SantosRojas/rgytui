use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::{Decoder, DeviceSinkBuilder, Player, Source};

use crate::domain::error::DomainError;
use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;
use crate::infrastructure::audio::spectrum::SpectrumSource;

const SPECTRUM_BANDS: usize = 16;

pub struct RodioBackend {
    _handle: rodio::MixerDeviceSink,
    player: Player,
    state: Arc<Mutex<PlayerState>>,
    volume: Arc<Mutex<f32>>,
    current_song: Arc<Mutex<Option<Song>>>,
    position: Arc<Mutex<f64>>,
    duration: Arc<Mutex<f64>>,
    spectrum_bands: Arc<Mutex<[f32; SPECTRUM_BANDS]>>,
}

impl RodioBackend {
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
            spectrum_bands: Arc::new(Mutex::new([0.0; SPECTRUM_BANDS])),
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

        let (source, bands) = SpectrumSource::new(decoder);
        self.spectrum_bands = bands;

        self.player.stop();
        self.player.append(source);
        self.player.set_volume(*self.volume.lock().unwrap());

        *self.state.lock().unwrap() = PlayerState::Playing;
        *self.current_song.lock().unwrap() = Some(song);
        *self.duration.lock().unwrap() = total_duration;
        *self.position.lock().unwrap() = 0.0;

        Ok(())
    }

    pub fn get_spectrum(&self) -> [f32; SPECTRUM_BANDS] {
        *self.spectrum_bands.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn pause(&mut self) -> Result<(), DomainError> {
        self.player.pause();
        *self.state.lock().unwrap() = PlayerState::Paused;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), DomainError> {
        self.player.play();
        *self.state.lock().unwrap() = PlayerState::Playing;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), DomainError> {
        self.player.stop();
        *self.state.lock().unwrap() = PlayerState::Stopped;
        *self.position.lock().unwrap() = 0.0;
        Ok(())
    }

    pub fn set_volume(&mut self, vol: f32) {
        let vol = vol.clamp(0.0, 1.0);
        self.player.set_volume(vol);
        *self.volume.lock().unwrap() = vol;
    }

    pub fn volume(&self) -> f32 {
        *self.volume.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn state(&self) -> PlayerState {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn current_position(&self) -> f64 {
        let pos = self.player.get_pos().as_secs_f64();
        *self.position.lock().unwrap() = pos;
        pos
    }

    pub fn current_duration(&self) -> f64 {
        *self.duration.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn current_song(&self) -> Option<Song> {
        self.current_song.lock().ok().and_then(|s| s.clone())
    }

    pub fn is_sink_empty(&self) -> bool {
        self.player.empty()
    }

    pub fn has_sink(&self) -> bool {
        !self.player.empty()
    }
}
