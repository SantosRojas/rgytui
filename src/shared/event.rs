use crate::domain::media::Song;

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
    Exit,
}
