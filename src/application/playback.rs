use std::process::Stdio;
use std::time::Duration;

use tempfile::NamedTempFile;

use crate::domain::error::DomainError;
use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;
use crate::infrastructure::audio::mpv_backend::MpvBackend;
use crate::infrastructure::audio::rodio_backend::RodioBackend;
use crate::infrastructure::audio::spectrum::SpectrumFrame;
use crate::infrastructure::audio::AudioMode;
use crate::infrastructure::ytdlp::client::YtDlpClient;

pub struct PlaybackUseCase {
    ytdlp: YtDlpClient,
    audio: RodioBackend,
    mpv: MpvBackend,
    mode: AudioMode,
    temp_file: Option<NamedTempFile>,
}

impl PlaybackUseCase {
    pub fn new(ytdlp: YtDlpClient, audio: RodioBackend, mpv: MpvBackend) -> Self {
        Self {
            ytdlp,
            audio,
            mpv,
            mode: AudioMode::Audio,
            temp_file: None,
        }
    }

    pub fn mode(&self) -> AudioMode {
        self.mode
    }

    pub fn toggle_mode(&mut self) {
        self.stop().ok();
        self.mode = match self.mode {
            AudioMode::Audio => AudioMode::Video,
            AudioMode::Video => AudioMode::Audio,
        };
    }

    pub async fn play(&mut self, song: &Song) -> Result<(), DomainError> {
        match self.mode {
            AudioMode::Video => {
                let stream_url = self.ytdlp.get_stream_url(&song.webpage_url, false).await?;
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

        std::fs::write(&path, &output.stdout)?;
        self.audio.play_file(&path, song.clone())?;
        self.temp_file = Some(tmp);

        Ok(())
    }

    /// Play audio from in-memory bytes (used after background download completes).
    pub fn play_bytes(&mut self, data: Vec<u8>, song: Song) -> Result<(), DomainError> {
        self.audio.play_bytes(data, song)
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

    pub fn has_sink(&self) -> bool {
        self.audio.has_sink()
    }

    pub fn get_spectrum(&self) -> SpectrumFrame {
        self.audio.get_spectrum()
    }

    pub fn ytdlp_clone(&self) -> YtDlpClient {
        self.ytdlp.clone()
    }

    pub async fn download_song(&self, song: &Song, output_dir: &str, audio_format: &str) -> Result<String, DomainError> {
        std::fs::create_dir_all(output_dir)?;
        self.ytdlp.download(&song.webpage_url, output_dir, audio_format).await
    }
}
