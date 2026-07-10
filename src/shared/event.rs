use crate::domain::media::Song;

#[derive(Clone, Debug)]
pub enum AppEvent {
    SearchResults(Vec<Song>),
    SearchError(String),
    PlaybackStarted(Song),
    PlaybackProgress(f64, f64),
    PlaybackFinished,
    PlaybackPaused,
    PlaybackResumed,
    PlaybackStopped,
    PlaybackError(String),
    VolumeChanged(f32),
    DownloadComplete { song_title: String, file_path: String },
    DownloadError(String),
    Exit,
}
