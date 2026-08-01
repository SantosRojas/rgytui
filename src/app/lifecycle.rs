use super::*;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{enable_raw_mode, EnterAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

impl App {
    pub(super) fn init_terminal() -> std::io::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
        enable_raw_mode()?;
        std::io::stdout().execute(EnterAlternateScreen)?;
        std::io::stdout().execute(EnableMouseCapture)?;
        let backend = CrosstermBackend::new(std::io::stdout());
        Terminal::new(backend)
    }

    pub(crate) fn queue_play(&mut self, song: Song) {
        // New play generation: background tasks spawned from this pending play
        // carry this counter, so events from any earlier task are recognized
        // as stale and dropped.
        self.play_generation += 1;
        self.ui.player.current_song = Some(song.clone());
        self.pending_play = Some(song);
    }

    pub(crate) async fn try_save_playlist(&mut self) {
        let current = self.playlist.playlist().version;
        if current != self.last_saved_playlist_version {
            if let Err(e) = self.config.save_playlist(self.playlist.playlist()).await {
                tracing::warn!("Failed to save playlist: {e}");
                self.ui.push_notification(
                    format!("Failed to save playlist: {}", e),
                    crate::interface::state::NotificationLevel::Warning,
                );
            } else {
                self.last_saved_playlist_version = current;
            }
        }
    }

    pub(crate) async fn on_exit(&mut self) {
        // Signal cancellation to all spawned background tasks
        self.cancel_token.cancel();

        if let Err(e) = self.playback.stop() {
            tracing::warn!("Failed to stop playback on exit: {}", e);
        }
        self.settings.volume = self.playback.volume();
        self.settings.theme = self.ui.config.theme_name.clone();
        self.settings.accent_color = self.ui.config.accent_color.clone();
        self.settings.default_search_limit = self.ui.config.default_search_limit;
        self.settings.download_path = self.ui.config.download_path.clone();
        self.settings.language = self.ui.config.language.clone();
        self.settings.audio_mode = matches!(self.playback.mode(), AudioMode::Video);
        self.settings.repeat_mode = self.playlist.repeat_mode().as_str().to_string();
        if let Err(e) = self.config.save_settings(&self.settings).await {
            tracing::warn!("Failed to save settings: {}", e);
        }
        if let Err(e) = self.config.save_playlist(self.playlist.playlist()).await {
            tracing::warn!("Failed to save playlist: {}", e);
        }
    }
}

pub(super) struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        use crossterm::terminal::{disable_raw_mode, LeaveAlternateScreen};
        use crossterm::ExecutableCommand;
        disable_raw_mode().ok();
        std::io::stdout().execute(DisableMouseCapture).ok();
        std::io::stdout().execute(LeaveAlternateScreen).ok();
    }
}
