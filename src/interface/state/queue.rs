#[derive(Clone, Debug)]
pub struct QueueViewState {
    pub queue_selected: usize,
    #[allow(dead_code)]
    pub scroll_offset: usize,
}

impl QueueViewState {
    pub fn new() -> Self {
        Self {
            queue_selected: 0,
            scroll_offset: 0,
        }
    }
}

impl Default for QueueViewState {
    fn default() -> Self {
        Self::new()
    }
}
