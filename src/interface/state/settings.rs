#[derive(Clone, Debug)]
pub struct SettingsState {
    pub settings_focus: usize,
}

impl SettingsState {
    pub fn new() -> Self {
        Self { settings_focus: 0 }
    }
}

impl Default for SettingsState {
    fn default() -> Self {
        Self::new()
    }
}
