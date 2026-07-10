use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::domain::error::DomainError;
use crate::domain::media::Playlist;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub volume: f32,
    pub audio_mode: bool,
    pub default_search_limit: usize,
    pub theme: String,
    pub accent_color: String,
    pub download_path: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            volume: 0.8,
            audio_mode: true,
            default_search_limit: 10,
            theme: "dark".into(),
            accent_color: "#00ffff".into(),
            download_path: default_download_path(),
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

pub struct ConfigStore {
    settings_path: PathBuf,
    playlists_path: PathBuf,
    settings: AppSettings,
}

impl ConfigStore {
    pub fn new() -> Result<Self, DomainError> {
        let proj_dirs = ProjectDirs::from("com", "rgytui", "rgytui")
            .ok_or_else(|| DomainError::Other("Cannot determine config directory".into()))?;

        let config_dir = proj_dirs.config_dir().to_path_buf();
        std::fs::create_dir_all(&config_dir)?;

        let settings_path = config_dir.join("settings.json");
        let playlists_path = config_dir.join("playlist.json");

        let settings = if settings_path.exists() {
            let content = std::fs::read_to_string(&settings_path)?;
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

    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut AppSettings {
        &mut self.settings
    }

    pub fn save_settings(&self) -> Result<(), DomainError> {
        let content = serde_json::to_string_pretty(&self.settings)?;
        std::fs::write(&self.settings_path, content)?;
        Ok(())
    }

    pub fn save_playlist(&self, playlist: &Playlist) -> Result<(), DomainError> {
        let content = serde_json::to_string_pretty(playlist)?;
        std::fs::write(&self.playlists_path, content)?;
        Ok(())
    }

    pub fn load_playlist(&self) -> Playlist {
        if self.playlists_path.exists() {
            match std::fs::read_to_string(&self.playlists_path) {
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
