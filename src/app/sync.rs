use super::*;

impl App {
    pub(crate) fn update_progress(&mut self) {
        let state = self.playback.state();
        if let PlayerState::Playing | PlayerState::Paused = state {
            if self.playback.state() == PlayerState::Playing
                && self.playback.is_sink_empty()
            {
                if let Err(e) = self.playback.stop() {
                    tracing::warn!("Failed to stop on auto-advance: {}", e);
                }

                if let Some(next) = self.playlist.next().cloned() {
                    self.queue_play(next);
                }
            }
        }
    }

    pub(crate) fn schedule_play_selected(&mut self) {
        if self.ui.search.search_results.is_empty() {
            return;
        }
        let idx = self.ui.search.selected_index;
        let song = match self.ui.search.search_results.get(idx) {
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
