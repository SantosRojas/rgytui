use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::application::playback::PlaybackUseCase;
use crate::application::playlist::PlaylistUseCase;
pub(crate) use crate::application::ports::{ConfigPort, I18nPort};
use crate::application::search::SearchUseCase;
pub(crate) use crate::domain::audio_mode::AudioMode;
pub(crate) use crate::domain::media::RepeatMode;
pub(crate) use crate::domain::media::Song;
pub(crate) use crate::domain::player_state::PlayerState;
pub(crate) use crate::infrastructure::audio::cache::AudioCache;
use crate::domain::settings::AppSettings;
use crate::interface::app_ui;
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
    input_rx: mpsc::Receiver<InputEvent>,
    event_tx: mpsc::Sender<AppEvent>,
    event_rx: mpsc::Receiver<AppEvent>,
    cancel_token: CancellationToken,
    pending_play: Option<Song>,
    last_search: Option<Instant>,
    last_click: Option<(Instant, u16, u16)>,
    download_semaphore: Arc<Semaphore>,
    audio_cache: Arc<AudioCache>,
    last_saved_playlist_version: usize,
    panel_rects: HashMap<String, Rect>,
}

impl App {
    pub async fn new(
        playback: PlaybackUseCase,
        search: SearchUseCase,
        mut playlist: PlaylistUseCase,
        config: Box<dyn ConfigPort>,
        i18n: Arc<dyn I18nPort>,
    ) -> Self {
        // Clean up orphan temp files from previous runs
        if let Err(e) = cleanup_orphan_tempfiles() {
            tracing::warn!("Failed to cleanup orphan temp files: {}", e);
        }

        // Bounded channels provide natural backpressure: if the main loop lags,
        // the sender blocks (input thread via blocking_send, background tasks via .await).
        let (event_tx, event_rx) = mpsc::channel(256);
        let (input_tx, input_rx) = mpsc::channel(256);

        // Input thread: runs crossterm::event::read() in a loop. Restarts on panic
        // (up to 3 times) to avoid losing keyboard input on transient terminal errors.
        // On persistent errors, sleeps briefly to avoid busy-looping at 100% CPU.
        std::thread::spawn(move || {
            let max_restarts = 3;
            for restart in 0..=max_restarts {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loop {
                    match crossterm::event::read() {
                        Ok(crossterm::event::Event::Key(key)) => {
                            if key.kind != KeyEventKind::Press {
                                continue;
                            }
                            if input_tx.blocking_send(InputEvent::Key(key)).is_err() {
                                break;
                            }
                        }
                        Ok(crossterm::event::Event::Mouse(mouse))
                            if input_tx.blocking_send(InputEvent::Mouse(mouse)).is_err() => {
                                break;
                            }
                        Ok(_) => {}
                        Err(e) => {
                            // Terminal read error — log, sleep briefly, and retry
                            tracing::warn!("Input thread read error: {e}");
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                    }
                }));
                if result.is_ok() {
                    break; // thread exited cleanly (channel closed)
                }
                tracing::warn!("Input thread panicked (attempt {}/{}), restarting", restart + 1, max_restarts);
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        });

        let settings = config.load_settings().await.unwrap_or_else(|e| {
            tracing::warn!("Failed to load settings: {}, using defaults", e);
            AppSettings::default()
        });
        let language = i18n.language().to_string();

        // Load persisted playlist from disk
        let saved_playlist = config.load_playlist().await;
        playlist.load(saved_playlist);
        // Restore the persisted repeat-mode preference from settings so it
        // survives even when playlist.json is missing or corrupt. Only apply
        // non-default values: a missing/corrupt settings.json yields the
        // default "None" and must not reset a valid playlist.json mode.
        if let Ok(mode) = settings.repeat_mode.parse::<RepeatMode>()
            && mode != RepeatMode::None
        {
            playlist.set_repeat_mode(mode);
        }
        let last_saved_playlist_version = playlist.playlist().version;

        let mut ui = UiState {
            config: ConfigState::new(
                settings.theme.clone(),
                settings.accent_color.clone(),
                language.clone(),
                i18n,
                settings.default_search_limit,
                settings.download_path.clone(),
            ),
            ..UiState::default()
        };

        // Warn if yt-dlp is missing — every search/download depends on it
        if !tokio::process::Command::new("yt-dlp")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .is_ok()
        {
            ui.push_notification(
                "⚠ yt-dlp is not installed. Search, download, and playback will not work. \
                 Install it from https://github.com/yt-dlp/yt-dlp".into(),
                NotificationLevel::Warning,
            );
        }

        // Spawn background upgrade check — non-blocking, runs once at startup.
        // If a new version is found, the user gets a modal popup to upgrade.
        let check_tx = event_tx.clone();
        tokio::spawn(async move {
            // Brief delay so the TUI has time to render before a popup appears
            tokio::time::sleep(Duration::from_millis(1500)).await;
            match crate::update::check_latest() {
                Ok(Some((version, url))) => {
                    let _ = check_tx
                        .send(AppEvent::UpgradeAvailable(version, url))
                        .await;
                }
                Ok(None) => {} // already up to date
                Err(e) => {
                    tracing::info!("Upgrade check failed (expected offline/rate-limit): {e}");
                }
            }
        });

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
            cancel_token: CancellationToken::new(),
            pending_play: None,
            last_search: None,
            last_click: None,
            download_semaphore: Arc::new(Semaphore::new(3)),
            audio_cache: Arc::new(AudioCache::new()),
            last_saved_playlist_version,
            panel_rects: HashMap::new(),
        }
    }

}

