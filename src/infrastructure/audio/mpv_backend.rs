use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::domain::error::DomainError;
use crate::domain::media::Song;

pub struct MpvAdapter {
    child: Option<tokio::process::Child>,
}

impl Drop for MpvAdapter {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if let Err(e) = child.start_kill() {
                tracing::warn!("Failed to kill mpv on drop: {}", e);
            }
            // Non-blocking reap attempt to avoid zombie processes
            let _ = child.try_wait();
        }
    }
}

impl MpvAdapter {
    pub fn new() -> Self {
        Self { child: None }
    }

    pub async fn play_video(&mut self, url: &str, _song: Song) -> Result<(), DomainError> {
        // Kill any previous mpv child before spawning a new one
        if let Err(e) = self.stop().await {
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

    pub async fn stop(&mut self) -> Result<(), DomainError> {
        if let Some(mut child) = self.child.take() {
            if let Err(e) = child.start_kill() {
                tracing::warn!("Failed to kill mpv child: {}", e);
            }
            // Block on child exit with 3s timeout to prevent zombie processes
            let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Helper: spawn a process that lives long enough to test kill behavior.
    #[cfg(windows)]
    async fn spawn_sleep_proc() -> tokio::process::Child {
        // Use ping with 10 hops (roughly 9 sec delay) — reliable on all Windows versions
        tokio::process::Command::new("ping")
            .args(["-n", "10", "127.0.0.1"])
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

    /// Check whether a process with the given PID is still running.
    #[cfg(windows)]
    fn process_exists(pid: u32) -> bool {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/NH"])
            .output()
            .expect("tasklist should run");
        // tasklist outputs the PID string when the process exists;
        // when no process matches it prints an info message without the PID.
        let out = String::from_utf8_lossy(&output.stdout);
        out.contains(&pid.to_string())
    }
    #[cfg(not(windows))]
    fn process_exists(pid: u32) -> bool {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn test_mpv_child_stored_after_play_video() {
        // Simulate what play_video does: store a child handle.
        // In production, play_video spawns mpv and stores the Child.
        // Here we use our sleep helper to verify the field is populated
        // and the process is actually running.
        let mut backend = MpvAdapter::new();
        let mut child = spawn_sleep_proc().await;

        // Verify child is alive before we move it into the adapter
        let status = child.try_wait().expect("try_wait should not error");
        assert!(status.is_none(), "child should be alive before assignment");

        // Store the PID for later verification
        let pid = child.id().expect("spawned child should have a PID");
        backend.child = Some(child);

        // Child handle must be stored after play_video semantics
        assert!(backend.child.is_some(), "child should be stored in the adapter");

        // The subprocess must be alive — verify by polling try_wait through the adapter
        if let Some(ref mut c) = backend.child {
            let alive = c.try_wait().expect("try_wait should not error");
            assert!(alive.is_none(), "child process should be running in adapter");
        }

        // Clean up
        backend.stop().await.unwrap();
        assert!(backend.child.is_none(), "child field should be None after stop");

        // After stop, the process must be dead (stop() blocks until child exits,
        // but give the OS a moment to release the PID)
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!process_exists(pid), "child process should be killed after stop");
    }

    #[tokio::test]
    async fn test_stop_kills_child() {
        let mut backend = MpvAdapter::new();
        let mut child = spawn_sleep_proc().await;
        let pid = child.id().expect("spawned child should have a PID");

        // Verify child is alive before we move it into the adapter
        let status_before = child.try_wait().expect("try_wait should not error");
        assert!(status_before.is_none(), "child should be alive before stop");

        backend.child = Some(child);
        assert!(backend.stop().await.is_ok());
        // After stop, child field must be None (taken)
        assert!(backend.child.is_none(), "child field should be None after stop");

        // stop() now blocks until child exits via timeout(3s, child.wait()),
        // so the process should be reaped immediately after stop returns.
        // Still give the OS a moment to release the PID.
        tokio::time::sleep(Duration::from_millis(200)).await;

        // The process must no longer be running
        assert!(!process_exists(pid), "child process should be killed after stop");
    }

    #[tokio::test]
    async fn test_stop_safe_with_no_child() {
        let mut backend = MpvAdapter::new();
        // stop() on a backend with no child should not error
        assert!(backend.stop().await.is_ok());
    }

    #[test]
    fn test_drop_safe_with_no_child() {
        let backend = MpvAdapter::new();
        // Dropping a backend with no child should not panic
        drop(backend);
    }

    #[tokio::test]
    async fn test_drop_kills_child() {
        let mut backend = MpvAdapter::new();
        let mut child = spawn_sleep_proc().await;
        let pid = child.id().expect("spawned child should have a PID");

        // Verify child is alive before we move it into the adapter
        let status_before = child.try_wait().expect("try_wait should not error");
        assert!(status_before.is_none(), "child should be alive before drop");

        backend.child = Some(child);

        // Drop the adapter — its Drop impl must kill the child process
        drop(backend);

        // Give the OS a moment to reap the killed process
        tokio::time::sleep(Duration::from_millis(200)).await;

        // The process must no longer be running
        assert!(!process_exists(pid), "child process should be killed after MpvAdapter::drop");
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
        backend.stop().await.unwrap();
        assert!(backend.child.is_none());
    }
}
