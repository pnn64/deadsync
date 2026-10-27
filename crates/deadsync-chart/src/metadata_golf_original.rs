// Frozen runtime from 8139799a1.
//! Pack-authored golf pars, loaded by the library scan, never by a renderer.
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

#[derive(Debug, Default)]
pub struct GolfPars(FxHashMap<String, SongPars>);

#[derive(Debug, Default)]
struct SongPars {
    // Slot zero is the default; slots 1..=6 are difficulty overrides.
    values: [f64; 7],
    present: u8,
}

fn difficulty_index(difficulty: &str) -> Option<usize> {
    crate::song::standard_difficulty_index(difficulty)
        .or_else(|| difficulty.eq_ignore_ascii_case("Edit").then_some(5))
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
            // Overrides normally update an existing song; borrow its owned key.
            if index != 0
                && let Some(pars) = result.0.get_mut(song)
            {
                pars.values[index] = par;
                pars.present |= 1 << index;
                continue;
            }
            let pars = result.0.entry(song.to_owned()).or_default();
            pars.values[index] = par;
            pars.present |= 1 << index;
        }
        result
    }

    #[must_use]
    pub fn get(&self, song_folder: &str, difficulty: &str) -> Option<f64> {
        let song = self.0.get(song_folder)?;
        if song.present & !1 != 0
            && let Some(index) = difficulty_index(difficulty)
            && song.present & (1 << (index + 1)) != 0
        {
            return Some(song.values[index + 1]);
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
