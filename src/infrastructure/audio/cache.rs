use std::path::PathBuf;

use directories::ProjectDirs;

use crate::domain::error::DomainError;

/// Audio download cache on disk.
///
/// Stores downloaded audio bytes keyed by song ID so replaying a song
/// from the queue reads from disk instead of re-downloading from the network.
///
/// Cache lifecycle is tied to the queue:
/// - Song played → audio is cached automatically
/// - Song removed from queue → cache entry is deleted
/// - Queue cleared → all cache entries are deleted
pub struct AudioCache {
    cache_dir: PathBuf,
}

impl AudioCache {
    /// Create a cache using the platform-standard cache directory.
    ///
    /// Falls back to a temp directory if ProjectDirs is unavailable.
    pub fn new() -> Self {
        let cache_dir = ProjectDirs::from("com", "rgytui", "rgytui")
            .map(|d| d.cache_dir().to_path_buf().join("audio"))
            .unwrap_or_else(|| std::env::temp_dir().join("rgytui").join("audio-cache"));
        Self { cache_dir }
    }

    /// Create a cache at a specific path (useful in tests).
    #[allow(dead_code)]
    pub fn with_dir(path: PathBuf) -> Self {
        Self { cache_dir: path }
    }

    /// Full path on disk for a given song ID.
    ///
    /// Song IDs from yt-dlp are YouTube video IDs (`[a-zA-Z0-9_-]+`),
    /// which are safe to use directly as filenames.
    fn song_path(&self, song_id: &str) -> PathBuf {
        self.cache_dir.join(song_id)
    }

    /// Check whether audio for a given song is already cached.
    #[allow(dead_code)]
    pub fn is_cached(&self, song_id: &str) -> bool {
        self.song_path(song_id).exists()
    }

    /// Read cached audio bytes for a song.
    ///
    /// Returns `Ok(None)` if the song is not cached.
    pub async fn get(&self, song_id: &str) -> Result<Option<Vec<u8>>, DomainError> {
        let path = self.song_path(song_id);
        if path.exists() {
            let data = tokio::fs::read(&path).await
                .map_err(|e| DomainError::Other(format!("Failed to read cache: {e}")))?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    /// Store audio bytes in the cache, keyed by song ID.
    pub async fn put(&self, song_id: &str, data: &[u8]) -> Result<(), DomainError> {
        tokio::fs::create_dir_all(&self.cache_dir).await
            .map_err(|e| DomainError::Other(format!("Failed to create cache dir: {e}")))?;
        tokio::fs::write(self.song_path(song_id), data).await
            .map_err(|e| DomainError::Other(format!("Failed to write cache: {e}")))?;
        Ok(())
    }

    /// Remove a single song from the cache.
    pub fn remove(&self, song_id: &str) -> Result<(), DomainError> {
        let path = self.song_path(song_id);
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| DomainError::Other(format!("Failed to remove cache: {e}")))?;
        }
        Ok(())
    }

    /// Remove every cached audio file by destroying and recreating the cache directory.
    #[allow(dead_code)]
    pub fn clear(&self) -> Result<(), DomainError> {
        if self.cache_dir.exists() {
            std::fs::remove_dir_all(&self.cache_dir)
                .map_err(|e| DomainError::Other(format!("Failed to clear cache: {e}")))?;
            std::fs::create_dir_all(&self.cache_dir)
                .map_err(|e| DomainError::Other(format!("Failed to recreate cache dir: {e}")))?;
        }
        Ok(())
    }

    /// Return the total size (in bytes) of all cached files.
    #[allow(dead_code)]
    pub async fn total_size(&self) -> Result<u64, DomainError> {
        if !self.cache_dir.exists() {
            return Ok(0);
        }
        let mut total = 0u64;
        let mut read_dir = tokio::fs::read_dir(&self.cache_dir).await
            .map_err(|e| DomainError::Other(format!("Failed to read cache dir: {e}")))?;
        while let Some(entry) = read_dir.next_entry().await
            .map_err(|e| DomainError::Other(format!("Failed to read cache entry: {e}")))? {
            if entry.file_type().await.map(|t| t.is_file()).unwrap_or(false) {
                total += entry.metadata().await.map(|m| m.len()).unwrap_or(0);
            }
        }
        Ok(total)
    }
}
