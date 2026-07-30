use std::sync::Arc;

use crate::application::ports::I18nPort;
use crate::domain::loading_animation::LoadingAnimation;
use crate::interface::i18n::Translations;
use crate::interface::theme::Theme;

#[derive(Clone, Debug)]
pub struct ConfigState {
    pub theme_name: String,
    pub accent_color: String,
    pub language: String,
    pub translations: Arc<dyn I18nPort>,
    cached_theme: Option<Theme>,
    pub default_search_limit: usize,
    pub download_path: String,
    pub loading_animation: LoadingAnimation,
}

impl ConfigState {
    pub fn new(
        theme_name: String,
        accent_color: String,
        language: String,
        translations: Arc<dyn I18nPort>,
        default_search_limit: usize,
        download_path: String,
        loading_animation: LoadingAnimation,
    ) -> Self {
        Self {
            theme_name,
            accent_color,
            language,
            translations,
            cached_theme: None,
            default_search_limit,
            download_path,
            loading_animation,
        }
    }

    pub fn tr(&self, key: &str) -> String {
        self.translations.t(key)
    }

    pub fn get_or_create_theme(&mut self) -> Theme {
        if let Some(theme) = self.cached_theme {
            return theme;
        }
        let theme = Theme::from_settings(&self.theme_name, &self.accent_color);
        self.cached_theme = Some(theme);
        theme
    }

    pub fn invalidate_theme(&mut self) {
        self.cached_theme = None;
    }
}

impl Default for ConfigState {
    fn default() -> Self {
        Self {
            theme_name: "dark".into(),
            accent_color: "#00ffff".into(),
            language: "es".into(),
            translations: Arc::new(Translations::load("es")),
            cached_theme: None,
            default_search_limit: 10,
            download_path: String::new(),
            loading_animation: LoadingAnimation,
        }
    }
}
