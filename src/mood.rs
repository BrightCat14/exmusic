use serde_json::Value;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug)]
pub struct AudioFeatures {
    pub energy: f32,
    pub valence: f32,
    pub acousticness: f32,
    pub danceability: f32,
    pub explicit: bool,
}

impl Default for AudioFeatures {
    fn default() -> Self {
        Self {
            energy: 0.5,
            valence: 0.55,
            acousticness: 0.45,
            danceability: 0.5,
            explicit: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Neutral,
}

#[derive(Clone, Debug)]
pub struct MoodPalette {
    pub name: &'static str,
    pub top: Cow<'static, str>,
    pub mid: Cow<'static, str>,
    pub bottom: Cow<'static, str>,
    pub glow: Cow<'static, str>,
}

impl MoodPalette {
    fn new(
        name: &'static str,
        top: &'static str,
        mid: &'static str,
        bottom: &'static str,
        glow: &'static str,
    ) -> Self {
        Self {
            name,
            top: Cow::Borrowed(top),
            mid: Cow::Borrowed(mid),
            bottom: Cow::Borrowed(bottom),
            glow: Cow::Borrowed(glow),
        }
    }
}

pub fn classify(features: AudioFeatures) -> Mood {
    let AudioFeatures {
        energy,
        valence,
        acousticness,
        danceability,
        ..
    } = features;

    if energy >= 0.7 && valence < 0.5 {
        Mood::Red
    } else if danceability >= 0.7 && energy >= 0.5 {
        Mood::Purple
    } else if energy >= 0.6 && valence >= 0.6 {
        Mood::Orange
    } else if valence >= 0.7 && energy >= 0.4 {
        Mood::Yellow
    } else if energy < 0.4 && valence < 0.4 {
        Mood::Blue
    } else if acousticness >= 0.5 && energy < 0.6 {
        Mood::Green
    } else {
        Mood::Neutral
    }
}

pub fn palette(mood: Mood) -> MoodPalette {
    match mood {
        Mood::Red => MoodPalette::new("red", "#FFD8DF", "#FFB9C7", "#F79CB0", "#FF9DB4"),
        Mood::Orange => MoodPalette::new("orange", "#FFE4CF", "#FFCAA6", "#FFB080", "#FFA36E"),
        Mood::Yellow => MoodPalette::new("yellow", "#FFF6CC", "#FFE9A3", "#FFDA78", "#FFD05A"),
        Mood::Green => MoodPalette::new("green", "#D9F6E4", "#B7ECCB", "#91DCAE", "#78D69B"),
        Mood::Blue => MoodPalette::new("blue", "#DCEAFF", "#BDD6FF", "#9FC4FF", "#82B0FF"),
        Mood::Purple => MoodPalette::new("purple", "#ECDEFF", "#D8C2FF", "#C3A4FF", "#B18CFF"),
        Mood::Neutral => MoodPalette::new("pink", "#FFE2EE", "#FFC9DC", "#FFAFC9", "#FF9DBF"),
    }
}

pub fn palette_for(features: AudioFeatures) -> MoodPalette {
    palette(classify(features))
}

pub fn features_from_track(value: &Value, album_genre: Option<&str>) -> AudioFeatures {
    let track = value
        .get("track")
        .filter(|track| track.get("title").is_some())
        .unwrap_or(value);
    let params = value
        .get("trackParameters")
        .or_else(|| value.get("track_parameters"))
        .or_else(|| track.get("trackParameters"))
        .or_else(|| track.get("track_parameters"));
    let blocks = [
        track.get("features"),
        track.get("derivedMetadata"),
        track.get("derived_metadata"),
        params,
        Some(track),
        Some(value),
    ];

    let genre = track
        .pointer("/metaData/genre")
        .and_then(Value::as_str)
        .or_else(|| track.pointer("/albums/0/genre").and_then(Value::as_str))
        .or(album_genre)
        .unwrap_or_default()
        .to_ascii_lowercase();

    let read = |key: &str| {
        blocks
            .iter()
            .flatten()
            .find_map(|block| number_field(block, key))
    };
    let explicit_energy = read("energy");
    let explicit_valence = read("valence");
    let explicit_acousticness = read("acousticness");
    let explicit_danceability = read("danceability");
    let explicit = explicit_energy.is_some()
        || explicit_valence.is_some()
        || explicit_acousticness.is_some()
        || explicit_danceability.is_some();
    let bpm = read("bpm");
    let hue = read("hue");
    let loudness = track
        .pointer("/r128/i")
        .and_then(value_number)
        .or_else(|| track.pointer("/normalization/gain").and_then(value_number));

    let energy_fallback = match loudness {
        Some(loudness) => {
            let loudness = ((loudness + 24.0) / 18.0).clamp(0.0, 1.0);
            loudness * 0.6 + energy_bias(&genre) * 0.4
        }
        None => energy_bias(&genre),
    };
    let danceability_fallback = match bpm {
        Some(bpm) => {
            let tempo = ((bpm - 60.0) / 100.0).clamp(0.0, 1.0);
            tempo * 0.55 + danceability_bias(&genre) * 0.45
        }
        None => danceability_bias(&genre),
    };
    let valence_fallback = match hue {
        Some(hue) => {
            let hue = (0.5 + 0.25 * hue.to_radians().sin()).clamp(0.0, 1.0);
            valence_bias(&genre) * 0.55 + hue * 0.45
        }
        None => match cover_valence(track) {
            Some(color) => valence_bias(&genre) * 0.55 + color * 0.45,
            None => valence_bias(&genre),
        },
    };

    AudioFeatures {
        energy: explicit_energy.map(unit).unwrap_or(energy_fallback),
        valence: explicit_valence.map(unit).unwrap_or(valence_fallback),
        acousticness: explicit_acousticness
            .map(unit)
            .unwrap_or_else(|| acousticness_bias(&genre)),
        danceability: explicit_danceability
            .map(unit)
            .unwrap_or(danceability_fallback),
        explicit,
    }
    .clamped()
}

pub fn cover_palette(value: &Value) -> Option<MoodPalette> {
    let track = value
        .get("track")
        .filter(|track| track.get("title").is_some())
        .unwrap_or(value);
    let album = track
        .get("albums")
        .and_then(Value::as_array)
        .and_then(|albums| albums.first());
    let cover = track
        .get("cover")
        .filter(|cover| cover.is_object())
        .or_else(|| {
            album
                .and_then(|album| album.get("cover"))
                .filter(|cover| cover.is_object())
        })
        .or_else(|| value.get("cover").filter(|cover| cover.is_object()))
        .or_else(|| {
            value
                .get("albums")
                .and_then(Value::as_array)
                .and_then(|albums| albums.first())
                .and_then(|album| album.get("cover"))
                .filter(|cover| cover.is_object())
        });
    let derived = cover
        .and_then(|cover| {
            cover
                .get("derivedColors")
                .or_else(|| cover.get("derived_colors"))
        })
        .or_else(|| track.get("derivedColors"))
        .or_else(|| track.get("derived_colors"))
        .or_else(|| album.and_then(|album| album.get("derivedColors")))
        .or_else(|| album.and_then(|album| album.get("derived_colors")));

    let primary = cover
        .and_then(|cover| text_field(cover, &["color"]))
        .or_else(|| derived.and_then(|derived| text_field(derived, &["average"])))
        .or_else(|| derived.and_then(|derived| text_field(derived, &["accent"])))?;
    let accent = derived
        .and_then(|derived| text_field(derived, &["accent"]))
        .unwrap_or(primary);
    let mini = derived
        .and_then(|derived| text_field(derived, &["miniPlayer", "mini_player"]))
        .unwrap_or(primary);
    let wave = derived
        .and_then(|derived| text_field(derived, &["waveText", "wave_text", "wave"]))
        .unwrap_or(primary);

    let top = pastel_hex(primary, 0.62)?;
    let mid = pastel_hex(mini, 0.52).unwrap_or_else(|| top.clone());
    let bottom = pastel_hex(wave, 0.42).unwrap_or_else(|| mid.clone());
    let glow = pastel_hex(accent, 0.28).unwrap_or_else(|| primary.to_owned());

    Some(MoodPalette {
        name: "cover",
        top: Cow::Owned(top),
        mid: Cow::Owned(mid),
        bottom: Cow::Owned(bottom),
        glow: Cow::Owned(glow),
    })
}
impl AudioFeatures {
    fn clamped(self) -> Self {
        Self {
            energy: self.energy.clamp(0.0, 1.0),
            valence: self.valence.clamp(0.0, 1.0),
            acousticness: self.acousticness.clamp(0.0, 1.0),
            danceability: self.danceability.clamp(0.0, 1.0),
            explicit: self.explicit,
        }
    }
}

fn value_number(value: &Value) -> Option<f32> {
    value
        .as_f64()
        .map(|value| value as f32)
        .or_else(|| value.as_str().and_then(|value| value.parse::<f32>().ok()))
}

fn number_field(value: &Value, key: &str) -> Option<f32> {
    value.get(key).and_then(value_number)
}

fn text_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .filter(|value| !value.is_empty())
}

fn pastel_hex(value: &str, amount: f32) -> Option<String> {
    let (red, green, blue) = parse_hex(value)?;
    let mix = |channel: f32| channel + (1.0 - channel) * amount;
    let red = (mix(red) * 255.0).round() as u8;
    let green = (mix(green) * 255.0).round() as u8;
    let blue = (mix(blue) * 255.0).round() as u8;
    Some(format!("#{red:02X}{green:02X}{blue:02X}"))
}

fn unit(value: f32) -> f32 {
    let value = if value > 1.5 { value / 100.0 } else { value };
    value.clamp(0.0, 1.0)
}

fn has_any(genre: &str, words: &[&str]) -> bool {
    words.iter().any(|word| genre.contains(word))
}

fn energy_bias(genre: &str) -> f32 {
    let mut value: f32 = 0.5;
    if has_any(
        genre,
        &[
            "rock",
            "metal",
            "punk",
            "hardcore",
            "drum",
            "dnb",
            "electronic",
            "techno",
            "house",
            "trance",
            "rap",
            "hip-hop",
            "trap",
        ],
    ) {
        value += 0.24;
    }
    if has_any(
        genre,
        &[
            "ambient",
            "classical",
            "acoustic",
            "folk",
            "ballad",
            "lullaby",
        ],
    ) {
        value -= 0.22;
    }
    value.clamp(0.15, 0.85)
}

fn valence_bias(genre: &str) -> f32 {
    let mut value: f32 = 0.55;
    if has_any(
        genre,
        &[
            "pop", "dance", "disco", "funk", "reggae", "ska", "soul", "r&b", "happy", "summer",
        ],
    ) {
        value += 0.22;
    }
    if has_any(
        genre,
        &[
            "ambient",
            "trip-hop",
            "post-rock",
            "shoegaze",
            "doom",
            "dark",
            "sad",
            "melancholy",
            "lo-fi",
            "blues",
        ],
    ) {
        value -= 0.24;
    }
    if has_any(genre, &["metal", "rock", "punk", "industrial"]) {
        value -= 0.12;
    }
    value.clamp(0.15, 0.85)
}

fn acousticness_bias(genre: &str) -> f32 {
    let mut value: f32 = 0.42;
    if has_any(
        genre,
        &[
            "acoustic",
            "folk",
            "jazz",
            "classical",
            "country",
            "blues",
            "singer-songwriter",
            "indie",
        ],
    ) {
        value += 0.34;
    }
    if has_any(
        genre,
        &[
            "electronic",
            "techno",
            "house",
            "trance",
            "synth",
            "edm",
            "industrial",
        ],
    ) {
        value -= 0.3;
    }
    value.clamp(0.12, 0.85)
}

fn danceability_bias(genre: &str) -> f32 {
    let mut value: f32 = 0.5;
    if has_any(
        genre,
        &[
            "dance",
            "disco",
            "house",
            "techno",
            "trance",
            "edm",
            "electronic",
            "hip-hop",
            "r&b",
            "funk",
        ],
    ) {
        value += 0.24;
    }
    if has_any(genre, &["ambient", "classical", "acoustic", "ballad"]) {
        value -= 0.18;
    }
    value.clamp(0.15, 0.85)
}

fn cover_valence(track: &Value) -> Option<f32> {
    let color = track
        .pointer("/derivedColors/accent")
        .or_else(|| track.pointer("/derivedColors/average"))
        .or_else(|| track.pointer("/derived_colors/accent"))
        .or_else(|| track.pointer("/derived_colors/average"))
        .and_then(Value::as_str)?;
    let (red, green, blue) = parse_hex(color)?;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let lightness = (max + min) * 0.5;
    let warmth = red - blue;
    Some((0.5 + warmth * 0.28 + (lightness - 0.5) * 0.35).clamp(0.15, 0.85))
}

fn parse_hex(value: &str) -> Option<(f32, f32, f32)> {
    let value = value.trim().trim_start_matches('#');
    let value = if value.len() == 8 { &value[2..] } else { value };
    if value.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&value[0..2], 16).ok()? as f32 / 255.0;
    let green = u8::from_str_radix(&value[2..4], 16).ok()? as f32 / 255.0;
    let blue = u8::from_str_radix(&value[4..6], 16).ok()? as f32 / 255.0;
    Some((red, green, blue))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn features(energy: f32, valence: f32, acousticness: f32, danceability: f32) -> AudioFeatures {
        AudioFeatures {
            energy,
            valence,
            acousticness,
            danceability,
            explicit: true,
        }
    }

