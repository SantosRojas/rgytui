use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Song {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub duration: f64,
    pub thumbnail: Option<String>,
    pub webpage_url: String,
}

impl Song {
    pub fn duration_formatted(&self) -> String {
        let total_secs = self.duration.max(0.0) as u64;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{:02}:{:02}", mins, secs)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Playlist {
    pub name: String,
    pub songs: Vec<Song>,
    pub current_index: usize,
}

impl Default for Playlist {
    fn default() -> Self {
        Self {
            name: String::from("Queue"),
            songs: Vec::new(),
            current_index: 0,
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
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.songs.len() {
            let was_before_current = index < self.current_index;
            self.songs.remove(index);
            if self.songs.is_empty() {
                self.current_index = 0;
            } else if was_before_current {
                self.current_index = self.current_index.saturating_sub(1);
            } else if self.current_index >= self.songs.len() {
                self.current_index = self.songs.len() - 1;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.songs.is_empty()
    }
}
