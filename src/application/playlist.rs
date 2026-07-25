use std::sync::Arc;

use crate::domain::media::{Playlist, RepeatMode, Song};

pub struct PlaylistUseCase {
    pub(crate) playlist: Playlist,
    songs_arc: Arc<[Song]>,
    last_version: usize,
}

impl PlaylistUseCase {
    pub fn new() -> Self {
        Self {
            playlist: Playlist::default(),
            songs_arc: Arc::from([] as [Song; 0]),
            last_version: 0,
        }
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

    pub fn has_current_song(&self) -> bool {
        self.playlist.current_song().is_some()
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

    pub fn songs(&self) -> &[Song] {
        self.playlist.songs()
    }

    /// Returns an `Arc<[Song]>` that is cached and only rebuilt when the playlist version changes.
    /// This avoids cloning the entire song Vec every render frame.
    pub fn repeat_mode(&self) -> RepeatMode {
        self.playlist.repeat_mode
    }

    pub fn cycle_repeat_mode(&mut self) -> RepeatMode {
        self.playlist.repeat_mode = self.playlist.repeat_mode.next();
        self.playlist.repeat_mode
    }

    pub fn songs_arc(&mut self) -> Arc<[Song]> {
        let v = self.playlist.version;
        if v != self.last_version {
            self.songs_arc = self.playlist.songs().to_vec().into();
            self.last_version = v;
        }
        self.songs_arc.clone()
    }
}
