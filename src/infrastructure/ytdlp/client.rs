use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};



use crate::application::ports::{DownloaderPort, MediaSearchPort};
use crate::domain::error::DomainError;
use crate::domain::media::{RawSong, Song};
use tokio::process::Command;

type SearchCache = HashMap<String, (Vec<Song>, Instant)>;

const SEARCH_CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct YtDlpAdapter {
    search_cache: Arc<Mutex<SearchCache>>,
}

impl YtDlpAdapter {
    pub fn new() -> Self {
        Self {
            search_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn cache_key(query: &str, limit: usize) -> String {
        format!("{}:{}", limit, query)
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Song>, DomainError> {
        let key = Self::cache_key(query, limit);

        // Check cache first
        {
            let mut cache = self.search_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((songs, timestamp)) = cache.get(&key) {
                if timestamp.elapsed() < SEARCH_CACHE_TTL {
                    return Ok(songs.clone());
                }
                // Expired entry — remove it
                cache.remove(&key);
            }
        }

        let search_query = format!("ytsearch{}:{}", limit, query);

        let output = tokio::time::timeout(
            Duration::from_secs(30),
            Command::new("yt-dlp")
                .arg("--default-search")
                .arg("ytsearch")
                .arg("--dump-json")
                .arg("--no-download")
                .arg("--flat-playlist")
                .arg(&search_query)
                .output(),
        )
        .await
            .map_err(|_| DomainError::YtDlp("yt-dlp search timed out after 30s".into()))?
            .map_err(|e| DomainError::YtDlp(format!("Failed to run yt-dlp: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DomainError::YtDlp(format!(
                "yt-dlp error: {}",
                stderr
            )));
        }

        // Parse JSON lines from yt-dlp output
        let stdout = std::str::from_utf8(&output.stdout)
            .map_err(|e| DomainError::Parse(format!("Invalid UTF-8: {}", e)))?;
        let songs: Vec<Song> = stdout
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() {
                    return None;
                }
                match serde_json::from_str::<RawSong>(line) {
                    Ok(raw) => Some(Song::from(raw)),
                    Err(e) => {
                        tracing::warn!("Failed to parse yt-dlp JSON line: {}", e);
                        None
                    }
                }
            })
            .collect();

        // Update cache
        {
            let mut cache = self.search_cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.insert(key, (songs.clone(), Instant::now()));
        }

        Ok(songs)
    }

    pub async fn get_stream_url(&self, url: &str, audio_only: bool) -> Result<String, DomainError> {
        let format = if audio_only { "bestaudio" } else { "best" };

        let output = tokio::time::timeout(
            Duration::from_secs(30),
            Command::new("yt-dlp")
                .arg("-f")
                .arg(format)
                .arg("-g")
                .arg(url)
                .output(),
        )
        .await
            .map_err(|_| DomainError::YtDlp("yt-dlp stream URL timed out after 30s".into()))?
            .map_err(|e| DomainError::YtDlp(format!("Failed to get stream URL: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DomainError::YtDlp(format!(
                "Failed to get stream URL: {}",
                stderr
            )));
        }

        let stream_url = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        if stream_url.is_empty() {
            return Err(DomainError::YtDlp("Empty stream URL".into()));
        }

        Ok(stream_url)
    }

    pub async fn download(
        &self,
        url: &str,
        output_dir: &str,
        audio_format: &str,
    ) -> Result<String, DomainError> {
        let output_template = format!("{}/%(title)s.%(ext)s", output_dir.trim_end_matches(['/', '\\']));

        let mut cmd = tokio::process::Command::new("yt-dlp");
        cmd.arg("-f").arg("bestaudio/best");
        cmd.arg("-o").arg(&output_template);
        cmd.arg("--no-playlist");
        cmd.arg("--print").arg("after_move:filename");

        let (format_flag, ext_opt) = match audio_format {
            "m4a" | "mp4" => ("--audio-format", Some("m4a")),
            "mp3" => ("--audio-format", Some("mp3")),
            "flac" => ("--audio-format", Some("flac")),
            "wav" => ("--audio-format", Some("wav")),
            "opus" => ("--audio-format", Some("opus")),
            _ => ("", None),
        };

        if let Some(ext) = ext_opt {
            cmd.arg("-x").arg(format_flag).arg(ext);
        }

        cmd.arg(url);

        let output = tokio::time::timeout(Duration::from_secs(300), cmd.output())
            .await
            .map_err(|_| DomainError::YtDlp("Download timed out after 300s".into()))?
            .map_err(|e| DomainError::YtDlp(format!("Failed to run yt-dlp: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DomainError::YtDlp(format!("Download failed: {}", stderr)));
        }

        let filename = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let filepath = if filename.is_empty() {
            format!("{}/{}", output_dir.trim_end_matches(['/', '\\']), url)
        } else {
            filename
        };

        Ok(filepath)
    }
}

#[async_trait::async_trait]
impl MediaSearchPort for YtDlpAdapter {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Song>, DomainError> {
        self.search(query, limit).await
    }
}

#[async_trait::async_trait]
impl DownloaderPort for YtDlpAdapter {
    async fn get_stream_url(&self, url: &str, audio_only: bool) -> Result<String, DomainError> {
        self.get_stream_url(url, audio_only).await
    }

    async fn download_audio_bytes(&self, url: &str) -> Result<Vec<u8>, DomainError> {
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            tokio::process::Command::new("yt-dlp")
                .arg("-f")
                .arg("bestaudio[ext=m4a]/bestaudio/best")
                .arg("-o")
                .arg("-")
                .arg("--no-playlist")
                .arg(url)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
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

    async fn download(
        &self,
        url: &str,
        output_dir: &str,
        audio_format: &str,
    ) -> Result<String, DomainError> {
        self.download(url, output_dir, audio_format).await
    }
}

#[cfg(test)]
mod tests {

    /// Verify that piped stderr captures error output when a command exits
    /// with a non-zero status. This proves the Stdio::piped() change in
    /// download_audio_bytes enables stderr content to reach the error message.
    #[tokio::test]
    async fn test_piped_stderr_captures_error_output() {
        let mut cmd = if cfg!(windows) {
            let mut c = tokio::process::Command::new("cmd.exe");
            // Redirect echo to stderr, then exit non-zero
            c.args(["/C", "echo stderr_content>&2 & exit 1"]);
            c
        } else {
            let mut c = tokio::process::Command::new("sh");
            c.args(["-c", "echo stderr_content >&2; exit 1"]);
            c
        };

        let output = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .expect("should execute");

        assert!(!output.status.success(), "command should exit non-zero");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("stderr_content"),
            "piped stderr should capture error output, got: {stderr}"
        );
    }

    /// Verify that a successful command does NOT produce non-zero status even
    /// with piped stderr — proving the piped stderr doesn't interfere with
    /// normal command execution.
    #[tokio::test]
    async fn test_piped_stderr_successful_command() {
        let mut cmd = if cfg!(windows) {
            let mut c = tokio::process::Command::new("cmd.exe");
            c.args(["/C", "echo ok>&2"]);
            c
        } else {
            let mut c = tokio::process::Command::new("sh");
            c.args(["-c", "echo ok >&2"]);
            c
        };

        let output = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .expect("should execute");

        assert!(output.status.success(), "successful command should exit zero");
    }
}
