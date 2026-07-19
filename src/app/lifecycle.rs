use super::*;

use crossterm::terminal::{enable_raw_mode, EnterAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

impl App {
    pub(super) fn init_terminal() -> std::io::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
        enable_raw_mode()?;
        std::io::stdout().execute(EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(std::io::stdout());
        Terminal::new(backend)
    }

    pub(crate) fn queue_play(&mut self, song: Song) {
        self.ui.player.current_song = Some(song.clone());
        self.pending_play = Some(song);
    }

    pub(crate) async fn on_exit(&mut self) {
        if let Err(e) = self.playback.stop() {
            tracing::warn!("Failed to stop playback on exit: {}", e);
        }
        self.settings.volume = self.playback.volume();
        self.settings.theme = self.ui.config.theme_name.clone();
        self.settings.accent_color = self.ui.config.accent_color.clone();
        self.settings.default_search_limit = self.ui.config.default_search_limit;
        self.settings.download_path = self.ui.config.download_path.clone();
        self.settings.language = self.ui.config.language.clone();
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
        std::io::stdout().execute(LeaveAlternateScreen).ok();
    }
}