    #[test]
    fn classifies_moods() {
        assert_eq!(classify(features(0.85, 0.3, 0.2, 0.4)), Mood::Red);
        assert_eq!(classify(features(0.8, 0.8, 0.2, 0.8)), Mood::Purple);
        assert_eq!(classify(features(0.65, 0.65, 0.2, 0.5)), Mood::Orange);
        assert_eq!(classify(features(0.5, 0.8, 0.2, 0.5)), Mood::Yellow);
        assert_eq!(classify(features(0.25, 0.25, 0.2, 0.4)), Mood::Blue);
        assert_eq!(classify(features(0.35, 0.5, 0.7, 0.4)), Mood::Green);
    }

    #[test]
    fn parses_explicit_features() {
        let value = json!({
            "features": {
                "energy": 0.9,
                "valence": 0.2,
                "acousticness": 0.1,
                "danceability": 0.4
            }
        });
        let features = features_from_track(&value, None);
        assert_eq!(classify(features), Mood::Red);
    }

    #[test]
    fn parses_rotor_parameters() {
        let value = json!({
            "type": "track",
            "track": {
                "title": "Track",
                "albums": [{ "genre": "metal" }]
            },
            "trackParameters": { "energy": 0.9, "bpm": 170, "hue": 10 }
        });
        let features = features_from_track(&value, None);
        assert!(features.energy > 0.7);
        assert!(features.valence < 0.5);
        assert_eq!(classify(features), Mood::Red);
    }

    #[test]
    fn parses_album_genre_fallback() {
        let value = json!({ "title": "Track" });
        let features = features_from_track(&value, Some("acoustic folk"));
        assert!(features.acousticness > 0.5);
        assert_eq!(classify(features), Mood::Green);
    }

    #[test]
    fn parses_cover_palette() {
        let value = json!({
            "title": "Track",
            "albums": [{
                "cover": {
                    "color": "#6d6e72",
                    "derivedColors": {
                        "average": "#6d6e72",
                        "miniPlayer": "#8a6d72",
                        "waveText": "#4d6e72",
                        "accent": "#c26d72"
                    }
                }
            }]
        });
        let palette = cover_palette(&value).expect("cover palette");
        assert_eq!(palette.name, "cover");
        assert!(palette.top.starts_with('#'));
        assert!(palette.glow.starts_with('#'));
    }

    #[test]
    fn cover_palette_is_optional() {
        let value = json!({ "title": "Track" });
        assert!(cover_palette(&value).is_none());
    }
}
