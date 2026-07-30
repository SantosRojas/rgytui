pub mod config;
pub mod download;
pub mod notification;
pub mod player;
pub mod queue;
pub mod render;
pub mod search;
pub mod settings;
pub use config::*;
pub use download::*;
pub use notification::*;
pub use player::*;
pub use queue::*;
pub use render::*;
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
    pub config: ConfigState,
    pub settings: SettingsState,
    pub download: DownloadPopupState,
    pub notifications: NotificationState,
    pub show_exit_confirmation: bool,
    pub show_upgrade_popup: bool,
    /// (version_tag, download_url)
    pub pending_upgrade: Option<(String, String)>,
    pub is_upgrading: bool,
    pub player: PlayerViewState,
    pub queue: QueueViewState,
}

impl UiState {
    pub fn tr(&self, key: &str) -> String {
        self.config.tr(key)
    }

    pub fn tick_spinner(&mut self) {
        self.player.tick_spinner();
    }

    pub fn spinner_char(&self) -> &'static str {
        self.player.spinner_char()
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
            config: ConfigState::default(),
            settings: SettingsState::default(),
            download: DownloadPopupState::default(),
            notifications: NotificationState::default(),
            player: PlayerViewState::default(),
            queue: QueueViewState::default(),
            show_exit_confirmation: false,
            show_upgrade_popup: false,
            pending_upgrade: None,
            is_upgrading: false,
        }
    }
}
