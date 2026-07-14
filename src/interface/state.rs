use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;
use crate::infrastructure::audio::spectrum::SpectrumFrame;
use crate::interface::i18n::Translations;
use crate::interface::theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug)]
pub struct Notification {
    pub message: String,
    pub level: NotificationLevel,
    pub timestamp: Instant,
    pub duration: Duration,
}

impl Notification {
    pub fn new(message: String, level: NotificationLevel) -> Self {
        let duration = match level {
            NotificationLevel::Error => Duration::from_secs(8),
            _ => Duration::from_secs(4),
        };
        Self { message, level, timestamp: Instant::now(), duration }
    }

    pub fn icon(&self) -> &'static str {
        match self.level {
            NotificationLevel::Info    => "ℹ",
            NotificationLevel::Success => "✓",
            NotificationLevel::Warning => "⚠",
            NotificationLevel::Error   => "✕",
        }
    }

    pub fn expired(&self) -> bool {
        self.timestamp.elapsed() > self.duration
    }
}

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
    pub selected_index: usize,
    pub queue_selected: usize,
    pub loading_status: Option<String>,
    pub queue_songs: Vec<Song>,
    pub queue_current: usize,
    pub spectrum: SpectrumFrame,
    pub theme_name: String,
    pub accent_color: String,
    pub default_search_limit: usize,
    pub settings_focus: usize,
    pub download_path: String,
    pub show_download_popup: bool,
    pub download_format: usize,
    pub download_song: Option<Song>,
    pub spinner_frame: usize,
    pub download_pending: Option<(Song, String, String)>,
    pub notifications: VecDeque<Notification>,
    pub language: String,
    pub translations: Translations,
    pub cached_theme: Option<Theme>,
}

impl UiState {
    pub fn tr(&self, key: &str) -> String {
        self.translations.t(key)
    }

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

    pub fn tick_spinner(&mut self) {
        self.spinner_frame = self.spinner_frame.wrapping_add(1);
    }

    pub fn spinner_char(&self) -> &'static str {
        const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        SPINNER[self.spinner_frame % SPINNER.len()]
    }

    pub fn push_notification(&mut self, message: String, level: NotificationLevel) {
        self.notifications.push_back(Notification::new(message, level));
        if self.notifications.len() > 5 {
            self.notifications.pop_front();
        }
    }

    pub fn dismiss_old_notifications(&mut self) {
        self.notifications.retain(|n| !n.expired());
    }

    pub fn active_notifications(&self) -> impl Iterator<Item = &Notification> {
        self.notifications.iter().filter(|n| !n.expired())
    }

    pub fn get_or_create_theme(&mut self) -> Theme {
        if let Some(theme) = self.cached_theme {
            return theme;
        }
        let theme = Theme::from_settings(&self.theme_name, &self.accent_color);
        self.cached_theme = Some(theme);
        theme
    }

    pub fn invalidate_theme(&mut self) {
        self.cached_theme = None;
    }
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
            selected_index: 0,
            queue_selected: 0,
            loading_status: None,
            queue_songs: Vec::new(),
            queue_current: 0,
            spectrum: SpectrumFrame::default(),
            theme_name: "dark".into(),
            accent_color: "#00ffff".into(),
            default_search_limit: 10,
            settings_focus: 0,
            download_path: String::new(),
            show_download_popup: false,
            download_format: 0,
            download_song: None,
            download_pending: None,
            notifications: VecDeque::new(),
            spinner_frame: 0,
            language: "es".into(),
            translations: Translations::load("es"),
            cached_theme: None,
        }
    }
}
