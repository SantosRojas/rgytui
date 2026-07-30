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

    /// Full path on disk for a given song ID.
    ///
    /// Song IDs from yt-dlp are YouTube video IDs (`[a-zA-Z0-9_-]+`),
    /// which are safe to use directly as filenames.
    fn song_path(&self, song_id: &str) -> PathBuf {
        self.cache_dir.join(song_id)
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
    ///
    /// Atomic write: writes to a `.tmp` file first, then renames to the final
    /// path. This prevents partial/corrupt cache entries if the write is
    /// interrupted (crash, power loss).
    pub async fn put(&self, song_id: &str, data: &[u8]) -> Result<(), DomainError> {
        tokio::fs::create_dir_all(&self.cache_dir).await
            .map_err(|e| DomainError::Other(format!("Failed to create cache dir: {e}")))?;

        let final_path = self.song_path(song_id);
        let tmp_path = self.cache_dir.join(format!("{song_id}.tmp"));

        // Rename is atomic on the same filesystem — write to .tmp first.
        if let Err(e) = tokio::fs::write(&tmp_path, data).await {
            let _ = tokio::fs::remove_file(&tmp_path).await; // best-effort cleanup
            return Err(DomainError::Other(format!("Failed to write cache: {e}")));
        }
        tokio::fs::rename(&tmp_path, &final_path).await
            .map_err(|e| DomainError::Other(format!("Failed to finalize cache: {e}")))?;

        Ok(())
    }

    /// Remove a single song from the cache.
    pub async fn remove(&self, song_id: &str) -> Result<(), DomainError> {
        let path = self.song_path(song_id);
        match tokio::fs::metadata(&path).await {
            Ok(_) => {
                tokio::fs::remove_file(&path).await
                    .map_err(|e| DomainError::Other(format!("Failed to remove cache: {e}")))?;
                Ok(())
            }
            Err(_) => Ok(()), // file doesn't exist — nothing to remove
        }
    }

}
