// Frozen from 241a16adc for differential checks and paired benchmarks.
//! Pack-authored golf pars, loaded by the library scan, never by a renderer.
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct GolfPars(HashMap<String, SongPars>);

#[derive(Debug, Default)]
struct SongPars {
    default: Option<f64>,
    difficulties: HashMap<String, f64>,
}

impl GolfPars {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut result = Self::default();
        let mut section = None;
        for line in text.lines() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                section = Some(name.to_owned());
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if !key.trim().eq_ignore_ascii_case("Par") {
                continue;
            }
            let Some(name) = &section else {
                continue;
            };
            let Some(par) = value
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|p| p.is_finite() && *p >= 0.0)
            else {
                continue;
            };
            let chart = name.rsplit_once(':').filter(|(_, diff)| {
                ["Beginner", "Easy", "Medium", "Hard", "Challenge", "Edit"]
                    .iter()
                    .any(|d| d.eq_ignore_ascii_case(diff))
            });
            if let Some((song, difficulty)) = chart {
                result
                    .0
                    .entry(song.to_owned())
                    .or_default()
                    .difficulties
                    .insert(difficulty.to_ascii_lowercase(), par);
            } else {
                result.0.entry(name.clone()).or_default().default = Some(par);
            }
        }
        result
    }

    #[must_use]
    pub fn get(&self, song_folder: &str, difficulty: &str) -> Option<f64> {
        let song = self.0.get(song_folder)?;
        song.difficulties
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(difficulty))
            .map(|(_, par)| *par)
            .or(song.default)
    }
}
