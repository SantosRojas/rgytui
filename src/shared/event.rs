use crossterm::event::{KeyEvent, MouseEvent};

use crate::domain::media::Song;

#[derive(Clone, Debug)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum AppEvent {
    SearchResults(Vec<Song>),
    SearchError(String),
    PlaybackStarted(Song),
    PlaybackFinished,
    PlaybackPaused,
    PlaybackResumed,
    PlaybackStopped,
    PlaybackError(String),
    VolumeChanged(f32),
    DownloadComplete { song_title: String, #[allow(dead_code)] file_path: String },
    DownloadError(String),
    /// Audio bytes are ready for playback (downloaded in background).
    AudioReady { song: Song, data: Vec<u8> },
    /// Background audio download failed.
    AudioDownloadError(String),
    /// Show a confirmation dialog when Ctrl+C is pressed during an active download.
    ShowConfirmExit,
    Exit,
}
