use super::*;

impl App {
    pub(crate) fn spawn_search(&self, query: String, limit: usize) {
        let tx = self.event_tx.clone();
        let search_uc = self.search.clone();
        tokio::spawn(async move {
            match search_uc.execute(&query, limit).await {
                Ok(songs) => {
                    let _ = tx.send(AppEvent::SearchResults(songs));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::SearchError(e.to_string()));
                }
            }
        });
    }

    pub(crate) async fn handle_pending_play(&mut self) -> bool {
        if let Some(song) = self.pending_play.take() {
            let song_name = song.title.clone();
            self.ui.player.loading_status =
                Some(self.ui.tr("downloading").replace("{}", &song_name));

            match self.playback.mode() {
                AudioMode::Video => {
                    // Video mode: still blocking (mpv handles its own window)
                    match self.playback.play(&song).await {
                        Ok(()) => {
                            self.ui.player_state = PlayerState::Playing;
                            self.ui.progress = 0.0;
                            self.ui.duration = self.playback.current_duration();
                            self.ui.player.loading_status = None;
                        }
                        Err(e) => {
                            self.ui.player_state = PlayerState::Stopped;
                            self.ui.push_notification(
                                self.ui.tr("err_playback").replace("{}", &e.to_string()),
                                NotificationLevel::Error,
                            );
                            self.ui.player.loading_status = None;
                            self.ui.player.current_song = None;
                        }
                    }
                }
                AudioMode::Audio => {
                    // Audio mode: spawn download in background so UI keeps animating
                    let tx = self.event_tx.clone();
                    let url = song.webpage_url.clone();
                    let song_for_event = song.clone();
                    tokio::spawn(async move {
                        match PlaybackUseCase::download_audio_bytes(url).await {
                            Ok(data) => {
                                let _ =
                                    tx.send(AppEvent::AudioReady { song: song_for_event, data });
                            }
                            Err(e) => {
                                let _ = tx.send(AppEvent::AudioDownloadError(e.to_string()));
                            }
                        }
                    });
                }
            }
            true
        } else {
            false
        }
    }

    pub(crate) async fn handle_download_pending(&mut self) -> bool {
        if let Some((song, dir, fmt)) = self.ui.download.download_pending.take() {
            let song_title = song.title.clone();
            let song_title_clone = song_title.clone();
            let tx = self.event_tx.clone();
            let ytdlp = self.playback.downloader_clone();
            let sem = self.download_semaphore.clone();
            tokio::spawn(async move {
                let _permit = sem.acquire().await;
                if let Err(e) = tokio::fs::create_dir_all(&dir).await {
                    tracing::warn!("Failed to create download directory: {}", e);
                }
                match ytdlp.download(&song.webpage_url, &dir, &fmt).await {
                    Ok(path) => {
                        let _ = tx.send(AppEvent::DownloadComplete {
                            song_title: song_title_clone,
                            file_path: path,
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::DownloadError(e.to_string()));
                    }
                }
            });
            self.ui.push_notification(
                self.ui.tr("downloading").replace("{}", &song_title),
                NotificationLevel::Info,
            );
            true
        } else {
            false
        }
    }
}
