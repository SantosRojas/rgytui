use thiserror::Error;

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("yt-dlp error: {0}")]
    YtDlp(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Audio error: {0}")]
    Audio(String),

    #[error("Player error: {0}")]
    Player(String),

    #[error("{0}")]
    Other(String),
}

impl From<serde_json::Error> for DomainError {
    fn from(e: serde_json::Error) -> Self {
        DomainError::Parse(e.to_string())
    }
}


