//! Pack-authored golf pars, loaded by the library scan, never by a renderer.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

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

static PARS: LazyLock<RwLock<HashMap<PathBuf, GolfPars>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Replace metadata on every scan/reload, including when Golf.ini was removed.
pub fn replace_pack(pack_dir: &Path, data: GolfPars) {
    PARS.write()
        .unwrap_or_else(|e| e.into_inner())
        .insert(pack_dir.to_path_buf(), data);
}

#[must_use]
pub fn par(simfile: &Path, difficulty: &str) -> Option<f64> {
    let song_dir = simfile.parent()?;
    let pack_dir = song_dir.parent()?;
    let folder = song_dir.file_name()?.to_str()?;
    PARS.read()
        .unwrap_or_else(|e| e.into_inner())
        .get(pack_dir)?
        .get(folder, difficulty)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_overrides_fractional_pars_crlf_and_bracketed_folders() {
        let pars = GolfPars::parse(
            "\u{feff}; test\r\n[Komm - [Zaia]]\r\nPar=2.1\r\n[Komm - [Zaia]:Challenge]\r\nPar=4\r\n[Broken]\r\nPar=NaN\r\n[Negative]\r\nPar=-2\r\n",
        );
        assert_eq!(pars.get("Komm - [Zaia]", "Hard"), Some(2.1));
        assert_eq!(pars.get("Komm - [Zaia]", "challenge"), Some(4.0));
        assert_eq!(pars.get("Broken", "Hard"), None);
        assert_eq!(pars.get("Negative", "Hard"), None);
        assert_eq!(pars.get("Other", "Hard"), None);
    }
}
