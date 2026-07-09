#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayerState {
    Idle,
    Loading,
    Playing,
    Paused,
    Stopped,
}

impl PlayerState {
    pub fn can_play(&self) -> bool {
        matches!(self, PlayerState::Idle | PlayerState::Stopped)
    }

    pub fn can_pause(&self) -> bool {
        matches!(self, PlayerState::Playing)
    }

    pub fn can_resume(&self) -> bool {
        matches!(self, PlayerState::Paused)
    }

    pub fn can_stop(&self) -> bool {
        matches!(self, PlayerState::Playing | PlayerState::Paused)
    }

    pub fn label(&self) -> &str {
        match self {
            PlayerState::Idle => "Idle",
            PlayerState::Loading => "Loading",
            PlayerState::Playing => "Playing",
            PlayerState::Paused => "Paused",
            PlayerState::Stopped => "Stopped",
        }
    }
}
