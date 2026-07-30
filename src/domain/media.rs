use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum RepeatMode {
    #[default]
    None,
    All,
    One,
}

impl RepeatMode {
    pub fn next(self) -> Self {
        match self {
            Self::None => Self::All,
            Self::All => Self::One,
            Self::One => Self::None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Song {
    pub id: String,
    pub title: String,
    #[serde(default = "default_channel")]
    pub channel: String,
    #[serde(default)]
    pub duration: f64,
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub webpage_url: String,
}

fn default_channel() -> String {
    "Unknown".into()
}

impl Song {
    pub fn duration_formatted(&self) -> String {
        let total_secs = self.duration.max(0.0) as u64;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{:02}:{:02}", mins, secs)
    }
}

/// Raw JSON structure from yt-dlp with optional fields for flexible parsing.
/// Converted to `Song` after deserialization to handle field aliases and defaults.
#[derive(Deserialize)]
pub struct RawSong {
    pub id: String,
    pub title: String,
    pub channel: Option<String>,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub webpage_url: Option<String>,
}

impl From<RawSong> for Song {
    fn from(r: RawSong) -> Self {
        let channel = r.channel.or(r.uploader).unwrap_or_else(default_channel);
        let webpage_url = r.webpage_url.unwrap_or_else(|| {
            if r.id.starts_with("http") {
                r.id.clone()
            } else {
                format!("https://youtube.com/watch?v={}", r.id)
            }
        });
        Song {
            id: r.id,
            title: r.title,
            channel,
            duration: r.duration.unwrap_or(0.0),
            thumbnail: r.thumbnail,
            webpage_url,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Playlist {
    pub name: String,
    pub songs: Vec<Song>,
    #[serde(default)]
    pub current_index: usize,
    #[serde(default)]
    pub repeat_mode: RepeatMode,
    #[serde(skip)]
    pub version: usize,
}

impl Default for Playlist {
    fn default() -> Self {
        Self {
            name: String::from("Queue"),
            songs: Vec::new(),
            current_index: 0,
            repeat_mode: RepeatMode::None,
            version: 0,
        }
    }
}

impl Playlist {
    pub fn current_song(&self) -> Option<&Song> {
        self.songs.get(self.current_index)
    }

    pub fn next(&mut self) -> Option<&Song> {
        if self.songs.is_empty() {
            return None;
        }
        if self.current_index + 1 < self.songs.len() {
            self.current_index += 1;
            self.songs.get(self.current_index)
        } else if self.repeat_mode == RepeatMode::All {
            self.current_index = 0;
            self.songs.get(self.current_index)
        } else {
            None
        }
    }

    pub fn previous(&mut self) -> Option<&Song> {
        if self.songs.is_empty() {
            return None;
        }
        if self.current_index > 0 {
            self.current_index -= 1;
            self.songs.get(self.current_index)
        } else if self.repeat_mode == RepeatMode::All {
            self.current_index = self.songs.len() - 1;
            self.songs.get(self.current_index)
        } else {
            None
        }
    }

    pub fn add(&mut self, song: Song) {
        self.songs.push(song);
        self.version = self.version.wrapping_add(1);
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.songs.len() {
            let was_before_current = index < self.current_index;
            let was_current = index == self.current_index;
            self.songs.remove(index);
            if self.songs.is_empty() {
                self.current_index = 0;
            } else if was_current {
                self.current_index = self.current_index.min(self.songs.len().saturating_sub(1));
            } else if was_before_current {
                self.current_index = self.current_index.saturating_sub(1);
            } else if self.current_index >= self.songs.len() {
                self.current_index = self.songs.len() - 1;
            }
            self.version = self.version.wrapping_add(1);
        }
    }

    pub fn clear(&mut self) {
        self.songs.clear();
        self.current_index = 0;
        self.version = self.version.wrapping_add(1);
    }

    pub fn len(&self) -> usize {
        self.songs.len()
    }

    pub fn songs(&self) -> &[Song] {
        &self.songs
    }

    pub fn set_current_index(&mut self, index: usize) {
        if index < self.songs.len() {
            self.current_index = index;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str) -> Song {
        Song {
            id: id.to_string(),
            title: format!("Song {id}"),
            channel: "Test".into(),
            duration: 120.0,
            thumbnail: None,
            webpage_url: format!("https://youtube.com/watch?v={id}"),
        }
    }

    #[test]
    fn playlist_default_is_empty_queue() {
        let p = Playlist::default();
        assert!(p.songs.is_empty());
        assert_eq!(p.name, "Queue");
        assert_eq!(p.current_index, 0);
        assert_eq!(p.version, 0);
    }

    #[test]
    fn playlist_add_appends_song() {
        let mut p = Playlist::default();
        p.add(song("a"));
        assert_eq!(p.songs.len(), 1);
        assert_eq!(p.songs[0].id, "a");
    }

    #[test]
    fn playlist_add_increments_version() {
        let mut p = Playlist::default();
        assert_eq!(p.version, 0);
        p.add(song("a"));
        assert_eq!(p.version, 1);
        p.add(song("b"));
        assert_eq!(p.version, 2);
    }

    #[test]
    fn playlist_current_song_returns_none_when_empty() {
        let p = Playlist::default();
        assert!(p.current_song().is_none());
    }

    #[test]
    fn playlist_current_song_returns_first_song_after_add() {
        let mut p = Playlist::default();
        p.add(song("a"));
        assert_eq!(p.current_song().map(|s| s.id.as_str()), Some("a"));
    }

    #[test]
    fn playlist_next_increments_index() {
        let mut p = Playlist::default();
        p.add(song("a"));
        p.add(song("b"));
        p.add(song("c"));
        assert_eq!(p.current_index, 0);
        p.next();
        assert_eq!(p.current_index, 1);
        assert_eq!(p.current_song().map(|s| s.id.as_str()), Some("b"));
    }

    #[test]
    fn playlist_next_at_end_without_repeat_returns_none() {
        let mut p = Playlist::default();
        p.add(song("a"));
        p.add(song("b"));
        // Move to last song
        p.next();
        // Now at "b", there's no more
        assert!(p.next().is_none());
    }

    #[test]
    fn playlist_next_wraps_with_repeat_all() {
        let mut p = Playlist { repeat_mode: RepeatMode::All, ..Default::default() };
        p.add(song("a"));
        p.add(song("b"));
        p.next();
        p.next(); // past last
        assert!(p.current_song().is_some());
        assert_eq!(p.current_index, 0);
    }

    #[test]
    fn playlist_next_on_empty_returns_none() {
        let mut p = Playlist::default();
        assert!(p.next().is_none());
    }

    #[test]
    fn playlist_previous_decrements_index() {
        let mut p = Playlist::default();
        p.add(song("a"));
        p.add(song("b"));
        p.set_current_index(1);
        p.previous();
        assert_eq!(p.current_index, 0);
        assert_eq!(p.current_song().map(|s| s.id.as_str()), Some("a"));
    }

    #[test]
    fn playlist_previous_at_start_without_repeat_returns_none() {
        let mut p = Playlist::default();
        p.add(song("a"));
        assert!(p.previous().is_none());
    }

    #[test]
    fn playlist_previous_wraps_with_repeat_all() {
        let mut p = Playlist { repeat_mode: RepeatMode::All, ..Default::default() };
        p.add(song("a"));
        p.add(song("b"));
        p.previous(); // at 0, wraps to end
        assert_eq!(p.current_index, 1);
    }

    #[test]
    fn playlist_previous_on_empty_returns_none() {
        let mut p = Playlist::default();
        assert!(p.previous().is_none());
    }

    #[test]
    fn playlist_remove_current_adjusts_index() {
        let mut p = Playlist::default();
        p.add(song("a"));
        p.add(song("b"));
        p.add(song("c"));
        p.set_current_index(1); // "b"
        p.remove(1);
        assert_eq!(p.songs.len(), 2);
        // current was "b" (removed), so index clamps to min(1, 1) = 1 → "c"
        assert_eq!(p.current_song().map(|s| s.id.as_str()), Some("c"));
    }

    #[test]
    fn playlist_remove_before_current_shifts_index() {
        let mut p = Playlist::default();
        p.add(song("a"));
        p.add(song("b"));
        p.add(song("c"));
        p.set_current_index(2); // "c"
        p.remove(0); // remove "a"
        assert_eq!(p.current_index, 1);
        assert_eq!(p.current_song().map(|s| s.id.as_str()), Some("c"));
    }

    #[test]
    fn playlist_remove_after_current_does_not_affect_index() {
        let mut p = Playlist::default();
        p.add(song("a"));
        p.add(song("b"));
        p.add(song("c"));
        p.set_current_index(0); // "a"
        p.remove(2); // remove "c"
        assert_eq!(p.current_index, 0);
        assert_eq!(p.current_song().map(|s| s.id.as_str()), Some("a"));
    }

    #[test]
    fn playlist_remove_last_current_song_empty() {
        let mut p = Playlist::default();
        p.add(song("a"));
        p.remove(0);
        assert_eq!(p.songs.len(), 0);
        assert_eq!(p.current_index, 0);
        assert!(p.current_song().is_none());
    }

    #[test]
    fn playlist_remove_out_of_range_does_nothing() {
        let mut p = Playlist::default();
        p.add(song("a"));
        let v = p.version; // 1 after add
        p.remove(10);
        assert_eq!(p.songs.len(), 1);
        assert_eq!(p.version, v); // not incremented again
    }

    #[test]
    fn playlist_remove_increments_version() {
        let mut p = Playlist::default();
        p.add(song("a"));
        p.add(song("b"));
        let v = p.version;
        p.remove(0);
        assert!(p.version > v);
    }

    #[test]
    fn playlist_clear_empties_all() {
        let mut p = Playlist::default();
        p.add(song("a"));
        p.add(song("b"));
        p.set_current_index(1);
        p.clear();
        assert!(p.songs.is_empty());
        assert_eq!(p.current_index, 0);
    }

    #[test]
    fn playlist_clear_increments_version() {
        let mut p = Playlist::default();
        p.add(song("a"));
        let v = p.version;
        p.clear();
        assert!(p.version > v);
    }

    #[test]
    fn playlist_len_reflects_song_count() {
        let mut p = Playlist::default();
        assert_eq!(p.len(), 0);
        p.add(song("a"));
        assert_eq!(p.len(), 1);
        p.add(song("b"));
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn playlist_set_current_index_valid() {
        let mut p = Playlist::default();
        p.add(song("a"));
        p.add(song("b"));
        p.set_current_index(1);
        assert_eq!(p.current_index, 1);
    }

    #[test]
    fn playlist_set_current_index_out_of_bounds_does_not_change() {
        let mut p = Playlist::default();
        p.add(song("a"));
        p.set_current_index(5);
        assert_eq!(p.current_index, 0);
    }

    #[test]
    fn playlist_set_current_index_empty_does_not_panic() {
        let mut p = Playlist::default();
        p.set_current_index(0);
        assert_eq!(p.current_index, 0);
    }

    #[test]
    fn playlist_songs_returns_slice() {
        let mut p = Playlist::default();
        p.add(song("a"));
        let s = p.songs();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].id, "a");
    }

    #[test]
    fn playlist_repeat_mode_default_is_none() {
        assert_eq!(RepeatMode::default(), RepeatMode::None);
    }

    #[test]
    fn playlist_repeat_mode_cycle() {
        assert_eq!(RepeatMode::None.next(), RepeatMode::All);
        assert_eq!(RepeatMode::All.next(), RepeatMode::One);
        assert_eq!(RepeatMode::One.next(), RepeatMode::None);
    }
}
