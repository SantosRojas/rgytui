use super::*;

impl App {
    pub(crate) async fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::SearchResults(songs) => {
                let count = songs.len();
                self.ui.search.search_results = songs;
                self.ui.search.is_searching = false;
                self.ui.search.selected_index = 0;
                self.ui.focus = Focus::SearchResults;
                self.ui.push_notification(
                    self.ui.tr("notif_search_count").replace("{}", &count.to_string()),
                    NotificationLevel::Info,
                );
            }
            AppEvent::SearchError(err) => {
                self.ui.search.is_searching = false;
                self.ui.push_notification(
                    self.ui.tr("err_search").replace("{}", &err),
                    NotificationLevel::Error,
                );
            }
            AppEvent::PlaybackFinished => {
                // Auto-advance is handled by update_progress() polling.
                // This arm exists only for future event-driven use.
                // Keep it as a no-op to avoid double-advance bugs.
            }
            AppEvent::PlaybackError { message, generation } => {
                // Video stream resolution failed in a background task. Ignore
                // it if a newer play superseded it (generation bump) or the
                // mode flipped since (the task belongs to the old mode) —
                // clearing current_song here would abort the fresh playback
                // that the toggle just started.
                if generation != self.play_generation
                    || !matches!(self.playback.mode(), AudioMode::Video)
                {
                    return;
                }
                self.ui.player.loading_status = None;
                self.ui.push_notification(
                    self.ui.tr("err_playback").replace("{}", &message),
                    NotificationLevel::Error,
                );
                self.ui.player.current_song = None;
            }
            AppEvent::PlaybackHealthError(err) => {
                // Live backend error (watchdog detected a dead/stalled audio
                // device). Always current — clear playback state.
                self.ui.player.loading_status = None;
                self.ui.push_notification(
                    self.ui.tr("err_playback").replace("{}", &err),
                    NotificationLevel::Error,
                );
                self.ui.player.current_song = None;
            }
            AppEvent::DownloadComplete { song_title } => {
                self.ui.player.loading_status = None;
                self.ui.push_notification(
                    self.ui.tr("notif_downloaded").replace("{}", &song_title),
                    NotificationLevel::Success,
                );
            }
            AppEvent::DownloadError(err) => {
                self.ui.player.loading_status = None;
                self.ui.push_notification(
                    self.ui.tr("err_download_failed").replace("{}", &err),
                    NotificationLevel::Error,
                );
            }
            AppEvent::AudioReady { song, data, generation } => {
                // Drop stale events: the song was switched away, the playback
                // mode changed after the download started, or the task belongs
                // to an older play generation (a mode toggle re-queues the
                // song and spawns a fresh task in the new mode). Playing them
                // would start the OLD mode's backend over the new one.
                let song_is_current = self
                    .ui
                    .player
                    .current_song
                    .as_ref()
                    .map(|s| s.id == song.id)
                    .unwrap_or(false);
                if generation != self.play_generation
                    || !song_is_current
                    || !matches!(self.playback.mode(), AudioMode::Audio)
                {
                    return;
                }
                match self.playback.play_bytes(data, song) {
                    Ok(()) => {
                        self.ui.push_notification(self.ui.tr("notif_playing"), NotificationLevel::Info);
                    }
                    Err(e) => {
                        self.ui.push_notification(
                            self.ui.tr("err_playback").replace("{}", &e.to_string()),
                            NotificationLevel::Error,
                        );
                        self.ui.player.current_song = None;
                    }
                }
                self.ui.player.loading_status = None;
            }
            AppEvent::AudioDownloadError { message, generation } => {
                // Background audio download failed. Ignore it if a newer play
                // superseded it or the mode flipped to Video since — clearing
                // current_song would abort the fresh playback just started.
                if generation != self.play_generation
                    || !matches!(self.playback.mode(), AudioMode::Audio)
                {
                    return;
                }
                self.ui.push_notification(
                    self.ui.tr("err_playback").replace("{}", &message),
                    NotificationLevel::Error,
                );
                self.ui.player.loading_status = None;
                self.ui.player.current_song = None;
            }
            AppEvent::VideoStreamReady { song, stream_url, generation } => {
                // Drop stale events from a mode toggle or song switch — the
                // stream belongs to the old mode/selection, or the task is
                // from an older play generation. Without this, a leftover
                // video stream would spawn mpv over the new mode. Note the
                // loading indicator is cleared only AFTER the guards: a stale
                // event must not wipe the fresh play's spinner.
                let song_is_current = self
                    .ui
                    .player
                    .current_song
                    .as_ref()
                    .map(|s| s.id == song.id)
                    .unwrap_or(false);
                if generation != self.play_generation
                    || !song_is_current
                    || !matches!(self.playback.mode(), AudioMode::Video)
                {
                    return;
                }
                match self.playback.play_video_stream(&stream_url, song).await {
                    Ok(()) => {}
                    Err(e) => {
                        self.ui.push_notification(
                            self.ui.tr("err_playback").replace("{}", &e.to_string()),
                            NotificationLevel::Error,
                        );
                        self.ui.player.current_song = None;
                    }
                }
                self.ui.player.loading_status = None;
            }
            AppEvent::ShowConfirmExit => {
                self.ui.show_exit_confirmation = true;
                self.ui.push_notification(
                    self.ui.tr("confirm_exit"),
                    NotificationLevel::Warning,
                );
            }
            AppEvent::UpgradeAvailable(version, url) => {
                self.ui.pending_upgrade = Some((version, url));
                self.ui.show_upgrade_popup = true;
            }
            AppEvent::Notification(msg) => {
                self.ui.is_upgrading = false;
                let (key, level) = if msg.starts_with("upgrade_complete") {
                    ("upgrade_complete", NotificationLevel::Success)
                } else if msg.starts_with("upgrade_failed") {
                    ("upgrade_failed", NotificationLevel::Error)
                } else {
                    // Generic notification — use as-is
                    self.ui.push_notification(msg, NotificationLevel::Info);
                    return;
                };
                self.ui.push_notification(self.ui.tr(key), level);
            }
        }
    }
}
