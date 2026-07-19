use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
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
pub(crate) use crate::interface::state::{ActiveScreen, ConfigState, Focus, NotificationLevel, RenderSnapshot, UiState};
pub(crate) use crate::shared::event::AppEvent;
use crate::shared::event::InputEvent;

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
    input_rx: mpsc::UnboundedReceiver<InputEvent>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    pending_play: Option<Song>,
    last_search: Option<Instant>,
    download_semaphore: Arc<Semaphore>,
}

impl App {
    pub async fn new(
        playback: PlaybackUseCase,
        search: SearchUseCase,
        playlist: PlaylistUseCase,
        config: Box<dyn ConfigPort>,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (input_tx, input_rx) = mpsc::unbounded_channel();

        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loop {
                match crossterm::event::read() {
                    Ok(crossterm::event::Event::Key(key)) => {
                        if key.kind != KeyEventKind::Press {
                            continue;
                        }
                        if input_tx.send(InputEvent::Key(key)).is_err() {
                            break;
                        }
                    }
                    Ok(crossterm::event::Event::Mouse(mouse)) => {
                        if input_tx.send(InputEvent::Mouse(mouse)).is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }));
            if let Err(e) = result {
                tracing::error!("Input thread panicked: {:?}", e);
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
            config: ConfigState::new(
                settings.theme.clone(),
                settings.accent_color.clone(),
                language.clone(),
                translations,
                settings.default_search_limit,
                settings.download_path.clone(),
            ),
            ..UiState::default()
        };

        Self {
            ui,
            playback,
            search,
            playlist,
            config,
            settings,
            input_rx,
            event_tx,
            event_rx,
            pending_play: None,
            last_search: None,
            download_semaphore: Arc::new(Semaphore::new(3)),
        }
    }

}

impl App {
    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        let mut terminal = Self::init_terminal()?;
        let _guard = TerminalGuard;

