#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayerState {
    Idle,
    Loading,
    Playing,
    Paused,
    Stopped,
}

impl PlayerState {}
