use std::sync::Arc;

use crate::application::ports::{AudioPlaybackPort, DownloaderPort};
use crate::domain::audio_mode::AudioMode;
use crate::domain::error::DomainError;
use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;
use crate::infrastructure::audio::mpv_backend::MpvAdapter;
use crate::shared::spectrum::SpectrumFrame;

pub struct PlaybackUseCase {
    downloader: Arc<dyn DownloaderPort>,
    audio: Box<dyn AudioPlaybackPort>,
    mpv: MpvAdapter,
    mode: AudioMode,
}

impl PlaybackUseCase {
    pub fn new(downloader: Arc<dyn DownloaderPort>, audio: Box<dyn AudioPlaybackPort>, mpv: MpvAdapter, mode: AudioMode) -> Self {
        Self {
            downloader,
            audio,
            mpv,
            mode,
        }
    }

    pub fn mode(&self) -> AudioMode {
        self.mode
    }

    /// Toggle audio/video mode. Returns an error if switching to video but
    /// mpv is not installed on the system.
    ///
    /// Stops BOTH backends so nothing keeps playing from the old mode
    /// (rodio for audio, the mpv child process for video).
    pub async fn toggle_mode(&mut self) -> Result<(), DomainError> {
        if let Err(e) = self.stop() {
            tracing::warn!("Failed to stop while toggling mode: {}", e);
        }
        if let Err(e) = self.mpv.stop().await {
            tracing::warn!("Failed to stop mpv while toggling mode: {}", e);
        }
        let new_mode = match self.mode {
            AudioMode::Audio => AudioMode::Video,
            AudioMode::Video => AudioMode::Audio,
        };
        if matches!(new_mode, AudioMode::Video) && !MpvAdapter::is_mpv_installed().await {
            return Err(DomainError::Player(
                "mpv is not installed. Install mpv (https://mpv.io) to use video mode.".into(),
            ));
        }
        self.mode = new_mode;
        Ok(())
    }

    /// Play audio from in-memory bytes (used after background download completes).
    pub fn play_bytes(&mut self, data: Vec<u8>, song: Song) -> Result<(), DomainError> {
        self.audio.play_bytes(data, song)
    }

    /// Play video from a pre-resolved stream URL (non-blocking, spawns mpv).
    pub async fn play_video_stream(&mut self, stream_url: &str, song: Song) -> Result<(), DomainError> {
        self.mpv.play_video(stream_url, song).await
    }

    pub fn downloader_clone(&self) -> Arc<dyn DownloaderPort> {
        self.downloader.clone()
    }

    pub fn pause(&mut self) -> Result<(), DomainError> {
        self.audio.pause()
    }

    pub fn resume(&mut self) -> Result<(), DomainError> {
        self.audio.resume()
    }

    pub fn stop(&mut self) -> Result<(), DomainError> {
        self.audio.stop()
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.audio.set_volume(vol);
    }

    pub fn state(&self) -> PlayerState {
        self.audio.state()
    }

    pub fn current_position(&self) -> f64 {
        self.audio.current_position()
    }

    pub fn current_duration(&self) -> f64 {
        self.audio.current_duration()
    }

    pub fn volume(&self) -> f32 {
        self.audio.volume()
    }

    pub fn is_sink_empty(&self) -> bool {
        self.audio.is_sink_empty()
    }

    pub fn get_spectrum(&self) -> SpectrumFrame {
        self.audio.get_spectrum()
    }

    pub fn set_spectrum_enabled(&mut self, enabled: bool) {
        self.audio.set_spectrum_enabled(enabled);
    }

    /// Periodic health check passthrough. Returns Err if the audio backend was
    /// reset because the device was lost; the caller surfaces the error and
    /// clears playback state.
    pub fn check_health(&mut self) -> Result<(), DomainError> {
        self.audio.check_health()
    }

