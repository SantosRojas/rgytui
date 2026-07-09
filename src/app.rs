use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use crossterm::terminal::{enable_raw_mode, EnterAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

use crate::application::playback::PlaybackUseCase;
use crate::application::playlist::PlaylistUseCase;
use crate::application::search::SearchUseCase;
use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;
use crate::infrastructure::config::store::ConfigStore;
use crate::interface::app_ui;
use crate::interface::state::{ActiveScreen, UiState};
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

        let ui = UiState {
            volume: config.settings().volume.clamp(0.0, 1.0),
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
        }
    }

    fn init_terminal() -> std::io::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
        enable_raw_mode()?;
        std::io::stdout().execute(EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(std::io::stdout());
        Terminal::new(backend)
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
            terminal.draw(|frame| {
                app_ui::render(frame, &self.ui, self.playback.mode());
            })?;

            if let Some(song) = self.pending_play.take() {
                let song_name = song.title.clone();
                self.ui.loading_status = Some(format!("Downloading {}...", song_name));
                // Drain any stale events from previous playback
                while self.event_rx.try_recv().is_ok() {}
                match self.playback.play(&song).await {
                    Ok(()) => {
                        self.ui.player_state = PlayerState::Playing;
                        self.ui.progress = 0.0;
                        self.ui.duration = self.playback.current_duration();
                        self.ui.loading_status = None;
                    }
                    Err(e) => {
                        self.ui.player_state = PlayerState::Stopped;
                        self.ui.error_message = Some(format!("Playback error: {}", e));
                        self.ui.loading_status = None;
                        self.ui.current_song = None;
                    }
                }
                continue;
            }

            let should_exit = tokio::select! {
                Some(key) = self.keyboard_rx.recv() => {
                    match self.handle_key(key).await {
                        Ok(true) => true,
                        Ok(false) => false,
                        Err(e) => {
                            self.ui.error_message = Some(format!("Error: {}", e));
                            false
                        }
                    }
                }
                Some(event) = self.event_rx.recv() => {
                    self.handle_event(event);
                    false
                }
                _ = tokio::time::sleep(Duration::from_millis(200)) => {
                    self.update_progress();
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
        self.playback.stop().ok();
        self.config.settings_mut().volume = self.playback.volume();
        self.config.save_settings().ok();
        if let Err(e) = self
            .config
            .save_playlist(self.playlist.playlist())
        {
            tracing::warn!("Failed to save playlist: {}", e);
        }
    }

    fn update_progress(&mut self) {
        let state = self.playback.state();
        if let PlayerState::Playing | PlayerState::Paused = state {
            self.ui.progress = self.playback.current_position();
            self.ui.duration = self.playback.current_duration();
            self.ui.player_state = state;

            if self.playback.state() == PlayerState::Playing
                && self.playback.is_sink_empty()
            {
                self.playback.stop().ok();
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
        self.ui.active_screen = ActiveScreen::Player;
        self.pending_play = Some(song);
    }

    fn dismiss_error(&mut self) {
        self.ui.error_message = None;
    }

    async fn handle_key(&mut self, key: KeyEvent) -> Result<bool, anyhow::Error> {
        if self.ui.error_message.is_some() {
            self.dismiss_error();
        }

        if self.ui.active_screen == ActiveScreen::Help {
            self.ui.active_screen = ActiveScreen::Search;
            return Ok(false);
        }

        match key.code {
            KeyCode::Tab => {
                if self.ui.active_screen == ActiveScreen::Search {
                    self.ui.focus_search = !self.ui.focus_search;
                }
            }
            KeyCode::Up => {
                if !self.ui.search_results.is_empty() {
                    self.ui.selected_index = self.ui.selected_index.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if !self.ui.search_results.is_empty() {
                    self.ui.selected_index =
                        (self.ui.selected_index + 1).min(self.ui.search_results.len().saturating_sub(1));
                }
            }
            KeyCode::Enter => {
                if self.ui.active_screen == ActiveScreen::Search {
                    if self.ui.focus_search && !self.ui.search_query.is_empty() {
                        self.ui.is_searching = true;
                        self.ui.search_results.clear();
                        let query = self.ui.search_query.clone();
                        self.spawn_search(query, 10);
                    } else if !self.ui.search_results.is_empty() {
                        self.schedule_play_selected();
                    }
                }
            }
            KeyCode::Esc => {
                if self.ui.focus_search {
                    self.ui.focus_search = false;
                } else {
                    self.ui.active_screen = ActiveScreen::Search;
                }
            }
            KeyCode::Backspace => {
                if self.ui.focus_search {
                    self.ui.search_query.pop();
                }
            }
            KeyCode::Char(c) if self.ui.focus_search => {
                if self.ui.search_query.len() < 200 {
                    self.ui.search_query.push(c);
                }
            }
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('?') => {
                self.ui.active_screen = if self.ui.active_screen == ActiveScreen::Help {
                    ActiveScreen::Search
                } else {
                    ActiveScreen::Help
                };
            }
            KeyCode::Char('/') => {
                self.ui.focus_search = true;
                self.ui.search_query.clear();
                self.ui.active_screen = ActiveScreen::Search;
            }
            KeyCode::Char(' ') => match self.playback.state() {
                PlayerState::Playing => {
                    self.playback.pause().ok();
                    self.ui.player_state = PlayerState::Paused;
                }
                PlayerState::Paused => {
                    self.playback.resume().ok();
                    self.ui.player_state = PlayerState::Playing;
                }
                _ => {
                    self.schedule_play_selected();
                }
            },
            KeyCode::Char('s') => {
                self.pending_play = None;
                self.playback.stop().ok();
                self.ui.player_state = PlayerState::Stopped;
                self.ui.progress = 0.0;
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
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                let vol = (self.playback.volume() - 0.05).max(0.0);
                self.playback.set_volume(vol);
                self.ui.volume = vol;
            }
            KeyCode::Char('v') => {
                self.playback.toggle_mode();
            }
            KeyCode::Char('a') => {
                if !self.ui.search_results.is_empty() {
                    let idx = self.ui.selected_index;
                    if let Some(song) = self.ui.search_results.get(idx) {
                        self.playlist.add(song.clone());
                        self.ui.status_message = Some(format!("Added '{}' to queue", song.title));
                    }
                }
            }
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
        self.playlist.add(song.clone());
        self.queue_play(song);
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::SearchResults(songs) => {
                self.ui.search_results = songs;
                self.ui.is_searching = false;
                self.ui.selected_index = 0;
                self.ui.focus_search = false;
            }
            AppEvent::SearchError(err) => {
                self.ui.is_searching = false;
                self.ui.error_message = Some(err);
            }
            AppEvent::PlaybackStarted(song) => {
                self.ui.current_song = Some(song);
                self.ui.player_state = PlayerState::Loading;
                self.ui.active_screen = ActiveScreen::Player;
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
                self.ui.error_message = Some(err);
            }
            AppEvent::VolumeChanged(vol) => {
                self.ui.volume = vol;
            }
            AppEvent::Exit => {}
        }
    }
}