        loop {
            self.ui.dismiss_old_notifications();

            let theme = self.ui.get_or_create_theme();
            let render_state = RenderSnapshot::from_use_cases(&self.playback, &self.playlist);
            terminal.draw(|frame| {
                app_ui::render(frame, &self.ui, &render_state, &theme);
            })?;

            if self.handle_pending_play().await {
                continue;
            }

            if self.handle_download_pending().await {
                continue;
            }

            let should_exit = tokio::select! {
                Some(input) = self.input_rx.recv() => {
                    match input {
                        InputEvent::Key(key) => {
                            match self.handle_key(key).await {
                                Ok(true) => true,
                                Ok(false) => false,
                                Err(e) => {
                                    self.ui.push_notification(self.ui.tr("err_generic").replace("{}", &e.to_string()), NotificationLevel::Error);
                                    false
                                }
                            }
                        }
                        InputEvent::Mouse(mouse) => {
                            self.handle_mouse(mouse);
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

    fn handle_mouse(&mut self, event: MouseEvent) {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let col = event.column;
                let row = event.row;
                let rects = self.ui.panel_rects.borrow();

                // Check search input — click to focus
                if let Some(rect) = rects.get("search_input") {
                    if row >= rect.y && row < rect.y + rect.height
                        && col >= rect.x && col < rect.x + rect.width
                    {
                        self.ui.active_screen = ActiveScreen::Search;
                        self.ui.focus = Focus::SearchInput;
                        return;
                    }
                }

                // Check search results — click to select + focus
                if let Some(rect) = rects.get("search_results") {
                    if row >= rect.y && row < rect.y + rect.height
                        && col >= rect.x && col < rect.x + rect.width
                    {
                        self.ui.active_screen = ActiveScreen::Search;
                        self.ui.focus = Focus::SearchResults;

                        // Resolve click Y coordinate to list index,
                        // matching the render-time scroll offset logic
                        let visible_height = (rect.height.saturating_sub(2)) as usize;
                        if visible_height > 0 && row >= rect.y + 1 {
                            let relative_row = (row - rect.y - 1) as usize;
                            if relative_row < visible_height {
                                let item_count = self.ui.search.search_results.len();
                                let offset = if item_count > visible_height {
                                    self.ui.search.selected_index
                                        .min(item_count.saturating_sub(visible_height))
                                } else {
                                    0
                                };
                                let idx = offset + relative_row;
                                if idx < item_count {
                                    self.ui.search.selected_index = idx;
                                }
                            }
                        }
                        return;
                    }
                }

                // Check queue — click to select + focus
                if let Some(rect) = rects.get("queue") {
                    if row >= rect.y && row < rect.y + rect.height
                        && col >= rect.x && col < rect.x + rect.width
                    {
                        self.ui.active_screen = ActiveScreen::Search;
                        self.ui.focus = Focus::QueueList;

                        let visible_height = (rect.height.saturating_sub(2)) as usize;
                        if visible_height > 0 && row >= rect.y + 1 {
                            let relative_row = (row - rect.y - 1) as usize;
                            if relative_row < visible_height {
                                let item_count = self.playlist.songs().len();
                                let offset = if item_count > visible_height {
                                    self.ui.queue.queue_selected
                                        .min(item_count.saturating_sub(visible_height))
                                } else {
                                    0
                                };
                                let idx = offset + relative_row;
                                if idx < item_count {
                                    self.ui.queue.queue_selected = idx;
                                }
                            }
                        }
                        return;
                    }
                }

                // Click outside any panel — silently ignored
            }
            MouseEventKind::ScrollDown => {
                self.handle_scroll_down();
            }
            MouseEventKind::ScrollUp => {
                self.handle_scroll_up();
            }
            _ => {}
        }
    }

    fn handle_scroll_up(&mut self) {
        match self.ui.focus {
            Focus::SearchResults => {
                self.ui.search.selected_index = self.ui.search.selected_index.saturating_sub(1);
            }
            Focus::QueueList => {
                self.ui.queue.queue_selected = self.ui.queue.queue_selected.saturating_sub(1);
            }
            Focus::SearchInput => {
                // Scroll wheel on search input — cycle focus to results
                self.ui.focus = Focus::SearchResults;
            }
        }
    }

    fn handle_scroll_down(&mut self) {
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
            Focus::SearchInput => {
                // Scroll wheel on search input — cycle focus to results
                self.ui.focus = Focus::SearchResults;
            }
        }
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

    fn song(id: u32) -> Song {
        Song {
            id: format!("id-{id}"),
            title: format!("Song {id}"),
            channel: "Test".into(),
            duration: 100.0,
            thumbnail: None,
            webpage_url: "http://example.com".into(),
        }
    }

    fn songs(count: u32) -> Vec<Song> {
        (0..count).map(song).collect()
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
        app.ui.player.current_song = Some(song);

        // Send PlaybackError
        app.handle_event(AppEvent::PlaybackError("Something went wrong".into()));
        assert!(app.ui.player.current_song.is_none(), "current_song should be cleared after error");
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
        app.ui.player.current_song = Some(song);

        app.handle_event(AppEvent::AudioDownloadError("Download failed".into()));
        assert!(app.ui.player.current_song.is_none(), "current_song should be cleared after download error");
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
        assert!(app.ui.player.current_song.is_none());

        // Now simulate re-selecting the same song: call schedule_play_selected
        // with the song in search results
        app.ui.search.search_results.push(song.clone());
        app.schedule_play_selected();

        // After retry, pending_play should be Some (it schedules a new play)
        assert!(app.pending_play.is_some(), "retry should set pending_play");
    }

    // ── Task 3.1: j/k list navigation ──────────────────────────────────────

    #[tokio::test]
    async fn test_j_navigates_down_in_search_results() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;
        app.ui.search.search_results = songs(5);
        app.ui.search.selected_index = 1;

        let result = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)).await.unwrap();
        assert!(!result, "j should not signal exit");
        assert_eq!(app.ui.search.selected_index, 2, "j should increment selected_index");
    }

