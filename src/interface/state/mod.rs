use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;
use crate::shared::spectrum::SpectrumFrame;

pub mod config;
pub mod download;
pub mod notification;
pub mod search;
pub mod settings;
pub use config::*;
pub use download::*;
pub use notification::*;
pub use search::*;
pub use settings::*;

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
    pub search: SearchState,
    pub player_state: PlayerState,
    pub current_song: Option<Song>,
    pub progress: f64,
    pub duration: f64,
    pub volume: f32,
    pub queue_selected: usize,
    pub loading_status: Option<String>,
    pub queue_songs: Vec<Song>,
    pub queue_current: usize,
    pub spectrum: SpectrumFrame,
    pub config: ConfigState,
    pub settings: SettingsState,
    pub download: DownloadPopupState,
    pub spinner_frame: usize,
    pub notifications: NotificationState,
}

impl UiState {
    pub fn tr(&self, key: &str) -> String {
        self.config.tr(key)
    }

    pub fn progress_percent(&self) -> f64 {
        if self.duration > 0.0 {
            (self.progress / self.duration * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        }
    }

    #[allow(dead_code)]
    pub fn volume_bar(&self) -> String {
        let vol = self.volume.clamp(0.0, 1.0);
        let filled = (vol * 20.0) as usize;
        let empty = 20usize.saturating_sub(filled);
        format!("{}█{}", "█".repeat(filled), "░".repeat(empty))
    }

    pub fn tick_spinner(&mut self) {
        self.spinner_frame = self.spinner_frame.wrapping_add(1);
    }

    pub fn spinner_char(&self) -> &'static str {
        const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        SPINNER[self.spinner_frame % SPINNER.len()]
    }

    pub fn push_notification(&mut self, message: String, level: NotificationLevel) {
        self.notifications.push(message, level);
    }

    pub fn dismiss_old_notifications(&mut self) {
        self.notifications.dismiss_old();
    }

    pub fn active_notifications(&self) -> impl Iterator<Item = &Notification> {
        self.notifications.active()
    }

    pub fn get_or_create_theme(&mut self) -> crate::interface::theme::Theme {
        self.config.get_or_create_theme()
    }

    pub fn invalidate_theme(&mut self) {
        self.config.invalidate_theme();
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            active_screen: ActiveScreen::Search,
            focus: Focus::SearchInput,
            search: SearchState::default(),
            player_state: PlayerState::Idle,
            current_song: None,
            progress: 0.0,
            duration: 0.0,
            volume: 0.8,
            queue_selected: 0,
            loading_status: None,
            queue_songs: Vec::new(),
            queue_current: 0,
            spectrum: SpectrumFrame::default(),
            config: ConfigState::default(),
            settings: SettingsState::default(),
            download: DownloadPopupState::default(),
            notifications: NotificationState::default(),
            spinner_frame: 0,
        }
    }
}
