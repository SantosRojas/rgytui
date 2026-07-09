use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ActiveScreen {
    Search,
    Player,
    Help,
}

#[derive(Clone, Debug)]
pub struct UiState {
    pub active_screen: ActiveScreen,
    pub search_query: String,
    pub search_results: Vec<Song>,
    pub is_searching: bool,
    pub player_state: PlayerState,
    pub current_song: Option<Song>,
    pub progress: f64,
    pub duration: f64,
    pub volume: f32,
    pub status_message: Option<String>,
    pub focus_search: bool,
    pub selected_index: usize,
    pub error_message: Option<String>,
    pub loading_status: Option<String>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            active_screen: ActiveScreen::Search,
            search_query: String::new(),
            search_results: Vec::new(),
            is_searching: false,
            player_state: PlayerState::Idle,
            current_song: None,
            progress: 0.0,
            duration: 0.0,
            volume: 0.8,
            status_message: None,
            focus_search: true,
            selected_index: 0,
            error_message: None,
            loading_status: None,
        }
    }
}

impl UiState {
    pub fn progress_percent(&self) -> f64 {
        if self.duration > 0.0 {
            (self.progress / self.duration * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        }
    }

    pub fn volume_bar(&self) -> String {
        let vol = self.volume.clamp(0.0, 1.0);
        let filled = (vol * 20.0) as usize;
        let empty = 20usize.saturating_sub(filled);
        format!("{}█{}", "█".repeat(filled), "░".repeat(empty))
    }
}
