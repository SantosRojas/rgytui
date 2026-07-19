#[derive(Clone, Debug)]
pub struct QueueViewState {
    pub queue_selected: usize,
}

impl QueueViewState {
    pub fn new() -> Self {
        Self { queue_selected: 0 }
    }
}

impl Default for QueueViewState {
    fn default() -> Self {
        Self::new()
    }
}
