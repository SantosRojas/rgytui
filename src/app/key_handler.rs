use super::*;

use std::time::{Duration, Instant};

use crate::interface::i18n::Translations;

impl App {
    #[allow(clippy::collapsible_match)]
    pub(crate) async fn handle_key(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        // Universal keys — always work regardless of screen or focus
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(true);
        }
        // Ctrl+C — graceful exit (intercept before regular 'c' key dispatch)
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(true);
        }

        // Download format popup handler (priority check before universal keys intercept)
        if self.ui.download.show_download_popup {
            return self.handle_download_popup_key(key);
        }

        // Global toggle keys
        match key.code {
            KeyCode::Esc => {
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
            _ => {}
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
        self.handle_search_queue_key(key)
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
            KeyCode::Esc => {
                self.ui.download.show_download_popup = false;
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_help_key(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        match key.code {
            KeyCode::Char('q') => {
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
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Esc => {
                self.ui.active_screen = ActiveScreen::Search;
                self.ui.focus = Focus::SearchInput;
            }
            KeyCode::Up => {
                self.ui.settings.settings_focus = self.ui.settings.settings_focus.saturating_sub(1);
            }
            KeyCode::Down => {
                self.ui.settings.settings_focus = (self.ui.settings.settings_focus + 1).min(5);
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
                    let dir = tokio::task::spawn_blocking(|| {
                        rfd::FileDialog::new()
                            .set_title("Select Download Folder")
                            .pick_folder()
                    })
                    .await
                    .unwrap_or(None);
                    if let Some(p) = dir {
                        self.ui.config.download_path = p.to_string_lossy().to_string();
                    }
                }
                5 => {
                    self.ui.config.language = if self.ui.config.language == "es" {
                        "en".into()
                    } else {
                        "es".into()
                    };
                    self.ui.config.translations = Translations::load(&self.ui.config.language);
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
            KeyCode::Char('q') => {
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
    fn handle_search_queue_key(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        match key.code {
            // ── Keyboard shortcuts (focus-gated — NOT in SearchInput) ──
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
                    Focus::QueueList => {
                        if !self.playlist.songs().is_empty() {
                            self.ui.queue.queue_selected = (self.ui.queue.queue_selected + 1)
                                .min(self.playlist.songs().len().saturating_sub(1));
                        }
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
                    Focus::QueueList => {
                        if !self.playlist.songs().is_empty() {
                            self.ui.queue.queue_selected = self.ui.queue.queue_selected.saturating_sub(1);
                        }
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
            // ── Standard keys ──
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
                Focus::QueueList => {
                    if !self.playlist.songs().is_empty() {
                        self.ui.queue.queue_selected = self.ui.queue.queue_selected.saturating_sub(1);
                    }
                }
                _ => {}
            },
            KeyCode::Down => match self.ui.focus {
                Focus::SearchResults => {
                    self.ui.search.selected_index = (self.ui.search.selected_index + 1)
                        .min(self.ui.search.search_results.len().saturating_sub(1));
                }
                Focus::QueueList => {
                    if !self.playlist.songs().is_empty() {
                        self.ui.queue.queue_selected = (self.ui.queue.queue_selected + 1)
                            .min(self.playlist.songs().len().saturating_sub(1));
                    }
                }
                _ => {}
            },
            KeyCode::Enter => match self.ui.focus {
                Focus::SearchInput => {
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
                Focus::SearchResults => {
                    self.schedule_play_selected();
                }
                Focus::QueueList => {
                    let idx = self.ui.queue.queue_selected;
                    if idx < self.playlist.songs().len() {
                        self.playlist.set_current_index(idx);
                        if let Some(song) = self.playlist.current_song_cloned() {
                            self.queue_play(song);
                        }
                    }
                }
            },
            KeyCode::Backspace => {
                if self.ui.focus == Focus::SearchInput {
                    self.ui.search.search_query.pop();
                }
            }
            KeyCode::Char(c) if self.ui.focus == Focus::SearchInput => {
                if self.ui.search.search_query.len() < 200 {
                    self.ui.search.search_query.push(c);
                }
            }
            KeyCode::Char('/') => {
                self.ui.focus = Focus::SearchInput;
                self.ui.search.search_query.clear();
                self.ui.active_screen = ActiveScreen::Search;
            }
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
                }
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
                if let Err(e) = self.playback.toggle_mode() {
                    self.ui.push_notification(
                        self.ui.tr("err_playback").replace("{}", &e.to_string()),
                        NotificationLevel::Error,
                    );
                }
            }
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
                            self.playlist.add(song.clone());
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
                    self.playlist.remove(idx);
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
            KeyCode::Char('C') | KeyCode::Char('c') => {
                if self.ui.focus == Focus::QueueList {
                    self.playlist.clear();
                }
            }
            KeyCode::Char('s') => {
                self.ui.active_screen = ActiveScreen::Settings;
                self.ui.focus = Focus::SearchInput;
            }
            KeyCode::Char('q') => return Ok(true),
            _ => {}
        }

        Ok(false)
    }
}
