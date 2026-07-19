use crate::application::playback::PlaybackUseCase;
use crate::application::playlist::PlaylistUseCase;
use crate::domain::audio_mode::AudioMode;
use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;
use crate::shared::spectrum::SpectrumFrame;

#[derive(Clone, Debug)]
pub struct RenderSnapshot {
    pub player_state: PlayerState,
    pub progress: f64,
    pub duration: f64,
    pub volume: f32,
    pub spectrum: SpectrumFrame,
    pub queue_songs: Vec<Song>,
    pub queue_current: usize,
    pub audio_mode: AudioMode,
}

impl RenderSnapshot {
    pub fn from_use_cases(pb: &PlaybackUseCase, pl: &PlaylistUseCase) -> Self {
        Self {
            player_state: pb.state(),
            progress: pb.current_position(),
            duration: pb.current_duration(),
            volume: pb.volume(),
            spectrum: pb.get_spectrum(),
            queue_songs: pl.songs().to_vec(),
            queue_current: pl.playlist().current_index,
            audio_mode: pb.mode(),
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

    /// Unicode volume bar for display
    #[allow(dead_code)]
    pub fn volume_bar(&self) -> String {
        let vol = self.volume.clamp(0.0, 1.0);
        let filled = (vol * 20.0) as usize;
        let empty = 20usize.saturating_sub(filled);
        format!("{}█{}", "█".repeat(filled), "░".repeat(empty))
    }
}
