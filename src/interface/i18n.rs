use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Translations {
    map: HashMap<String, String>,
}

impl Translations {
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

        Self { map }
    }

    pub fn t(&self, key: &str) -> String {
        self.map.get(key).cloned().unwrap_or_else(|| key.to_string())
    }
}
