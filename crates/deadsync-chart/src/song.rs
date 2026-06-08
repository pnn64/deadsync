use crate::chart::{ChartData, ChartDisplayBpm};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPref {
    Default,
    Null,
    Itg,
}

#[derive(Clone, Debug)]
pub enum SongBackgroundChangeTarget {
    File(PathBuf),
    Animation(String),
    NoSongBg,
    Random,
}

#[derive(Clone, Debug)]
pub struct SongBackgroundChange {
    pub start_beat: f32,
    pub target: SongBackgroundChangeTarget,
    pub rate: f32,
    pub effect: String,
    pub file2: Option<PathBuf>,
    pub transition: String,
    pub color1: Option<[f32; 4]>,
    pub color2: Option<[f32; 4]>,
}

impl SongBackgroundChange {
    pub fn new(start_beat: f32, target: SongBackgroundChangeTarget) -> Self {
        Self {
            start_beat,
            target,
            rate: 1.0,
            effect: String::new(),
            file2: None,
            transition: String::new(),
            color1: None,
            color2: None,
        }
    }

    pub fn effect_is(&self, name: &str) -> bool {
        self.effect.eq_ignore_ascii_case(name)
    }

    pub fn transition_is(&self, name: &str) -> bool {
        self.transition.eq_ignore_ascii_case(name)
    }
}

#[derive(Clone, Debug)]
pub struct SongForegroundLuaChange {
    pub start_beat: f32,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct SongForegroundChange {
    pub start_beat: f32,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct SongBackgroundLuaChange {
    pub start_beat: f32,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct SongData {
    pub simfile_path: PathBuf,
    pub title: String,
    pub subtitle: String,
    pub translit_title: String,
    pub translit_subtitle: String,
    pub artist: String,
    pub genre: String,
    pub banner_path: Option<PathBuf>,
    pub background_path: Option<PathBuf>,
    pub background_changes: Vec<SongBackgroundChange>,
    pub background_layer2_changes: Vec<SongBackgroundChange>,
    pub foreground_changes: Vec<SongForegroundChange>,
    pub background_lua_changes: Vec<SongBackgroundLuaChange>,
    pub foreground_lua_changes: Vec<SongForegroundLuaChange>,
    pub has_lua: bool,
    pub cdtitle_path: Option<PathBuf>,
    pub music_path: Option<PathBuf>,
    pub display_bpm: String,
    pub offset: f32,
    pub sample_start: Option<f32>,
    pub sample_length: Option<f32>,
    pub min_bpm: f64,
    pub max_bpm: f64,
    pub normalized_bpms: String,
    /// Length of the music file in seconds (audio duration, including trailing silence).
    /// Mirrors `ITGmania`'s `Song::m_fMusicLengthSeconds` / `MusicLengthSeconds()` Lua.
    pub music_length_seconds: f32,
    /// Length of the chart in seconds based on the last note/hold (`Song::GetLastSecond()` semantics).
    pub total_length_seconds: i32,
    /// Float-precision song end time used by graph scaling and preview helpers.
    pub precise_last_second_seconds: f32,
    pub charts: Vec<ChartData>,
}

#[derive(Clone, Debug)]
pub struct SongPack {
    pub group_name: String,
    pub name: String,
    pub sort_title: String,
    pub translit_title: String,
    pub series: String,
    pub year: i32,
    pub sync_pref: SyncPref,
    pub directory: PathBuf,
    pub banner_path: Option<PathBuf>,
    pub songs: Vec<Arc<SongData>>,
}

impl SongData {
    #[inline(always)]
    fn is_video_path(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "mp4"
                        | "avi"
                        | "f4v"
                        | "flv"
                        | "m4v"
                        | "mov"
                        | "ogv"
                        | "webm"
                        | "mkv"
                        | "mpg"
                        | "mpeg"
                        | "wmv"
                )
            })
    }

    #[inline(always)]
    fn active_background_change(&self, beat: f32) -> Option<&SongBackgroundChange> {
        let mut active = None;
        for change in &self.background_changes {
            if change.start_beat > beat {
                break;
            }
            active = Some(change);
        }
        active
    }

    #[inline(always)]
    fn active_foreground_change(&self, beat: f32) -> Option<&SongForegroundChange> {
        let mut active = None;
        for change in &self.foreground_changes {
            if change.start_beat > beat {
                break;
            }
            active = Some(change);
        }
        active
    }

    #[inline(always)]
    fn fallback_background_path(&self, allow_video: bool) -> Option<&PathBuf> {
        let path = self.background_path.as_ref()?;
        if !path.is_file() {
            return None;
        }
        if allow_video || !Self::is_video_path(path) {
            Some(path)
        } else {
            None
        }
    }

    /// Float-precision song end time used by graph scaling.
    ///
    /// Mirrors ITGmania's `Song::GetLastSecond()` chart-selection behavior:
    /// if any non-Edit chart exists, ignore Edit charts for song length.
    pub fn precise_last_second(&self) -> f32 {
        let fallback = self.total_length_seconds.max(0) as f32;
        self.precise_last_second_seconds.max(fallback)
    }

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

