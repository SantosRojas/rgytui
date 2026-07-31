use super::*;

impl App {
    pub(crate) async fn update_progress(&mut self) {
        // Guard: skip 50ms polling work when no song is loaded
        if self.ui.player.current_song.is_none() {
            return;
        }

        // Watchdog: detect a dead/stalled audio backend (e.g. the device dying
        // after a long pause). Routed through the existing PlaybackError flow
        // so current_song is cleared and the user is notified, keeping the UI
        // responsive instead of freezing forever on the next play action.
        if let Err(e) = self.playback.check_health() {
            tracing::warn!("Playback health check failed: {e}");
            self.handle_event(AppEvent::PlaybackError(e.to_string())).await;
            return;
        }

        if self.playback.state() == PlayerState::Playing && self.playback.is_sink_empty() {
            if let Err(e) = self.playback.stop() {
                tracing::warn!("Failed to stop on auto-advance: {}", e);
            }

            if self.playlist.repeat_mode() == RepeatMode::One
                && self.playlist.has_current_song()
            {
                // Repeat-one: replay the same song
                self.ui.player.current_song = None;
                if let Some(song) = self.playlist.current_song_cloned() {
                    self.queue_play(song);
                }
            } else if let Some(next) = self.playlist.next().cloned() {
                self.queue_play(next);
            } else {
                self.ui.player.current_song = None;
            }
        }
    }

    /// Guard: skip re-play if the same song is already playing/paused/loading.
    /// Only allows re-play when `current_song` is `None` (idle or after an error).
    fn guard_already_playing(&self, song_id: &str) -> bool {
        if let Some(ref current) = self.ui.player.current_song
            && current.id == song_id
        {
            return true;
        }
        false
    }

    pub(crate) fn play_selected_from_queue(&mut self) {
        let idx = self.ui.queue.queue_selected;
        if idx < self.playlist.songs().len() {
            self.playlist.set_current_index(idx);
            if let Some(song) = self.playlist.current_song_cloned() {
                if self.guard_already_playing(&song.id) {
                    self.ui.push_notification(self.ui.tr("notif_already_playing"), NotificationLevel::Info);
                    return;
                }
                self.queue_play(song);
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

        // Don't re-play the same song if it's already playing/paused/loading.
        // Re-play is only allowed after an error (current_song = None).
        if self.guard_already_playing(&song.id) {
            self.ui.push_notification(self.ui.tr("notif_already_playing"), NotificationLevel::Info);
            return;
        }

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