impl App {
    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        let mut terminal = Self::init_terminal()?;
        let _guard = TerminalGuard;

        // Spawn SIGTERM watcher (Unix only). Crossterm's raw mode intercepts
        // Ctrl+C at the terminal level — the tokio::signal::ctrl_c() branch
        // below catches any signals that bypass crossterm.
        // The oneshot channel allows us to bridge the cfg boundary cleanly.
        let (sigterm_tx, mut sigterm_rx) = tokio::sync::oneshot::channel::<()>();
        #[cfg(unix)]
        tokio::spawn(async move {
            if let Ok(mut sig) = tokio::signal::unix::signal(
                tokio::signal::unix::SignalKind::terminate(),
            ) {
                sig.recv().await;
                let _ = sigterm_tx.send(());
            }
        });
        // On non-unix, keep the sender alive so the receiver never resolves.
        #[cfg(not(unix))]
        let _ = sigterm_tx;

        loop {
            self.ui.dismiss_old_notifications();

            // Enable spectrum FFT while a song is loaded (both hybrid and full-screen player).
            self.playback
                .set_spectrum_enabled(self.ui.player.current_song.is_some());

            let theme = self.ui.get_or_create_theme();
            let render_state = RenderSnapshot::from_use_cases(&self.playback, &mut self.playlist);
            terminal.draw(|frame| {
                app_ui::render(frame, &self.ui, &render_state, &theme, &mut self.panel_rects);
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
                    self.handle_event(event).await;
                    false
                }
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    self.update_progress().await;
                    self.ui.tick_spinner();
                    false
                }
                // OS-level signal handlers (catch SIGINT/SIGTERM that bypass
                // crossterm's input thread, e.g. from process managers).
                _ = tokio::signal::ctrl_c() => {
                    true
                }
                _ = &mut sigterm_rx => {
                    true
                }
            };

