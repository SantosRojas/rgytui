use super::*;

impl App {
    pub(crate) fn spawn_search(&self, query: String, limit: usize) {
        let tx = self.event_tx.clone();
        let search_uc = self.search.clone();
        let token = self.cancel_token.clone();
        tokio::spawn(async move {
            tokio::select! {
                result = search_uc.execute(&query, limit) => {
                    match result {
                        Ok(songs) => {
                            let _ = tx.send(AppEvent::SearchResults(songs));
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::SearchError(e.to_string()));
                        }
                    }
                }
                _ = token.cancelled() => {
                    // Task cancelled — drop silently
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
                            self.ui.player.loading_status = None;
                        }
                        Err(e) => {
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
                    // Audio mode: check cache first, then download in background
                    let tx = self.event_tx.clone();
                    let url = song.webpage_url.clone();
                    let song_for_event = song.clone();
                    let token = self.cancel_token.clone();
                    let cache = self.audio_cache.clone();
                    let song_id = song.id.clone();
                    tokio::spawn(async move {
                        // Fast path: try cache first (local read — no cancellation needed)
                        match cache.get(&song_id).await {
                            Ok(Some(data)) => {
                                let _ = tx.send(AppEvent::AudioReady { song: song_for_event, data });
                                return;
                            }
                            Ok(None) => {} // cache miss → download
                            Err(e) => {
                                tracing::warn!("Cache read failed, falling back to download: {e}");
                            }
                        }

                        // Slow path: download, save to cache, then play
                        tokio::select! {
                            result = PlaybackUseCase::download_audio_bytes(url) => {
                                match result {
                                    Ok(data) => {
                                        // Save to cache (best-effort — never fail playback on cache error)
                                        if let Err(e) = cache.put(&song_id, &data).await {
                                            tracing::warn!("Failed to cache audio: {e}");
                                        }
                                        let _ = tx.send(AppEvent::AudioReady { song: song_for_event, data });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(AppEvent::AudioDownloadError(e.to_string()));
                                    }
                                }
                            }
                            _ = token.cancelled() => {
                                // Task cancelled — drop silently
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
            let token = self.cancel_token.clone();
            tokio::spawn(async move {
                let _permit = sem.acquire().await;
                tokio::select! {
                    _ = token.cancelled() => {
                        // Task cancelled — drop silently
                    }
                    result = async {
                        if let Err(e) = tokio::fs::create_dir_all(&dir).await {
                            tracing::warn!("Failed to create download directory: {}", e);
                        }
                        ytdlp.download(&song.webpage_url, &dir, &fmt).await
                    } => {
                        match result {
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
