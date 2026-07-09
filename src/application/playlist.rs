use crate::domain::media::{Playlist, Song};

#[derive(Default)]
pub struct PlaylistUseCase {
    playlist: Playlist,
}

impl PlaylistUseCase {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn playlist(&self) -> &Playlist {
        &self.playlist
    }

    pub fn playlist_mut(&mut self) -> &mut Playlist {
        &mut self.playlist
    }

    pub fn add(&mut self, song: Song) {
        self.playlist.add(song);
    }

    pub fn remove(&mut self, index: usize) {
        self.playlist.remove(index);
    }

    pub fn next(&mut self) -> Option<&Song> {
        self.playlist.next()
    }

    pub fn previous(&mut self) -> Option<&Song> {
        self.playlist.previous()
    }
}
