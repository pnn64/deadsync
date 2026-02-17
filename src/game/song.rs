use crate::game::chart::ChartData;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct SongData {
    pub simfile_path: PathBuf,
    pub title: String,
    pub subtitle: String,
    pub translit_title: String,
    pub translit_subtitle: String,
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
    /// Mirrors `ITGmania`'s `Song::m_fMusicLengthSeconds` / `MusicLengthSeconds()` Lua.
    pub music_length_seconds: f32,
    /// Length of the chart in seconds based on the last note/hold (`Song::GetLastSecond()` semantics).
    pub total_length_seconds: i32,
    pub charts: Vec<ChartData>,
}

#[derive(Clone, Debug)]
pub struct SongPack {
    pub group_name: String,
    pub name: String,
    pub sort_title: String,
    #[allow(dead_code)]
    pub translit_title: String,
    #[allow(dead_code)]
    pub series: String,
    #[allow(dead_code)]
    pub year: i32,
    #[allow(dead_code)]
    pub sync_pref: rssp::pack::SyncPref,
    #[allow(dead_code)]
    pub directory: PathBuf,
    pub banner_path: Option<PathBuf>,
    pub songs: Vec<Arc<SongData>>,
}

static SONG_CACHE: std::sync::LazyLock<Mutex<Vec<SongPack>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

/// Provides safe, read-only access to the global song cache.
pub fn get_song_cache() -> std::sync::MutexGuard<'static, Vec<SongPack>> {
    SONG_CACHE.lock().unwrap()
}

/// A public function to allow the parser to populate the cache.
pub(super) fn set_song_cache(packs: Vec<SongPack>) {
    *SONG_CACHE.lock().unwrap() = packs;
}

impl SongData {
    pub fn display_title(&self, translit: bool) -> &str {
        if translit && !self.translit_title.trim().is_empty() {
            self.translit_title.as_str()
        } else {
            self.title.as_str()
        }
    }

    pub fn display_subtitle(&self, translit: bool) -> &str {
        if translit && !self.translit_subtitle.trim().is_empty() {
            self.translit_subtitle.as_str()
        } else {
            self.subtitle.as_str()
        }
    }

    pub fn display_full_title(&self, translit: bool) -> String {
        let title = self.display_title(translit);
        let subtitle = self.display_subtitle(translit);
        if subtitle.trim().is_empty() {
            title.to_string()
        } else {
            format!("{title} {subtitle}")
        }
    }

    #[inline(always)]
    fn parse_display_bpm_tag(s: &str) -> Option<(f64, f64)> {
        let parse_pair = |a: &str, b: &str| -> Option<(f64, f64)> {
            let a = a.trim().parse::<f64>().ok()?;
            let b = b.trim().parse::<f64>().ok()?;
            Some((a.min(b), a.max(b)))
        };
        if let Some((a, b)) = s.split_once(':') {
            return parse_pair(a, b);
        }
        if let Some((a, b)) = s.split_once('-') {
            return parse_pair(a, b);
        }
        let v = s.parse::<f64>().ok()?;
        Some((v, v))
    }

    pub fn display_bpm_range(&self) -> Option<(f64, f64)> {
        let s = self.display_bpm.trim();
        if !s.is_empty()
            && s != "*"
            && let Some((lo, hi)) = Self::parse_display_bpm_tag(s)
            && lo.is_finite()
            && hi.is_finite()
            && lo > 0.0
            && hi > 0.0
        {
            return Some((lo, hi));
        }
        let lo = self.min_bpm;
        let hi = self.max_bpm;
        if lo.is_finite() && hi.is_finite() && lo > 0.0 && hi > 0.0 {
            Some((lo.min(hi), lo.max(hi)))
        } else {
            None
        }
    }

    /// Formats display BPM for UI text.
    ///
    /// Matches Simply Love's semantics by treating non-positive DISPLAYBPM values
    /// as invalid and falling back to actual BPM range.
    pub fn formatted_display_bpm(&self) -> String {
        let Some((lo, hi)) = self.display_bpm_range() else {
            return String::new();
        };
        let lo_i = lo.round() as i32;
        let hi_i = hi.round() as i32;
        if lo_i == hi_i {
            lo_i.to_string()
        } else {
            format!("{} - {}", lo_i.min(hi_i), lo_i.max(hi_i))
        }
    }
}
