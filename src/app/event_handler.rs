use super::*;

impl App {
    pub(crate) fn handle_event(&mut self, event: AppEvent) {
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
            AppEvent::PlaybackError(err) => {
                self.ui.push_notification(
                    self.ui.tr("err_playback").replace("{}", &err),
                    NotificationLevel::Error,
                );
                self.ui.player.current_song = None;
            }
            AppEvent::DownloadComplete { song_title } => {
                self.ui.push_notification(
                    self.ui.tr("notif_downloaded").replace("{}", &song_title),
                    NotificationLevel::Success,
                );
            }
            AppEvent::DownloadError(err) => {
                self.ui.push_notification(
                    self.ui.tr("err_download_failed").replace("{}", &err),
                    NotificationLevel::Error,
                );
            }
            AppEvent::AudioReady { song, data } => {
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
            AppEvent::AudioDownloadError(err) => {
                self.ui.push_notification(
                    self.ui.tr("err_playback").replace("{}", &err),
                    NotificationLevel::Error,
                );
                self.ui.player.loading_status = None;
                self.ui.player.current_song = None;
            }
            AppEvent::ShowConfirmExit => {
                self.ui.show_exit_confirmation = true;
                self.ui.push_notification(
                    self.ui.tr("confirm_exit"),
                    NotificationLevel::Warning,
                );
            }
        }
    }
}
