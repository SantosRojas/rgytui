use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tempfile::NamedTempFile;

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
    temp_file: Option<NamedTempFile>,
}

impl PlaybackUseCase {
    pub fn new(downloader: Arc<dyn DownloaderPort>, audio: Box<dyn AudioPlaybackPort>, mpv: MpvAdapter, mode: AudioMode) -> Self {
        Self {
            downloader,
            audio,
            mpv,
            mode,
            temp_file: None,
        }
    }

    pub fn mode(&self) -> AudioMode {
        self.mode
    }

    /// Toggle audio/video mode. Returns an error if switching to video but
    /// mpv is not installed on the system.
    pub fn toggle_mode(&mut self) -> Result<(), DomainError> {
        if let Err(e) = self.stop() {
            tracing::warn!("Failed to stop while toggling mode: {}", e);
        }
        let new_mode = match self.mode {
            AudioMode::Audio => AudioMode::Video,
            AudioMode::Video => AudioMode::Audio,
        };
        if matches!(new_mode, AudioMode::Video) && !MpvAdapter::is_mpv_installed() {
            return Err(DomainError::Player(
                "mpv is not installed. Install mpv (https://mpv.io) to use video mode.".into(),
            ));
        }
        self.mode = new_mode;
        Ok(())
    }

    pub async fn play(&mut self, song: &Song) -> Result<(), DomainError> {
        match self.mode {
            AudioMode::Video => {
                if !MpvAdapter::is_mpv_installed() {
                    return Err(DomainError::Player(
                        "mpv no está instalado. Instalá mpv (https://mpv.io) para usar el modo video."
                            .into(),
                    ));
                }
                let stream_url = self.downloader.get_stream_url(&song.webpage_url, false).await?;
                self.mpv.play_video(&stream_url, song.clone()).await?;
                Ok(())
            }
            AudioMode::Audio => {
                self.download_and_play(song).await
            }
        }
    }

    async fn download_and_play(&mut self, song: &Song) -> Result<(), DomainError> {
        let tmp = NamedTempFile::new()?;
        let path = tmp.path().to_owned();
        // Assign temp_file immediately so Drop cleans up on early error return.
        self.temp_file = Some(tmp);

        let output = tokio::time::timeout(
            Duration::from_secs(120),
            tokio::process::Command::new("yt-dlp")
                .arg("-f")
                .arg("bestaudio[ext=m4a]/bestaudio/best")
                .arg("-o")
                .arg("-")
                .arg("--no-playlist")
                .arg(&song.webpage_url)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output(),
        )
        .await
            .map_err(|_| DomainError::YtDlp("Audio download timed out after 120s".into()))?
            .map_err(|e| DomainError::YtDlp(format!("Failed to run yt-dlp: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DomainError::YtDlp(format!("Audio download failed: {}", stderr)));
        }

        tokio::fs::write(&path, &output.stdout).await?;
        self.audio.play_file(&path, song.clone())?;

        Ok(())
    }

    /// Play audio from in-memory bytes (used after background download completes).
    pub fn play_bytes(&mut self, data: Vec<u8>, song: Song) -> Result<(), DomainError> {
        self.audio.play_bytes(data, song)
    }

    pub fn downloader_clone(&self) -> Arc<dyn DownloaderPort> {
        self.downloader.clone()
    }

    /// Download audio bytes in the background. Returns raw bytes suitable for play_bytes().
    pub async fn download_audio_bytes(url: String) -> Result<Vec<u8>, DomainError> {
        let output = tokio::time::timeout(
            Duration::from_secs(120),
            tokio::process::Command::new("yt-dlp")
                .arg("-f")
                .arg("bestaudio[ext=m4a]/bestaudio/best")
                .arg("-o")
                .arg("-")
                .arg("--no-playlist")
                .arg(&url)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output(),
        )
        .await
            .map_err(|_| DomainError::YtDlp("Audio download timed out after 120s".into()))?
            .map_err(|e| DomainError::YtDlp(format!("Failed to run yt-dlp: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DomainError::YtDlp(format!("Audio download failed: {}", stderr)));
        }

        Ok(output.stdout)
    }

    pub fn pause(&mut self) -> Result<(), DomainError> {
        self.audio.pause()
    }

    pub fn resume(&mut self) -> Result<(), DomainError> {
        self.audio.resume()
    }

    pub fn stop(&mut self) -> Result<(), DomainError> {
        self.temp_file = None;
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

    #[allow(dead_code)]
    pub fn has_sink(&self) -> bool {
        self.audio.has_sink()
    }

    pub fn get_spectrum(&self) -> SpectrumFrame {
        self.audio.get_spectrum()
    }

    #[allow(dead_code)]
    pub async fn download_song(&self, song: &Song, output_dir: &str, audio_format: &str) -> Result<String, DomainError> {
        tokio::fs::create_dir_all(output_dir).await?;
        self.downloader.download(&song.webpage_url, output_dir, audio_format).await
    }
}

#[cfg(test)]
mod tests {
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
