use crate::domain::media::{Playlist, Song};

#[derive(Default)]
pub struct PlaylistUseCase {
    pub(crate) playlist: Playlist,
}

impl PlaylistUseCase {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn playlist(&self) -> &Playlist {
        &self.playlist
    }

    pub fn add(&mut self, song: Song) {
        self.playlist.add(song);
    }

    pub fn remove(&mut self, index: usize) {
        self.playlist.remove(index);
    }

    pub fn clear(&mut self) {
        self.playlist.clear();
    }

    pub fn next(&mut self) -> Option<&Song> {
        self.playlist.next()
    }

    pub fn previous(&mut self) -> Option<&Song> {
        self.playlist.previous()
    }

    pub fn current_song_cloned(&self) -> Option<Song> {
        self.playlist.current_song().cloned()
    }

    pub fn set_current_index(&mut self, index: usize) {
        self.playlist.set_current_index(index);
    }

    pub fn len(&self) -> usize {
        self.playlist.len()
    }

    pub fn is_empty(&self) -> bool {
        self.playlist.is_empty()
    }

    pub fn songs(&self) -> &[Song] {
        self.playlist.songs()
    }
}
