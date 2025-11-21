use crate::game::chart::ChartData;
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct SongData {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub banner_path: Option<PathBuf>,
    pub background_path: Option<PathBuf>,
    pub music_path: Option<PathBuf>,
    pub display_bpm: String,
    pub offset: f32,
    pub sample_start: Option<f32>,
    pub sample_length: Option<f32>,
    pub min_bpm: f64,
    pub max_bpm: f64,
    pub normalized_bpms: String,
    pub normalized_stops: String,
    pub normalized_delays: String,
    pub normalized_warps: String,
    pub normalized_speeds: String,
    pub normalized_scrolls: String,
    pub normalized_fakes: String,
    /// Length of the music file in seconds (audio duration, including trailing silence).
    /// Mirrors ITGmania's `Song::m_fMusicLengthSeconds` / `MusicLengthSeconds()` Lua.
    pub music_length_seconds: f32,
    /// Length of the chart in seconds based on the last note/hold (`Song::GetLastSecond()` semantics).
    pub total_length_seconds: i32,
    pub charts: Vec<ChartData>,
}

#[derive(Clone, Debug)]
pub struct SongPack {
    pub name: String,
    pub songs: Vec<Arc<SongData>>,
}

static SONG_CACHE: Lazy<Mutex<Vec<SongPack>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Provides safe, read-only access to the global song cache.
pub fn get_song_cache() -> std::sync::MutexGuard<'static, Vec<SongPack>> {
    SONG_CACHE.lock().unwrap()
}

/// A public function to allow the parser to populate the cache.
pub(super) fn set_song_cache(packs: Vec<SongPack>) {
    *SONG_CACHE.lock().unwrap() = packs;
}

impl SongData {
    /// Formats the display BPM for the UI, prioritizing #DISPLAYBPM and cleaning up the format
    /// to match ITGmania (e.g., "128" instead of "128.000000"). Falls back to the
    /// calculated min-max range if #DISPLAYBPM is absent or set to "*".
    pub fn formatted_display_bpm(&self) -> String {
        if !self.display_bpm.is_empty() && &self.display_bpm != "*" {
            let s = &self.display_bpm;
            // Handle range "min:max" or "min - max"
            let parts: Vec<&str> = s.split(|c| c == ':' || c == '-').map(str::trim).collect();
            if parts.len() == 2 {
                if let (Some(min), Some(max)) = (parts[0].parse::<f32>().ok(), parts[1].parse::<f32>().ok()) {
                    let min_i = min.round() as i32;
                    let max_i = max.round() as i32;
                    if min_i == max_i {
                        format!("{}", min_i)
                    } else {
                        format!("{} - {}", min_i.min(max_i), min_i.max(max_i))
                    }
                } else {
                    s.clone() // Fallback if parsing fails
                }
            } else if let Ok(val) = s.parse::<f32>() {
                // Handle single value "128.000000"
                format!("{}", val.round() as i32)
            } else {
                s.clone() // Fallback for other formats
            }
        } else {
            let min = self.min_bpm.round() as i32;
            let max = self.max_bpm.round() as i32;
            if (self.min_bpm - self.max_bpm).abs() < 1e-6 {
                format!("{}", min)
            } else {
                format!("{} - {}", min, max)
            }
        }
    }
}
