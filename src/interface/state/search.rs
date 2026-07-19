use crate::domain::media::Song;

#[derive(Clone, Debug)]
pub struct SearchState {
    pub search_query: String,
    pub search_results: Vec<Song>,
    pub is_searching: bool,
    pub selected_index: usize,
    pub scroll_offset: usize,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            search_query: String::new(),
            search_results: Vec::new(),
            is_searching: false,
            selected_index: 0,
            scroll_offset: 0,
        }
    }
}

impl Default for SearchState {
    fn default() -> Self {
        Self::new()
    }
}
