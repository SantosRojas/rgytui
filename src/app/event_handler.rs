use super::*;

impl App {
    pub(crate) fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::SearchResults(songs) => {
                let count = songs.len();
                self.ui.search_results = songs;
                self.ui.is_searching = false;
                self.ui.selected_index = 0;
                self.ui.focus = Focus::SearchResults;
                self.ui.push_notification(
                    self.ui.tr("notif_search_count").replace("{}", &count.to_string()),
                    NotificationLevel::Info,
                );
            }
            AppEvent::SearchError(err) => {
                self.ui.is_searching = false;
                self.ui.push_notification(
                    self.ui.tr("err_search").replace("{}", &err),
                    NotificationLevel::Error,
                );
            }
            AppEvent::PlaybackStarted(song) => {
                self.ui.current_song = Some(song);
                self.ui.player_state = PlayerState::Loading;
                self.ui.is_searching = false;
            }
            AppEvent::PlaybackFinished => {
                self.ui.player_state = PlayerState::Stopped;
                if let Some(next) = self.playlist.next().cloned() {
                    self.queue_play(next);
                }
            }
            AppEvent::PlaybackPaused => {
                self.ui.player_state = PlayerState::Paused;
            }
            AppEvent::PlaybackResumed => {
                self.ui.player_state = PlayerState::Playing;
            }
            AppEvent::PlaybackStopped => {
                self.ui.player_state = PlayerState::Stopped;
                self.ui.progress = 0.0;
            }
            AppEvent::PlaybackError(err) => {
                self.ui.player_state = PlayerState::Stopped;
                self.ui.push_notification(
                    self.ui.tr("err_playback").replace("{}", &err),
                    NotificationLevel::Error,
                );
                self.ui.current_song = None;
            }
            AppEvent::VolumeChanged(vol) => {
                self.ui.volume = vol;
                self.ui.push_notification(
                    self.ui.tr("notif_volume").replace("{:.0}", &format!("{:.0}", vol * 100.0)),
                    NotificationLevel::Info,
                );
            }
            AppEvent::DownloadComplete { song_title, file_path: _ } => {
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
                        self.ui.player_state = PlayerState::Playing;
                        self.ui.progress = 0.0;
                        self.ui.duration = self.playback.current_duration();
                        self.ui.push_notification(self.ui.tr("notif_playing"), NotificationLevel::Info);
                    }
                    Err(e) => {
                        self.ui.player_state = PlayerState::Stopped;
                        self.ui.push_notification(
                            self.ui.tr("err_playback").replace("{}", &e.to_string()),
                            NotificationLevel::Error,
                        );
                        self.ui.current_song = None;
                    }
                }
                self.ui.loading_status = None;
            }
            AppEvent::AudioDownloadError(err) => {
                self.ui.player_state = PlayerState::Stopped;
                self.ui.push_notification(
                    self.ui.tr("err_playback").replace("{}", &err),
                    NotificationLevel::Error,
                );
                self.ui.loading_status = None;
                self.ui.current_song = None;
            }
            AppEvent::Exit => {
                tracing::info!("Exit event received — triggering cleanup");
            }
        }
    }
}
