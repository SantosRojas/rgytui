use std::time::Duration;

use crate::application::ports::MediaSearchPort;
use crate::domain::error::DomainError;
use crate::domain::media::Song;
use serde_json::Value;
use tokio::process::Command;

#[derive(Clone)]
pub struct YtDlpClient;

impl YtDlpClient {
    pub fn new() -> Self {
        Self
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Song>, DomainError> {
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

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut songs = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            match serde_json::from_str::<Value>(line) {
                Ok(json) => {
                    if let Some(song) = Self::parse_song(&json) {
                        songs.push(song);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse yt-dlp JSON line: {}", e);
                }
            }
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

    pub async fn get_metadata(&self, url: &str) -> Result<Song, DomainError> {
        let output = tokio::time::timeout(
            Duration::from_secs(30),
            Command::new("yt-dlp")
                .arg("--dump-json")
                .arg("--no-download")
                .arg(url)
                .output(),
        )
        .await
            .map_err(|_| DomainError::YtDlp("yt-dlp metadata timed out after 30s".into()))?
            .map_err(|e| DomainError::YtDlp(format!("Failed to get metadata: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DomainError::YtDlp(format!(
                "Failed to get metadata: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: Value = serde_json::from_str(stdout.trim())?;

        Self::parse_song(&json).ok_or_else(|| DomainError::Parse("Failed to parse song metadata".into()))
    }

    fn parse_song(json: &Value) -> Option<Song> {
        let id = json.get("id")?.as_str()?.to_string();
        let title = json.get("title")?.as_str()?.to_string();
        let channel = json
            .get("channel")
            .or_else(|| json.get("uploader"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let duration = json
            .get("duration")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let thumbnail = json.get("thumbnail").and_then(|v| v.as_str()).map(String::from);
        let webpage_url = json
            .get("webpage_url")
            .or_else(|| json.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| {
                if s.starts_with("http") {
                    s.to_string()
                } else {
                    format!("https://youtube.com/watch?v={}", s)
                }
            })?;

        Some(Song {
            id,
            title,
            channel,
            duration,
            thumbnail,
            webpage_url,
        })
    }
}

impl MediaSearchPort for YtDlpClient {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Song>, DomainError> {
        self.search(query, limit).await
    }

    async fn get_stream_url(&self, url: &str, audio_only: bool) -> Result<String, DomainError> {
        self.get_stream_url(url, audio_only).await
    }
}
