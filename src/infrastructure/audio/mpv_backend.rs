use std::process::Stdio;

use tokio::process::Command;

use crate::domain::error::DomainError;
use crate::domain::media::Song;

pub struct MpvBackend;

impl MpvBackend {
    pub fn new() -> Self {
        Self
    }

    pub async fn play_video(&self, url: &str, _song: Song) -> Result<(), DomainError> {
        Command::new("mpv")
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| {
                DomainError::Player(format!(
                    "Failed to start mpv. Is it installed? Error: {}",
                    e
                ))
            })?;

        Ok(())
    }

    pub fn is_mpv_installed() -> bool {
        std::process::Command::new("mpv")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }
}
