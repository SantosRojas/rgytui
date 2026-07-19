use crate::domain::media::Song;

#[derive(Clone, Debug)]
pub struct PlayerViewState {
    pub current_song: Option<Song>,
    pub loading_status: Option<String>,
    pub spinner_frame: usize,
}

impl PlayerViewState {
    pub fn new() -> Self {
        Self {
            current_song: None,
            loading_status: None,
            spinner_frame: 0,
        }
    }

    pub fn tick_spinner(&mut self) {
        self.spinner_frame = self.spinner_frame.wrapping_add(1);
    }

    pub fn spinner_char(&self) -> &'static str {
        const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        SPINNER[self.spinner_frame % SPINNER.len()]
    }
}

impl Default for PlayerViewState {
    fn default() -> Self {
        Self::new()
    }
}
