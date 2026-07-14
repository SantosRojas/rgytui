use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{enable_raw_mode, EnterAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::{mpsc, Semaphore};

use crate::application::playback::PlaybackUseCase;
use crate::application::playlist::PlaylistUseCase;
use crate::application::search::SearchUseCase;
use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;
use crate::infrastructure::config::store::ConfigStore;
use crate::interface::app_ui;
use crate::interface::i18n::Translations;
use crate::interface::state::{ActiveScreen, Focus, NotificationLevel, UiState};
use crate::shared::event::AppEvent;

pub struct App {
    ui: UiState,
    playback: PlaybackUseCase,
    search: SearchUseCase,
    playlist: PlaylistUseCase,
    config: ConfigStore,
    keyboard_rx: mpsc::UnboundedReceiver<KeyEvent>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    pending_play: Option<Song>,
    last_search: Option<Instant>,
    download_semaphore: Arc<Semaphore>,
    last_playlist_version: usize,
}

impl App {
    pub fn new(
        playback: PlaybackUseCase,
        search: SearchUseCase,
        playlist: PlaylistUseCase,
        config: ConfigStore,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (kb_tx, keyboard_rx) = mpsc::unbounded_channel();

        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loop {
                if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    if kb_tx.send(key).is_err() {
                        break;
                    }
                }
            }));
            if let Err(e) = result {
                tracing::error!("Keyboard thread panicked: {:?}", e);
            }
        });

        let settings = config.settings();
        let language = settings.language.clone();
        let translations = Translations::load(&language);
        let ui = UiState {
            volume: settings.volume.clamp(0.0, 1.0),
            theme_name: settings.theme.clone(),
            accent_color: settings.accent_color.clone(),
            default_search_limit: settings.default_search_limit,
            download_path: settings.download_path.clone(),
            language: language.clone(),
            translations,
            ..UiState::default()
        };

        Self {
            ui,
            playback,
            search,
            playlist,
            config,
            keyboard_rx,
            event_tx,
            event_rx,
            pending_play: None,
            last_search: None,
            download_semaphore: Arc::new(Semaphore::new(3)),
            last_playlist_version: 0,
        }
    }

    fn init_terminal() -> std::io::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
        enable_raw_mode()?;
        std::io::stdout().execute(EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(std::io::stdout());
        Terminal::new(backend)
    }

    fn sync_ui_queue(&mut self) {
        let current_version = self.playlist.playlist().version;
        if current_version != self.last_playlist_version {
            self.ui.queue_songs = self.playlist.songs().to_vec();
            self.last_playlist_version = current_version;
        }
        if let Some(song) = self.playlist.playlist().current_song()
            && let Some(pos) = self.ui.queue_songs.iter().position(|s| s.id == song.id)
        {
            self.ui.queue_current = pos;
        }
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        use crossterm::terminal::{disable_raw_mode, LeaveAlternateScreen};
        use crossterm::ExecutableCommand;
        disable_raw_mode().ok();
        std::io::stdout().execute(LeaveAlternateScreen).ok();
    }
}

