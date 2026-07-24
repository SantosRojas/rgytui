use std::collections::HashMap;

use crate::application::ports::I18nPort;

#[derive(Clone, Debug)]
pub struct Translations {
    map: HashMap<String, String>,
    language: String,
}

impl Translations {
    /// Detect the system locale using the `sys-locale` crate.
    /// Returns the two-letter language code (e.g., "en", "es").
    /// Falls back to "en" if detection fails.
    pub fn detect_locale() -> String {
        sys_locale::get_locale()
            .and_then(|l| l.get(..2).map(|c| c.to_lowercase()))
            .unwrap_or_else(|| "en".into())
    }

    /// Load translations for the given language.
    /// If the requested language file doesn't exist or parsing fails,
    /// falls back to English.
    pub fn load(lang: &str) -> Self {
        let json_str = match lang {
            "es" => include_str!("translations/es.json"),
            _ => include_str!("translations/en.json"),
        };

        let map: HashMap<String, String> = serde_json::from_str(json_str)
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to parse translations for '{}': {}. Falling back to English.", lang, e);
                serde_json::from_str(include_str!("translations/en.json"))
                    .unwrap_or_default()
            });

        Self {
            map,
            language: lang.to_string(),
        }
    }

    pub fn t(&self, key: &str) -> String {
        self.map.get(key).cloned().unwrap_or_else(|| key.to_string())
    }
}

impl I18nPort for Translations {
    fn t(&self, key: &str) -> String {
        self.t(key)
    }

    fn language(&self) -> &str {
        &self.language
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_locale_returns_string() {
        let locale = Translations::detect_locale();
        assert!(!locale.is_empty(), "locale should not be empty");
        assert_eq!(locale.len(), 2, "locale should be a two-letter code");
    }

    #[test]
    fn test_load_spanish_translations() {
        let t = Translations::load("es");
        // "app_subtitle" has different values in es vs en
        let subtitle = t.t("app_subtitle");
        assert_eq!(subtitle, "Reproductor de Música YouTube",
            "Spanish subtitle should be translated, got '{}'", subtitle);
    }

    #[test]
    fn test_load_english_translations() {
        let t = Translations::load("en");
        assert_eq!(t.t("app_subtitle"), "YouTube Music Player");
    }

    #[test]
    fn test_unsupported_locale_falls_back_to_english() {
        let t = Translations::load("fr");
        // French isn't supported, so it should fall back to English
        assert_eq!(t.t("app_subtitle"), "YouTube Music Player");
    }

    #[test]
    fn test_load_auto_detects_locale() {
        // Use I18nPort trait to test language
        use crate::application::ports::I18nPort;
        let t = Translations::load(&Translations::detect_locale());
        let lang = t.language();
        assert!(!lang.is_empty(), "auto-detected language should not be empty");
        assert!(lang == "en" || lang == "es",
            "auto-detected language should be 'en' or 'es', got '{}'", lang);
        // Translations should be loaded correctly
        let subtitle = t.t("app_subtitle");
        assert!(!subtitle.is_empty(), "auto-loaded translations should have 'app_subtitle' key");
    }

    #[test]
    fn test_language_getter() {
        use crate::application::ports::I18nPort;
        let t = Translations::load("es");
        assert_eq!(t.language(), "es");
        let t = Translations::load("en");
        assert_eq!(t.language(), "en");
    }
}
