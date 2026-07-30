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
    PlaybackError(String),
    DownloadComplete { song_title: String },
    DownloadError(String),
    /// Audio bytes are ready for playback (downloaded in background).
    AudioReady { song: Song, data: Vec<u8> },
    /// Background audio download failed.
    AudioDownloadError(String),
    /// Stream URL resolved for video playback (non-blocking).
    VideoStreamReady { song: Song, stream_url: String },
    /// Show a confirmation dialog when Ctrl+C is pressed during an active download.
    ShowConfirmExit,
    /// A new rgytui version is available: (version_tag, download_url)
    UpgradeAvailable(String, String),
    /// Generic notification message displayed in the UI.
    Notification(String),
}
