use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tokio::sync::{mpsc, Semaphore};

use crate::application::playback::PlaybackUseCase;
use crate::application::playlist::PlaylistUseCase;
pub(crate) use crate::application::ports::ConfigPort;
use crate::application::search::SearchUseCase;
pub(crate) use crate::domain::audio_mode::AudioMode;
pub(crate) use crate::domain::media::Song;
pub(crate) use crate::domain::player_state::PlayerState;
pub(crate) use crate::infrastructure::config::store::AppSettings;
use crate::interface::app_ui;
use crate::interface::i18n::Translations;
pub(crate) use crate::interface::state::{ActiveScreen, Focus, NotificationLevel, UiState};
pub(crate) use crate::shared::event::AppEvent;

pub mod lifecycle;
pub mod sync;
pub mod event_handler;
pub mod background;
pub mod key_handler;

use self::lifecycle::TerminalGuard;

pub struct App {
    ui: UiState,
    playback: PlaybackUseCase,
    search: SearchUseCase,
    playlist: PlaylistUseCase,
    config: Box<dyn ConfigPort>,
    settings: AppSettings,
    keyboard_rx: mpsc::UnboundedReceiver<KeyEvent>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    pending_play: Option<Song>,
    last_search: Option<Instant>,
    download_semaphore: Arc<Semaphore>,
    last_playlist_version: usize,
}

impl App {
    pub async fn new(
        playback: PlaybackUseCase,
        search: SearchUseCase,
        playlist: PlaylistUseCase,
        config: Box<dyn ConfigPort>,
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

        let settings = config.load_settings().await.unwrap_or_else(|e| {
            tracing::warn!("Failed to load settings: {}, using defaults", e);
            AppSettings::default()
        });
        // Use system locale detection by default; fall back to persisted language only
        // if the user explicitly changed it from the default "en".
        let language = if settings.language == "en" {
            Translations::detect_locale()
        } else {
            settings.language.clone()
        };
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
            settings,
            keyboard_rx,
            event_tx,
            event_rx,
            pending_play: None,
            last_search: None,
            download_semaphore: Arc::new(Semaphore::new(3)),
            last_playlist_version: 0,
        }
    }

}

impl App {
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

            if self.handle_pending_play().await {
                continue;
            }

            if self.handle_download_pending().await {
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
                self.on_exit().await;
                break;
            }
        }

        Ok(())
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use crate::application::playback::PlaybackUseCase;
    use crate::application::playlist::PlaylistUseCase;
    use crate::application::ports::{AudioPlaybackPort, ConfigPort, DownloaderPort, MediaSearchPort};
    use crate::application::search::SearchUseCase;
    use crate::domain::media::Song;
    use crate::infrastructure::audio::mpv_backend::MpvAdapter;
    use crate::infrastructure::audio::rodio_backend::RodioAdapter;
    use crate::infrastructure::config::store::ConfigAdapter;
    use crate::infrastructure::ytdlp::client::YtDlpAdapter;

    /// Helper to build a test App. Returns None if audio device isn't available.
    async fn build_test_app() -> Option<App> {
        let config = ConfigAdapter::new().await.ok()?;
        let audio: Box<dyn AudioPlaybackPort> = Box::new(RodioAdapter::new().ok()?);
        let mpv = MpvAdapter::new();
        let ytdlp = YtDlpAdapter::new();
        let downloader: Arc<dyn DownloaderPort> = Arc::new(ytdlp.clone());
        let search_port: Arc<dyn MediaSearchPort> = Arc::new(ytdlp);
        let playback = PlaybackUseCase::new(downloader, audio, mpv);
        let search = SearchUseCase::new(search_port);
        let playlist = PlaylistUseCase::new();
        let config_port: Box<dyn ConfigPort> = Box::new(config);
        Some(App::new(playback, search, playlist, config_port).await)
    }

    #[tokio::test]
    async fn test_ctrl_c_triggers_exit() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => {
                eprintln!("Skipping test: audio device not available");
                return;
            }
        };

        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let result = app.handle_key(ctrl_c).await.unwrap();
        assert!(result, "Ctrl+C should return Ok(true) to signal exit");
    }

    #[tokio::test]
    async fn test_regular_c_still_works() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => {
                eprintln!("Skipping test: audio device not available");
                return;
            }
        };

        // Regular 'c' (without Ctrl) should NOT trigger exit
        let regular_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
        let result = app.handle_key(regular_c).await.unwrap();
        // Regular 'c' should not trigger exit (returns Ok(false))
        // Unless queue focus triggers clear() which also returns Ok(false)
        assert!(!result, "Regular 'c' should not signal exit");
    }

    #[tokio::test]
    async fn test_ctrl_q_still_works() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => {
                eprintln!("Skipping test: audio device not available");
                return;
            }
        };

        let ctrl_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        let result = app.handle_key(ctrl_q).await.unwrap();
        assert!(result, "Ctrl+Q should still return Ok(true) to signal exit");
    }

    #[tokio::test]
    async fn test_error_clears_current_song() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => {
                eprintln!("Skipping test: audio device not available");
                return;
            }
        };
        // Set a current song
        let song = Song {
            id: "test-id".into(),
            title: "Test Song".into(),
            channel: "Test".into(),
            duration: 100.0,
            thumbnail: None,
            webpage_url: "http://example.com".into(),
        };
        app.ui.current_song = Some(song);

        // Send PlaybackError
        app.handle_event(AppEvent::PlaybackError("Something went wrong".into()));
        assert!(app.ui.current_song.is_none(), "current_song should be cleared after error");
        assert_eq!(app.ui.player_state, PlayerState::Stopped);
    }

    #[tokio::test]
    async fn test_audio_download_error_clears_current_song() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => {
                eprintln!("Skipping test: audio device not available");
                return;
            }
        };
        let song = Song {
            id: "test-id".into(),
            title: "Test Song".into(),
            channel: "Test".into(),
            duration: 100.0,
            thumbnail: None,
            webpage_url: "http://example.com".into(),
        };
        app.ui.current_song = Some(song);

        app.handle_event(AppEvent::AudioDownloadError("Download failed".into()));
        assert!(app.ui.current_song.is_none(), "current_song should be cleared after download error");
        assert_eq!(app.ui.player_state, PlayerState::Stopped);
    }

    #[tokio::test]
    async fn test_error_then_retry_same_song_sets_loading() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => {
                eprintln!("Skipping test: audio device not available");
                return;
            }
        };

        // Add a song to the playlist first (simulating initial play)
        let song = Song {
            id: "retry-test-id".into(),
            title: "Retry Song".into(),
            channel: "Test".into(),
            duration: 100.0,
            thumbnail: None,
            webpage_url: "http://example.com".into(),
        };

        // Simulate initial play: add to queue, trigger play
        app.playlist.add(song.clone());
        app.playlist.set_current_index(0);

        // Simulate an error
        app.handle_event(AppEvent::PlaybackError("Failed".into()));
        assert!(app.ui.current_song.is_none());
        assert_eq!(app.ui.player_state, PlayerState::Stopped);

        // Now simulate re-selecting the same song: call schedule_play_selected
        // with the song in search results
        app.ui.search_results.push(song.clone());
        app.schedule_play_selected();

        // After retry, state should be Loading and pending_play should be Some
        assert_eq!(app.ui.player_state, PlayerState::Loading,
            "retry after error should set Loading state");
        assert!(app.pending_play.is_some(), "retry should set pending_play");
    }
}