    /// Whether this song is tagged as not allowing CMod.
    ///
    /// Event organizers mark such charts by putting "no cmod" somewhere in the
    /// title or subtitle. Matching is case-insensitive and spans the combined
    /// title + subtitle so the tag is found regardless of which field carries
    /// it. Used by the player-options "No CMod alternative" auto-switch.
    pub fn is_no_cmod(&self) -> bool {
        self.display_full_title(false)
            .to_ascii_lowercase()
            .contains("no cmod")
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

    /// Returns (lo, hi) display BPM, checking the chart's display_bpm first,
    /// then falling back to the song-level display_bpm, then actual min/max.
    pub fn chart_display_bpm_range(&self, chart: Option<&ChartData>) -> Option<(f64, f64)> {
        if let Some(chart) = chart {
            match &chart.display_bpm {
                Some(ChartDisplayBpm::Specified { min, max }) => return Some((*min, *max)),
                Some(ChartDisplayBpm::Random) => return None,
                None => {
                    let lo = chart.min_bpm;
                    let hi = chart.max_bpm;
                    if lo.is_finite() && hi.is_finite() && lo > 0.0 && hi > 0.0 {
                        return Some((lo.min(hi), lo.max(hi)));
                    }
                }
            }
        }
        self.display_bpm_range()
    }

    /// Formats display BPM for UI text, checking chart-level tag first.
    pub fn formatted_chart_display_bpm(&self, chart: Option<&ChartData>) -> String {
        let Some((lo, hi)) = self.chart_display_bpm_range(chart) else {
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

    pub fn active_background_path(&self, beat: f32) -> Option<&PathBuf> {
        match self
            .active_background_change(beat)
            .map(|change| &change.target)
        {
            Some(SongBackgroundChangeTarget::File(path)) => Some(path),
            Some(SongBackgroundChangeTarget::Animation(_)) => None,
            Some(SongBackgroundChangeTarget::NoSongBg) => None,
            Some(SongBackgroundChangeTarget::Random) => None,
            None => self.background_path.as_ref(),
        }
    }

    pub fn active_foreground_path(&self, beat: f32) -> Option<&PathBuf> {
        let path = &self.active_foreground_change(beat)?.path;
        path.is_file().then_some(path)
    }

    pub fn gameplay_background_path_for_change_ix(
        &self,
        next_background_change_ix: usize,
        allow_video: bool,
    ) -> Option<&PathBuf> {
        self.gameplay_background_path_for_changes(
            &self.background_changes,
            next_background_change_ix,
            allow_video,
        )
    }

    pub fn gameplay_background_path_for_changes<'a>(
        &'a self,
        background_changes: &'a [SongBackgroundChange],
        next_background_change_ix: usize,
        allow_video: bool,
    ) -> Option<&'a PathBuf> {
        let active_ix = next_background_change_ix
            .min(background_changes.len())
            .checked_sub(1);
        match active_ix
            .and_then(|ix| background_changes.get(ix))
            .map(|change| &change.target)
        {
            Some(SongBackgroundChangeTarget::File(path)) => {
                let exists = path.is_file();
                if exists && (allow_video || !Self::is_video_path(path)) {
                    Some(path)
                } else {
                    self.fallback_background_path(allow_video)
                        .or(exists.then_some(path))
                }
            }
            Some(SongBackgroundChangeTarget::Animation(_)) => {
                self.fallback_background_path(allow_video)
            }
            Some(SongBackgroundChangeTarget::Random) => self.fallback_background_path(allow_video),
            Some(SongBackgroundChangeTarget::NoSongBg) => None,
            None => self.fallback_background_path(allow_video),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song_data() -> SongData {
        SongData {
            simfile_path: PathBuf::from("song.ssc"),
            title: "Original".to_string(),
            subtitle: "Mix".to_string(),
            translit_title: "Translit".to_string(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 120.0,
            max_bpm: 180.0,
            normalized_bpms: String::new(),
            music_length_seconds: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: Vec::new(),
        }
    }

    #[test]
    fn display_full_title_prefers_available_translit_parts() {
        let song = song_data();

        assert_eq!(song.display_full_title(false), "Original Mix");
        assert_eq!(song.display_full_title(true), "Translit Mix");
    }

    #[test]
    fn is_no_cmod_matches_title_or_subtitle_case_insensitively() {
        let mut song = song_data();
        assert!(!song.is_no_cmod());

        song.subtitle = "(NO CMOD)".to_string();
        assert!(song.is_no_cmod());

        song.subtitle = "Mix".to_string();
        song.title = "Hard Song [No CMod]".to_string();
        assert!(song.is_no_cmod());
    }

    #[test]
    fn display_bpm_uses_tag_before_actual_range() {
        let mut song = song_data();
        song.display_bpm = "200:100".to_string();

        assert_eq!(song.display_bpm_range(), Some((100.0, 200.0)));
        assert_eq!(song.formatted_display_bpm(), "100 - 200");
    }

    #[test]
    fn active_background_change_tracks_beat_order() {
        let mut song = song_data();
        song.background_path = Some(PathBuf::from("base.png"));
        song.background_changes = vec![
            SongBackgroundChange::new(4.0, SongBackgroundChangeTarget::NoSongBg),
            SongBackgroundChange::new(
                8.0,
                SongBackgroundChangeTarget::File(PathBuf::from("later.png")),
            ),
        ];

        assert_eq!(
            song.active_background_path(3.0).map(PathBuf::as_path),
            Some(Path::new("base.png"))
        );
        assert_eq!(song.active_background_path(4.0), None);
        assert_eq!(
            song.active_background_path(9.0).map(PathBuf::as_path),
            Some(Path::new("later.png"))
        );
    }
}
