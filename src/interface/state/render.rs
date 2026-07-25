use std::sync::Arc;

use crate::application::playback::PlaybackUseCase;
use crate::application::playlist::PlaylistUseCase;
use crate::domain::audio_mode::AudioMode;
use crate::domain::media::{RepeatMode, Song};
use crate::domain::player_state::PlayerState;
use crate::shared::spectrum::SpectrumFrame;

#[derive(Clone, Debug)]
pub struct RenderSnapshot {
    pub player_state: PlayerState,
    pub progress: f64,
    pub duration: f64,
    pub volume: f32,
    pub spectrum: SpectrumFrame,
    pub queue_songs: Arc<[Song]>,
    pub queue_current: usize,
    pub audio_mode: AudioMode,
    pub repeat_mode: RepeatMode,
}

impl RenderSnapshot {
    pub fn from_use_cases(pb: &PlaybackUseCase, pl: &mut PlaylistUseCase) -> Self {
        Self {
            player_state: pb.state(),
            progress: pb.current_position(),
            duration: pb.current_duration(),
            volume: pb.volume(),
            spectrum: pb.get_spectrum(),
            queue_songs: pl.songs_arc(),
            queue_current: pl.playlist().current_index,
            audio_mode: pb.mode(),
            repeat_mode: pl.repeat_mode(),
        }
    }

    /// Progress as a percentage (0.0–100.0)
    pub fn progress_percent(&self) -> f64 {
        if self.duration > 0.0 {
            (self.progress / self.duration * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        }
    }

}
