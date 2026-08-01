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
                            let _ = tx.send(AppEvent::SearchResults(songs)).await;
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::SearchError(e.to_string())).await;
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
            // Generation captured at spawn time: any event this task emits is
            // only honored if no newer play superseded it (queue_play bumps).
            let generation = self.play_generation;

            match self.playback.mode() {
                AudioMode::Video => {
                    // Video mode: resolve stream URL in background,
                    // then spawn mpv — keeps loading animation visible in the UI.
                    let tx = self.event_tx.clone();
                    let url = song.webpage_url.clone();
                    let song_for_event = song.clone();
                    let downloader = self.playback.downloader_clone();
                    tokio::spawn(async move {
                        let result = downloader.get_stream_url(&url, false).await;
                        match result {
                            Ok(stream_url) => {
                                let _ = tx.send(AppEvent::VideoStreamReady {
                                    song: song_for_event,
                                    stream_url,
                                    generation,
                                }).await;
                            }
                            Err(e) => {
                                let _ = tx.send(AppEvent::PlaybackError {
                                    message: e.to_string(),
                                    generation,
                                }).await;
                            }
                        }
                    });
                }
                AudioMode::Audio => {
                    // Audio mode: check cache first, then download in background
                    let tx = self.event_tx.clone();
                    let url = song.webpage_url.clone();
                    let song_for_event = song.clone();
                    let token = self.cancel_token.clone();
                    let cache = self.audio_cache.clone();
                    let song_id = song.id.clone();
                    let downloader = self.playback.downloader_clone();
                    tokio::spawn(async move {
                        // Fast path: try cache first (local read — no cancellation needed)
                        match cache.get(&song_id).await {
                            Ok(Some(data)) => {
                                let _ = tx.send(AppEvent::AudioReady { song: song_for_event, data, generation }).await;
                                return;
                            }
                            Ok(None) => {} // cache miss → download
                            Err(e) => {
                                tracing::warn!("Cache read failed, falling back to download: {e}");
                            }
                        }

                        // Slow path: download, save to cache, then play
                        tokio::select! {
                            result = downloader.download_audio_bytes(&url) => {
                                match result {
                                    Ok(data) => {
                                        // Save to cache (best-effort — never fail playback on cache error)
                                        if let Err(e) = cache.put(&song_id, &data).await {
                                            tracing::warn!("Failed to cache audio: {e}");
                                        }
                                        let _ = tx.send(AppEvent::AudioReady { song: song_for_event, data, generation }).await;
                                    }
                                    Err(e) => {
                                        let _ = tx.send(AppEvent::AudioDownloadError {
                                            message: e.to_string(),
                                            generation,
                                        }).await;
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
            let song_name = song.title.clone();
            self.ui.player.loading_status =
                Some(self.ui.tr("downloading").replace("{}", &song_name));
            let tx = self.event_tx.clone();
            let ytdlp = self.playback.downloader_clone();
            let sem = self.download_semaphore.clone();
            let token = self.cancel_token.clone();
            tokio::spawn(async move {
                let _permit = sem.acquire().await;
                tokio::select! {
                    _ = token.cancelled() => {
                    }
                    result = async {
                        if let Err(e) = tokio::fs::create_dir_all(&dir).await {
                            tracing::warn!("Failed to create download directory: {}", e);
                        }
                        ytdlp.download(&song.webpage_url, &dir, &fmt).await
                    } => {
                        match result {
                            Ok(_path) => {
                                let _ = tx.send(AppEvent::DownloadComplete {
                                    song_title: song_name,
                                }).await;
                            }
                            Err(e) => {
                                let _ = tx.send(AppEvent::DownloadError(e.to_string())).await;
                            }
                        }
                    }
                }
            });
            true
        } else {
            false
        }
    }
}