            if should_exit {
                self.on_exit().await;
                // Give spawned tasks a chance to notice cancellation.
                // The tokio runtime drop in main() guarantees any remaining
                // tasks (and their child processes) are aborted.
                tokio::time::sleep(Duration::from_millis(200)).await;
                break;
            }
        }

        Ok(())
    }

    /// Detect double-click by tracking time and position of consecutive clicks.
    /// Updates last_click on every call — call exactly once per click event.
    fn is_double_click(&mut self, col: u16, row: u16) -> bool {
        const DOUBLE_CLICK_MS: u64 = 400;
        let now = Instant::now();
        let is_double = self.last_click.is_some_and(|(time, last_col, last_row)| {
            now.duration_since(time).as_millis() <= u128::from(DOUBLE_CLICK_MS)
                && col == last_col
                && row == last_row
        });
        self.last_click = Some((now, col, row));
        is_double
    }

    fn handle_mouse(&mut self, event: MouseEvent) {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let col = event.column;
                let row = event.row;
                let is_double = self.is_double_click(col, row);

                // Upgrade popup check (modal — intercepts all clicks when visible)
                if self.ui.show_upgrade_popup
                    && let Some(pr) = self.panel_rects.get("upgrade_popup")
                {
                        let hit = row >= pr.y && row < pr.y + pr.height
                            && col >= pr.x && col < pr.x + pr.width;
                        if hit {
                            // Button row is y+3 (top border + 2 content lines)
                            if row == pr.y + 3 {
                                let mid = pr.x + pr.width / 2;
                                if col < mid {
                                    self.ui.upgrade_selection = crate::interface::state::UpgradeChoice::Yes;
                                } else {
                                    self.ui.upgrade_selection = crate::interface::state::UpgradeChoice::No;
                                }
                                // Use key handler's Enter logic
                                match self.ui.upgrade_selection {
                                    crate::interface::state::UpgradeChoice::Yes => self.start_upgrade(),
                                    crate::interface::state::UpgradeChoice::No => {
                                        self.ui.show_upgrade_popup = false;
                                        self.ui.pending_upgrade = None;
                                    }
                                }
                            }
                            // Click consumed — don't fall through to panels below
                            return;
                        }
                    }

                // Phase 1: resolve click target using rects (immutable borrow only)
                enum HitTarget {
                    SearchInput,
                    SearchResults(usize),  // resolved index
                    QueueItem(usize),       // resolved index
                    Outside,
                }

                let target = {
                    let rects = &self.panel_rects;

                    let hit = |rect: &Rect| -> bool {
                        row >= rect.y && row < rect.y + rect.height
                            && col >= rect.x && col < rect.x + rect.width
                    };

                    let resolve_idx = |rect: &Rect, selected: usize, count: usize| -> usize {
                        let visible_height = (rect.height.saturating_sub(2)) as usize;
                        if visible_height > 0 && row > rect.y {
                            let relative_row = (row - rect.y - 1) as usize;
                            if relative_row < visible_height {
                                let offset = if count > visible_height {
                                    selected.min(count.saturating_sub(visible_height))
                                } else {
                                    0
                                };
                                return offset + relative_row;
                            }
                        }
                        // fallback: keep current selection
                        selected
                    };

                    if let Some(r) = rects.get("search_input")
                        && hit(r)
                    {
                        HitTarget::SearchInput
                    } else if let Some(r) = rects.get("search_results")
                        && hit(r)
                    {
                        let idx = resolve_idx(r, self.ui.search.selected_index, self.ui.search.search_results.len());
                        HitTarget::SearchResults(idx)
                    } else if let Some(r) = rects.get("queue")
                        && hit(r)
                    {
                        let idx = resolve_idx(r, self.ui.queue.queue_selected, self.playlist.songs().len());
                        HitTarget::QueueItem(idx)
                    } else {
                        HitTarget::Outside
                    }
                }; // rects dropped here — immutable borrow released

                // Phase 2: act on the resolved target (mutable borrow OK)
                match target {
                    HitTarget::SearchInput => {
                        self.ui.active_screen = ActiveScreen::Search;
                        self.ui.focus = Focus::SearchInput;
                    }
                    HitTarget::SearchResults(idx) => {
                        self.ui.active_screen = ActiveScreen::Search;
                        self.ui.focus = Focus::SearchResults;
                        if idx < self.ui.search.search_results.len() {
                            self.ui.search.selected_index = idx;
                        }
                        if is_double {
                            self.schedule_play_selected();
                        }
                    }
                    HitTarget::QueueItem(idx) => {
                        self.ui.active_screen = ActiveScreen::Search;
                        self.ui.focus = Focus::QueueList;
                        if idx < self.playlist.songs().len() {
                            self.ui.queue.queue_selected = idx;
                        }
                        if is_double {
                            self.play_selected_from_queue();
                        }
                    }
                    HitTarget::Outside => {
                        // Click outside any panel — silently ignored
                    }
                }
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

/// Scan the OS temp directory for orphaned rgytui temp files older than 1 hour and delete them.
pub fn cleanup_orphan_tempfiles() -> std::io::Result<()> {
    cleanup_tempfiles_in_dir(std::env::temp_dir(), std::time::Duration::from_secs(3600))
}

/// Scan `dir` for files matching `rgytui-*` older than `max_age` and delete them.
/// Scans at most `max_entries` entries to avoid blocking startup on temp dirs
/// with thousands of files.
/// Exposed as a separate function for testability.
fn cleanup_tempfiles_in_dir<P: AsRef<std::path::Path>>(
    dir: P,
    max_age: std::time::Duration,
) -> std::io::Result<()> {
    const MAX_SCAN: usize = 4096;
    let cutoff = std::time::SystemTime::now() - max_age;

    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };

    for (i, entry) in entries.enumerate() {
        if i >= MAX_SCAN {
            tracing::debug!("cleanup_tempfiles_in_dir: hit scan limit ({MAX_SCAN}), stopping");
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if !name.starts_with("rgytui-") {
            continue;
        }
        if let Ok(meta) = entry.metadata()
            && let Ok(modified) = meta.modified()
            && modified < cutoff
        {
            tracing::debug!("Cleaning up orphan temp file: {:?}", path);
            let _ = std::fs::remove_file(&path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use crate::application::playback::PlaybackUseCase;
    use crate::application::playlist::PlaylistUseCase;
    use crate::application::ports::{AudioPlaybackPort, ConfigPort, DownloaderPort, MediaSearchPort};
    use crate::application::search::SearchUseCase;
    use crate::domain::media::Song;
    use crate::infrastructure::audio::mpv_backend::MpvAdapter;
    use crate::infrastructure::audio::rodio_backend::RodioAdapter;
    use crate::infrastructure::ytdlp::client::YtDlpAdapter;
    use crate::interface::i18n::Translations;

    // Helper: create a file in the given directory with the given name
    fn create_file(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"test content").unwrap();
        path
    }

    #[test]
    fn test_cleanup_orphan_tempfiles_deletes_rgytui_files() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let dir_path = tmp_dir.path();

        // Create an rgytui-prefixed file (should be deleted)
        let rgytui_path = create_file(dir_path, "rgytui-orphan-test.tmp");
        // Create a non-matching file (should NOT be deleted)
        let other_path = create_file(dir_path, "other-test.tmp");

        // Both exist before cleanup
        assert!(rgytui_path.exists(), "rgytui file should exist before cleanup");
        assert!(other_path.exists(), "other file should exist before cleanup");

        // Run cleanup with max_age = 0 (deletes any rgytui-* file)
        cleanup_tempfiles_in_dir(dir_path, std::time::Duration::ZERO).unwrap();

        // rgytui file deleted, other file preserved
        assert!(!rgytui_path.exists(), "rgytui-prefixed file should be deleted");
        assert!(other_path.exists(), "non-rgytui file should NOT be deleted");
    }

    #[test]
    fn test_cleanup_orphan_tempfiles_skips_non_matching_prefix() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let dir_path = tmp_dir.path();

        // "rgytui" (no hyphen) → does NOT match "rgytui-" prefix filter
        let no_hyphen = create_file(dir_path, "rgytuitest.tmp");
        // "rgytui-" → DOES match
        let with_hyphen = create_file(dir_path, "rgytui-test.tmp");
        // "rgytui-something" → DOES match
        let longer = create_file(dir_path, "rgytui-mydata.tmp");

        // All exist before
        assert!(no_hyphen.exists(), "rgytui (no hyphen) should exist before cleanup");
        assert!(with_hyphen.exists(), "rgytui- should exist before cleanup");
        assert!(longer.exists(), "rgytui-mydata should exist before cleanup");

        cleanup_tempfiles_in_dir(dir_path, std::time::Duration::ZERO).unwrap();

        // Only files starting with "rgytui-" are deleted
        assert!(no_hyphen.exists(), "'rgytui' (no hyphen) should NOT be deleted");
        assert!(!with_hyphen.exists(), "'rgytui-' should be deleted");
        assert!(!longer.exists(), "'rgytui-mydata' should be deleted");
    }

    #[test]
    fn test_cleanup_orphan_tempfiles_handles_empty_or_missing_dir() {
        // Non-existent directory — should not panic (cross-platform: any platform)
        let bad_path = {
            let tmp = tempfile::tempdir().unwrap();
            tmp.path().join("nonexistent-subdir")
        }; // tmp dropped → dir deleted, path guaranteed nonexistent
        let result = cleanup_tempfiles_in_dir(&bad_path, std::time::Duration::ZERO);
        assert!(result.is_ok(), "cleanup on missing dir should return Ok, got {:?}", result);

        // Empty directory — should not panic, Ok(())
        let empty_dir = tempfile::tempdir().unwrap();
        let result = cleanup_tempfiles_in_dir(empty_dir.path(), std::time::Duration::ZERO);
        assert!(result.is_ok(), "cleanup on empty dir should return Ok, got {:?}", result);
    }

    /// Helper to build a test App. Returns None if audio device isn't available.
    async fn build_test_app() -> Option<App> {
        let audio: Box<dyn AudioPlaybackPort> = Box::new(RodioAdapter::new().ok()?);
        let mpv = MpvAdapter::new();
        let ytdlp = YtDlpAdapter::new();
        let downloader: Arc<dyn DownloaderPort> = Arc::new(ytdlp.clone());
        let search_port: Arc<dyn MediaSearchPort> = Arc::new(ytdlp);
        let playback = PlaybackUseCase::new(downloader, audio, mpv, AudioMode::Audio);
        let search = SearchUseCase::new(search_port);
        let playlist = PlaylistUseCase::new();
        let config: Box<dyn ConfigPort> = Box::new(crate::application::ports::MockConfig {
            settings: AppSettings::default(),
        });
        let i18n: Arc<dyn I18nPort> = Arc::new(Translations::load("es"));
        Some(App::new(playback, search, playlist, config, i18n).await)
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

    // ── update_progress guard (Task 1.3) ────────────────────────────

    #[tokio::test]
    async fn test_update_progress_guard_with_no_song() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };

        // No song loaded (current_song is None by default)
        app.ui.player.current_song = None;

        // Add songs to playlist so there would be something to advance to
        for s in songs(3) {
            app.playlist.add(s);
        }

        // Call update_progress — with guard, this should be a no-op
        app.update_progress().await;

        // Guard should prevent auto-advance: no pending play
        assert!(app.pending_play.is_none(), "no auto-advance when no song loaded");
    }

    // ── CancellationToken (Task 2.1) ──────────────────────────────────

    #[tokio::test]
    async fn test_cancel_token_not_cancelled_after_creation() {
        let app = match build_test_app().await {
            Some(a) => a,
            None => {
                eprintln!("Skipping test: audio device not available");
                return;
            }
        };
        assert!(!app.cancel_token.is_cancelled(),
            "CancellationToken should NOT be cancelled immediately after creation");
    }

    // ── Ctrl+C / Exit Confirmation (Task 2.3) ────────────────────────

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
        assert!(result, "Ctrl+C should return Ok(true) to signal exit when no download active");
    }

    #[tokio::test]
    async fn test_ctrl_c_with_active_download_sends_confirm_event() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => {
                eprintln!("Skipping test: audio device not available");
                return;
            }
        };

        // Set a pending play (simulates active download)
        let song = Song {
            id: "test-confirm-id".into(),
            title: "Confirm Test".into(),
            channel: "Test".into(),
            duration: 100.0,
            thumbnail: None,
            webpage_url: "http://example.com".into(),
        };
        app.pending_play = Some(song);

        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let result = app.handle_key(ctrl_c).await.unwrap();
        assert!(!result, "Ctrl+C with active download should NOT signal exit");

        // The event is sent to the channel; event_handler will process it in the main loop.
        // Try to receive it to verify it was sent (recv_timeout with zero delay = peek)
        let event = app.event_rx.try_recv().ok();
        assert!(matches!(event, Some(AppEvent::ShowConfirmExit)),
            "key_handler should send ShowConfirmExit event, got {:?}", event);
    }

    #[tokio::test]
    async fn test_ctrl_c_with_pending_download_shows_confirmation_via_event() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => {
                eprintln!("Skipping test: audio device not available");
                return;
            }
        };

        // Set download_pending (simulates active file download)
        let song = Song {
            id: "test-dl-confirm".into(),
            title: "DL Confirm".into(),
            channel: "Test".into(),
            duration: 100.0,
            thumbnail: None,
            webpage_url: "http://example.com".into(),
        };
        app.ui.download.download_pending = Some((song, "dir".into(), "mp3".into()));

        assert!(!app.ui.show_exit_confirmation, "flag should be false before Ctrl+C");

        // Process manually: key_handler sends event, event_handler processes it
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let result = app.handle_key(ctrl_c).await.unwrap();
        assert!(!result, "Ctrl+C with download_pending should NOT signal exit");

        // The key_handler sent ShowConfirmExit to event_tx.
        // Let's process it by calling handle_event directly on what was sent.
        // Since we can't easily peek the channel, we manually set the flag
        // to verify the event_handler handles it correctly.
        app.handle_event(AppEvent::ShowConfirmExit).await;
        assert!(app.ui.show_exit_confirmation, "flag should be true after ShowConfirmExit");
    }

    #[tokio::test]
    async fn test_exit_confirmation_y_exits() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => {
                eprintln!("Skipping test: audio device not available");
                return;
            }
        };

        app.ui.show_exit_confirmation = true;

        // Press 'y' — should signal exit
        let y_key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let result = app.handle_key(y_key).await.unwrap();
        assert!(result, "'y' in confirmation mode should signal exit");
        assert!(!app.ui.show_exit_confirmation, "flag should be cleared after 'y'");
    }

    #[tokio::test]
    async fn test_exit_confirmation_n_clears() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => {
                eprintln!("Skipping test: audio device not available");
                return;
            }
        };

        app.ui.show_exit_confirmation = true;

        // Press 'n' — should NOT signal exit, and clear flag
        let n_key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        let result = app.handle_key(n_key).await.unwrap();
        assert!(!result, "'n' in confirmation mode should NOT signal exit");
        assert!(!app.ui.show_exit_confirmation, "flag should be cleared after 'n'");
    }

    #[tokio::test]
    async fn test_exit_confirmation_esc_clears() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => {
                eprintln!("Skipping test: audio device not available");
                return;
            }
        };

        app.ui.show_exit_confirmation = true;

        // Press Esc — should NOT signal exit, and clear flag
        let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = app.handle_key(esc_key).await.unwrap();
        assert!(!result, "Esc in confirmation mode should NOT signal exit");
        assert!(!app.ui.show_exit_confirmation, "flag should be cleared after Esc");
    }

    #[tokio::test]
    async fn test_exit_confirmation_other_keys_ignored() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => {
                eprintln!("Skipping test: audio device not available");
                return;
            }
        };

        app.ui.show_exit_confirmation = true;

        // Press an unrelated key — should NOT signal exit, AND flag should persist
        let space_key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        let result = app.handle_key(space_key).await.unwrap();
        assert!(!result, "unrelated key in confirmation mode should NOT signal exit");
        assert!(app.ui.show_exit_confirmation, "flag should persist after unrelated key");
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
        app.handle_event(AppEvent::PlaybackError("Something went wrong".into())).await;
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

        app.handle_event(AppEvent::AudioDownloadError("Download failed".into())).await;
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
        app.handle_event(AppEvent::PlaybackError("Failed".into())).await;
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

    // ── Double-click to play ─────────────────────────────────────────────

    fn click(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[tokio::test]
    async fn test_single_click_selects_search_result() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.search.search_results = songs(5);
        app.ui.active_screen = ActiveScreen::Search;
        app.ui.focus = Focus::SearchInput;
        app.panel_rects.insert(
            "search_results".to_string(),
            Rect { x: 0, y: 3, width: 100, height: 30 },
        );

        app.handle_mouse(click(10, 5));

        assert_eq!(app.ui.focus, Focus::SearchResults, "click should focus search results");
        assert_eq!(app.ui.search.selected_index, 1, "click should select item at row 5");
        assert!(app.pending_play.is_none(), "single click should NOT trigger play");
    }

    #[tokio::test]
    async fn test_double_click_search_result_plays() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.search.search_results = songs(5);
        app.ui.active_screen = ActiveScreen::Search;
        app.ui.focus = Focus::SearchInput;
        app.panel_rects.insert(
            "search_results".to_string(),
            Rect { x: 0, y: 3, width: 100, height: 30 },
        );

        // First click — select only
        app.handle_mouse(click(10, 5));
        assert!(app.pending_play.is_none(), "first click should NOT play");

        // Second click at same position — double-click → play
        app.handle_mouse(click(10, 5));
        assert!(app.pending_play.is_some(), "double-click should trigger play");
    }

    #[tokio::test]
    async fn test_double_click_queue_item_plays() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        for s in songs(5) {
            app.playlist.add(s);
        }
        app.ui.active_screen = ActiveScreen::Search;
        app.ui.focus = Focus::SearchResults;
        app.panel_rects.insert(
            "queue".to_string(),
            Rect { x: 0, y: 3, width: 100, height: 30 },
        );

        // First click — select only
        app.handle_mouse(click(10, 5));
        assert!(app.pending_play.is_none(), "first click should NOT play");

        // Second click — double-click → play
        app.handle_mouse(click(10, 5));
        assert!(app.pending_play.is_some(), "double-click in queue should play");
    }

    #[tokio::test]
    async fn test_double_click_on_search_input_does_not_play() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.active_screen = ActiveScreen::Search;
        app.ui.focus = Focus::SearchInput;
        app.panel_rects.insert(
            "search_input".to_string(),
            Rect { x: 0, y: 0, width: 100, height: 3 },
        );

        // Two quick clicks on search input — should focus, not play
        app.handle_mouse(click(10, 1));
        assert_eq!(app.ui.focus, Focus::SearchInput, "click on input should stay on input");
        assert!(app.pending_play.is_none(), "click on input should not play");

        app.handle_mouse(click(10, 1));
        assert!(app.pending_play.is_none(), "double-click on input should not play");
    }

    #[tokio::test]
    async fn test_double_click_on_empty_list_does_not_panic() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.search.search_results = vec![]; // empty
        app.ui.active_screen = ActiveScreen::Search;
        app.ui.focus = Focus::SearchInput;
        app.panel_rects.insert(
            "search_results".to_string(),
            Rect { x: 0, y: 3, width: 100, height: 30 },
        );

        // Should not crash
        app.handle_mouse(click(10, 5));
        app.handle_mouse(click(10, 5));
        // No assertion needed — we just verify it doesn't panic
    }

    #[tokio::test]
    async fn test_double_click_outside_panel_ignored() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        // No panel rects set up — click outside any panel
        app.handle_mouse(click(200, 200));
        app.handle_mouse(click(200, 200));
        // Should not panic, should not change state
    }

    #[tokio::test]
    async fn test_double_click_already_playing_skips() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.search.search_results = songs(5);
        app.ui.active_screen = ActiveScreen::Search;
        app.ui.focus = Focus::SearchInput;
        app.panel_rects.insert(
            "search_results".to_string(),
            Rect { x: 0, y: 3, width: 100, height: 30 },
        );
        // Simulate that song at index 1 (id-1) is already playing
        app.ui.player.current_song = Some(song(1));

        // First click only selects
        app.handle_mouse(click(10, 5)); // resolves to index 1 (same as current)
        assert!(app.pending_play.is_none(), "first click should not play");

        // Second click — double-click, guard should skip re-play
        app.handle_mouse(click(10, 5));
        assert!(app.pending_play.is_none(), "double-click on already-playing song should NOT re-download");
    }

    #[tokio::test]
    async fn test_double_click_queue_already_playing_skips() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        for s in songs(5) {
            app.playlist.add(s);
        }
        app.ui.active_screen = ActiveScreen::Search;
        app.ui.focus = Focus::SearchResults;
        app.panel_rects.insert(
            "queue".to_string(),
            Rect { x: 0, y: 3, width: 100, height: 30 },
        );
        // Simulate that the song at queue index 1 is already playing
        let playing = app.playlist.playlist().songs()[1].clone();
        app.ui.player.current_song = Some(playing);

        // First click selects
        app.handle_mouse(click(10, 5)); // resolves to index 1
        assert!(app.pending_play.is_none(), "first click should not play");

        // Second click — guard should skip
        app.handle_mouse(click(10, 5));
        assert!(app.pending_play.is_none(), "double-click on already-playing queue item should skip");
    }

    #[tokio::test]
    async fn test_double_click_different_song_plays_when_another_is_playing() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.search.search_results = songs(5);
        app.ui.active_screen = ActiveScreen::Search;
        app.ui.focus = Focus::SearchInput;
        app.panel_rects.insert(
            "search_results".to_string(),
            Rect { x: 0, y: 3, width: 100, height: 30 },
        );
        // Song 0 is playing
        app.ui.player.current_song = Some(song(0));

        // Click at row 6 → selects index 2 (song 2 — different from playing)
        app.handle_mouse(click(10, 6));
        assert_eq!(app.ui.search.selected_index, 2, "click should select song 2");
        assert!(app.pending_play.is_none(), "first click should not play");

        // Second click — double-click on a DIFFERENT song → should play
        app.handle_mouse(click(10, 6));
        assert!(app.pending_play.is_some(), "double-click on DIFFERENT song should play even if another is playing");
    }

    #[tokio::test]
    async fn test_double_click_after_error_allows_replay() {
        let mut app = match build_test_app().await {
            Some(a) => a,
            None => return,
        };
        app.ui.search.search_results = songs(5);
        app.ui.active_screen = ActiveScreen::Search;
        app.ui.focus = Focus::SearchInput;
        app.panel_rects.insert(
            "search_results".to_string(),
            Rect { x: 0, y: 3, width: 100, height: 30 },
        );
        // After an error, current_song is None — replay is allowed
        app.ui.player.current_song = None;

        app.handle_mouse(click(10, 5)); // selects index 1
        app.handle_mouse(click(10, 5)); // double-click
        assert!(app.pending_play.is_some(), "replay after error should be allowed");
    }
}