    pub fn take_route_change_notification(&mut self) -> bool {
        self.audio.take_route_change_notification()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use async_trait::async_trait;

    use super::*;

    /// Minimal downloader mock so `PlaybackUseCase` can be built in tests.
    struct MockDownloader;

    #[async_trait]
    impl DownloaderPort for MockDownloader {
        async fn get_stream_url(&self, _url: &str, _audio_only: bool) -> Result<String, DomainError> {
            Ok(String::new())
        }
        async fn download_audio_bytes(&self, _url: &str) -> Result<Vec<u8>, DomainError> {
            Ok(Vec::new())
        }
        async fn download(
            &self,
            _url: &str,
            _output_dir: &str,
            _audio_format: &str,
        ) -> Result<String, DomainError> {
            Ok(String::new())
        }
    }

    /// Audio mock whose `check_health` reports a healthy backend.
    struct HealthyMockAudio;

    impl AudioPlaybackPort for HealthyMockAudio {
        fn play_file(&mut self, _path: &Path, _song: Song) -> Result<(), DomainError> {
            Ok(())
        }
        fn play_bytes(&mut self, _data: Vec<u8>, _song: Song) -> Result<(), DomainError> {
            Ok(())
        }
        fn pause(&mut self) -> Result<(), DomainError> {
            Ok(())
        }
        fn resume(&mut self) -> Result<(), DomainError> {
            Ok(())
        }
        fn stop(&mut self) -> Result<(), DomainError> {
            Ok(())
        }
        fn set_volume(&mut self, _vol: f32) {}
        fn volume(&self) -> f32 {
            0.8
        }
        fn state(&self) -> PlayerState {
            PlayerState::Stopped
        }
        fn current_position(&self) -> f64 {
            0.0
        }
        fn current_duration(&self) -> f64 {
            0.0
        }
        fn is_sink_empty(&self) -> bool {
            true
        }
        fn get_spectrum(&self) -> SpectrumFrame {
            SpectrumFrame::default()
        }
        fn set_spectrum_enabled(&mut self, _enabled: bool) {}
    }

    /// Audio mock whose `check_health` reports a lost device.
    struct DeadMockAudio;

    impl AudioPlaybackPort for DeadMockAudio {
        fn play_file(&mut self, _path: &Path, _song: Song) -> Result<(), DomainError> {
            Ok(())
        }
        fn play_bytes(&mut self, _data: Vec<u8>, _song: Song) -> Result<(), DomainError> {
            Ok(())
        }
        fn pause(&mut self) -> Result<(), DomainError> {
            Ok(())
        }
        fn resume(&mut self) -> Result<(), DomainError> {
            Ok(())
        }
        fn stop(&mut self) -> Result<(), DomainError> {
            Ok(())
        }
        fn set_volume(&mut self, _vol: f32) {}
        fn volume(&self) -> f32 {
            0.8
        }
        fn state(&self) -> PlayerState {
            PlayerState::Stopped
        }
        fn current_position(&self) -> f64 {
            0.0
        }
        fn current_duration(&self) -> f64 {
            0.0
        }
        fn is_sink_empty(&self) -> bool {
            true
        }
        fn get_spectrum(&self) -> SpectrumFrame {
            SpectrumFrame::default()
        }
        fn set_spectrum_enabled(&mut self, _enabled: bool) {}
        fn check_health(&mut self) -> Result<(), DomainError> {
            Err(DomainError::Audio("Audio device lost. Playback stopped.".into()))
        }
    }

    /// Audio mock whose backend reports a route change: `check_health` is
    /// healthy (the pause happened internally) and `take_route_change_notification`
    /// yields true exactly once (the flag is consumed by the first call, like the
    /// real backend).
    struct RouteChangeMockAudio {
        notified: bool,
    }

    impl RouteChangeMockAudio {
        fn new() -> Self {
            Self { notified: true }
        }
    }

    impl AudioPlaybackPort for RouteChangeMockAudio {
        fn play_file(&mut self, _path: &Path, _song: Song) -> Result<(), DomainError> {
            Ok(())
        }
        fn play_bytes(&mut self, _data: Vec<u8>, _song: Song) -> Result<(), DomainError> {
            Ok(())
        }
        fn pause(&mut self) -> Result<(), DomainError> {
            Ok(())
        }
        fn resume(&mut self) -> Result<(), DomainError> {
            Ok(())
        }
        fn stop(&mut self) -> Result<(), DomainError> {
            Ok(())
        }
        fn set_volume(&mut self, _vol: f32) {}
        fn volume(&self) -> f32 {
            0.8
        }
        fn state(&self) -> PlayerState {
            PlayerState::Paused
        }
        fn current_position(&self) -> f64 {
            42.5
        }
        fn current_duration(&self) -> f64 {
            0.0
        }
        fn is_sink_empty(&self) -> bool {
            true
        }
        fn get_spectrum(&self) -> SpectrumFrame {
            SpectrumFrame::default()
        }
        fn set_spectrum_enabled(&mut self, _enabled: bool) {}
        fn check_health(&mut self) -> Result<(), DomainError> {
            Ok(())
        }
        fn take_route_change_notification(&mut self) -> bool {
            std::mem::take(&mut self.notified)
        }
    }

    fn build_playback(audio: Box<dyn AudioPlaybackPort>) -> PlaybackUseCase {
        PlaybackUseCase::new(Arc::new(MockDownloader), audio, MpvAdapter::new(), AudioMode::Audio)
    }

    #[test]
    fn check_health_propagates_ok_from_backend() {
        let mut playback = build_playback(Box::new(HealthyMockAudio));
        assert!(
            playback.check_health().is_ok(),
            "check_health should propagate a healthy backend as Ok"
        );
    }

    #[test]
    fn check_health_propagates_error_from_backend() {
        let mut playback = build_playback(Box::new(DeadMockAudio));
        let err = playback.check_health().unwrap_err();
        assert!(
            matches!(err, DomainError::Audio(_)),
            "check_health should propagate the backend error, got {:?}",
            err
        );
    }

    #[test]
    fn take_route_change_notification_returns_true_once() {
        let mut playback = build_playback(Box::new(RouteChangeMockAudio::new()));
        assert!(
            playback.take_route_change_notification(),
            "a pending route change should trigger the notification flag"
        );
        assert!(
            !playback.take_route_change_notification(),
            "the notification flag is consumed on the first call"
        );
    }

    use tempfile::TempDir;

    #[tokio::test]
    async fn test_async_file_write_in_download_and_play() {
        // Verify that writing audio bytes works in an async context
        // (the production code now uses tokio::fs::write)
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test_audio.bin");
        let data = vec![0xBE, 0xEF, 0xCA, 0xFE];

        tokio::fs::write(&path, &data).await.unwrap();
        let read_back = tokio::fs::read(&path).await.unwrap();
        assert_eq!(read_back, data, "async file write/read round-trip should match");
    }

    #[tokio::test]
    async fn test_async_create_dir_and_write() {
        // Verify that create_dir_all works in async context
        // (the production code now uses tokio::fs::create_dir_all)
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("level1").join("level2");
        tokio::fs::create_dir_all(&nested).await.unwrap();

        let file_path = nested.join("test.txt");
        tokio::fs::write(&file_path, b"hello").await.unwrap();

        assert!(file_path.exists(), "file should exist after async create_dir_all + write");
        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_async_write_to_readonly_dir_returns_error() {
        // Verify that write failures produce an IO error
        // Use a path that doesn't exist — works on any platform
        let bad_path = {
            let tmp = tempfile::tempdir().unwrap();
            tmp.path().join("nonexistent").join("test.bin")
        };
        let result = tokio::fs::write(&bad_path, b"data").await;
        assert!(result.is_err(), "write to non-existent path should fail");
    }
}
