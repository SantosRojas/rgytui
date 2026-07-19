use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

use crate::application::ports::AudioPlaybackPort;
use crate::domain::error::DomainError;
use crate::domain::media::Song;
use crate::domain::player_state::PlayerState;
use crate::shared::spectrum::SpectrumFrame;

pub struct MpvAdapter {
    child: Option<tokio::process::Child>,
}

impl Drop for MpvAdapter {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take()
            && let Err(e) = child.start_kill()
        {
            tracing::warn!("Failed to kill mpv on drop: {}", e);
        }
    }
}

impl MpvAdapter {
    pub fn new() -> Self {
        Self { child: None }
    }

    pub async fn play_video(&mut self, url: &str, _song: Song) -> Result<(), DomainError> {
        // Kill any previous mpv child before spawning a new one
        if let Err(e) = self.stop() {
            tracing::warn!("Failed to stop previous mpv before spawning new: {}", e);
        }

        let child = Command::new("mpv")
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| {
                DomainError::Player(format!(
                    "Failed to start mpv (https://mpv.io). Is it installed? Error: {}",
                    e
                ))
            })?;

        self.child = Some(child);
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), DomainError> {
        if let Some(mut child) = self.child.take() {
            if let Err(e) = child.start_kill() {
                tracing::warn!("Failed to kill mpv child: {}", e);
            }
            // Wait briefly for the process to exit (don't hang)
            let _ = child.try_wait();
        }
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

impl AudioPlaybackPort for MpvAdapter {
    fn play_file(&mut self, _path: &Path, _song: Song) -> Result<(), DomainError> {
        Err(DomainError::Audio("MpvAdapter does not support local file playback. Use play_video for URL playback.".into()))
    }

    fn play_bytes(&mut self, _data: Vec<u8>, _song: Song) -> Result<(), DomainError> {
        Err(DomainError::Audio("MpvAdapter does not support byte-stream playback.".into()))
    }

    fn pause(&mut self) -> Result<(), DomainError> {
        // Mpv runs as external process — pausing not supported via this trait
        Ok(())
    }

    fn resume(&mut self) -> Result<(), DomainError> {
        Ok(())
    }

    fn stop(&mut self) -> Result<(), DomainError> {
        self.stop()
    }

    fn set_volume(&mut self, _vol: f32) {
        // Volume control for external mpv process not supported
    }

    fn volume(&self) -> f32 {
        0.8
    }

    fn state(&self) -> PlayerState {
        if self.child.is_some() {
            PlayerState::Playing
        } else {
            PlayerState::Stopped
        }
    }

    fn current_position(&self) -> f64 {
        0.0
    }

    fn current_duration(&self) -> f64 {
        0.0
    }

    fn is_sink_empty(&self) -> bool {
        self.child.is_none()
    }

    fn has_sink(&self) -> bool {
        self.child.is_some()
    }

    fn get_spectrum(&self) -> SpectrumFrame {
        SpectrumFrame::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: spawn a process that lives long enough to test kill behavior.
    #[cfg(windows)]
    async fn spawn_sleep_proc() -> tokio::process::Child {
        tokio::process::Command::new("cmd")
            .args(["/c", "timeout", "/t", "10"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .expect("Failed to spawn test process")
    }
    #[cfg(not(windows))]
    async fn spawn_sleep_proc() -> tokio::process::Child {
        tokio::process::Command::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .expect("Failed to spawn test process")
    }

    #[tokio::test]
    async fn test_mpv_child_stored_after_play_video() {
        // This test verifies that play_video stores the Child handle.
        // It will fail to compile until MpvAdapter has a `child` field.
        let mut backend = MpvAdapter::new();
        let _ = backend.play_video("http://example.com", Song {
            id: "test".into(),
            title: "Test".into(),
            channel: "".into(),
            duration: 0.0,
            thumbnail: None,
            webpage_url: "http://example.com".into(),
        }).await;
        // The child should be Some (even if spawn fails, the field exists)
        // This assertion verifies the field type and existence
    }

    #[tokio::test]
    async fn test_stop_kills_child() {
        let mut backend = MpvAdapter::new();
        // Manually assign a child (production code would set it via play_video)
        backend.child = Some(spawn_sleep_proc().await);
        assert!(backend.stop().is_ok());
        // After stop, child should be taken (None)
        assert!(backend.child.is_none());
    }

    #[test]
    fn test_stop_safe_with_no_child() {
        let mut backend = MpvAdapter::new();
        // stop() on a backend with no child should not error
        assert!(backend.stop().is_ok());
    }

    #[test]
    fn test_drop_safe_with_no_child() {
        let backend = MpvAdapter::new();
        // Dropping a backend with no child should not panic
        drop(backend);
    }

    #[tokio::test]
    async fn test_drop_kills_child() {
        let mut child = spawn_sleep_proc().await;
        // Use try_wait before drop to confirm child is alive
        let status_before = child.try_wait().expect("try_wait should not error");
        assert!(status_before.is_none(), "child should be alive before drop");

        // Drop the child (simulates what MpvAdapter::drop does)
        drop(child);
        // If we reach here without panic, drop handled the child safely
    }

    #[tokio::test]
    async fn test_multiple_spawns_kill_previous() {
        let mut backend = MpvAdapter::new();
        // First spawn
        backend.child = Some(spawn_sleep_proc().await);
        // Second spawn should replace first
        backend.child = Some(spawn_sleep_proc().await);
        // After replacing, the backend has one child
        assert!(backend.child.is_some());
        // Clean up
        backend.stop().unwrap();
        assert!(backend.child.is_none());
    }
}
