use std::sync::Arc;

use crate::application::ports::MediaSearchPort;
use crate::domain::error::DomainError;
use crate::domain::media::Song;

#[derive(Clone)]
pub struct SearchUseCase {
    client: Arc<dyn MediaSearchPort>,
}

impl SearchUseCase {
    pub fn new(client: Arc<dyn MediaSearchPort>) -> Self {
        Self { client }
    }

    pub async fn execute(&self, query: &str, limit: usize) -> Result<Vec<Song>, DomainError> {
        self.client.search(query, limit).await
    }
}
