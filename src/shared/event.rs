use crossterm::event::{KeyEvent, MouseEvent};

use crate::domain::media::Song;

#[derive(Clone, Debug)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
}

#[derive(Clone, Debug)]
pub enum AppEvent {
    SearchResults(Vec<Song>),
    SearchError(String),
    /// Emitted when a song finishes naturally (used for auto-advance).
    #[allow(dead_code)]
    PlaybackFinished,
    /// Video stream URL resolution failed (background task, Video mode).
    /// `generation` matches the play generation the task was spawned under;
    /// events from older generations are stale and dropped.
    PlaybackError { message: String, generation: u64 },
    /// Live audio backend error (health check) — always current, never stale.
    PlaybackHealthError(String),
    DownloadComplete { song_title: String },
    DownloadError(String),
    /// Audio bytes are ready for playback (downloaded in background).
    AudioReady { song: Song, data: Vec<u8>, generation: u64 },
    /// Background audio download failed. `generation` matches the play
    /// generation the task was spawned under; stale generations are dropped.
    AudioDownloadError { message: String, generation: u64 },
    /// Stream URL resolved for video playback (non-blocking).
    VideoStreamReady { song: Song, stream_url: String, generation: u64 },
    /// Show a confirmation dialog when Ctrl+C is pressed during an active download.
    ShowConfirmExit,
    /// A new rgytui version is available: (version_tag, download_url)
    UpgradeAvailable(String, String),
    /// Generic notification message displayed in the UI.
    Notification(String),
}
