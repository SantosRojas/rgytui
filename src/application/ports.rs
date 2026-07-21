use std::path::Path;

use async_trait::async_trait;

use crate::domain::error::DomainError;
use crate::domain::media::{Playlist, Song};
use crate::domain::player_state::PlayerState;
use crate::shared::spectrum::SpectrumFrame;

/// Port for media search (existing, unmodified).
#[async_trait]
pub trait MediaSearchPort: Send + Sync {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Song>, DomainError>;
    #[allow(dead_code)]
    async fn get_stream_url(&self, url: &str, audio_only: bool) -> Result<String, DomainError>;
}

/// Port for audio/video playback abstraction.
pub trait AudioPlaybackPort: Send {
    fn play_file(&mut self, path: &Path, song: Song) -> Result<(), DomainError>;
    fn play_bytes(&mut self, data: Vec<u8>, song: Song) -> Result<(), DomainError>;
    fn pause(&mut self) -> Result<(), DomainError>;
    fn resume(&mut self) -> Result<(), DomainError>;
    fn stop(&mut self) -> Result<(), DomainError>;
    fn set_volume(&mut self, vol: f32);
    fn volume(&self) -> f32;
    fn state(&self) -> PlayerState;
    fn current_position(&self) -> f64;
    fn current_duration(&self) -> f64;
    fn is_sink_empty(&self) -> bool;
    #[allow(dead_code)]
    fn has_sink(&self) -> bool;
    fn get_spectrum(&self) -> SpectrumFrame;
}

/// Port for downloading audio from URLs.
#[async_trait]
pub trait DownloaderPort: Send + Sync {
    async fn get_stream_url(&self, url: &str, audio_only: bool) -> Result<String, DomainError>;
    #[allow(dead_code)]
    async fn download_audio_bytes(&self, url: &str) -> Result<Vec<u8>, DomainError>;
    async fn download(
        &self,
        url: &str,
        output_dir: &str,
        audio_format: &str,
    ) -> Result<String, DomainError>;
}

/// Port for loading/saving configuration and playlists.
#[async_trait]
pub trait ConfigPort: Send {
    async fn load_settings(&self) -> Result<AppSettings, DomainError>;
    async fn save_settings(&self, settings: &AppSettings) -> Result<(), DomainError>;
    #[allow(dead_code)]
    async fn load_playlist(&self) -> Result<Playlist, DomainError>;
    async fn save_playlist(&self, playlist: &Playlist) -> Result<(), DomainError>;
}

/// Port for i18n / translations.
#[allow(dead_code)]
pub trait I18nPort: Send + std::fmt::Debug {
    fn t(&self, key: &str) -> String;
    fn language(&self) -> &str;
}

