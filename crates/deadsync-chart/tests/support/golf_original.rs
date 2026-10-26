//! Frozen production reference from 83024d64d.
//! Pack-authored golf pars, loaded by the library scan, never by a renderer.
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct GolfPars(HashMap<String, SongPars>);

#[derive(Debug, Default)]
struct SongPars {
    // Slot zero is the default; slots 1..=6 are difficulty overrides.
    values: [f64; 7],
    present: u8,
}

const DIFFICULTIES: [&str; 6] = ["Beginner", "Easy", "Medium", "Hard", "Challenge", "Edit"];

fn difficulty_index(difficulty: &str) -> Option<usize> {
    DIFFICULTIES
        .iter()
        .position(|name| name.eq_ignore_ascii_case(difficulty))
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
                section = Some(name);
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if !key.trim().eq_ignore_ascii_case("Par") {
                continue;
            }
            let Some(name) = section else {
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
            let chart = name.rsplit_once(':').and_then(|(song, difficulty)| {
                difficulty_index(difficulty).map(|index| (song, index + 1))
            });
            let (song, index) = chart.unwrap_or((name, 0));
            let pars = result.0.entry(song.to_owned()).or_default();
            pars.values[index] = par;
            pars.present |= 1 << index;
        }
        result
    }

    #[must_use]
    pub fn get(&self, song_folder: &str, difficulty: &str) -> Option<f64> {
        let song = self.0.get(song_folder)?;
        let mut overrides = song.present & !1;
        while overrides != 0 {
            let index = overrides.trailing_zeros() as usize;
            if DIFFICULTIES[index - 1].eq_ignore_ascii_case(difficulty) {
                return Some(song.values[index]);
            }
            overrides &= overrides - 1;
        }
        (song.present & 1 != 0).then_some(song.values[0])
    }
}
