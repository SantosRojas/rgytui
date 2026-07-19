use serde::{Deserialize, Serialize};

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
    pub current_index: usize,
    #[serde(skip)]
    pub version: usize,
}

impl Default for Playlist {
    fn default() -> Self {
        Self {
            name: String::from("Queue"),
            songs: Vec::new(),
            current_index: 0,
            version: 0,
        }
    }
}

impl Playlist {
    pub fn current_song(&self) -> Option<&Song> {
        self.songs.get(self.current_index)
    }

    pub fn next(&mut self) -> Option<&Song> {
        if self.current_index + 1 < self.songs.len() {
            self.current_index += 1;
            self.songs.get(self.current_index)
        } else {
            None
        }
    }

    pub fn previous(&mut self) -> Option<&Song> {
        if self.current_index > 0 {
            self.current_index -= 1;
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

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.songs.is_empty()
    }

    pub fn songs(&self) -> &[Song] {
        &self.songs
    }

    pub fn set_current_index(&mut self, index: usize) {
        if index < self.songs.len() {
            self.current_index = index;
        }
    }

    #[allow(dead_code)]
    pub fn insert(&mut self, index: usize, song: Song) {
        if index <= self.songs.len() {
            self.songs.insert(index, song);
            if index <= self.current_index {
                self.current_index += 1;
            }
            self.version = self.version.wrapping_add(1);
        }
    }
}