// Re-use AppSettings from config/store for the ConfigPort trait.
use crate::infrastructure::config::store::AppSettings;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::spectrum::BANDS;

    // ── Mock: AudioPlaybackPort ──

    struct MockAudioPlayback {
        vol: f32,
        state: PlayerState,
        spectrum: SpectrumFrame,
    }

    impl MockAudioPlayback {
        fn new() -> Self {
            Self {
                vol: 0.8,
                state: PlayerState::Idle,
                spectrum: SpectrumFrame::default(),
            }
        }
    }

    impl AudioPlaybackPort for MockAudioPlayback {
        fn play_file(&mut self, _path: &Path, _song: Song) -> Result<(), DomainError> {
            self.state = PlayerState::Playing;
            Ok(())
        }
        fn play_bytes(&mut self, _data: Vec<u8>, _song: Song) -> Result<(), DomainError> {
            self.state = PlayerState::Playing;
            Ok(())
        }
        fn pause(&mut self) -> Result<(), DomainError> {
            self.state = PlayerState::Paused;
            Ok(())
        }
        fn resume(&mut self) -> Result<(), DomainError> {
            self.state = PlayerState::Playing;
            Ok(())
        }
        fn stop(&mut self) -> Result<(), DomainError> {
            self.state = PlayerState::Stopped;
            Ok(())
        }
        fn set_volume(&mut self, vol: f32) {
            self.vol = vol;
        }
        fn volume(&self) -> f32 {
            self.vol
        }
        fn state(&self) -> PlayerState {
            self.state
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
        fn has_sink(&self) -> bool {
            false
        }
        fn get_spectrum(&self) -> SpectrumFrame {
            self.spectrum
        }
    }

    #[test]
    fn audio_playback_port_play_file_transitions_to_playing() {
        let mut mock = MockAudioPlayback::new();
        let song = Song {
            id: "test".into(),
            title: "Test".into(),
            channel: "".into(),
            duration: 0.0,
            thumbnail: None,
            webpage_url: "".into(),
        };
        assert!(mock.play_file(Path::new("/dev/null"), song).is_ok());
        assert_eq!(mock.state(), PlayerState::Playing);
    }

    #[test]
    fn audio_playback_port_pause_resume_cycle() {
        let mut mock = MockAudioPlayback::new();
        // Start playing
        assert!(mock.play_bytes(vec![0u8; 100], Song {
            id: "t".into(),
            title: "T".into(),
            channel: "".into(),
            duration: 0.0,
            thumbnail: None,
            webpage_url: "".into(),
        }).is_ok());
        assert_eq!(mock.state(), PlayerState::Playing);
        // Pause
        assert!(mock.pause().is_ok());
        assert_eq!(mock.state(), PlayerState::Paused);
        // Resume
        assert!(mock.resume().is_ok());
        assert_eq!(mock.state(), PlayerState::Playing);
    }

    #[test]
    fn audio_playback_port_stop_transitions_to_stopped() {
        let mut mock = MockAudioPlayback::new();
        assert!(mock.play_bytes(vec![], Song {
            id: "x".into(),
            title: "X".into(),
            channel: "".into(),
            duration: 0.0,
            thumbnail: None,
            webpage_url: "".into(),
        }).is_ok());
        assert!(mock.stop().is_ok());
        assert_eq!(mock.state(), PlayerState::Stopped);
    }

    #[test]
    fn audio_playback_port_volume_get_set() {
        let mut mock = MockAudioPlayback::new();
        mock.set_volume(0.5);
        assert!((mock.volume() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn audio_playback_port_get_spectrum_returns_frame() {
        let mock = MockAudioPlayback::new();
        let spec = mock.get_spectrum();
        assert_eq!(spec.bands.len(), BANDS);
        assert_eq!(spec.peaks.len(), BANDS);
    }

    // ── Mock: DownloaderPort ──

    struct MockDownloader;

    #[async_trait]
    impl DownloaderPort for MockDownloader {
        async fn get_stream_url(&self, _url: &str, _audio_only: bool) -> Result<String, DomainError> {
            Ok("https://example.com/stream".into())
        }
        async fn download_audio_bytes(&self, _url: &str) -> Result<Vec<u8>, DomainError> {
            Ok(vec![0u8; 100])
        }
        async fn download(
            &self,
            _url: &str,
            _output_dir: &str,
            _audio_format: &str,
        ) -> Result<String, DomainError> {
            Ok("/tmp/test.mp3".into())
        }
    }

    #[tokio::test]
    async fn downloader_port_get_stream_url_returns_url() {
        let mock = MockDownloader;
        let url = mock.get_stream_url("https://youtube.com/watch?v=test", true).await.unwrap();
        assert!(url.starts_with("http"));
    }

    #[tokio::test]
    async fn downloader_port_download_audio_bytes_returns_data() {
        let mock = MockDownloader;
        let data = mock.download_audio_bytes("https://youtube.com/watch?v=test").await.unwrap();
        assert!(!data.is_empty(), "should return non-empty bytes");
    }

    #[tokio::test]
    async fn downloader_port_download_returns_path() {
        let mock = MockDownloader;
        let path = mock.download("https://youtube.com/watch?v=test", "/tmp", "mp3").await.unwrap();
        assert!(!path.is_empty());
    }

    // ── Mock: ConfigPort ──

    struct MockConfig {
        settings: AppSettings,
        playlist: Playlist,
    }

    #[async_trait]
    impl ConfigPort for MockConfig {
        async fn load_settings(&self) -> Result<AppSettings, DomainError> {
            Ok(self.settings.clone())
        }
        async fn save_settings(&self, _settings: &AppSettings) -> Result<(), DomainError> {
            Ok(())
        }
        async fn load_playlist(&self) -> Result<Playlist, DomainError> {
            Ok(self.playlist.clone())
        }
        async fn save_playlist(&self, _playlist: &Playlist) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn config_port_load_settings_returns_settings() {
        let config = MockConfig {
            settings: AppSettings::default(),
            playlist: Playlist::default(),
        };
        let settings = config.load_settings().await.unwrap();
        assert!((settings.volume - 0.8).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn config_port_save_settings_returns_ok() {
        let config = MockConfig {
            settings: AppSettings::default(),
            playlist: Playlist::default(),
        };
        assert!(config.save_settings(&AppSettings::default()).await.is_ok());
    }

    #[tokio::test]
    async fn config_port_load_playlist_returns_playlist() {
        let config = MockConfig {
            settings: AppSettings::default(),
            playlist: Playlist {
                name: "Test".into(),
                ..Playlist::default()
            },
        };
        let pl = config.load_playlist().await.unwrap();
        assert_eq!(pl.name, "Test");
    }

    #[tokio::test]
    async fn config_port_save_playlist_returns_ok() {
        let config = MockConfig {
            settings: AppSettings::default(),
            playlist: Playlist::default(),
        };
        assert!(config.save_playlist(&Playlist::default()).await.is_ok());
    }

    // ── Mock: I18nPort ──

    #[derive(Debug)]
    struct MockI18n;

    impl I18nPort for MockI18n {
        fn t(&self, key: &str) -> String {
            format!("[{}]", key)
        }
        fn language(&self) -> &str {
            "en"
        }
    }

    #[test]
    fn i18n_port_t_returns_translated_key() {
        let i18n = MockI18n;
        assert_eq!(i18n.t("hello"), "[hello]");
    }

    #[test]
    fn i18n_port_language_returns_code() {
        let i18n = MockI18n;
        assert_eq!(i18n.language(), "en");
    }
}
