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
    /// Persisted repeat mode as the `RepeatMode` discriminant name
    /// ("None", "All", "One"). Stored as a string so a corrupted value
    /// can't invalidate the whole settings file.
    #[serde(default = "default_repeat_mode")]
    pub repeat_mode: String,
}

fn default_language() -> String {
    "en".into()
}

fn default_repeat_mode() -> String {
    crate::domain::media::RepeatMode::None.as_str().into()
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
            repeat_mode: crate::domain::media::RepeatMode::None.as_str().into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::media::RepeatMode;

    #[test]
    fn default_repeat_mode_is_none() {
        assert_eq!(AppSettings::default().repeat_mode, RepeatMode::None.as_str());
    }

    #[test]
    fn settings_serde_round_trip_preserves_repeat_mode() {
        let s = AppSettings {
            repeat_mode: RepeatMode::All.as_str().to_string(),
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repeat_mode, RepeatMode::All.as_str());
        assert_eq!(back.repeat_mode.parse::<RepeatMode>().unwrap(), RepeatMode::All);
    }

    #[test]
    fn settings_missing_repeat_mode_field_uses_default() {
        // An existing settings.json written before repeat_mode existed must
        // still deserialize instead of resetting the whole config.
        let json = r##"{"volume":0.5,"audio_mode":false,"default_search_limit":10,"theme":"dark","accent_color":"#00ffff","download_path":"","language":"en"}"##;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.repeat_mode, RepeatMode::None.as_str());
        assert!((s.volume - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn settings_corrupt_repeat_mode_value_does_not_break_parse() {
        // A garbage repeat_mode value must not invalidate the whole file —
        // a String field is intentionally used instead of the enum.
        let json = r##"{"volume":0.5,"audio_mode":false,"default_search_limit":10,"theme":"dark","accent_color":"#00ffff","download_path":"","language":"en","repeat_mode":"garbage"}"##;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.repeat_mode, "garbage");
        assert!((s.volume - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn settings_default_round_trips() {
        let json = serde_json::to_string(&AppSettings::default()).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repeat_mode, RepeatMode::None.as_str());
    }
}
