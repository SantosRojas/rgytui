use super::*;

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::interface::i18n::Translations;

impl App {
    #[allow(clippy::collapsible_match)]
    pub(crate) async fn handle_key(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        // Handle exit confirmation response first
        if self.ui.show_exit_confirmation {
            return match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.ui.show_exit_confirmation = false;
                    Ok(true)
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                    self.ui.show_exit_confirmation = false;
                    Ok(false)
                }
                _ => Ok(false),
            };
        }

        // Handle upgrade confirmation response first (higher priority than exit)
        if self.ui.show_upgrade_popup {
            return self.handle_upgrade_popup_key(key);
        }

        // Block exit during upgrade — the binary is being replaced
        if self.ui.is_upgrading {
            let wants_exit = matches!(key.code, KeyCode::Char('q' | 'Q'))
                || (matches!(key.code, KeyCode::Char('c'))
                    && key.modifiers.contains(KeyModifiers::CONTROL));
            if wants_exit {
                self.ui.push_notification(
                    self.ui.tr("upgrade_in_progress"),
                    NotificationLevel::Warning,
                );
                return Ok(false);
            }
        }

        // Universal keys — always work regardless of screen or focus
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(true);
        }
        // Ctrl+C — graceful exit (intercept before regular 'c' key dispatch)
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if self.pending_play.is_some() || self.ui.download.download_pending.is_some() {
                let _ = self.event_tx.send(AppEvent::ShowConfirmExit).await;
                return Ok(false);
            }
            return Ok(true);
        }

        // Download format popup handler (priority check before universal keys intercept)
        if self.ui.download.show_download_popup {
            return self.handle_download_popup_key(key);
        }

        // Global toggle keys
        if key.code == KeyCode::Esc {
            match self.ui.active_screen {
                ActiveScreen::Help | ActiveScreen::Settings | ActiveScreen::Player => {
                    self.ui.active_screen = ActiveScreen::Search;
                    self.ui.focus = Focus::SearchInput;
                }
                ActiveScreen::Search => {
                    if self.ui.focus == Focus::SearchInput && !self.ui.search.search_query.is_empty() {
                        self.ui.search.search_query.clear();
                    } else {
                        self.ui.focus = match self.ui.focus {
                            Focus::SearchInput | Focus::SearchResults => Focus::SearchInput,
                            Focus::QueueList => Focus::SearchResults,
                        };
                    }
                }
            }
            return Ok(false);
        }

        // Screen-specific handlers
        if self.ui.active_screen == ActiveScreen::Help {
            return self.handle_help_key(key);
        }

        if self.ui.active_screen == ActiveScreen::Settings {
            return self.handle_settings_key(key).await;
        }

        if self.ui.active_screen == ActiveScreen::Player {
            return self.handle_player_key(key);
        }

        // Search/Queue screen — dispatch by focus
        self.handle_search_queue_key(key).await
    }

    fn handle_upgrade_popup_key(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        use crate::interface::state::UpgradeChoice;

        match key.code {
            KeyCode::Left => {
                self.ui.upgrade_selection = UpgradeChoice::Yes;
            }
            KeyCode::Right => {
                self.ui.upgrade_selection = UpgradeChoice::No;
            }
            KeyCode::Enter => {
                match self.ui.upgrade_selection {
                    UpgradeChoice::Yes => self.start_upgrade(),
                    UpgradeChoice::No => {
                        self.ui.show_upgrade_popup = false;
                        self.ui.pending_upgrade = None;
                    }
                }
            }
            // Backward compat: y/Y also confirms, n/N/Esc dismisses
            KeyCode::Char('y') | KeyCode::Char('Y') => self.start_upgrade(),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.ui.show_upgrade_popup = false;
                self.ui.pending_upgrade = None;
            }
            _ => {}
        }
        Ok(false)
    }

    pub(crate) fn start_upgrade(&mut self) {
        use crate::interface::state::{NotificationLevel, UpgradeChoice};

        self.ui.show_upgrade_popup = false;
        self.ui.is_upgrading = true;
        self.ui.upgrade_selection = UpgradeChoice::Yes; // reset for next time
        self.ui.push_notification(
            self.ui.tr("upgrade_downloading"),
            NotificationLevel::Info,
        );
        if let Some((version, url)) = self.ui.pending_upgrade.take() {
            let event_tx = self.event_tx.clone();
            tokio::spawn(async move {
                let result = tokio::task::spawn_blocking(move || {
                    crate::update::perform_upgrade(&version, &url)
                })
                .await;
                let msg = match result {
                    Ok(Ok(())) => "upgrade_complete".into(),
                    Ok(Err(e)) => format!("upgrade_failed: {e}"),
                    Err(e) => format!("upgrade_failed: spawn error {e}"),
                };
                let _ = event_tx.send(AppEvent::Notification(msg)).await;
            });
        }
    }

    fn handle_download_popup_key(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        match key.code {
            KeyCode::Up => {
                self.ui.download.download_format = self.ui.download.download_format.saturating_sub(1);
            }
            KeyCode::Down => {
                self.ui.download.download_format = (self.ui.download.download_format + 1).min(4);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(song) = self.ui.download.download_song.take() {
                    let dir = self.ui.config.download_path.clone();
                    let fmt = match self.ui.download.download_format {
                        0 => "m4a",
                        1 => "mp3",
                        2 => "opus",
                        3 => "flac",
                        _ => "wav",
                    }
                    .to_string();
                    self.ui.download.show_download_popup = false;
                    self.ui.download.download_pending = Some((song, dir, fmt));
                } else {
                    self.ui.download.show_download_popup = false;
                }
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.ui.download.show_download_popup = false;
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_help_key(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                // q — close help (NOT exit app)
                self.ui.active_screen = ActiveScreen::Search;
                Ok(false)
            }
            KeyCode::Esc | KeyCode::Char('h') => {
                self.ui.active_screen = ActiveScreen::Search;
                Ok(false)
            }
            _ => Ok(false), // all other keys silently ignored
        }
    }

    async fn handle_settings_key(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.ui.active_screen = ActiveScreen::Search;
                self.ui.focus = Focus::SearchInput;
            }
            KeyCode::Esc => {
                self.ui.active_screen = ActiveScreen::Search;
                self.ui.focus = Focus::SearchInput;
            }
            KeyCode::Up => {
                self.ui.settings.settings_focus = self.ui.settings.settings_focus.saturating_sub(1);
            }
            KeyCode::Down => {
                self.ui.settings.settings_focus = (self.ui.settings.settings_focus + 1).min(7);
            }
            KeyCode::Enter | KeyCode::Char(' ') => match self.ui.settings.settings_focus {
                0 => {
                    self.ui.config.theme_name = if self.ui.config.theme_name == "dark" {
                        "light".into()
                    } else {
                        "dark".into()
                    };
                    self.ui.invalidate_theme();
                    self.ui.push_notification(
                        self.ui.tr("notif_theme").replace("{}", &self.ui.config.theme_name),
                        NotificationLevel::Info,
                    );
                }
                1 => {
                    const PRESETS: &[&str] = &[
                        "#00ddff", "#ff77aa", "#55ff77", "#ffaa22",
                        "#aa66ff", "#ff6644", "#44ddff", "#ff44aa",
                        "#88dd00", "#00ffbb", "#dd88ff", "#ffbb33",
                        "#33ffaa", "#ff8833", "#6699ff", "#ff5599",
                    ];
                    let i = PRESETS
                        .iter()
                        .position(|&c| c == self.ui.config.accent_color)
                        .map(|i| (i + 1) % PRESETS.len())
                        .unwrap_or(0);
                    self.ui.config.accent_color = PRESETS[i].to_string();
                    self.ui.invalidate_theme();
                    self.ui.push_notification(
                        self.ui.tr("notif_accent"),
                        NotificationLevel::Info,
                    );
                }
                4 => {
                    // Try native file dialog; fall back gracefully in headless
                    // environments (SSH, tmux, WSL without X server) where rfd
                    // would panic or return None.
                    let dir = tokio::task::spawn_blocking(|| {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            rfd::FileDialog::new()
                                .set_title("Select Download Folder")
                                .pick_folder()
                        }))
                        .ok()
                        .flatten()
                    })
                    .await
                    .unwrap_or(None);
                    if let Some(p) = dir {
                        self.ui.config.download_path = p.to_string_lossy().to_string();
                    } else {
                        self.ui.push_notification(
                            self.ui.tr("notif_manual_path"),
                            NotificationLevel::Info,
                        );
                    }
                }
                5 => {
                    self.ui.config.language = if self.ui.config.language == "es" {
                        "en".into()
                    } else {
                        "es".into()
                    };
                    self.ui.config.translations = Arc::new(Translations::load(&self.ui.config.language));
                }
                6 => {
                    // Mirror the player-screen 'v' key: toggle mode via
                    // PlaybackUseCase (which validates mpv presence for video).
                    if let Err(e) = self.playback.toggle_mode().await {
                        self.ui.push_notification(
                            self.ui.tr("err_playback").replace("{}", &e.to_string()),
                            NotificationLevel::Error,
                        );
                    } else {
                        let mode = self.playback.mode();
                        self.settings.audio_mode = matches!(mode, AudioMode::Video);
                        let label = match mode {
                            AudioMode::Audio => self.ui.tr("status_audio"),
                            AudioMode::Video => self.ui.tr("status_video"),
                        };
                        self.ui.push_notification(
                            self.ui.tr("notif_mode").replace("{}", &label),
                            NotificationLevel::Info,
                        );
                    }
                }
                7 => {
                    let next = self.playlist.repeat_mode().next();
                    let mode = self.playlist.set_repeat_mode(next);
                    self.settings.repeat_mode = mode.as_str().to_string();
                    let key = match mode {
                        RepeatMode::None => "notif_repeat_off",
                        RepeatMode::All => "notif_repeat_all",
                        RepeatMode::One => "notif_repeat_one",
                    };
                    self.ui.push_notification(self.ui.tr(key), NotificationLevel::Info);
                }
                _ => {}
            },
            KeyCode::Char('=') | KeyCode::Char('+') => match self.ui.settings.settings_focus {
                2 => {
                    let vol = (self.playback.volume() + 0.05).min(1.0);
                    self.playback.set_volume(vol);
                }
                3 => {
                    self.ui.config.default_search_limit =
                        (self.ui.config.default_search_limit + 5).min(50);
                }
                _ => {}
            },
            KeyCode::Char('-') | KeyCode::Char('_') => match self.ui.settings.settings_focus {
                2 => {
                    let vol = (self.playback.volume() - 0.05).max(0.0);
                    self.playback.set_volume(vol);
                }
                3 => {
                    self.ui.config.default_search_limit = self
                        .ui
                        .config
                        .default_search_limit
                        .saturating_sub(5)
                        .max(1);
                }
                _ => {}
            },
            KeyCode::Char(c) if self.ui.settings.settings_focus == 4 => {
                if self.ui.config.download_path.len() < 300 {
                    self.ui.config.download_path.push(c);
                }
            }
            KeyCode::Backspace if self.ui.settings.settings_focus == 4 => {
                self.ui.config.download_path.pop();
            }
            _ => {}
        }
        Ok(false)
    }

    #[allow(clippy::collapsible_match)]
    fn handle_player_key(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        match key.code {
            KeyCode::Char(' ') => match self.playback.state() {
                PlayerState::Playing => {
                    if self.playback.pause().is_ok() {
                        self.ui.push_notification(
                            self.ui.tr("notif_paused"),
                            NotificationLevel::Info,
                        );
                    }
                }
                PlayerState::Paused => {
                    if self.playback.resume().is_ok() {
                        self.ui.push_notification(
                            self.ui.tr("notif_resumed"),
                            NotificationLevel::Info,
                        );
                    }
                }
                _ => {}
            },
            KeyCode::Char('n') => {
                if let Some(next) = self.playlist.next().cloned() {
                    self.queue_play(next);
                }
            }
            KeyCode::Char('p') => {
                if let Some(prev) = self.playlist.previous().cloned() {
                    self.queue_play(prev);
                }
            }
            KeyCode::Char('r') => {
                let mode = self.playlist.cycle_repeat_mode();
                let key = match mode {
                    RepeatMode::None => "notif_repeat_off",
                    RepeatMode::All => "notif_repeat_all",
                    RepeatMode::One => "notif_repeat_one",
                };
                self.ui.push_notification(self.ui.tr(key), NotificationLevel::Info);
            }
            KeyCode::Char('=') | KeyCode::Char('+') => {
                let vol = (self.playback.volume() + 0.05).min(1.0);
                self.playback.set_volume(vol);
                self.ui.push_notification(
                    self.ui.tr("notif_volume")
                        .replace("{:.0}", &format!("{:.0}", vol * 100.0)),
                    NotificationLevel::Info,
                );
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                let vol = (self.playback.volume() - 0.05).max(0.0);
                self.playback.set_volume(vol);
                self.ui.push_notification(
                    self.ui.tr("notif_volume")
                        .replace("{:.0}", &format!("{:.0}", vol * 100.0)),
                    NotificationLevel::Info,
                );
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                // q — exit player, return to search
                self.ui.active_screen = ActiveScreen::Search;
                self.ui.focus = Focus::SearchInput;
            }
            KeyCode::Char('s') => {
                self.ui.active_screen = ActiveScreen::Settings;
                self.ui.focus = Focus::SearchInput;
            }
            _ => {}
        }
        Ok(false)
    }

    #[allow(clippy::collapsible_match)]
    async fn handle_search_queue_key(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        // '/' — always switches to search input, regardless of focus
        if key.code == KeyCode::Char('/') {
            self.ui.focus = Focus::SearchInput;
            self.ui.search.search_query.clear();
            self.ui.active_screen = ActiveScreen::Search;
            return Ok(false);
        }

        // Delegate by focus
        match self.ui.focus {
            Focus::SearchInput => self.handle_search_input(key),
            Focus::SearchResults | Focus::QueueList => {
                // Dispatch by key code to the appropriate sub-handler.
                // Each key matches exactly one handler — no fallthrough.
                match key.code {
                    // Ctrl+u / Ctrl+d — always navigation (scroll)
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.handle_list_navigation(key)
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.handle_list_navigation(key)
                    }
                    // Navigation keys
                    KeyCode::BackTab
                    | KeyCode::Tab
                    | KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Char('j' | 'k' | 'g' | 'G' | 'h') => {
                        self.handle_list_navigation(key)
                    }
                    // Playback controls
                    KeyCode::Char(' ')
                    | KeyCode::Char('n' | 'p' | 'r' | '=' | '+' | '-' | '_' | 'v' | 's' | 'q' | 'Q') => {
                        self.handle_playback_controls(key).await
                    }
                    // Queue actions (including Enter and plain d)
                    KeyCode::Enter | KeyCode::Delete | KeyCode::Char('a' | 'd' | 'C' | 'c') => {
                        self.handle_queue_actions(key).await
                    }
                    _ => Ok(false),
                }
            }
        }
    }

    /// Keys active when focus is on the search input field: Tab, BackTab,
    /// Backspace, typeable chars, and Enter (spawn search).
    fn handle_search_input(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        match key.code {
            KeyCode::Tab => {
                self.ui.focus = Focus::SearchResults;
            }
            KeyCode::BackTab => {
                self.ui.focus = Focus::QueueList;
            }
            KeyCode::Enter => {
                if !self.ui.search.search_query.is_empty() {
                    // Debounce: ignore rapid consecutive searches
                    if let Some(last) = self.last_search
                        && last.elapsed() < Duration::from_millis(300)
                    {
                        // too soon — skip
                    } else {
                        self.last_search = Some(Instant::now());
                        self.ui.search.is_searching = true;
                        self.ui.search.search_results.clear();
                        let query = self.ui.search.search_query.clone();
                        self.spawn_search(query, self.ui.config.default_search_limit);
                    }
                }
            }
            KeyCode::Backspace => {
                self.ui.search.search_query.pop();
            }
            KeyCode::Char(c) if self.ui.search.search_query.len() < 200 => {
                self.ui.search.search_query.push(c);
            }
            _ => {}
        }
        Ok(false)
    }

    /// List navigation keys: Tab/Shift+Tab, j/k, g/G, Ctrl+u/d, Up/Down, h.
    fn handle_list_navigation(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        match key.code {
            KeyCode::BackTab => {
                // Shift+Tab — reverse focus cycle
                self.ui.focus = match self.ui.focus {
                    Focus::SearchInput => Focus::QueueList,
                    Focus::SearchResults => Focus::SearchInput,
                    Focus::QueueList => Focus::SearchResults,
                };
            }
            KeyCode::Char('j') if self.ui.focus != Focus::SearchInput => {
                // j — Down in lists
                match self.ui.focus {
                    Focus::SearchResults => {
                        self.ui.search.selected_index = (self.ui.search.selected_index + 1)
                            .min(self.ui.search.search_results.len().saturating_sub(1));
                    }
                    Focus::QueueList if !self.playlist.songs().is_empty() => {
                        self.ui.queue.queue_selected = (self.ui.queue.queue_selected + 1)
                            .min(self.playlist.songs().len().saturating_sub(1));
                    }
                    _ => {}
                }
            }
            KeyCode::Char('k') if self.ui.focus != Focus::SearchInput => {
                // k — Up in lists
                match self.ui.focus {
                    Focus::SearchResults => {
                        self.ui.search.selected_index = self.ui.search.selected_index.saturating_sub(1);
                    }
                    Focus::QueueList if !self.playlist.songs().is_empty() => {
                        self.ui.queue.queue_selected = self.ui.queue.queue_selected.saturating_sub(1);
                    }
                    _ => {}
                }
            }
            KeyCode::Char('g') if self.ui.focus != Focus::SearchInput => {
                // g — Go to top of list
                match self.ui.focus {
                    Focus::SearchResults => self.ui.search.selected_index = 0,
                    Focus::QueueList => self.ui.queue.queue_selected = 0,
                    _ => {}
                }
            }
            KeyCode::Char('G') if self.ui.focus != Focus::SearchInput => {
                // G — Go to bottom of list
                match self.ui.focus {
                    Focus::SearchResults => {
                        self.ui.search.selected_index =
                            self.ui.search.search_results.len().saturating_sub(1);
                    }
                    Focus::QueueList => {
                        self.ui.queue.queue_selected =
                            self.playlist.songs().len().saturating_sub(1);
                    }
                    _ => {}
                }
            }
            KeyCode::Char('h') if self.ui.focus != Focus::SearchInput => {
                // h — Toggle help (alias for ?)
                self.ui.active_screen = if self.ui.active_screen == ActiveScreen::Help {
                    ActiveScreen::Search
                } else {
                    ActiveScreen::Help
                };
            }
            KeyCode::Char('u')
                if self.ui.focus != Focus::SearchInput
                    && key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                // Ctrl+u — scroll half page up
                match self.ui.focus {
                    Focus::SearchResults => {
                        self.ui.search.selected_index =
                            self.ui.search.selected_index.saturating_sub(10);
                    }
                    Focus::QueueList => {
                        self.ui.queue.queue_selected =
                            self.ui.queue.queue_selected.saturating_sub(10);
                    }
                    _ => {}
                }
            }
            KeyCode::Char('d')
                if self.ui.focus != Focus::SearchInput
                    && key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                // Ctrl+d — scroll half page down
                match self.ui.focus {
                    Focus::SearchResults => {
                        let max = self.ui.search.search_results.len().saturating_sub(1);
                        self.ui.search.selected_index =
                            (self.ui.search.selected_index + 10).min(max);
                    }
                    Focus::QueueList => {
                        let max = self.playlist.songs().len().saturating_sub(1);
                        self.ui.queue.queue_selected =
                            (self.ui.queue.queue_selected + 10).min(max);
                    }
                    _ => {}
                }
            }
            KeyCode::Tab => {
                self.ui.focus = match self.ui.focus {
                    Focus::SearchInput => Focus::SearchResults,
                    Focus::SearchResults => Focus::QueueList,
                    Focus::QueueList => Focus::SearchInput,
                };
            }
            KeyCode::Up => match self.ui.focus {
                Focus::SearchResults => {
                    self.ui.search.selected_index = self.ui.search.selected_index.saturating_sub(1);
                }
                Focus::QueueList if !self.playlist.songs().is_empty() => {
                    self.ui.queue.queue_selected = self.ui.queue.queue_selected.saturating_sub(1);
                }
                _ => {}
            },
            KeyCode::Down => match self.ui.focus {
                Focus::SearchResults => {
                    self.ui.search.selected_index = (self.ui.search.selected_index + 1)
                        .min(self.ui.search.search_results.len().saturating_sub(1));
                }
                Focus::QueueList if !self.playlist.songs().is_empty() => {
                    self.ui.queue.queue_selected = (self.ui.queue.queue_selected + 1)
                        .min(self.playlist.songs().len().saturating_sub(1));
                }
                _ => {}
            },
            _ => {}
        }
        Ok(false)
    }

    /// Playback control keys: space (play/pause), n/p (next/prev), r (repeat),
    /// +/- (volume), v (toggle video mode), s (settings), q/Q (exit).
    async fn handle_playback_controls(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        match key.code {
            KeyCode::Char(' ') => match self.playback.state() {
                PlayerState::Playing => {
                    if self.playback.pause().is_ok() {
                        self.ui.push_notification(
                            self.ui.tr("notif_paused"),
                            NotificationLevel::Info,
                        );
                    }
                }
                PlayerState::Paused => {
                    if self.playback.resume().is_ok() {
                        self.ui.push_notification(
                            self.ui.tr("notif_resumed"),
                            NotificationLevel::Info,
                        );
                    }
                }
                _ => {
                    self.schedule_play_selected();
                    self.try_save_playlist().await;
                }
            },
            KeyCode::Char('n') if self.ui.focus != Focus::SearchInput => {
                if let Some(next) = self.playlist.next().cloned() {
                    self.queue_play(next);
                }
            }
            KeyCode::Char('p') if self.ui.focus != Focus::SearchInput => {
                if let Some(prev) = self.playlist.previous().cloned() {
                    self.queue_play(prev);
                }
            }
            KeyCode::Char('r') if self.ui.focus != Focus::SearchInput => {
                let mode = self.playlist.cycle_repeat_mode();
                let key = match mode {
                    RepeatMode::None => "notif_repeat_off",
                    RepeatMode::All => "notif_repeat_all",
                    RepeatMode::One => "notif_repeat_one",
                };
                self.ui.push_notification(self.ui.tr(key), NotificationLevel::Info);
            }
            KeyCode::Char('=') | KeyCode::Char('+') => {
                let vol = (self.playback.volume() + 0.05).min(1.0);
                self.playback.set_volume(vol);
                self.ui.push_notification(
                    self.ui.tr("notif_volume")
                        .replace("{:.0}", &format!("{:.0}", vol * 100.0)),
                    NotificationLevel::Info,
                );
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                let vol = (self.playback.volume() - 0.05).max(0.0);
                self.playback.set_volume(vol);
                self.ui.push_notification(
                    self.ui.tr("notif_volume")
                        .replace("{:.0}", &format!("{:.0}", vol * 100.0)),
                    NotificationLevel::Info,
                );
            }
            KeyCode::Char('v') => {
                if let Err(e) = self.playback.toggle_mode().await {
                    self.ui.push_notification(
                        self.ui.tr("err_playback").replace("{}", &e.to_string()),
                        NotificationLevel::Error,
                    );
                }
            }
            KeyCode::Char('s') => {
                self.ui.active_screen = ActiveScreen::Settings;
                self.ui.focus = Focus::SearchInput;
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(true),
            _ => {}
        }
        Ok(false)
    }

    /// Queue/playlist action keys: Enter (play selected), a (add to queue),
    /// Delete (remove from queue), d (download popup), C/c (clear queue).
    async fn handle_queue_actions(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        match key.code {
            KeyCode::Enter => match self.ui.focus {
                Focus::SearchResults => {
                    self.schedule_play_selected();
                    self.try_save_playlist().await;
                }
                Focus::QueueList => {
                    self.play_selected_from_queue();
                }
                _ => {}
            },
            KeyCode::Char('a') => {
                if self.ui.focus == Focus::SearchResults && !self.ui.search.search_results.is_empty() {
                    let idx = self.ui.search.selected_index;
                    if let Some(song) = self.ui.search.search_results.get(idx) {
                        if self.playlist.songs().iter().any(|s| s.id == song.id) {
                            self.ui.push_notification(
                                self.ui.tr("notif_already_in_queue")
                                    .replace("{}", &song.title),
                                NotificationLevel::Warning,
                            );
                        } else {
                            let song = song.clone();
                            self.playlist.add(song.clone());
                            self.try_save_playlist().await;
                            self.ui.push_notification(
                                self.ui.tr("notif_added_to_queue")
                                    .replace("{}", &song.title),
                                NotificationLevel::Success,
                            );
                        }
                    }
                }
            }
            KeyCode::Delete => {
                if self.ui.focus == Focus::QueueList && !self.playlist.songs().is_empty() {
                    let idx = self.ui.queue.queue_selected;
                    // Remove cached audio before removing from playlist
                    if let Some(song) = self.playlist.songs().get(idx) {
                        let song_id = song.id.clone();
                        if let Err(e) = self.audio_cache.remove(&song_id).await {
                            tracing::warn!("Failed to remove cache for '{}': {e}", song_id);
                        }
                    }
                    self.playlist.remove(idx);
                    self.try_save_playlist().await;
                }
            }
            KeyCode::Char('d') => {
                match self.ui.focus {
                    Focus::SearchResults => {
                        self.ui.download.download_song =
                            self.ui.search.search_results.get(self.ui.search.selected_index).cloned();
                    }
                    Focus::QueueList => {
                        self.ui.download.download_song =
                            self.playlist.songs().get(self.ui.queue.queue_selected).cloned();
                    }
                    _ => {}
                }
                if self.ui.download.download_song.is_some() {
                    self.ui.download.show_download_popup = true;
                    self.ui.download.download_format = 0;
                }
            }
            KeyCode::Char('C') | KeyCode::Char('c') if self.ui.focus == Focus::QueueList => {
                // Remove cached audio for all queued songs before clearing
                let ids: Vec<String> = self.playlist.songs().iter().map(|s| s.id.clone()).collect();
                for id in &ids {
                    if let Err(e) = self.audio_cache.remove(id).await {
                        tracing::warn!("Failed to remove cache for '{}': {e}", id);
                    }
                }
                self.playlist.clear();
                self.try_save_playlist().await;
            }
            _ => {}
        }
        Ok(false)
    }
}