impl App {
    fn spawn_search(&self, query: String, limit: usize) {
        let tx = self.event_tx.clone();
        let search_uc = self.search.clone();
        tokio::spawn(async move {
            match search_uc.execute(&query, limit).await {
                Ok(songs) => {
                    let _ = tx.send(AppEvent::SearchResults(songs));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::SearchError(e.to_string()));
                }
            }
        });
    }

    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        let mut terminal = Self::init_terminal()?;
        let _guard = TerminalGuard;

        loop {
            self.ui.dismiss_old_notifications();

            self.sync_ui_queue();
            let theme = self.ui.get_or_create_theme();
            terminal.draw(|frame| {
                app_ui::render(frame, &self.ui, self.playback.mode(), &theme);
            })?;

            if let Some(song) = self.pending_play.take() {
                let song_name = song.title.clone();
                self.ui.loading_status = Some(self.ui.tr("downloading").replace("{}", &song_name));

                match self.playback.mode() {
                    crate::infrastructure::audio::AudioMode::Video => {
                        // Video mode: still blocking (mpv handles its own window)
                        match self.playback.play(&song).await {
                            Ok(()) => {
                                self.ui.player_state = PlayerState::Playing;
                                self.ui.progress = 0.0;
                                self.ui.duration = self.playback.current_duration();
                                self.ui.loading_status = None;
                            }
                            Err(e) => {
                                self.ui.player_state = PlayerState::Stopped;
                                self.ui.push_notification(self.ui.tr("err_playback").replace("{}", &e.to_string()), NotificationLevel::Error);
                                self.ui.loading_status = None;
                                self.ui.current_song = None;
                            }
                        }
                    }
                    crate::infrastructure::audio::AudioMode::Audio => {
                        // Audio mode: spawn download in background so UI keeps animating
                        let tx = self.event_tx.clone();
                        let url = song.webpage_url.clone();
                        let song_for_event = song.clone();
                        tokio::spawn(async move {
                            match PlaybackUseCase::download_audio_bytes(url).await {
                                Ok(data) => {
                                    let _ = tx.send(AppEvent::AudioReady { song: song_for_event, data });
                                }
                                Err(e) => {
                                    let _ = tx.send(AppEvent::AudioDownloadError(e.to_string()));
                                }
                            }
                        });
                    }
                }
                continue;
            }

            if let Some((song, dir, fmt)) = self.ui.download_pending.take() {
                let song_title = song.title.clone();
                let song_title_clone = song_title.clone();
                let tx = self.event_tx.clone();
                let ytdlp = self.playback.ytdlp_clone();
                let sem = self.download_semaphore.clone();
                tokio::spawn(async move {
                    let _permit = sem.acquire().await;
                    if let Err(e) = std::fs::create_dir_all(&dir) {
                        tracing::warn!("Failed to create download directory: {}", e);
                    }
                    match ytdlp.download(&song.webpage_url, &dir, &fmt).await {
                        Ok(path) => {
                            let _ = tx.send(AppEvent::DownloadComplete {
                                song_title: song_title_clone,
                                file_path: path,
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::DownloadError(e.to_string()));
                        }
                    }
                });
                self.ui.push_notification(self.ui.tr("downloading").replace("{}", &song_title), NotificationLevel::Info);
                continue;
            }

            let should_exit = tokio::select! {
                Some(key) = self.keyboard_rx.recv() => {
                    match self.handle_key(key).await {
                        Ok(true) => true,
                        Ok(false) => false,
                        Err(e) => {
                            self.ui.push_notification(self.ui.tr("err_generic").replace("{}", &e.to_string()), NotificationLevel::Error);
                            false
                        }
                    }
                }
                Some(event) = self.event_rx.recv() => {
                    self.handle_event(event);
                    false
                }
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    self.update_progress();
                    self.ui.tick_spinner();
                    false
                }
            };

            if should_exit {
                self.on_exit();
                break;
            }
        }

        Ok(())
    }

    fn on_exit(&mut self) {
        if let Err(e) = self.playback.stop() {
            tracing::warn!("Failed to stop playback on exit: {}", e);
        }
        let s = self.config.settings_mut();
        s.volume = self.playback.volume();
        s.theme = self.ui.theme_name.clone();
        s.accent_color = self.ui.accent_color.clone();
        s.default_search_limit = self.ui.default_search_limit;
        s.download_path = self.ui.download_path.clone();
        s.language = self.ui.language.clone();
        if let Err(e) = self.config.save_settings() {
            tracing::warn!("Failed to save settings: {}", e);
        }
        if let Err(e) = self.config.save_playlist(self.playlist.playlist()) {
            tracing::warn!("Failed to save playlist: {}", e);
        }
    }

    fn update_progress(&mut self) {
        self.ui.spectrum = self.playback.get_spectrum();
        let state = self.playback.state();
        if let PlayerState::Playing | PlayerState::Paused = state {
            self.ui.progress = self.playback.current_position();
            self.ui.duration = self.playback.current_duration();
            self.ui.player_state = state;

            if self.playback.state() == PlayerState::Playing
                && self.playback.is_sink_empty()
            {
                if let Err(e) = self.playback.stop() {
                    tracing::warn!("Failed to stop on auto-advance: {}", e);
                }
                self.ui.player_state = PlayerState::Stopped;
                self.ui.progress = 0.0;

                if let Some(next) = self.playlist.next().cloned() {
                    self.queue_play(next);
                }
            }
        }
    }

    fn queue_play(&mut self, song: Song) {
        self.ui.current_song = Some(song.clone());
        self.ui.player_state = PlayerState::Loading;
        self.pending_play = Some(song);
    }

    #[allow(clippy::collapsible_match)]
    async fn handle_key(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        // Universal keys — always work regardless of screen or focus
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(true);
        }

        // Download format popup handler (priority check before universal keys intercept)
        if self.ui.show_download_popup {
            match key.code {
                KeyCode::Up => {
                    self.ui.download_format = self.ui.download_format.saturating_sub(1);
                }
                KeyCode::Down => {
                    self.ui.download_format = (self.ui.download_format + 1).min(4);
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some(song) = self.ui.download_song.take() {
                        let dir = self.ui.download_path.clone();
                        let fmt = match self.ui.download_format {
                            0 => "m4a", 1 => "mp3", 2 => "opus", 3 => "flac", _ => "wav",
                        }.to_string();
                        self.ui.show_download_popup = false;
                        self.ui.download_pending = Some((song, dir, fmt));
                    } else {
                        self.ui.show_download_popup = false;
                    }
                }
                KeyCode::Esc => {
                    self.ui.show_download_popup = false;
                }
                _ => {}
            }
            return Ok(false);
        }

        match key.code {
            KeyCode::Char('?') => {
                self.ui.active_screen = if self.ui.active_screen == ActiveScreen::Help {
                    ActiveScreen::Search
                } else {
                    ActiveScreen::Help
                };
                return Ok(false);
            }
            KeyCode::Esc => {
                match self.ui.active_screen {
                    ActiveScreen::Help | ActiveScreen::Settings | ActiveScreen::Player => {
                        self.ui.active_screen = ActiveScreen::Search;
                        self.ui.focus = Focus::SearchInput;
                    }
                    ActiveScreen::Search => {
                        if self.ui.focus == Focus::SearchInput && !self.ui.search_query.is_empty() {
                            self.ui.search_query.clear();
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
            match key.code {
                KeyCode::Char('q') => return Ok(true),
                _ => {
                    self.ui.active_screen = ActiveScreen::Search;
                    return Ok(false);
                }
            }
        }

        if self.ui.active_screen == ActiveScreen::Settings {
            match key.code {
                KeyCode::Char('q') => return Ok(true),
                KeyCode::Esc => {
                    self.ui.active_screen = ActiveScreen::Search;
                    self.ui.focus = Focus::SearchInput;
                }
                KeyCode::Up => {
                    self.ui.settings_focus = self.ui.settings_focus.saturating_sub(1);
                }
                KeyCode::Down => {
                    self.ui.settings_focus = (self.ui.settings_focus + 1).min(5);
                }
                KeyCode::Enter | KeyCode::Char(' ') => match self.ui.settings_focus {
                    0 => {
                        self.ui.theme_name = if self.ui.theme_name == "dark" {
                            "light".into()
                        } else {
                            "dark".into()
                        };
                        self.ui.invalidate_theme();
                        self.ui.push_notification(
                            self.ui.tr("notif_theme").replace("{}", &self.ui.theme_name),
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
                            .position(|&c| c == self.ui.accent_color)
                            .map(|i| (i + 1) % PRESETS.len())
                            .unwrap_or(0);
                        self.ui.accent_color = PRESETS[i].to_string();
                        self.ui.invalidate_theme();
                        self.ui.push_notification(self.ui.tr("notif_accent"), NotificationLevel::Info);
                    }
                    4 => {
                        let dir = tokio::task::spawn_blocking(|| {
                            rfd::FileDialog::new()
                                .set_title("Select Download Folder")
                                .pick_folder()
                        }).await.unwrap_or(None);
                        if let Some(p) = dir {
                            self.ui.download_path = p.to_string_lossy().to_string();
                        }
                    }
                    5 => {
                        self.ui.language = if self.ui.language == "es" {
                            "en".into()
                        } else {
                            "es".into()
                        };
                        self.ui.translations = Translations::load(&self.ui.language);
                    }
                    _ => {}
                },
                KeyCode::Char('=') | KeyCode::Char('+') => match self.ui.settings_focus {
                    2 => {
                        self.ui.volume = (self.ui.volume + 0.05).min(1.0);
                        self.playback.set_volume(self.ui.volume);
                    }
                    3 => {
                        self.ui.default_search_limit =
                            (self.ui.default_search_limit + 5).min(50);
                    }
                    _ => {}
                },
                KeyCode::Char('-') | KeyCode::Char('_') => match self.ui.settings_focus {
                    2 => {
                        self.ui.volume = (self.ui.volume - 0.05).max(0.0);
                        self.playback.set_volume(self.ui.volume);
                    }
                    3 => {
                        self.ui.default_search_limit = self
                            .ui
                            .default_search_limit
                            .saturating_sub(5)
                            .max(1);
                    }
                    _ => {}
                },
                KeyCode::Char(c) if self.ui.settings_focus == 4 => {
                    if self.ui.download_path.len() < 300 {
                        self.ui.download_path.push(c);
                    }
                }
                KeyCode::Backspace if self.ui.settings_focus == 4 => {
                    self.ui.download_path.pop();
                }
                _ => {}
            }
            return Ok(false);
        }

        if self.ui.active_screen == ActiveScreen::Player {
            match key.code {
                KeyCode::Char(' ') => match self.playback.state() {
                    PlayerState::Playing => {
                        if self.playback.pause().is_ok() {
                            self.ui.player_state = PlayerState::Paused;
                            self.ui.push_notification(self.ui.tr("notif_paused"), NotificationLevel::Info);
                        }
                    }
                    PlayerState::Paused => {
                        if self.playback.resume().is_ok() {
                            self.ui.player_state = PlayerState::Playing;
                            self.ui.push_notification(self.ui.tr("notif_resumed"), NotificationLevel::Info);
                        }
                    }
                    _ => {}
                },
                KeyCode::Char('s') => {
                    self.pending_play = None;
                    if let Err(e) = self.playback.stop() {
                        tracing::warn!("Failed to stop playback: {}", e);
                    }
                    self.ui.player_state = PlayerState::Stopped;
                    self.ui.progress = 0.0;
                    self.ui.push_notification(self.ui.tr("notif_stopped"), NotificationLevel::Info);
                }
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
                    self.ui.volume = vol;
                    self.ui.push_notification(
                        self.ui.tr("notif_volume").replace("{:.0}", &format!("{:.0}", vol * 100.0)),
                        NotificationLevel::Info,
                    );
                }
                KeyCode::Char('-') | KeyCode::Char('_') => {
                    let vol = (self.playback.volume() - 0.05).max(0.0);
                    self.playback.set_volume(vol);
                    self.ui.volume = vol;
                    self.ui.push_notification(
                        self.ui.tr("notif_volume").replace("{:.0}", &format!("{:.0}", vol * 100.0)),
                        NotificationLevel::Info,
                    );
                }
                KeyCode::Char('t') => {
                    self.ui.active_screen = ActiveScreen::Settings;
                    self.ui.focus = Focus::SearchInput;
                }
                _ => {}
            }
            return Ok(false);
        }

        // Search/Queue screen — dispatch by focus
        match key.code {
            KeyCode::Tab => {
                self.ui.focus = match self.ui.focus {
                    Focus::SearchInput => Focus::SearchResults,
                    Focus::SearchResults => Focus::QueueList,
                    Focus::QueueList => Focus::SearchInput,
                };
            }
            KeyCode::Up => match self.ui.focus {
                Focus::SearchResults => {
                    self.ui.selected_index = self.ui.selected_index.saturating_sub(1);
                }
                Focus::QueueList => {
                    if !self.ui.queue_songs.is_empty() {
                        self.ui.queue_selected = self.ui.queue_selected.saturating_sub(1);
                    }
                }
                _ => {}
            },
            KeyCode::Down => match self.ui.focus {
                Focus::SearchResults => {
                    self.ui.selected_index = (self.ui.selected_index + 1)
                        .min(self.ui.search_results.len().saturating_sub(1));
                }
                Focus::QueueList => {
                    if !self.ui.queue_songs.is_empty() {
                        self.ui.queue_selected = (self.ui.queue_selected + 1)
                            .min(self.ui.queue_songs.len().saturating_sub(1));
                    }
                }
                _ => {}
            },
            KeyCode::Enter => match self.ui.focus {
                Focus::SearchInput => {
                    if !self.ui.search_query.is_empty() {
                        // Debounce: ignore rapid consecutive searches
                        if let Some(last) = self.last_search
                            && last.elapsed() < Duration::from_millis(300)
                        {
                            // too soon — skip
                        } else {
                            self.last_search = Some(Instant::now());
                            self.ui.is_searching = true;
                            self.ui.search_results.clear();
                            let query = self.ui.search_query.clone();
                            self.spawn_search(query, self.ui.default_search_limit);
                        }
                    }
                }
                Focus::SearchResults => {
                    self.schedule_play_selected();
                }
                Focus::QueueList => {
                    let idx = self.ui.queue_selected;
                    if idx < self.ui.queue_songs.len() {
                        self.playlist.set_current_index(idx);
                        if let Some(song) = self.playlist.current_song_cloned() {
                            self.queue_play(song);
                        }
                    }
                }
            },
            KeyCode::Backspace => {
                if self.ui.focus == Focus::SearchInput {
                    self.ui.search_query.pop();
                }
            }
            KeyCode::Char(c) if self.ui.focus == Focus::SearchInput => {
                if self.ui.search_query.len() < 200 {
                    self.ui.search_query.push(c);
                }
            }
            KeyCode::Char('/') => {
                self.ui.focus = Focus::SearchInput;
                self.ui.search_query.clear();
                self.ui.active_screen = ActiveScreen::Search;
            }
            KeyCode::Char(' ') => match self.playback.state() {
                PlayerState::Playing => {
                    if self.playback.pause().is_ok() {
                        self.ui.player_state = PlayerState::Paused;
                        self.ui.push_notification(self.ui.tr("notif_paused"), NotificationLevel::Info);
                    }
                }
                PlayerState::Paused => {
                    if self.playback.resume().is_ok() {
                        self.ui.player_state = PlayerState::Playing;
                        self.ui.push_notification(self.ui.tr("notif_resumed"), NotificationLevel::Info);
                    }
                }
                _ => {
                    self.schedule_play_selected();
                }
            },
            KeyCode::Char('s') => {
                self.pending_play = None;
                if let Err(e) = self.playback.stop() {
                    tracing::warn!("Failed to stop playback: {}", e);
                }
                self.ui.player_state = PlayerState::Stopped;
                self.ui.progress = 0.0;
                self.ui.push_notification(self.ui.tr("notif_stopped"), NotificationLevel::Info);
            }
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
                self.ui.volume = vol;
                self.ui.push_notification(
                    self.ui.tr("notif_volume").replace("{:.0}", &format!("{:.0}", vol * 100.0)),
                    NotificationLevel::Info,
                );
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                let vol = (self.playback.volume() - 0.05).max(0.0);
                self.playback.set_volume(vol);
                self.ui.volume = vol;
                self.ui.push_notification(
                    self.ui.tr("notif_volume").replace("{:.0}", &format!("{:.0}", vol * 100.0)),
                    NotificationLevel::Info,
                );
            }
            KeyCode::Char('v') => {
                self.playback.toggle_mode();
            }
            KeyCode::Char('a') => {
                if self.ui.focus == Focus::SearchResults && !self.ui.search_results.is_empty() {
                    let idx = self.ui.selected_index;
                    if let Some(song) = self.ui.search_results.get(idx) {
                        if self.playlist.songs().iter().any(|s| s.id == song.id) {
                            self.ui.push_notification(self.ui.tr("notif_already_in_queue").replace("{}", &song.title), NotificationLevel::Warning);
                        } else {
                            self.playlist.add(song.clone());
                            self.ui.push_notification(self.ui.tr("notif_added_to_queue").replace("{}", &song.title), NotificationLevel::Success);
                        }
                    }
                }
            }
            KeyCode::Delete => {
                if self.ui.focus == Focus::QueueList && !self.ui.queue_songs.is_empty() {
                    let idx = self.ui.queue_selected;
                    self.playlist.remove(idx);
                }
            }
            KeyCode::Char('d') => {
                match self.ui.focus {
                    Focus::SearchResults => {
                        self.ui.download_song = self.ui.search_results.get(self.ui.selected_index).cloned();
                    }
                    Focus::QueueList => {
                        self.ui.download_song = self.ui.queue_songs.get(self.ui.queue_selected).cloned();
                    }
                    _ => {}
                }
                if self.ui.download_song.is_some() {
                    self.ui.show_download_popup = true;
                    self.ui.download_format = 0;
                }
            }
            KeyCode::Char('C') | KeyCode::Char('c') => {
                if self.ui.focus == Focus::QueueList {
                    self.playlist.clear();
                }
            }
            KeyCode::Char('t') => {
                self.ui.active_screen = ActiveScreen::Settings;
                self.ui.focus = Focus::SearchInput;
            }
            KeyCode::Char('q') => return Ok(true),
            _ => {}
        }

        Ok(false)
    }

    fn schedule_play_selected(&mut self) {
        if self.ui.search_results.is_empty() {
            return;
        }
        let idx = self.ui.selected_index;
        let song = match self.ui.search_results.get(idx) {
            Some(s) => s.clone(),
            None => return,
        };
        if self.playlist.songs().iter().any(|s| s.id == song.id) {
            return;
        }
        let pos = self.playlist.len();
        self.playlist.add(song.clone());
        self.playlist.set_current_index(pos);
        self.queue_play(song);
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::SearchResults(songs) => {
                let count = songs.len();
                self.ui.search_results = songs;
                self.ui.is_searching = false;
                self.ui.selected_index = 0;
                self.ui.focus = Focus::SearchResults;
                self.ui.push_notification(
                    self.ui.tr("notif_search_count").replace("{}", &count.to_string()),
                    NotificationLevel::Info,
                );
            }
            AppEvent::SearchError(err) => {
                self.ui.is_searching = false;
                self.ui.push_notification(
                    self.ui.tr("err_search").replace("{}", &err),
                    NotificationLevel::Error,
                );
            }
            AppEvent::PlaybackStarted(song) => {
                self.ui.current_song = Some(song);
                self.ui.player_state = PlayerState::Loading;
                self.ui.is_searching = false;
            }
            AppEvent::PlaybackProgress(_, _) => {}
            AppEvent::PlaybackFinished => {
                self.ui.player_state = PlayerState::Stopped;
                if let Some(next) = self.playlist.next().cloned() {
                    self.queue_play(next);
                }
            }
            AppEvent::PlaybackPaused => {
                self.ui.player_state = PlayerState::Paused;
            }
            AppEvent::PlaybackResumed => {
                self.ui.player_state = PlayerState::Playing;
            }
            AppEvent::PlaybackStopped => {
                self.ui.player_state = PlayerState::Stopped;
                self.ui.progress = 0.0;
            }
            AppEvent::PlaybackError(err) => {
                self.ui.player_state = PlayerState::Stopped;
                self.ui.push_notification(
                    self.ui.tr("err_playback").replace("{}", &err),
                    NotificationLevel::Error,
                );
            }
            AppEvent::VolumeChanged(vol) => {
                self.ui.volume = vol;
                self.ui.push_notification(
                    self.ui.tr("notif_volume").replace("{:.0}", &format!("{:.0}", vol * 100.0)),
                    NotificationLevel::Info,
                );
            }
            AppEvent::DownloadComplete { song_title, file_path: _ } => {
                self.ui.push_notification(
                    self.ui.tr("notif_downloaded").replace("{}", &song_title),
                    NotificationLevel::Success,
                );
            }
            AppEvent::DownloadError(err) => {
                self.ui.push_notification(
                    self.ui.tr("err_download_failed").replace("{}", &err),
                    NotificationLevel::Error,
                );
            }
            AppEvent::AudioReady { song, data } => {
                match self.playback.play_bytes(data, song) {
                    Ok(()) => {
                        self.ui.player_state = PlayerState::Playing;
                        self.ui.progress = 0.0;
                        self.ui.duration = self.playback.current_duration();
                        self.ui.push_notification(self.ui.tr("notif_playing"), NotificationLevel::Info);
                    }
                    Err(e) => {
                        self.ui.player_state = PlayerState::Stopped;
                        self.ui.push_notification(
                            self.ui.tr("err_playback").replace("{}", &e.to_string()),
                            NotificationLevel::Error,
                        );
                        self.ui.current_song = None;
                    }
                }
                self.ui.loading_status = None;
            }
            AppEvent::AudioDownloadError(err) => {
                self.ui.player_state = PlayerState::Stopped;
                self.ui.push_notification(
                    self.ui.tr("err_playback").replace("{}", &err),
                    NotificationLevel::Error,
                );
                self.ui.loading_status = None;
                self.ui.current_song = None;
            }
            AppEvent::Exit => {}
        }
    }
}
