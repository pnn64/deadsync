use crate::chart::{ChartData, ChartDisplayBpm};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPref {
    Default,
    Null,
    Itg,
}

pub const ITG_SYNC_OFFSET_SECONDS: f32 = -0.009;

#[inline(always)]
pub const fn resolve_sync_pref(pref: SyncPref, default: SyncPref) -> SyncPref {
    match pref {
        SyncPref::Default => default,
        SyncPref::Null => SyncPref::Null,
        SyncPref::Itg => SyncPref::Itg,
    }
}

#[inline(always)]
pub const fn sync_pref_offset(pref: SyncPref, default: SyncPref) -> f32 {
    match resolve_sync_pref(pref, default) {
        SyncPref::Itg => ITG_SYNC_OFFSET_SECONDS,
        SyncPref::Default | SyncPref::Null => 0.0,
    }
}

pub fn format_display_bpm_range(range: Option<(f64, f64)>, music_rate: f32) -> String {
    let Some((lo, hi)) = range else {
        return String::new();
    };
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    let lo = lo * f64::from(rate);
    let hi = hi * f64::from(rate);
    let use_decimals = (rate - 1.0).abs() > 0.001;
    let fmt_one = |v: f64| {
        if use_decimals {
            let s = format!("{v:.1}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            format!("{v:.0}")
        }
    };
    if (lo - hi).abs() < 1.0e-6 {
        fmt_one(lo)
    } else {
        format!("{} - {}", fmt_one(lo.min(hi)), fmt_one(lo.max(hi)))
    }
}

pub const STANDARD_DIFFICULTY_NAMES: [&str; 5] =
    ["Beginner", "Easy", "Medium", "Hard", "Challenge"];
pub const STANDARD_DIFFICULTY_COUNT: usize = STANDARD_DIFFICULTY_NAMES.len();

#[inline(always)]
pub fn standard_difficulty_index(difficulty_name: &str) -> Option<usize> {
    STANDARD_DIFFICULTY_NAMES
        .iter()
        .position(|name| difficulty_name.eq_ignore_ascii_case(name))
}

#[inline]
pub fn chart_ix_for_steps_index(
    standard_charts: &[Option<usize>; STANDARD_DIFFICULTY_COUNT],
    steps_index: usize,
    edits_sorted: &[usize],
) -> Option<usize> {
    if steps_index < STANDARD_DIFFICULTY_COUNT {
        return standard_charts[steps_index];
    }

    let edit_index = steps_index - STANDARD_DIFFICULTY_COUNT;
    edits_sorted.get(edit_index).copied()
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
    /// First charted step second across the song, mirroring `ITGmania`'s
    /// `Song::GetFirstSecond()` selection behavior.
    pub first_second: f32,
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

    #[inline(always)]
    pub fn precise_first_second(&self) -> f32 {
        if self.first_second.is_finite() {
            self.first_second
        } else {
            0.0
        }
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
        title_subtitle_contains_ignore_ascii_case(&self.title, &self.subtitle, "no cmod")
    }

    pub fn has_standard_difficulty(&self, chart_type: &str, difficulty_index: usize) -> bool {
        let Some(target) = STANDARD_DIFFICULTY_NAMES.get(difficulty_index) else {
            return false;
        };
        self.charts.iter().any(|chart| {
            chart.chart_type.eq_ignore_ascii_case(chart_type)
                && chart.difficulty.eq_ignore_ascii_case(target)
        })
    }

    pub fn edit_charts_sorted(&self, chart_type: &str) -> Vec<&ChartData> {
        let mut edits: Vec<&ChartData> = self
            .charts
            .iter()
            .filter(|chart| {
                chart.chart_type.eq_ignore_ascii_case(chart_type)
                    && chart.difficulty.eq_ignore_ascii_case("edit")
            })
            .collect();
        edits.sort_by_cached_key(|chart| {
            (
                chart.description.to_lowercase(),
                chart.meter,
                chart.short_hash.as_str(),
            )
        });
        edits
    }

    pub fn edit_chart_indices_sorted(&self, chart_type: &str) -> Vec<usize> {
        let mut indices: Vec<usize> = self
            .charts
            .iter()
            .enumerate()
            .filter_map(|(index, chart)| {
                if chart.chart_type.eq_ignore_ascii_case(chart_type)
                    && chart.difficulty.eq_ignore_ascii_case("edit")
                {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        indices.sort_by_cached_key(|&index| {
            let chart = &self.charts[index];
            (
                chart.description.to_lowercase(),
                chart.meter,
                chart.short_hash.as_str(),
            )
        });
        indices
    }

    #[inline]
    pub fn standard_chart_indices(
        &self,
        chart_type: &str,
    ) -> [Option<usize>; STANDARD_DIFFICULTY_COUNT] {
        let mut out = [None; STANDARD_DIFFICULTY_COUNT];
        for (chart_ix, chart) in self.charts.iter().enumerate() {
            if !chart.chart_type.eq_ignore_ascii_case(chart_type) {
                continue;
            }
            for (diff_ix, diff_name) in STANDARD_DIFFICULTY_NAMES.iter().enumerate() {
                if out[diff_ix].is_none() && chart.difficulty.eq_ignore_ascii_case(diff_name) {
                    out[diff_ix] = Some(chart_ix);
                    break;
                }
            }
        }
        out
    }

    pub fn chart_for_steps_index(
        &self,
        chart_type: &str,
        steps_index: usize,
    ) -> Option<&ChartData> {
        if let Some(diff_name) = STANDARD_DIFFICULTY_NAMES.get(steps_index) {
            return self.charts.iter().find(|chart| {
                chart.chart_type.eq_ignore_ascii_case(chart_type)
                    && chart.difficulty.eq_ignore_ascii_case(diff_name)
            });
        }

        let edit_index = steps_index.checked_sub(STANDARD_DIFFICULTY_COUNT)?;
        self.edit_charts_sorted(chart_type).get(edit_index).copied()
    }

    #[inline]
    pub fn chart_music_path(&self, chart_type: &str, steps_index: usize) -> Option<&PathBuf> {
        self.chart_for_steps_index(chart_type, steps_index)
            .and_then(|chart| chart.music_path.as_ref())
    }

    pub fn steps_index_for_chart_hash(&self, chart_type: &str, chart_hash: &str) -> Option<usize> {
        let chart = self.charts.iter().find(|chart| {
            chart.chart_type.eq_ignore_ascii_case(chart_type) && chart.short_hash == chart_hash
        })?;

        if let Some(index) = standard_difficulty_index(&chart.difficulty) {
            return Some(index);
        }
        if chart.difficulty.eq_ignore_ascii_case("edit") {
            let edits = self.edit_charts_sorted(chart_type);
            let pos = edits
                .iter()
                .position(|chart| chart.short_hash == chart_hash)?;
            return Some(STANDARD_DIFFICULTY_COUNT + pos);
        }
        None
    }

    pub fn steps_len(&self, chart_type: &str) -> usize {
        STANDARD_DIFFICULTY_COUNT + self.edit_charts_sorted(chart_type).len()
    }

    pub fn best_steps_index(
        &self,
        chart_type: &str,
        preferred_difficulty_index: usize,
    ) -> Option<usize> {
        let preferred = preferred_difficulty_index.min(STANDARD_DIFFICULTY_COUNT - 1);
        let mut best_standard = None;
        let mut best_distance = usize::MAX;
        for index in 0..STANDARD_DIFFICULTY_COUNT {
            if self.chart_for_steps_index(chart_type, index).is_none() {
                continue;
            }
            let distance = index.abs_diff(preferred);
            if distance < best_distance {
                best_distance = distance;
                best_standard = Some(index);
            }
        }
        if best_standard.is_some() {
            return best_standard;
        }

        if self.edit_charts_sorted(chart_type).is_empty() {
            None
        } else {
            Some(STANDARD_DIFFICULTY_COUNT)
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
        format_display_bpm_range(self.display_bpm_range(), 1.0)
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

    pub fn display_bpm_pair_or(&self, chart: Option<&ChartData>, fallback: [f32; 2]) -> [f32; 2] {
        self.chart_display_bpm_range(chart)
            .map(|(lo, hi)| {
                let lo = lo as f32;
                let hi = hi as f32;
                if lo.is_finite() && hi.is_finite() && lo > 0.0 && hi > 0.0 {
                    [lo, hi]
                } else {
                    fallback
                }
            })
            .unwrap_or(fallback)
    }

    /// Formats display BPM for UI text, checking chart-level tag first.
    pub fn formatted_chart_display_bpm(&self, chart: Option<&ChartData>) -> String {
        format_display_bpm_range(self.chart_display_bpm_range(chart), 1.0)
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

#[inline]
fn title_subtitle_contains_ignore_ascii_case(title: &str, subtitle: &str, needle: &str) -> bool {
    let subtitle = (!subtitle.trim().is_empty()).then_some(subtitle);
    let joined_len = title
        .len()
        .saturating_add(subtitle.map_or(0, |value| value.len().saturating_add(1)));
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return true;
    }
    if needle.len() > joined_len {
        return false;
    }

    let title = title.as_bytes();
    let subtitle = subtitle.map(str::as_bytes).unwrap_or_default();
    (0..=joined_len - needle.len()).any(|start| {
        needle.iter().enumerate().all(|(offset, expected)| {
            let index = start + offset;
            let actual = if index < title.len() {
                title[index]
            } else if index == title.len() {
                b' '
            } else {
                subtitle[index - title.len() - 1]
            };
            actual.eq_ignore_ascii_case(expected)
        })
    })
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[inline(always)]
pub fn title_subtitle_is_no_cmod_for_bench(title: &str, subtitle: &str) -> bool {
    title_subtitle_contains_ignore_ascii_case(title, subtitle, "no cmod")
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[inline(always)]
pub fn title_subtitle_is_no_cmod_legacy_for_bench(title: &str, subtitle: &str) -> bool {
    let full_title = if subtitle.trim().is_empty() {
        title.to_owned()
    } else {
        format!("{title} {subtitle}")
    };
    full_title.to_ascii_lowercase().contains("no cmod")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::{ArrowStats, StaminaCounts, TechCounts};

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
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: Vec::new(),
        }
    }

    fn chart_data() -> ChartData {
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 12,
            step_artist: String::new(),
            music_path: None,
            short_hash: "hash".to_string(),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 150.0,
            max_bpm: 210.0,
        }
    }

    fn chart_with_difficulty(difficulty: &str) -> ChartData {
        let mut chart = chart_data();
        chart.difficulty = difficulty.to_string();
        chart.short_hash = format!("{difficulty}-hash");
        chart
    }

    fn song_with_charts(difficulties: &[&str]) -> SongData {
        let mut song = song_data();
        song.charts = difficulties
            .iter()
            .map(|difficulty| chart_with_difficulty(difficulty))
            .collect();
        song
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

        song.title = "Ends with no".to_string();
        song.subtitle = "cMoD crossover".to_string();
        assert!(song.is_no_cmod());

        song.title = "NØ CMOD".to_string();
        song.subtitle = "   ".to_string();
        assert!(!song.is_no_cmod());
    }

    #[test]
    fn display_bpm_uses_tag_before_actual_range() {
        let mut song = song_data();
        song.display_bpm = "200:100".to_string();

        assert_eq!(song.display_bpm_range(), Some((100.0, 200.0)));
        assert_eq!(song.formatted_display_bpm(), "100 - 200");
    }

    #[test]
    fn display_bpm_formatter_applies_music_rate() {
        assert_eq!(
            format_display_bpm_range(Some((100.0, 200.0)), 1.5),
            "150 - 300"
        );
        assert_eq!(
            format_display_bpm_range(Some((150.0, 150.0)), 1.25),
            "187.5"
        );
        assert_eq!(
            format_display_bpm_range(Some((200.0, 100.0)), f32::NAN),
            "100 - 200"
        );
        assert_eq!(format_display_bpm_range(None, 1.0), "");
    }

    #[test]
    fn display_bpm_pair_uses_chart_then_fallback() {
        let mut song = song_data();
        song.display_bpm = "100:160".to_string();
        let mut chart = chart_data();

        assert_eq!(
            song.display_bpm_pair_or(Some(&chart), [60.0, 60.0]),
            [150.0, 210.0]
        );

        chart.display_bpm = Some(ChartDisplayBpm::Specified {
            min: 180.0,
            max: 120.0,
        });
        assert_eq!(
            song.display_bpm_pair_or(Some(&chart), [60.0, 60.0]),
            [180.0, 120.0]
        );

        chart.display_bpm = Some(ChartDisplayBpm::Random);
        assert_eq!(
            song.display_bpm_pair_or(Some(&chart), [60.0, 60.0]),
            [60.0, 60.0]
        );

        assert_eq!(song.display_bpm_pair_or(None, [60.0, 60.0]), [100.0, 160.0]);
    }

    #[test]
    fn chart_steps_index_returns_exact_standard_match() {
        let song = song_with_charts(&["Beginner", "Easy", "Medium", "Hard", "Challenge"]);

        assert_eq!(song.best_steps_index("dance-single", 4), Some(4));
        assert_eq!(
            song.chart_for_steps_index("dance-single", 4)
                .map(|chart| chart.difficulty.as_str()),
            Some("Challenge")
        );
    }

    #[test]
    fn chart_steps_index_returns_nearest_standard_match() {
        let song = song_with_charts(&["Beginner", "Easy", "Hard"]);

        assert_eq!(song.best_steps_index("dance-single", 4), Some(3));
        let result = song.best_steps_index("dance-single", 2);
        assert!(result == Some(1) || result == Some(3));
    }

    #[test]
    fn chart_steps_index_fallback_does_not_corrupt_preference() {
        let song_full = song_with_charts(&["Beginner", "Easy", "Medium", "Hard", "Challenge"]);
        let song_partial = song_with_charts(&["Beginner", "Easy", "Hard"]);
        let preferred = 4;
        let mut selected = 4;

        if let Some(index) = song_full.best_steps_index("dance-single", preferred) {
            selected = index;
        }
        assert_eq!(selected, 4);

        if let Some(index) = song_partial.best_steps_index("dance-single", preferred) {
            selected = index;
        }
        assert_eq!(selected, 3);

        if let Some(index) = song_full.best_steps_index("dance-single", preferred) {
            selected = index;
        }
        assert_eq!(selected, 4);
    }

    #[test]
    fn edit_chart_steps_follow_sorted_order_after_standard_slots() {
        let mut song = song_with_charts(&["Easy"]);
        let mut later = chart_with_difficulty("Edit");
        later.description = "Zeta".to_string();
        later.meter = 8;
        later.short_hash = "later-edit".to_string();
        let mut first = chart_with_difficulty("Edit");
        first.description = "alpha".to_string();
        first.meter = 9;
        first.short_hash = "first-edit".to_string();
        song.charts.push(later);
        song.charts.push(first);

        assert_eq!(
            song.steps_len("dance-single"),
            STANDARD_DIFFICULTY_COUNT + 2
        );
        assert_eq!(
            song.chart_for_steps_index("dance-single", STANDARD_DIFFICULTY_COUNT)
                .map(|chart| chart.short_hash.as_str()),
            Some("first-edit")
        );
        assert_eq!(
            song.steps_index_for_chart_hash("dance-single", "later-edit"),
            Some(STANDARD_DIFFICULTY_COUNT + 1)
        );
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

    #[test]
    fn sync_pref_offset_uses_pack_pref_before_default() {
        assert_eq!(sync_pref_offset(SyncPref::Null, SyncPref::Itg), 0.0);
        assert_eq!(
            sync_pref_offset(SyncPref::Itg, SyncPref::Null),
            ITG_SYNC_OFFSET_SECONDS
        );
    }

    #[test]
    fn sync_pref_offset_uses_default_for_default_pack_pref() {
        assert_eq!(sync_pref_offset(SyncPref::Default, SyncPref::Null), 0.0);
        assert_eq!(
            sync_pref_offset(SyncPref::Default, SyncPref::Itg),
            ITG_SYNC_OFFSET_SECONDS
        );
    }
}
