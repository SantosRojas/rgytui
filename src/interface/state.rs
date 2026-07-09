use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;
use crate::infrastructure::audio::spectrum::SpectrumFrame;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ActiveScreen {
    Search,
    Player,
    Help,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Focus {
    SearchInput,
    SearchResults,
    QueueList,
}

#[derive(Clone, Debug)]
pub struct UiState {
    pub active_screen: ActiveScreen,
    pub focus: Focus,
    pub search_query: String,
    pub search_results: Vec<Song>,
    pub is_searching: bool,
    pub player_state: PlayerState,
    pub current_song: Option<Song>,
    pub progress: f64,
    pub duration: f64,
    pub volume: f32,
    pub status_message: Option<String>,
    pub selected_index: usize,
    pub queue_selected: usize,
    pub error_message: Option<String>,
    pub loading_status: Option<String>,
    pub queue_songs: Vec<Song>,
    pub queue_current: usize,
    pub spectrum: SpectrumFrame,
    pub theme_name: String,
    pub accent_color: String,
    pub default_search_limit: usize,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            active_screen: ActiveScreen::Search,
            focus: Focus::SearchInput,
            search_query: String::new(),
            search_results: Vec::new(),
            is_searching: false,
            player_state: PlayerState::Idle,
            current_song: None,
            progress: 0.0,
            duration: 0.0,
            volume: 0.8,
            status_message: None,
            selected_index: 0,
            queue_selected: 0,
            error_message: None,
            loading_status: None,
            queue_songs: Vec::new(),
            queue_current: 0,
            spectrum: SpectrumFrame::default(),
            theme_name: "dark".into(),
            accent_color: "#00ffff".into(),
            default_search_limit: 10,
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
