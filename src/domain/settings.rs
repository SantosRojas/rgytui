use serde::{Deserialize, Serialize};

/// User-configurable application settings.
///
/// Pure domain model — no I/O, no infrastructure dependencies.
/// Default values and persistence are handled by the config adapter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub volume: f32,
    pub audio_mode: bool,
    pub default_search_limit: usize,
    pub theme: String,
    pub accent_color: String,
    pub download_path: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_loading_animation")]
    pub loading_animation: String,
}

fn default_language() -> String {
    "en".into()
}

fn default_loading_animation() -> String {
    "pulse".into()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            volume: 0.8,
            audio_mode: false,
            default_search_limit: 10,
            theme: "dark".into(),
            accent_color: "#00ffff".into(),
            download_path: String::new(),
            language: "en".into(),
            loading_animation: "pulse".into(),
        }
    }
}
