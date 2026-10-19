use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;

const EMBEDDED: &[(&str, &str)] = &[
    ("en", include_str!("../assets/translations/en.json")),
    ("ru", include_str!("../assets/translations/ru.json")),
    ("lolcat", include_str!("../assets/translations/lolcat.json")),
];

#[derive(Clone, Default)]
struct Language {
    name: String,
    strings: HashMap<String, String>,
}

#[derive(Clone, Default)]
pub struct Translations {
    order: Vec<String>,
    languages: HashMap<String, Language>,
}

impl Translations {
    pub fn load() -> Self {
        let mut translations = Self::default();
        for (code, json) in EMBEDDED {
            translations.insert_json(code, json);
        }
        for directory in external_directories() {
            translations.load_directory(&directory);
        }
        translations
    }

    fn insert_json(&mut self, code: &str, json: &str) {
        let Ok(Value::Object(entries)) = serde_json::from_str::<Value>(json) else {
            return;
        };
        let mut language = self.languages.remove(code).unwrap_or_default();
        for (key, value) in entries {
            let Value::String(text) = value else {
                continue;
            };
            if key == "_name" {
                language.name = text;
            } else {
                language.strings.insert(key, text);
            }
        }
        if language.name.is_empty() {
            language.name = code.to_owned();
        }
        if !self.languages.contains_key(code) {
            self.order.push(code.to_owned());
        }
        self.languages.insert(code.to_owned(), language);
    }

    fn load_directory(&mut self, directory: &PathBuf) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect();
        files.sort();
        for path in files {
            let Some(code) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if let Ok(text) = std::fs::read_to_string(&path) {
                self.insert_json(code, &text);
            }
        }
    }

    pub fn translate(&self, language: &str, key: &str) -> String {
        self.languages
            .get(language)
            .and_then(|language| language.strings.get(key))
            .or_else(|| self.languages.get("en")?.strings.get(key))
            .cloned()
            .unwrap_or_else(|| key.to_owned())
    }

    pub fn languages_json(&self) -> String {
        let entries: Vec<Value> = self
            .order
            .iter()
            .filter_map(|code| {
                self.languages
                    .get(code)
                    .map(|language| serde_json::json!({ "code": code, "name": language.name }))
            })
            .collect();
        Value::Array(entries).to_string()
    }

    pub fn track_count(language: &str, count: i64) -> String {
        match language {
            "ru" => format!(
                "{count} {}",
                russian_plural(count, "трек", "трека", "треков")
            ),
            "lolcat" => format!("{count} {}", if count == 1 { "track" } else { "trackz" }),
            _ => format!("{count} {}", if count == 1 { "track" } else { "tracks" }),
        }
    }

    pub fn favorite_count(language: &str, count: i64) -> String {
        let tracks = Self::track_count(language, count);
        match language {
            "ru" => format!("{tracks} в избранном"),
            "lolcat" => format!("{tracks} in favoritz"),
            _ => format!("{tracks} in favorites"),
        }
    }
}

fn russian_plural(count: i64, one: &str, few: &str, many: &str) -> String {
    let mod10 = count % 10;
    let mod100 = count % 100;
    if mod10 == 1 && mod100 != 11 {
        one.to_owned()
    } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
        few.to_owned()
    } else {
        many.to_owned()
    }
}

fn external_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            directories.push(parent.join("translations"));
        }
    }
    if let Some(data) = dirs::data_local_dir().or_else(dirs::home_dir) {
        directories.push(data.join("exmusic").join("translations"));
    }
    directories
}

#[cfg(test)]
mod tests {
    use super::Translations;

    #[test]
    fn embedded_translations_cover_all_languages() {
        let translations = Translations::load();
        assert_eq!(translations.translate("ru", "nav.search"), "Поиск");
        assert_eq!(translations.translate("lolcat", "nav.search"), "Serch");
        assert_eq!(translations.translate("en", "nav.search"), "Search");
    }

    #[test]
    fn unknown_language_and_key_fall_back() {
        let translations = Translations::load();
        assert_eq!(translations.translate("de", "nav.search"), "Search");
        assert_eq!(translations.translate("en", "missing.key"), "missing.key");
    }

    #[test]
    fn plurals_and_language_list() {
        assert_eq!(Translations::track_count("en", 1), "1 track");
        assert_eq!(Translations::track_count("en", 3), "3 tracks");
        assert_eq!(Translations::track_count("lolcat", 3), "3 trackz");
        assert_eq!(Translations::track_count("ru", 1), "1 трек");
        assert_eq!(Translations::track_count("ru", 3), "3 трека");
        assert_eq!(Translations::track_count("ru", 5), "5 треков");
        assert_eq!(
            Translations::favorite_count("lolcat", 1),
            "1 track in favoritz"
        );
        assert!(Translations::load().languages_json().contains("Lolcat"));
    }
}
