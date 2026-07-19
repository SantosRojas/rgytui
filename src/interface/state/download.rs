use crate::domain::media::Song;

#[derive(Clone, Debug)]
pub struct DownloadPopupState {
    pub show_download_popup: bool,
    pub download_format: usize,
    pub download_song: Option<Song>,
    pub download_pending: Option<(Song, String, String)>,
}

impl DownloadPopupState {
    pub fn new() -> Self {
        Self {
            show_download_popup: false,
            download_format: 0,
            download_song: None,
            download_pending: None,
        }
    }
}

impl Default for DownloadPopupState {
    fn default() -> Self {
        Self::new()
    }
}
