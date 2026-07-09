use crate::domain::error::DomainError;
use crate::domain::media::Song;

pub trait MediaSearchPort: Send + Sync {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Song>, DomainError>;
    async fn get_stream_url(&self, url: &str, audio_only: bool) -> Result<String, DomainError>;
}
