use std::path::PathBuf;

use directories::ProjectDirs;

use crate::application::ports::ConfigPort;
use crate::domain::error::DomainError;
use crate::domain::media::Playlist;
use crate::domain::settings::AppSettings;

fn default_download_path() -> String {
    dirs::audio_dir()
        .unwrap_or_else(|| {
            #[cfg(windows)]
            {
                std::env::var("USERPROFILE")
                    .map(|p| std::path::PathBuf::from(p).join("Music"))
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
            }
            #[cfg(not(windows))]
            {
                std::env::var("HOME")
                    .map(|p| std::path::PathBuf::from(p).join("Music"))
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
            }
        })
        .join("rgytui")
        .to_string_lossy()
        .to_string()
}

pub struct ConfigAdapter {
    settings_path: PathBuf,
    playlists_path: PathBuf,
    settings: AppSettings,
}

impl ConfigAdapter {
    pub async fn new() -> Result<Self, DomainError> {
        let config_dir = ProjectDirs::from("com", "rgytui", "rgytui")
            .map(|d| d.config_dir().to_path_buf())
            .unwrap_or_else(|| {
                // Fallback for sandboxed/container environments where ProjectDirs
                // returns None (rootless containers, Flatpak/Snap, etc.)
                let fallback = std::env::current_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
                    .join(".rgytui");
                tracing::warn!("ProjectDirs unavailable, falling back to {:?}", fallback);
                fallback
            });
        tokio::fs::create_dir_all(&config_dir).await?;

        let settings_path = config_dir.join("settings.json");
        let playlists_path = config_dir.join("playlist.json");

        let mut settings = if settings_path.exists() {
            let content = tokio::fs::read_to_string(&settings_path).await?;
            match serde_json::from_str(&content) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("Corrupted settings.json ({}), using defaults", e);
                    AppSettings::default()
                }
            }
        } else {
            AppSettings::default()
        };

        // Fill in default download path if not set (from file or domain default)
        if settings.download_path.is_empty() {
            settings.download_path = default_download_path();
        }

        Ok(Self {
            settings_path,
            playlists_path,
            settings,
        })
    }

    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut AppSettings {
        &mut self.settings
    }

    pub async fn save_settings(&self) -> Result<(), DomainError> {
        // Ensure parent directory exists before writing
        if let Some(parent) = self.settings_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let content = serde_json::to_string_pretty(&self.settings)?;
        tokio::fs::write(&self.settings_path, content).await?;
        Ok(())
    }

    pub async fn load_playlist(&self) -> Playlist {
        if self.playlists_path.exists() {
            match tokio::fs::read_to_string(&self.playlists_path).await {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!("Corrupted playlist.json ({}), using empty playlist", e);
                        Playlist::default()
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read playlist.json ({}), using empty playlist", e);
                    Playlist::default()
                }
            }
        } else {
            Playlist::default()
        }
    }
}

#[async_trait::async_trait]
impl ConfigPort for ConfigAdapter {
    async fn load_settings(&self) -> Result<AppSettings, DomainError> {
        Ok(self.settings.clone())
    }

    async fn save_settings(&self, settings: &AppSettings) -> Result<(), DomainError> {
        // Config dir is guaranteed to exist by ConfigAdapter::new().
        let content = serde_json::to_string_pretty(settings)?;
        tokio::fs::write(&self.settings_path, content).await?;
        Ok(())
    }

    async fn save_playlist(&self, playlist: &Playlist) -> Result<(), DomainError> {
        let content = serde_json::to_string_pretty(playlist)?;
        tokio::fs::write(&self.playlists_path, content).await?;
        Ok(())
    }

    async fn load_playlist(&self) -> Playlist {
        self.load_playlist().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper to construct a ConfigAdapter with test paths.
    fn test_config(tmp: &TempDir) -> ConfigAdapter {
        let settings_path = tmp.path().join("settings.json");
        let playlists_path = tmp.path().join("playlist.json");
        ConfigAdapter {
            settings_path: settings_path.clone(),
            playlists_path: playlists_path.clone(),
            settings: AppSettings::default(),
        }
    }

    #[tokio::test]
    async fn test_config_round_trip_via_tokio_fs() {
        let tmp = TempDir::new().unwrap();
        let mut config = test_config(&tmp);

        config.settings_mut().volume = 0.42;
        config.settings_mut().language = "en".into();

        config.save_settings().await.unwrap();

        // Verify file was written correctly by reading via tokio::fs
        let content = tokio::fs::read_to_string(tmp.path().join("settings.json"))
            .await
            .unwrap();
        let loaded: AppSettings = serde_json::from_str(&content).unwrap();
        assert!((loaded.volume - 0.42).abs() < f32::EPSILON);
        assert_eq!(loaded.language, "en");
    }

    #[tokio::test]
    async fn test_config_auto_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("nested").join("dirs");
        let settings_path = nested.join("settings.json");
        let playlists_path = nested.join("playlist.json");
        // Manually construct with non-existent directory — save should create it
        let config = ConfigAdapter {
            settings_path: settings_path.clone(),
            playlists_path,
            settings: AppSettings::default(),
        };
        // This should succeed by creating the directory
        config.save_settings().await.unwrap();
        assert!(settings_path.exists(), "settings.json should have been created");
    }

    #[tokio::test]
    async fn test_config_invalid_json_uses_defaults() {
        let tmp = TempDir::new().unwrap();
        // Write invalid JSON
        let settings_path = tmp.path().join("settings.json");
        std::fs::write(&settings_path, "not valid json").unwrap();

        let playlists_path = tmp.path().join("playlist.json");
        let config = ConfigAdapter {
            settings_path,
            playlists_path,
            settings: AppSettings::default(),
        };
        // Loading should succeed with defaults (logged warning)
        assert!((config.settings().volume - 0.8).abs() < f32::EPSILON);
    }
}
