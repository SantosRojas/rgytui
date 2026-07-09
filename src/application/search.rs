use crate::domain::error::DomainError;
use crate::domain::media::Song;
use crate::infrastructure::ytdlp::client::YtDlpClient;

#[derive(Clone)]
pub struct SearchUseCase {
    client: YtDlpClient,
}

impl SearchUseCase {
    pub fn new(client: YtDlpClient) -> Self {
        Self { client }
    }

    pub async fn execute(&self, query: &str, limit: usize) -> Result<Vec<Song>, DomainError> {
        self.client.search(query, limit).await
    }
}