    #[tokio::test]
    async fn test_k_navigates_up_in_search_results() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;
        app.ui.search.search_results = songs(5);
        app.ui.search.selected_index = 3;

        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.search.selected_index, 2, "k should decrement selected_index");
    }

    #[tokio::test]
    async fn test_j_in_queue_list() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::QueueList;
        for s in songs(5) {
            app.playlist.add(s);
        }
        app.ui.queue.queue_selected = 1;

        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.queue.queue_selected, 2, "j in queue should increment queue_selected");
    }

    #[tokio::test]
    async fn test_k_in_queue_list() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::QueueList;
        for s in songs(5) {
            app.playlist.add(s);
        }
        app.ui.queue.queue_selected = 3;

        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.queue.queue_selected, 2, "k in queue should decrement queue_selected");
    }

    #[tokio::test]
    async fn test_j_in_search_input_types_char() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchInput;
        app.ui.search.search_query.clear();

        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.search.search_query, "j", "j in SearchInput should type 'j'");
    }

    #[tokio::test]
    async fn test_k_in_search_input_types_char() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchInput;
        app.ui.search.search_query.clear();

        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.search.search_query, "k", "k in SearchInput should type 'k'");
    }

    #[tokio::test]
    async fn test_j_at_bottom_does_not_overflow() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;
        app.ui.search.search_results = songs(3);
        app.ui.search.selected_index = 2; // last item

        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.search.selected_index, 2, "j at bottom should not overflow");
    }

    #[tokio::test]
    async fn test_k_at_top_does_not_underflow() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;
        app.ui.search.search_results = songs(3);
        app.ui.search.selected_index = 0;

        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.search.selected_index, 0, "k at top should not underflow");
    }

    // ── Task 3.2: g/G top/bottom navigation ─────────────────────────────────

    #[tokio::test]
    async fn test_g_goes_to_top_in_search() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;
        app.ui.search.search_results = songs(10);
        app.ui.search.selected_index = 7;

        app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.search.selected_index, 0, "g should go to index 0");
    }

    #[tokio::test]
    async fn test_g_capital_goes_to_bottom_in_search() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;
        app.ui.search.search_results = songs(10);
        app.ui.search.selected_index = 0;

        app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.search.selected_index, 9, "G should go to last index");
    }

    #[tokio::test]
    async fn test_g_goes_to_top_in_queue() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::QueueList;
        for s in songs(10) {
            app.playlist.add(s);
        }
        app.ui.queue.queue_selected = 5;

        app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.queue.queue_selected, 0, "g in queue should go to index 0");
    }

    #[tokio::test]
    async fn test_g_capital_goes_to_bottom_in_queue() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::QueueList;
        for s in songs(10) {
            app.playlist.add(s);
        }
        app.ui.queue.queue_selected = 0;

        app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.queue.queue_selected, 9, "G in queue should go to last index");
    }

    #[tokio::test]
    async fn test_g_on_empty_list_does_not_panic() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;
        // empty search_results

        app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.search.selected_index, 0, "g on empty list should not panic");
    }

    #[tokio::test]
    async fn test_g_capital_on_empty_list_does_not_panic() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;
        // empty search_results

        app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.search.selected_index, 0, "G on empty list should not panic");
    }

    #[tokio::test]
    async fn test_g_in_search_input_types_char() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchInput;
        app.ui.search.search_query.clear();

        app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.search.search_query, "g", "g in SearchInput should type 'g'");
    }

    // ── Task 3.3: Ctrl+u / Ctrl+d half-page scroll ─────────────────────────

    #[tokio::test]
    async fn test_ctrl_d_scrolls_down_half_page() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;
        app.ui.search.search_results = songs(30);
        app.ui.search.selected_index = 0;

        let ctrl_d = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        app.handle_key(ctrl_d).await.unwrap();
        assert_eq!(app.ui.search.selected_index, 10, "Ctrl+d should scroll down 10");
    }

    #[tokio::test]
    async fn test_ctrl_u_scrolls_up_half_page() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;
        app.ui.search.search_results = songs(30);
        app.ui.search.selected_index = 25;

        let ctrl_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        app.handle_key(ctrl_u).await.unwrap();
        assert_eq!(app.ui.search.selected_index, 15, "Ctrl+u should scroll up 10");
    }

    #[tokio::test]
    async fn test_ctrl_d_does_not_conflict_with_download() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;
        app.ui.search.search_results = songs(10);
        app.ui.search.selected_index = 5;
        app.ui.download.show_download_popup = false;

        let ctrl_d = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        app.handle_key(ctrl_d).await.unwrap();
        // Ctrl+d should scroll, NOT trigger download popup
        assert!(!app.ui.download.show_download_popup, "Ctrl+d should NOT trigger download popup");
        // selected should be clamped to last item (9)
        assert_eq!(app.ui.search.selected_index, 9, "Ctrl+d should scroll, clamped to max");
    }

    #[tokio::test]
    async fn test_plain_d_still_triggers_download() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;
        app.ui.search.search_results = songs(5);
        app.ui.search.selected_index = 2;

        // Regular 'd' (no Ctrl) should still trigger download popup
        app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)).await.unwrap();
        assert!(app.ui.download.show_download_popup, "plain d should trigger download popup");
    }

    #[tokio::test]
    async fn test_ctrl_u_at_top_does_not_underflow() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;
        app.ui.search.search_results = songs(30);
        app.ui.search.selected_index = 2;

        let ctrl_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        app.handle_key(ctrl_u).await.unwrap();
        // 2 - 10 = 0 (saturating)
        assert_eq!(app.ui.search.selected_index, 0, "Ctrl+u near top should saturate at 0");
    }

    // ── Task 3.4: Shift+Tab reverse focus cycling ──────────────────────────

    #[tokio::test]
    async fn test_shift_tab_reverse_cycles_from_search_input() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchInput;

        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.focus, Focus::QueueList);
    }

    #[tokio::test]
    async fn test_shift_tab_reverse_cycles_from_search_results() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchResults;

        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.focus, Focus::SearchInput);
    }

    #[tokio::test]
    async fn test_shift_tab_reverse_cycles_from_queue() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::QueueList;

        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.focus, Focus::SearchResults);
    }

    #[tokio::test]
    async fn test_tab_still_cycles_forward() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchInput;

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.focus, Focus::SearchResults);
    }

    // ── Task 3.5: h as help alias ──────────────────────────────────────────

    #[tokio::test]
    async fn test_h_toggles_help_on_from_search() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.active_screen = ActiveScreen::Search;
        app.ui.focus = Focus::SearchResults;

        app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.active_screen, ActiveScreen::Help, "h from search should toggle help ON");
    }

    #[tokio::test]
    async fn test_h_in_search_input_types_char() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.focus = Focus::SearchInput;
        app.ui.active_screen = ActiveScreen::Search;
        app.ui.search.search_query.clear();

        app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.search.search_query, "h", "h in SearchInput should type 'h'");
    }

    // ── Task 3.6: Player q exits to search ─────────────────────────────────

    #[tokio::test]
    async fn test_player_q_returns_to_search() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.active_screen = ActiveScreen::Player;

        let result = app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)).await.unwrap();
        assert!(!result, "q in player should not exit app");
        assert_eq!(app.ui.active_screen, ActiveScreen::Search, "q in player should go to Search");
        assert_eq!(app.ui.focus, Focus::SearchInput, "q in player should set focus to SearchInput");
    }

    // ── Task 3.7: Help screen restricted exit ──────────────────────────────

    #[tokio::test]
    async fn test_help_q_closes_help() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.active_screen = ActiveScreen::Help;

        let result = app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)).await.unwrap();
        assert!(!result, "q in help should NOT exit app (should close help)");
        assert_eq!(app.ui.active_screen, ActiveScreen::Search, "q in help should close to Search");
    }

    #[tokio::test]
    async fn test_help_esc_closes_help() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.active_screen = ActiveScreen::Help;

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.active_screen, ActiveScreen::Search, "Esc in help should close to Search");
    }

    #[tokio::test]
    async fn test_help_question_ignored() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.active_screen = ActiveScreen::Help;

        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.active_screen, ActiveScreen::Help, "? should be silently ignored (only h opens help)");
    }

    #[tokio::test]
    async fn test_help_h_closes_help() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.active_screen = ActiveScreen::Help;

        app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(app.ui.active_screen, ActiveScreen::Search, "h in help should close to Search");
    }

    #[tokio::test]
    async fn test_help_random_key_ignored() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.active_screen = ActiveScreen::Help;

        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)).await.unwrap();
        assert_eq!(
            app.ui.active_screen,
            ActiveScreen::Help,
            "random key in help should be silently ignored"
        );
    }
}
