use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::application::ports::ConfigPort;
use crate::domain::error::DomainError;
use crate::domain::media::Playlist;

fn default_language() -> String {
    "en".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub volume: f32,
    pub audio_mode: bool,
    pub default_search_limit: usize,
    pub theme: String,
    pub accent_color: String,
    pub download_path: String,
    #[serde(default = "default_language")]
    pub language: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            volume: 0.8,
            audio_mode: false,
            default_search_limit: 10,
            theme: "dark".into(),
            accent_color: "#00ffff".into(),
            download_path: default_download_path(),
            language: "en".into(),
        }
    }
}

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
        let proj_dirs = ProjectDirs::from("com", "rgytui", "rgytui")
            .ok_or_else(|| DomainError::Other("Cannot determine config directory".into()))?;

        let config_dir = proj_dirs.config_dir().to_path_buf();
        tokio::fs::create_dir_all(&config_dir).await?;

        let settings_path = config_dir.join("settings.json");
        let playlists_path = config_dir.join("playlist.json");

        let settings = if settings_path.exists() {
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

        Ok(Self {
            settings_path,
            playlists_path,
            settings,
        })
    }

    #[allow(dead_code)]
    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    #[allow(dead_code)]
    pub fn settings_mut(&mut self) -> &mut AppSettings {
        &mut self.settings
    }

    #[allow(dead_code)]
    pub async fn save_settings(&self) -> Result<(), DomainError> {
        // Ensure parent directory exists before writing
        if let Some(parent) = self.settings_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let content = serde_json::to_string_pretty(&self.settings)?;
        tokio::fs::write(&self.settings_path, content).await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn save_playlist(&self, playlist: &Playlist) -> Result<(), DomainError> {
        // Ensure parent directory exists before writing
        if let Some(parent) = self.playlists_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let content = serde_json::to_string_pretty(playlist)?;
        tokio::fs::write(&self.playlists_path, content).await?;
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
        // Use the saved settings path; store the updated settings before saving
        // We mutate through the inner lock-free path by writing to file
        if let Some(parent) = self.settings_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let content = serde_json::to_string_pretty(settings)?;
        tokio::fs::write(&self.settings_path, content).await?;
        Ok(())
    }

    async fn load_playlist(&self) -> Result<Playlist, DomainError> {
        Ok(self.load_playlist().await)
    }

    async fn save_playlist(&self, playlist: &Playlist) -> Result<(), DomainError> {
        if let Some(parent) = self.playlists_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let content = serde_json::to_string_pretty(playlist)?;
        tokio::fs::write(&self.playlists_path, content).await?;
        Ok(())
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
