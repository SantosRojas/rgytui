use super::*;

impl App {
    pub(crate) fn sync_ui_queue(&mut self) {
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

    pub(crate) fn update_progress(&mut self) {
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

    pub(crate) fn schedule_play_selected(&mut self) {
        if self.ui.search_results.is_empty() {
            return;
        }
        let idx = self.ui.selected_index;
        let song = match self.ui.search_results.get(idx) {
            Some(s) => s.clone(),
            None => return,
        };
        // If the song is already in the queue, jump to it and re-trigger playback
        // (allows retry after an error without the "already in queue" short-circuit)
        if let Some(pos) = self.playlist.songs().iter().position(|s| s.id == song.id) {
            self.playlist.set_current_index(pos);
            self.queue_play(song);
            return;
        }
        let pos = self.playlist.len();
        self.playlist.add(song.clone());
        self.playlist.set_current_index(pos);
        self.queue_play(song);
    }
}
